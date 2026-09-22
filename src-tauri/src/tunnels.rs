use crate::{connections::connect_ssh, state::Shared};
use anyhow::{anyhow, ensure, Result};
use serde::Deserialize;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tunnel {
    pub host_id: String,
    pub mode: String,
    pub bind_address: String,
    pub bind_port: u16,
    pub target_address: String,
    pub target_port: u16,
}
pub async fn start(app: AppHandle, state: Shared, id: String, tunnel: Tunnel) -> Result<()> {
    ensure!(
        ["127.0.0.1", "::1", "localhost"].contains(&tunnel.bind_address.as_str()),
        "This version binds tunnels to loopback only"
    );
    let remote = if tunnel.mode == "remote" {
        Some((tunnel.target_address.clone(), tunnel.target_port))
    } else {
        None
    };
    let ssh = Arc::new(connect_ssh(&app, &state, &id, &tunnel.host_id, remote).await?);
    let task_id = id.clone();
    let task = if tunnel.mode == "remote" {
        let port = ssh
            .handle
            .tcpip_forward(&tunnel.bind_address, tunnel.bind_port as u32)
            .await?;
        app.emit(
            "tunnel-event",
            serde_json::json!({"id":id,"status":"running","port":port}),
        )?;
        tokio::spawn(async move {
            let _keep = ssh;
            std::future::pending::<()>().await;
        })
    } else {
        ensure!(
            ["local", "dynamic"].contains(&tunnel.mode.as_str()),
            "Invalid tunnel mode"
        );
        let listener = TcpListener::bind((tunnel.bind_address.as_str(), tunnel.bind_port)).await?;
        let bound = listener.local_addr()?.port();
        app.emit(
            "tunnel-event",
            serde_json::json!({"id":id,"status":"running","port":bound}),
        )?;
        tokio::spawn(async move {
            let mut clients = tokio::task::JoinSet::new();
            loop {
                tokio::select! {accepted=listener.accept()=>{let Ok((mut stream,_))=accepted else{break;};let ssh=ssh.clone();let t=tunnel.clone();let app=app.clone();let id=id.clone();clients.spawn(async move{let result:Result<()>=async{
                let(address,port)=if t.mode=="dynamic"{let mut header=[0;2];stream.read_exact(&mut header).await?;ensure!(header[0]==5,"SOCKS5 required");let mut methods=vec![0;header[1]as usize];stream.read_exact(&mut methods).await?;ensure!(methods.contains(&0),"No supported SOCKS method");stream.write_all(&[5,0]).await?;let mut request=[0;4];stream.read_exact(&mut request).await?;ensure!(request[0]==5&&request[1]==1,"Only SOCKS5 CONNECT is supported");let address=match request[3]{1=>{let mut a=[0;4];stream.read_exact(&mut a).await?;std::net::Ipv4Addr::from(a).to_string()},4=>{let mut a=[0;16];stream.read_exact(&mut a).await?;std::net::Ipv6Addr::from(a).to_string()},3=>{let n=stream.read_u8().await?;let mut a=vec![0;n as usize];stream.read_exact(&mut a).await?;String::from_utf8(a)?},_=>return Err(anyhow!("Invalid SOCKS address"))};(address,stream.read_u16().await?)}else{(t.target_address,t.target_port)};
                let channel=ssh.handle.channel_open_direct_tcpip(&address,port as u32,"127.0.0.1",0).await?;if t.mode=="dynamic"{stream.write_all(&[5,0,0,1,127,0,0,1,0,0]).await?;}let mut remote=channel.into_stream();tokio::io::copy_bidirectional(&mut stream,&mut remote).await?;Ok(())}.await;if let Err(e)=result{let _=app.emit("tunnel-event",serde_json::json!({"id":id,"status":"connectionError","error":e.to_string()}));}});},_=clients.join_next(),if !clients.is_empty()=>{}}
            }
        })
    };
    if let Some(old) = state
        .tunnels
        .lock()
        .map_err(|_| anyhow!("Tunnel lock"))?
        .insert(task_id, task)
    {
        old.abort();
    }
    Ok(())
}
