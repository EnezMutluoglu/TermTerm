#![cfg(windows)]
use std::{os::windows::process::CommandExt, process::{Child, Command, Stdio}, time::{Duration, Instant}};
use termterm_vpn_windows_broker::windows::{identity::{self, Process}, pipe};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, CREATE_NO_WINDOW};

const CLIENT_PIPE: &str = "TERMTERM_TEST_CONTROL_PIPE";

struct OwnedClient(Child);
impl Drop for OwnedClient {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

#[tokio::test]
async fn control_client_child() {
    let Some(name) = std::env::var_os(CLIENT_PIPE) else { return; };
    let mut client = tokio::net::windows::named_pipe::ClientOptions::new().open(name).unwrap();
    client.write_all(&[1]).await.unwrap();
    let mut ack = [0];
    tokio::time::timeout(Duration::from_secs(10), client.read_exact(&mut ack)).await.unwrap().unwrap();
    assert_eq!(ack, [1]);
}

#[tokio::test]
async fn real_control_client_requires_live_matching_process_and_birth() {
    let identity = identity::process_identity(unsafe { GetCurrentProcess() }).unwrap();
    assert!(!identity.elevated, "this process/pipe test must stay unelevated");
    let name = format!(r"\\.\pipe\{}.test.{}", termterm_vpn_windows_broker::PIPE_NAMESPACE, uuid::Uuid::new_v4());
    let mut server = pipe::server(&name, &identity, false).unwrap();
    let mut child = OwnedClient(Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "control_client_child", "--nocapture"])
        .env(CLIENT_PIPE, &name).creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped())
        .spawn().unwrap());
    let mut expected = Process::open(child.0.id()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), server.connect()).await.unwrap().unwrap();
    let mut preamble = [0];
    tokio::time::timeout(Duration::from_secs(5), server.read_exact(&mut preamble)).await.unwrap().unwrap();
    assert_eq!(preamble, [1]);
    pipe::verify_client(&server, &expected).unwrap();

    let wrong = Process::open(std::process::id()).unwrap();
    assert_eq!(pipe::verify_client(&server, &wrong).unwrap_err().to_string(), "peer_process_mismatch_or_exited");
    expected.birth ^= 1;
    assert_eq!(pipe::verify_client(&server, &expected).unwrap_err().to_string(), "peer_pid_reused");
    expected.birth ^= 1;
    pipe::verify_client(&server, &expected).unwrap();

    server.write_all(&[1]).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "owned pipe client failed: {status}");
            break;
        }
        assert!(Instant::now() < deadline, "owned pipe client did not exit");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(expected.verify(expected.pid).unwrap_err().to_string(), "peer_process_mismatch_or_exited");
    assert_eq!(identity::process_identity(unsafe { GetCurrentProcess() }).unwrap(), identity);
}
