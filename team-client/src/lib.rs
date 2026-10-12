//! Rust-only desktop Team client. Bearer tokens and plaintext secrets never form desktop IPC responses.
pub mod access;
pub mod connection;
pub mod desktop;
pub mod keys;
pub mod history;
pub mod local;
pub mod offline;
pub mod profiles;
pub mod resources;
pub mod sharing;
pub mod recovery;
pub mod rotation;
pub mod shared_recovery;
pub mod moving;
pub mod transport;
pub mod compatibility;
// Reuse the audited vault primitives; the portable vault format is unchanged.
#[path = "../../src-tauri/src/crypto.rs"]
mod vault_crypto;

pub mod chain;
mod login_cache;
mod candidate_delivery;

pub mod proxy;
