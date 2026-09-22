//! Per-transfer control. Pausing finishes the current bounded I/O before waiting;
//! cancellation interrupts pending I/O. The final rename is a commit boundary.
use crate::state::Shared;
use anyhow::{anyhow, ensure, Result};
use std::{
    future::Future,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
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
}
impl Default for Control {
    fn default() -> Self {
        Self {
            mode: watch::channel(Mode::Running).0,
            pause_generation: AtomicU64::new(0),
            observed_generation: AtomicU64::new(0),
        }
    }
}
impl Control {
    pub async fn checkpoint(&self) -> Result<bool> {
        let mut rx = self.mode.subscribe();
        let mut paused = false;
        loop {
            let mode = *rx.borrow_and_update();
            match mode {
                Mode::Cancelled => anyhow::bail!("Transfer cancelled"),
                Mode::Paused => {
                    paused = true;
                    rx.changed().await?;
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
            ensure!(
                *rx.borrow_and_update() != Mode::Cancelled,
                "Transfer cancelled"
            );
            tokio::select! {
                result=&mut task => return result.map_err(|_|anyhow!("Transfer made no I/O progress for 30 seconds"))?.map_err(Into::into),
                change=rx.changed()=>{change?;}
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
