#![cfg(windows)]
use std::{io::{BufRead,BufReader,Write},path::PathBuf,process::{Command,Stdio},time::Duration};
use termterm_vpn_windows_broker::{protocol::read_sync,windows::{identity,pipe,process}};
use serde_json::{json,Value};

/// Optional native acceptance: actual Go helper, authenticated parent/job and inherited message pipe.
/// No elevation, adapter, DNS or host route changes. Missing explicit binaries are reported as skipped.
#[tokio::test]
async fn wireguard_helper_handshake_and_raw_pipe_payload_without_host_network_changes() {
    let Ok(engine)=std::env::var("TERMTERM_WG_ENGINE_TEST") else {eprintln!("SKIPPED native WireGuard helper test: set TERMTERM_WG_ENGINE_TEST and TERMTERM_WG_PEER_TEST or TERMTERM_WG_KERNEL_FIXTURE");return;};
    let engine=PathBuf::from(engine).canonicalize().unwrap();
    let owner=identity::Process::open(std::process::id()).unwrap();assert!(!owner.identity.elevated);
    struct Stop(std::process::Child);impl Drop for Stop {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
    let kernel=std::env::var_os("TERMTERM_WG_KERNEL_FIXTURE");
    let (config,_fixture)=if let Some(path)=&kernel {
        let value:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(value["kernelPeer"]["implementation"],"Linux kernel WireGuard");
        (value["profile"]["config"].as_str().unwrap().to_string(),None)
    } else {
        let peer=PathBuf::from(std::env::var("TERMTERM_WG_PEER_TEST").expect("Explicit peer fixture required")).canonicalize().unwrap();
        let mut fixture=Command::new(peer).arg("-test.run=^TestWindowsPeerFixture$").env("TERMTERM_WG_PEER_FIXTURE","1").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let mut output=BufReader::new(fixture.stdout.take().unwrap());let mut frame=String::new();output.read_line(&mut frame).unwrap();
        (serde_json::from_str::<Value>(&frame).unwrap()["config"].as_str().unwrap().to_string(),Some(Stop(fixture)))
    };
    let initial=termterm_vpn_core::wireguard::parse(&config).unwrap().engine_config().unwrap();
    let name=format!(r"\\.\pipe\{}.packet.{}",termterm_vpn_windows_broker::PIPE_NAMESPACE,uuid::Uuid::new_v4());let packet=pipe::server(&name,&owner.identity,true).unwrap();let endpoint=pipe::packet_endpoint(&name).unwrap();packet.connect().await.unwrap();
    let child=process::engine(&owner,&engine,endpoint.raw()).unwrap();drop(endpoint);
    let mut input=child.input.try_clone().unwrap();let output=child.output.try_clone().unwrap();let (sender,mut events)=tokio::sync::mpsc::channel(16);
    std::thread::spawn(move||{let mut reader=BufReader::new(output);loop{let event=read_sync(&mut reader);let closed=event.is_err();if sender.blocking_send(event).is_err()||closed{break;}}});
    let session=uuid::Uuid::new_v4();let write=|input:&mut std::fs::File,mut value:Value|{value["sessionId"]=session.to_string().into();value["generation"]=1.into();serde_json::to_writer(&mut *input,&value).unwrap();input.write_all(b"\n").unwrap();input.flush().unwrap();};
    async fn next(events:&mut tokio::sync::mpsc::Receiver<anyhow::Result<Value>>)->Value{tokio::time::timeout(Duration::from_secs(8),events.recv()).await.unwrap().unwrap().unwrap()}
    write(&mut input,json!({"wireguard":initial}));assert_eq!(next(&mut events).await["kind"],"network-plan");write(&mut input,json!({"action":"accept-network"}));assert_eq!(next(&mut events).await["kind"],"tunnel-ready");write(&mut input,json!({"action":"network-configured","packetHandle":child.packet_handle}));assert_eq!(next(&mut events).await["kind"],"progress");assert_eq!(next(&mut events).await["kind"],"connected");
    let offset=if kernel.is_some(){28}else{20};let mut sent=vec![0;offset+512];let length=(sent.len() as u16).to_be_bytes();sent[0]=0x45;sent[2..4].copy_from_slice(&length);sent[8]=64;sent[9]=if kernel.is_some(){1}else{253};sent[12..16].copy_from_slice(&[10,97,0,2]);sent[16..20].copy_from_slice(&[10,97,0,1]);for (index,byte) in sent[offset..].iter_mut().enumerate(){*byte=index as u8;}
    if kernel.is_some() {
        fn checksum(bytes:&[u8])->[u8;2]{let mut sum=bytes.chunks(2).map(|pair|u16::from_be_bytes([pair[0],*pair.get(1).unwrap_or(&0)]) as u32).sum::<u32>();while sum>>16!=0 {sum=(sum&65535)+(sum>>16);}(!(sum as u16)).to_be_bytes()}
        sent[20]=8;sent[25]=1;sent[27]=1;let icmp=checksum(&sent[20..]);sent[22..24].copy_from_slice(&icmp);let ip=checksum(&sent[..20]);sent[10..12].copy_from_slice(&ip);
    }
    pipe::write_packet(&packet,&sent,1420).await.unwrap();let response=tokio::time::timeout(Duration::from_secs(5),pipe::read_packet(&packet,1420)).await.unwrap().unwrap();assert_eq!(&response[offset..],&sent[offset..]);assert_eq!(&response[12..16],&sent[16..20]);if kernel.is_some(){assert_eq!(response[20],0);}
    use sha2::{Digest,Sha256};assert_eq!(Sha256::digest(&response[offset..]),Sha256::digest(&sent[offset..]));
    write(&mut input,json!({"action":"disconnect"}));assert_eq!(next(&mut events).await["kind"],"disconnected");child.terminate().unwrap();
}
