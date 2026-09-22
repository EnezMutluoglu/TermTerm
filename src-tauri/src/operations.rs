//! Cancellation is acknowledged only before the atomic commit boundary.
use crate::state::Shared;
use anyhow::{anyhow, ensure, Result};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};
use tauri::{AppHandle, Emitter};

pub struct Operation {
    pub id: String,
    kind: String,
    phase: AtomicU8, // 0 running, 1 cancelled, 2 committing, 3 finished
    app: AppHandle,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub stage: String,
    pub completed: usize,
    pub total: usize,
    pub error: Option<String>,
}
impl Operation {
    pub fn start(
        app: &AppHandle,
        state: &Shared,
        id: Option<String>,
        kind: &str,
    ) -> Result<Arc<Self>> {
        let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        ensure!(uuid::Uuid::parse_str(&id).is_ok(), "Invalid operation ID");
        let mut active = state
            .operations
            .lock()
            .map_err(|_| anyhow!("Operation lock"))?;
        ensure!(!active.contains_key(&id), "Operation is already running");
        let op = Arc::new(Self {
            id: id.clone(),
            kind: kind.into(),
            phase: AtomicU8::new(0),
            app: app.clone(),
        });
        active.insert(id, op.clone());
        op.progress("starting", 0, 0);
        Ok(op)
    }
    pub fn check(&self) -> Result<()> {
        ensure!(
            self.phase.load(Ordering::Acquire) != 1,
            "Operation cancelled"
        );
        Ok(())
    }
    pub fn commit(&self) -> Result<()> {
        self.phase
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| anyhow!("Operation cancelled"))?;
        self.progress("committing", 0, 0);
        Ok(())
    }
    pub fn progress(&self, stage: &str, completed: usize, total: usize) {
        let _ = self.app.emit(
            "operation-event",
            Event {
                id: self.id.clone(),
                kind: self.kind.clone(),
                state: "running".into(),
                stage: stage.into(),
                completed,
                total,
                error: None,
            },
        );
    }
    pub fn finish(&self, state: &Shared, error: Option<String>) {
        let cancelled = self.phase.swap(3, Ordering::AcqRel) == 1;
        let _ = self.app.emit(
            "operation-event",
            Event {
                id: self.id.clone(),
                kind: self.kind.clone(),
                state: if cancelled {
                    "cancelled"
                } else if error.is_some() {
                    "failed"
                } else {
                    "succeeded"
                }
                .into(),
                stage: "finished".into(),
                completed: 0,
                total: 0,
                error,
            },
        );
        if let Ok(mut active) = state.operations.lock() {
            active.remove(&self.id);
        }
    }
}
pub fn cancel(state: &Shared, id: &str) -> Result<bool> {
    let active = state
        .operations
        .lock()
        .map_err(|_| anyhow!("Operation lock"))?;
    let Some(op) = active.get(id) else {
        return Ok(false);
    };
    match op
        .phase
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
    {
        Ok(_) | Err(1) => Ok(true),
        _ => Ok(false),
    }
}
pub fn cancel_all(state: &Shared) {
    if let Ok(active) = state.operations.lock() {
        for op in active.values() {
            let _ = op
                .phase
                .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
        }
    }
}
