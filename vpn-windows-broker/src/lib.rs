//! Development-only Windows VPN broker. Network changes require an authenticated owner.
pub mod packet;
pub mod policy;
pub mod transaction;
pub mod protocol;
#[cfg(windows)]
pub mod windows;

pub const MAX_CONNECTIONS: usize = 3;
pub const CONTROL_LIMIT: usize = 2 * 1024 * 1024;
pub const PROTOCOL_VERSION: u32 = 2;

/// Scoped Windows development adapter/driver/watchdog/crash acceptance passed on 2026-10-26.
/// Negotiated network policy and runtime ownership guards still apply to every connection.
pub const PRIVILEGED_PATH_VALIDATED: bool = true;
