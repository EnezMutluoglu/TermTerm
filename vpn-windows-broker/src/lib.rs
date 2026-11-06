//! Windows VPN broker. Network changes require an authenticated owner and an explicit build contract.
#[cfg(all(feature = "production-candidate", any(feature = "privileged-validation", feature = "auth-only-test")))]
compile_error!("The production VPN candidate cannot include diagnostic launch modes");
pub mod packet;
pub mod bundle;
pub mod policy;
pub mod transaction;
pub mod protocol;
#[cfg(windows)]
pub mod windows;

pub const MAX_CONNECTIONS: usize = 3;
pub const CONTROL_LIMIT: usize = 2 * 1024 * 1024;
pub const PROTOCOL_VERSION: u32 = 2;
pub const BROKER_ID: &str = if cfg!(feature = "production-candidate") { "local.termterm.desktop.broker" } else { "local.termterm.desktop.vpndev.broker" };
pub const PIPE_NAMESPACE: &str = if cfg!(feature = "production-candidate") { "TermTerm.Vpn" } else { "TermTerm.VpnDev" };
pub const TUNNEL_TYPE: &str = if cfg!(feature = "production-candidate") { "TermTerm VPN" } else { "TermTerm VPN Development" };
pub const TUNNEL_DESCRIPTION: &str = if cfg!(feature = "production-candidate") { "TermTerm VPN Tunnel" } else { "TermTerm VPN Development Tunnel" };

/// Scoped Windows development adapter/driver/watchdog/crash acceptance passed on 2026-10-26.
/// Negotiated network policy and runtime ownership guards still apply to every connection.
pub const PRIVILEGED_PATH_VALIDATED: bool = true;
