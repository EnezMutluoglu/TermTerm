use super::{identity::{self, Identity, Process}, wide, win, Handle};
use anyhow::{ensure, Result};
use std::{os::windows::io::AsRawHandle, ptr};
use tokio::net::windows::named_pipe::{NamedPipeClient, NamedPipeServer, PipeMode, ServerOptions};
use windows_sys::Win32::{Foundation::*, Security::{Authorization::*, *}, Storage::FileSystem::*, System::{Pipes::*, Threading::*}};

pub struct Descriptor(*mut core::ffi::c_void);
impl Descriptor {
    pub fn from_sddl(sddl: &str) -> Result<Self> {
        let mut value = ptr::null_mut();
        unsafe { win(ConvertStringSecurityDescriptorToSecurityDescriptorW(wide(sddl)?.as_ptr(), SDDL_REVISION_1, &mut value, ptr::null_mut()))?; }
        Ok(Self(value))
    }
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES { SECURITY_ATTRIBUTES { nLength: size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: self.0, bInheritHandle: 0 } }
}
impl Drop for Descriptor { fn drop(&mut self) { unsafe { LocalFree(self.0); } } }
pub fn sid_string(sid: &[u8]) -> Result<String> {
    let mut text = ptr::null_mut();
    unsafe {
        win(ConvertSidToStringSidW(sid.as_ptr().cast_mut().cast(), &mut text))?;
        let length = (0..256).find(|i| *text.add(*i) == 0).ok_or_else(|| anyhow::anyhow!("sid_string_too_long"))?;
        let result = String::from_utf16(std::slice::from_raw_parts(text, length)); LocalFree(text.cast());
        Ok(result?)
    }
}
pub fn server(name: &str, owner: &Identity, packet: bool) -> Result<NamedPipeServer> {
    ensure!(name.starts_with(&format!(r"\\.\pipe\{}.", crate::PIPE_NAMESPACE)) && name.len() <= 180, "invalid_pipe_name");
    let descriptor = Descriptor::from_sddl(&format!("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;{})", sid_string(&owner.logon_sid)?))?;
    let attributes = descriptor.attributes();
    let mut options = ServerOptions::new();
    options.first_pipe_instance(true).reject_remote_clients(true).max_instances(1)
        .pipe_mode(if packet { PipeMode::Message } else { PipeMode::Byte })
        .in_buffer_size(if packet { 65536 } else { 65536 }).out_buffer_size(65536);
    Ok(unsafe { options.create_with_security_attributes_raw(name, (&attributes as *const SECURITY_ATTRIBUTES).cast_mut().cast())? })
}
/// Keep the inherited endpoint unregistered: IOCP association survives DuplicateHandle.
pub fn packet_endpoint(name: &str) -> Result<Handle> {
    ensure!(name.starts_with(&format!(r"\\.\pipe\{}.packet.", crate::PIPE_NAMESPACE)) && name.len() <= 180,"invalid_packet_pipe_name");
    let endpoint = Handle::new(unsafe {CreateFileW(wide(name)?.as_ptr(),GENERIC_READ | GENERIC_WRITE,0,ptr::null(),OPEN_EXISTING,
        FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,ptr::null_mut())})?;
    let mode = PIPE_READMODE_MESSAGE;
    unsafe {win(SetNamedPipeHandleState(endpoint.raw(),&mode,ptr::null(),ptr::null()))?;}
    Ok(endpoint)
}
struct Revert;
impl Drop for Revert {
    fn drop(&mut self) {
        // Continuing a privileged broker while still impersonating is unsafe.
        if unsafe { RevertToSelf() } == 0 { std::process::abort(); }
    }
}
pub fn verify_client(pipe: &NamedPipeServer, expected: &Process) -> Result<()> {
    let handle = pipe.as_raw_handle(); let mut pid = 0;
    unsafe { win(GetNamedPipeClientProcessId(handle, &mut pid))?; }
    expected.verify(pid)?;
    unsafe { win(ImpersonateNamedPipeClient(handle))?; }
    let revert = Revert;
    let mut token = ptr::null_mut();
    unsafe { win(OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token))?; }
    let token = Handle::new(token)?;
    let actual = identity::from_token(token.raw())?;
    identity::same_logon(&expected.identity, &actual)?;
    drop(revert);
    Ok(())
}
pub fn verify_server(pipe: &NamedPipeClient, expected: &Process, owner: &Identity) -> Result<()> {
    let mut pid = 0;
    unsafe { win(GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid))?; }
    expected.verify(pid)?;
    identity::verify_elevated_peer(owner,expected.handle.raw())?;
    ensure!(expected.identity.elevated, "broker_must_be_elevated");
    Ok(())
}

