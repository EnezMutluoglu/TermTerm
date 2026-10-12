use super::{terminal::connect_target, DesktopState, NativeConnections};
use crate::{
    state::Shared,
    tunnels::{self, Tunnel},
};
use anyhow::{ensure, Result};
use std::{sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

#[tauri::command]
pub async fn team_v2_tunnel(
    app: AppHandle,
    state: State<'_, Shared>,
    id: Uuid,
    tunnel: Tunnel,
) -> Result<(), String> {
    let epoch = app.state::<DesktopState>().current_epoch();
    let desktop = app.state::<DesktopState>();
    let result = tokio::select! {r=start(&app,&state,id.to_string(),tunnel,epoch)=>r.map_err(|e|format!("{e:#}")),_=desktop.cancelled(epoch)=>Err("Team session closed".into())};
    if result.is_err() {
        let _ = app.emit(
            "session-event",
            serde_json::json!({"id":id,"kind":"closed","detail":null}),
        );
    }
    result
}
async fn start(
    app: &AppHandle,
    state: &Shared,
    id: String,
    tunnel: Tunnel,
    epoch: u64,
) -> Result<()> {
    let native = app.state::<NativeConnections>();
    let _reservation = native.reserve(&id)?;
    ensure!(
        ["127.0.0.1", "::1", "localhost"].contains(&tunnel.bind_address.as_str()),
        "Tunnels bind to loopback only"
    );
    ensure!(
        ["local", "dynamic", "remote"].contains(&tunnel.mode.as_str()),
        "Invalid tunnel mode"
    );
    let host = Uuid::parse_str(&tunnel.host_id)?;
    {
        let tunnels = state
            .tunnels
            .lock()
            .map_err(|_| anyhow::anyhow!("Tunnel lock"))?;
        ensure!(
            tunnels.len() < 16 && !tunnels.contains_key(&id),
            "Tunnel limit or duplicate ID"
        );
    }
    let remote = if tunnel.mode == "remote" {
        Some((tunnel.target_address.clone(), tunnel.target_port))
    } else {
        None
    };
    let (ssh, _) = connect_target(app, state, &id, host, epoch, remote).await?;
    let connection = Arc::new(ssh);
    tunnels::start_connected(
        app.clone(),
        state.clone(),
        id.clone(),
        tunnel,
        connection.clone(),
    )
    .await?;
    let task_id = state
        .tunnels
        .lock()
        .map_err(|_| anyhow::anyhow!("Tunnel lock"))?
        .get(&id)
        .map(|t| t.id())
        .ok_or_else(|| anyhow::anyhow!("Tunnel stopped"))?;
    let state = state.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let desktop = app.state::<DesktopState>();
        let monitor = async {
            let mut ticks = tokio::time::interval(Duration::from_millis(250));
            let mut next = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                ticks.tick().await;
                if !state
                    .tunnels
                    .lock()
                    .map(|t| {
                        t.get(&id)
                            .is_some_and(|t| t.id() == task_id && !t.is_finished())
                    })
                    .unwrap_or(false)
                {
                    break;
                }
                if tokio::time::Instant::now() >= next {
                    if let Err(e) = desktop.check_connection(host, epoch).await {
                        if e.to_string() != "network_unavailable" {
                            break;
                        }
                    }
                    next = tokio::time::Instant::now() + Duration::from_secs(10);
                }
            }
        };
        tokio::select! {_=monitor=>{},_=desktop.cancelled(epoch)=>{}}
        if let Ok(mut tasks) = state.tunnels.lock() {
            if tasks.get(&id).is_some_and(|t| t.id() == task_id) {
                if let Some(task) = tasks.remove(&id) {
                    task.abort();
                }
            }
        }
        let _ = connection
            .handle
            .disconnect(russh::Disconnect::ByApplication, "Team tunnel closed", "en")
            .await;
        let _ = app.emit(
            "tunnel-event",
            serde_json::json!({"id":id,"status":"stopped"}),
        );
    });
    Ok(())
}
