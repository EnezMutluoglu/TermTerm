//! Per-transfer control. Pausing finishes the current bounded I/O before waiting;
//! cancellation interrupts pending I/O. The final rename is a commit boundary.
use crate::state::Shared;
use anyhow::{anyhow, ensure, Result};
use std::{
    future::Future,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Running,
    Paused,
    Cancelled,
    Committing,
    Finished,
}
pub struct Control {
    mode: watch::Sender<Mode>,
    pause_generation: AtomicU64,
    observed_generation: AtomicU64,
    vpns: Mutex<Vec<crate::vpn_lifecycle::Lease>>,
}
impl Default for Control {
    fn default() -> Self {
        Self {
            mode: watch::channel(Mode::Running).0,
            pause_generation: AtomicU64::new(0),
            observed_generation: AtomicU64::new(0),
            vpns: Mutex::new(Vec::new()),
        }
    }
}
impl Control {
    pub(crate) fn bind_vpns(&self, vpns: Vec<crate::vpn_lifecycle::Lease>) -> Result<Vec<crate::vpn_lifecycle::Registration>> {
        let registrations = vpns.iter().map(|lease|lease.register()).collect::<Result<Vec<_>>>()?;
        *self.vpns.lock().map_err(|_|anyhow!("Transfer VPN lock"))? = vpns;
        Ok(registrations)
    }
    fn check_vpns(&self) -> Result<()> {
        ensure!(self.vpns.lock().map_err(|_|anyhow!("Transfer VPN lock"))?.iter().all(|vpn|!vpn.is_stopping()), "Transfer cancelled: required VPN disconnected");
        Ok(())
    }
    async fn vpn_cancelled(&self) {
        let vpns = self.vpns.lock().map(|vpns|vpns.clone()).unwrap_or_default();
        if vpns.is_empty() { std::future::pending::<()>().await; }
        futures_util::future::select_all(vpns.into_iter().map(|vpn|Box::pin(async move {vpn.cancelled().await}))).await;
    }
    pub async fn checkpoint(&self) -> Result<bool> {
        let mut rx = self.mode.subscribe();
        let mut paused = false;
        loop {
            self.check_vpns()?;
            let mode = *rx.borrow_and_update();
            match mode {
                Mode::Cancelled => anyhow::bail!("Transfer cancelled"),
                Mode::Paused => {
                    paused = true;
                    tokio::select! { result=rx.changed()=>result?, _=self.vpn_cancelled()=>return Err(anyhow!("Transfer cancelled: required VPN disconnected")) }
                }
                _ => {
                    let generation = self.pause_generation.load(Ordering::SeqCst);
                    return Ok(paused
                        | (self.observed_generation.swap(generation, Ordering::SeqCst)
                            != generation));
                }
            }
        }
    }
    pub async fn io<T, E>(
        &self,
        future: impl Future<Output = std::result::Result<T, E>>,
    ) -> Result<T>
    where
        E: Into<anyhow::Error>,
    {
        let mut rx = self.mode.subscribe();
        let task = tokio::time::timeout(Duration::from_secs(30), future);
        tokio::pin!(task);
        loop {
            self.check_vpns()?;
            ensure!(
                *rx.borrow_and_update() != Mode::Cancelled,
                "Transfer cancelled"
            );
            tokio::select! {
                result=&mut task => return result.map_err(|_|anyhow!("Transfer made no I/O progress for 30 seconds"))?.map_err(Into::into),
                change=rx.changed()=>{change?;}
                _=self.vpn_cancelled()=>return Err(anyhow!("Transfer cancelled: required VPN disconnected")),
            }
        }
    }
    pub async fn begin_commit(&self) -> Result<()> {
        loop {
            self.checkpoint().await?;
            let accepted = self.mode.send_if_modified(|mode| {
                if *mode == Mode::Running {
                    *mode = Mode::Committing;
                    true
                } else {
                    false
                }
            });
            if accepted {
                return Ok(());
            }
            ensure!(
                *self.mode.borrow() == Mode::Paused,
                "Transfer interrupted before commit"
            );
        }
    }
    pub fn end_commit(&self) {
        self.mode.send_replace(Mode::Running);
    }
    fn action(&self, action: &str) -> Result<bool> {
        ensure!(
            ["pause", "resume", "cancel"].contains(&action),
            "Invalid transfer control"
        );
        let mut accepted = false;
        self.mode.send_if_modified(|mode| {
            if matches!(mode, Mode::Committing | Mode::Finished) {
                return false;
            }
            let next = match action {
                "cancel" => Mode::Cancelled,
                "pause" if *mode != Mode::Cancelled => Mode::Paused,
                "resume" if *mode != Mode::Cancelled => Mode::Running,
                _ => return false,
            };
            accepted = true;
            let changed = *mode != next;
            if changed && next == Mode::Paused {
                self.pause_generation.fetch_add(1, Ordering::SeqCst);
            }
            *mode = next;
            changed
        });
        Ok(accepted)
    }
}
pub fn control(app: &AppHandle, state: &Shared, id: &str, action: &str) -> Result<bool> {
    let active = state
        .transfers
        .lock()
        .map_err(|_| anyhow!("Transfer lock"))?;
    let Some(control) = active.get(id) else {
        return Ok(false);
    };
    let accepted = control.action(action)?;
    if accepted {
        emit(
            app,
            id,
            match action {
                "pause" => "paused",
                "resume" => "running",
                _ => "cancelling",
            },
            None,
        );
    }
    Ok(accepted)
}
pub fn cancel_all(state: &Shared) {
    if let Ok(active) = state.transfers.lock() {
        for c in active.values() {
            let _ = c.action("cancel");
        }
    }
}
pub fn emit(app: &AppHandle, id: &str, status: &str, error: Option<String>) {
    let _ = app.emit(
        "transfer-state",
        serde_json::json!({"id":id,"state":status,"error":error}),
    );
}
pub fn start(app: &AppHandle, state: &Shared, id: &str) -> Result<Arc<Control>> {
    ensure!(uuid::Uuid::parse_str(id).is_ok(), "Invalid transfer ID");
    let mut active = state
        .transfers
        .lock()
        .map_err(|_| anyhow!("Transfer lock"))?;
    ensure!(!active.contains_key(id), "Transfer is already running");
    let control = Arc::new(Control::default());
    active.insert(id.into(), control.clone());
    emit(app, id, "running", None);
    Ok(control)
}
pub fn finish(app: &AppHandle, state: &Shared, id: &str, result: &Result<u64>) {
    if let Ok(mut active) = state.transfers.lock() {
        if let Some(c) = active.remove(id) {
            c.mode.send_replace(Mode::Finished);
        }
    }
    let error = result.as_ref().err().map(|e| format!("{e:#}"));
    let status = if result.is_ok() {
        "done"
    } else if error
        .as_ref()
        .is_some_and(|e| e.contains("Transfer cancelled"))
    {
        "cancelled"
    } else {
        "error"
    };
    emit(app, id, status, error);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn vpn_loss_cancels_paused_transfer_and_waits_for_cleanup() {
        let state = Shared::default();
        let lease = crate::vpn_lifecycle::tests::test_lease(&state, "transfer", 1);
        let control = Arc::new(Control::default());
        let registrations = control.bind_vpns(vec![lease]).unwrap();
        control.action("pause").unwrap();
        let worker = control.clone();
        let task = tokio::spawn(async move {
            let _registrations = registrations;
            assert!(worker.checkpoint().await.is_err());
        });
        tokio::task::yield_now().await;
        tokio::time::timeout(Duration::from_secs(1),crate::vpn_lifecycle::quiesce(&state,"transfer",1)).await.unwrap().unwrap();
        task.await.unwrap();
        assert!(Control::default().checkpoint().await.is_ok());
    }
    #[tokio::test]
    async fn replacement_connection_keeps_its_own_generation() {
        let state = Shared::default();
        let old = Arc::new(crate::vpn_lifecycle::tests::test_lease(&state,"replacement",1));
        let new = Arc::new(crate::vpn_lifecycle::tests::test_lease(&state,"replacement",2));
        let mut endpoints = std::collections::HashMap::from([("same-id",old)]);
        let snapshot = endpoints["same-id"].clone();
        let old_control = Control::default();
        let old_registrations = old_control.bind_vpns(vec![(*snapshot).clone()]).unwrap();
        endpoints.insert("same-id",new.clone());
        let new_control = Control::default();
        let _new_registrations = new_control.bind_vpns(vec![(*endpoints["same-id"]).clone()]).unwrap();
        let closing = state.clone();
        let stop = tokio::spawn(async move {crate::vpn_lifecycle::quiesce(&closing,"replacement",1).await});
        tokio::task::yield_now().await;
        assert!(old_control.checkpoint().await.is_err());
        assert!(new_control.checkpoint().await.is_ok());
        assert!(!Arc::ptr_eq(&snapshot,&endpoints["same-id"]));
        drop(old_registrations);
        stop.await.unwrap().unwrap();
        assert!(!new.is_stopping());
    }
    #[tokio::test]
    async fn pause_resume_and_cancel_pending_io() {
        let c = Arc::new(Control::default());
        c.action("pause").unwrap();
        let worker = c.clone();
        let pending = tokio::spawn(async move { worker.checkpoint().await });
        tokio::task::yield_now().await;
        assert!(!pending.is_finished());
        c.action("resume").unwrap();
        assert!(pending.await.unwrap().unwrap());
        c.action("pause").unwrap();
        c.action("resume").unwrap();
        assert!(c.checkpoint().await.unwrap()); // A fast toggle still rechecks source identity.
        assert!(!c.checkpoint().await.unwrap());
        let worker = c.clone();
        let pending =
            tokio::spawn(async move { worker.io(std::future::pending::<Result<()>>()).await });
        tokio::task::yield_now().await;
        c.action("cancel").unwrap();
        assert!(tokio::time::timeout(Duration::from_secs(1), pending)
            .await
            .unwrap()
            .unwrap()
            .is_err());
    }
    #[tokio::test]
    async fn commit_cannot_be_cancelled_halfway() {
        let c = Control::default();
        c.begin_commit().await.unwrap();
        assert!(!c.action("cancel").unwrap());
        c.end_commit();
        assert!(c.action("cancel").unwrap());
        assert!(c.begin_commit().await.is_err());
    }
}