fn packet_pipe_error(error:std::io::Error)->anyhow::Error {
    if error.kind()==std::io::ErrorKind::BrokenPipe {std::io::Error::new(std::io::ErrorKind::UnexpectedEof,"packet_channel_closed").into()} else {error.into()}
}
pub async fn read_packet(pipe: &NamedPipeServer, mtu: usize) -> Result<Vec<u8>> {
    let mut packet = vec![0; crate::packet::MAX_PACKET];
    loop {
        pipe.readable().await.map_err(packet_pipe_error)?;
        match pipe.try_read(&mut packet) {
            Ok(0) => return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof,"packet_channel_closed").into()),
            Ok(size) => { packet.truncate(size); crate::packet::validate(&packet, mtu)?; return Ok(packet); }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            // ERROR_MORE_DATA is fatal: never turn an oversized message into multiple packets.
            Err(e) => return Err(packet_pipe_error(e)),
        }
    }
}
pub async fn write_packet(pipe: &NamedPipeServer, packet: &[u8], mtu: usize) -> Result<()> {
    crate::packet::validate(packet, mtu)?;
    loop {
        pipe.writable().await.map_err(packet_pipe_error)?;
        match pipe.try_write(packet) {
            Ok(size) => { ensure!(size == packet.len(), "partial_packet_write"); return Ok(()); }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => return Err(packet_pipe_error(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[tokio::test]
    async fn packet_peer_close_is_eof_but_nonempty_corruption_is_fatal_without_uac() {
        let owner=identity::process_identity(unsafe {GetCurrentProcess()}).unwrap();
        let name=format!(r"\\.\pipe\{}.packet.{}",crate::PIPE_NAMESPACE,uuid::Uuid::new_v4());
        let packet=server(&name,&owner,true).unwrap();
        let mut client=tokio::net::windows::named_pipe::ClientOptions::new().pipe_mode(PipeMode::Message).open(&name).unwrap();
        packet.connect().await.unwrap();client.write_all(&[0x45]).await.unwrap();
        let error=read_packet(&packet,1420).await.unwrap_err();
        assert_eq!(error.to_string(),"ipv4_header_truncated");assert!(error.downcast_ref::<std::io::Error>().is_none());
        drop(client);
        let error=tokio::time::timeout(std::time::Duration::from_secs(2),read_packet(&packet,1420)).await.unwrap().unwrap_err();
        assert_eq!(error.downcast_ref::<std::io::Error>().unwrap().kind(),std::io::ErrorKind::UnexpectedEof);
        let mut valid=[0;20];valid[0]=0x45;valid[3]=20;
        let error=tokio::time::timeout(std::time::Duration::from_secs(2),write_packet(&packet,&valid,1420)).await.unwrap().unwrap_err();
        assert_eq!(error.downcast_ref::<std::io::Error>().unwrap().kind(),std::io::ErrorKind::UnexpectedEof);
    }
    #[tokio::test]
    async fn owner_pipe_authenticates_actual_process_and_rejects_wrong_pid_without_uac() {
        let owner = identity::process_identity(unsafe { GetCurrentProcess() }).unwrap();
        let name = format!(r"\\.\pipe\{}.test.{}", crate::PIPE_NAMESPACE, uuid::Uuid::new_v4());
        let pipe_server = server(&name, &owner, false).unwrap();
        let mut client = tokio::net::windows::named_pipe::ClientOptions::new().open(&name).unwrap();
        pipe_server.connect().await.unwrap(); client.write_all(b"x").await.unwrap();
        let mut server_pipe = pipe_server; let mut byte = [0]; server_pipe.read_exact(&mut byte).await.unwrap();
        let process = Process::open(std::process::id()).unwrap(); verify_client(&server_pipe, &process).unwrap();
        let mut wrong = Process::open(std::process::id()).unwrap(); wrong.pid = 0;
        assert!(verify_client(&server_pipe, &wrong).is_err());
        // RevertToSelf succeeded; the current process identity remains available.
        assert_eq!(identity::process_identity(unsafe { GetCurrentProcess() }).unwrap(), owner);
        assert!(server(&name, &owner, false).is_err());
    }
    #[tokio::test]
    async fn compiled_namespace_accepts_real_control_and_packet_endpoints_and_rejects_the_other_mode() {
        let owner=identity::process_identity(unsafe {GetCurrentProcess()}).unwrap();
        let other=if cfg!(feature="production-candidate") {"TermTerm.VpnDev"} else {"TermTerm.Vpn"};
        assert!(server(&format!(r"\\.\pipe\{other}.control.{}",uuid::Uuid::new_v4()),&owner,false).is_err());
        assert!(packet_endpoint(&format!(r"\\.\pipe\{other}.packet.{}",uuid::Uuid::new_v4())).is_err());
        let control=format!(r"\\.\pipe\{}.control.{}",crate::PIPE_NAMESPACE,uuid::Uuid::new_v4());
        let control_server=server(&control,&owner,false).unwrap();
        let _control=tokio::net::windows::named_pipe::ClientOptions::new().open(&control).unwrap();
        control_server.connect().await.unwrap();
        let packet=format!(r"\\.\pipe\{}.packet.{}",crate::PIPE_NAMESPACE,uuid::Uuid::new_v4());
        let packet_server=server(&packet,&owner,true).unwrap();
        let _packet=packet_endpoint(&packet).unwrap();
        packet_server.connect().await.unwrap();
    }
}
