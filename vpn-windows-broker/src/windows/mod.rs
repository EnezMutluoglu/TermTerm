pub mod identity;
pub mod pipe;
pub mod wintun;
pub mod network;
pub mod store;
pub mod process;
pub mod runtime;
pub mod validation;
pub mod orphan;
pub mod driver;
#[cfg(feature="privileged-validation")]
pub(super) mod diagnostics;
use anyhow::{ensure, Result};
use std::{io, os::windows::ffi::OsStrExt, path::Path};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};

#[derive(Debug)]
pub struct Handle(pub(crate) HANDLE);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
impl Handle {
    pub(crate) fn new(raw: HANDLE) -> Result<Self> {
        ensure!(!raw.is_null() && raw != INVALID_HANDLE_VALUE, "{}", io::Error::last_os_error());
        Ok(Self(raw))
    }
    pub fn raw(&self) -> HANDLE { self.0 }
}
impl Drop for Handle { fn drop(&mut self) { unsafe { CloseHandle(self.0); } } }
pub(crate) fn wide(value: impl AsRef<std::ffi::OsStr>) -> Result<Vec<u16>> {
    let mut v: Vec<u16> = value.as_ref().encode_wide().collect();
    ensure!(!v.contains(&0), "nul_in_windows_string"); v.push(0); Ok(v)
}
pub(crate) fn win(ok: i32) -> Result<()> { ensure!(ok != 0, "{}", io::Error::last_os_error()); Ok(()) }
pub(crate) fn status(code: u32) -> Result<()> { ensure!(code == 0, "{}", io::Error::from_raw_os_error(code as i32)); Ok(()) }

pub(super) fn broker_startup_error(process: HANDLE, error: anyhow::Error) -> anyhow::Error {
    let mut code = 0;
    if unsafe { windows_sys::Win32::System::Threading::GetExitCodeProcess(process, &mut code) } != 0
        && code == store::MANAGER_ACTIVE_EXIT_CODE { return store::ManagerActive.into(); }
    error
}

#[derive(Clone, Debug)]
pub struct Readiness { pub available: bool, pub reason: String }
pub fn readiness() -> Readiness {
    Readiness { available: crate::PRIVILEGED_PATH_VALIDATED, reason: if crate::PRIVILEGED_PATH_VALIDATED {String::new()} else {"Windows VPN privileged adapter, watchdog, and crash cleanup validation is still required; no network changes are enabled".into()} }
}

pub struct VerifiedClient {
    pub stream: tokio::net::windows::named_pipe::NamedPipeClient,
    pub process: Handle,
    _image: std::fs::File,
}
impl tokio::io::AsyncRead for VerifiedClient {
    fn poll_read(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().stream).poll_read(cx, buf)
    }
}
impl tokio::io::AsyncWrite for VerifiedClient {
    fn poll_write(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>, buf: &[u8]) -> std::task::Poll<io::Result<usize>> {
        std::pin::Pin::new(&mut self.get_mut().stream).poll_write(cx, buf)
    }
    fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }
    fn poll_shutdown(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
    }
}
/// The gate must remain before elevation, so a status query never prompts for UAC.
pub async fn connect(helper: &Path) -> Result<VerifiedClient> {
    ensure!(crate::PRIVILEGED_PATH_VALIDATED, "{}", readiness().reason);
    connect_inner(helper,false).await
}
pub(crate) async fn connect_inner(helper: &Path, validation: bool) -> Result<VerifiedClient> {
    ensure!(!validation || cfg!(feature="privileged-validation"),"validation_build_required");
    ensure!(crate::PRIVILEGED_PATH_VALIDATED || (cfg!(feature="privileged-validation") && validation),"privileged_windows_path_not_validated");
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let helper = helper.canonicalize()?; let image = process::lock_image(&helper)?;
    let owner = identity::Process::open(std::process::id())?;
    ensure!(!owner.identity.elevated,"vpn_manager_must_be_unelevated");
    let owner_identity = owner.identity.clone(); let pipe_id = uuid::Uuid::new_v4();
    let server = tokio::task::spawn_blocking(move || process::elevate(&helper,pipe_id,&owner,validation)).await??;
    let name = format!(r"\\.\pipe\{}.control.{pipe_id}", crate::PIPE_NAMESPACE);
    let mut stream = tokio::time::timeout(std::time::Duration::from_secs(30),async {
        loop {
            server.verify(server.pid).map_err(|error|broker_startup_error(server.handle.raw(),error))?;
            match tokio::net::windows::named_pipe::ClientOptions::new().open(&name) {
                Ok(stream) => return Ok::<_,anyhow::Error>(stream),
                Err(e) if matches!(e.raw_os_error(),Some(2 | 231)) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
                Err(e) => return Err(e.into()),
            }
        }
    }).await??;
    pipe::verify_server(&stream,&server,&owner_identity)?;
    stream.write_all(&[1]).await?; let mut reply = [0];
    tokio::time::timeout(std::time::Duration::from_secs(5),stream.read_exact(&mut reply)).await??;
    ensure!(reply == [1],"broker_authentication_rejected");
    Ok(VerifiedClient {stream,process:server.handle,_image:image})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::{io::AsRawHandle, process::CommandExt};
    #[test]
    fn startup_reports_a_busy_cleanup_owner_without_relaxing_peer_rejection() {
        for code in [store::MANAGER_ACTIVE_EXIT_CODE,1] {
            let mut child=std::process::Command::new("cmd.exe").args(["/d","/c",&format!("exit {code}")])
                .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW).spawn().unwrap();
            child.wait().unwrap();
            let error=broker_startup_error(child.as_raw_handle(),anyhow::anyhow!("peer_process_mismatch_or_exited"));
            assert_eq!(error.is::<store::ManagerActive>(),code==store::MANAGER_ACTIVE_EXIT_CODE);
            if code==1 {assert_eq!(error.to_string(),"peer_process_mismatch_or_exited");}
        }
        let current=unsafe {windows_sys::Win32::System::Threading::GetCurrentProcess()};
        assert_eq!(broker_startup_error(current,anyhow::anyhow!("peer_pid_reused")).to_string(),"peer_pid_reused");
    }
}
