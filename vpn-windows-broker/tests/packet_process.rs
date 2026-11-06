#![cfg(windows)]
use termterm_vpn_windows_broker::windows::{identity,pipe,process};
use std::{path::Path,time::Duration};
use windows_sys::Win32::{Foundation::WAIT_OBJECT_0,System::Threading::{GetCurrentProcess,GetExitCodeProcess,WaitForSingleObject}};

async fn probe(scenario: &str) {
    let owner = identity::process_identity(unsafe {GetCurrentProcess()}).unwrap();
    assert!(!owner.elevated,"these tests must stay unelevated");
    let name = format!(r"\\.\pipe\{}.packet.{}",termterm_vpn_windows_broker::PIPE_NAMESPACE,uuid::Uuid::new_v4());
    let server = pipe::server(&name,&owner,true).unwrap(); let endpoint = pipe::packet_endpoint(&name).unwrap(); server.connect().await.unwrap();
    let helper = Path::new(env!("CARGO_BIN_EXE_termterm-vpn-broker"));
    let child = process::packet_probe(helper,&endpoint,scenario).unwrap(); drop(endpoint);
    match scenario {
        "echo-cancel" => {
            let mut packet = vec![0;20];packet[0]=0x45;packet[3]=20;
            tokio::time::timeout(Duration::from_secs(5),pipe::write_packet(&server,&packet,1500)).await.unwrap().unwrap();
            let echo = tokio::time::timeout(Duration::from_secs(5),pipe::read_packet(&server,1500)).await.unwrap().unwrap();assert_eq!(echo,packet);
        }
        "oversize" => {loop {server.writable().await.unwrap();match server.try_write(&vec![0;65536]) {Ok(65536)=>break,Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>continue,result=>panic!("oversize test write: {result:?}")}}}
        "eof" => drop(server),
        _=>unreachable!(),
    }
    let raw = child.process.raw() as usize;
    let (wait,success,code) = tokio::task::spawn_blocking(move || {let mut code = 0;let wait = unsafe {WaitForSingleObject(raw as _,10000)};let success = unsafe {GetExitCodeProcess(raw as _,&mut code)};(wait,success,code)}).await.unwrap();
    assert_eq!(wait,WAIT_OBJECT_0);assert_ne!(success,0);assert_eq!(code,0);
}
#[tokio::test] async fn inherited_raw_overlapped_pipe_echo_and_cancel() {probe("echo-cancel").await;}
#[tokio::test] async fn inherited_raw_pipe_rejects_oversized_message() {probe("oversize").await;}
#[tokio::test] async fn inherited_raw_pipe_observes_owner_endpoint_eof() {probe("eof").await;}
