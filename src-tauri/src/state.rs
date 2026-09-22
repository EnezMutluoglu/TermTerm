use crate::vault::Vault;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, oneshot};

pub enum SessionInput {
    Data(Vec<u8>),
    Resize(u16, u16),
    Close,
}
#[derive(Default)]
pub struct AppState {
    pub vault: Arc<Mutex<Option<Vault>>>,
    pub sessions: Mutex<HashMap<String, mpsc::Sender<SessionInput>>>,
    pub prompts: Mutex<HashMap<String, oneshot::Sender<Vec<String>>>>,
    pub sftp: tokio::sync::Mutex<HashMap<String, Arc<crate::connections::SftpConnection>>>,
    pub tunnels: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    pub sync_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub sync_gate: tokio::sync::Mutex<()>,
    pub edits: tokio::sync::Mutex<HashMap<String, crate::files::Edit>>,
    pub shares: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    pub shared_writers: Mutex<HashMap<String, bool>>,
    pub bridge: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub metrics: Mutex<HashMap<String, crate::metrics::Monitor>>,
    pub operations: Mutex<HashMap<String, Arc<crate::operations::Operation>>>,
    pub transfers: Mutex<HashMap<String, Arc<crate::transfers::Control>>>,
}
pub type Shared = Arc<AppState>;
