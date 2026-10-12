use super::{terminal::connect_target, DesktopState};
use crate::{connections::SftpConnection, state::Shared};
use anyhow::{ensure, Result};
use std::{sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;
#[tauri::command]
pub async fn team_v2_sftp(
    app: AppHandle,
    state: State<'_, Shared>,
    id: Uuid,
    host_id: Uuid,
) -> Result<String, String> {
    let epoch = app.state::<DesktopState>().current_epoch();
    let desktop = app.state::<DesktopState>();
    let result = tokio::select! {r=connect(&app,&state,id.to_string(),host_id,epoch)=>r.map_err(|e|format!("{e:#}")),_=desktop.cancelled(epoch)=>Err("Team session closed".into())};
    if result.is_err() {
        let _ = app.emit(
            "session-event",
            serde_json::json!({"id":id,"kind":"closed","detail":null}),
        );
    }
    result
}
async fn connect(
    app: &AppHandle,
    state: &Shared,
    id: String,
    host: Uuid,
    epoch: u64,
) -> Result<String> {
    let native = app.state::<super::NativeConnections>();
    let _reservation = native.reserve(&id)?;
    {
        let connections = state.sftp.lock().await;
        ensure!(
            connections.len() < 16 && !connections.contains_key(&id),
            "SFTP limit or duplicate connection ID"
        );
    }
    let (ssh, _) = connect_target(app, state, &id, host, epoch, None).await?;
    let channel = ssh.handle.channel_open_session().await?;
    channel.request_subsystem(true, "sftp").await?;
    let sftp = russh_sftp::client::SftpSession::new(channel.into_stream()).await?;
    sftp.set_timeout(30);
    let home = sftp.canonicalize(".").await?;
    app.state::<DesktopState>()
        .check_connection(host, epoch)
        .await?;
    let connection = Arc::new(SftpConnection { sftp, _ssh: ssh });
    {
        let mut connections = state.sftp.lock().await;
        ensure!(
            connections.len() < 16 && !connections.contains_key(&id),
            "SFTP limit or duplicate connection ID"
        );
        connections.insert(id.clone(), connection.clone());
    }
    let app = app.clone();
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        let desktop = app.state::<DesktopState>();
        let monitor = async {
            let mut ticks = tokio::time::interval(Duration::from_millis(250));
            let mut next_check = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                ticks.tick().await;
                if !state
                    .sftp
                    .lock()
                    .await
                    .get(&id)
                    .is_some_and(|c| Arc::ptr_eq(c, &connection))
                {
                    break;
                }
                if tokio::time::Instant::now() >= next_check {
                    if let Err(e) = desktop.check_connection(host, epoch).await {
                        if e.to_string() != "network_unavailable" {
                            break;
                        }
                    }
                    next_check = tokio::time::Instant::now() + Duration::from_secs(10);
                }
            }
        };
        tokio::select! {_=monitor=>{},_=desktop.cancelled(epoch)=>{}}
        let removed = {
            let mut connections = state.sftp.lock().await;
            if connections
                .get(&id)
                .is_some_and(|c| Arc::ptr_eq(c, &connection))
            {
                connections.remove(&id);
                true
            } else {
                false
            }
        };
        if removed {
            // Invalidate the visible directory before waiting for transport shutdown.
            let _ = app.emit("sftp-closed", serde_json::json!({"id": id}));
        }
        // Close the transport too: transfer tasks may still hold an Arc.
        let _ = connection
            ._ssh
            .handle
            .disconnect(russh::Disconnect::ByApplication, "Team SFTP closed", "en")
            .await;
    });
    Ok(home)
}
