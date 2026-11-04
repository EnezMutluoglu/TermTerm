//! Isolated lab driver for the shared route reservation policy.
use serde::Deserialize;
use std::io::{self, BufRead, Write};
use termterm_vpn_core::network::{NetworkPlan, Reservations};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
enum Request {
    PasswordChallengeFields {
        challenge: serde_json::Value,
    },
    ValidateProfile {
        profile: termterm_vpn_core::Profile,
    },
    WireGuardConfig {
        profile: termterm_vpn_core::Profile,
    },
    Reserve {
        id: String,
        generation: u64,
        plan: NetworkPlan,
        #[serde(default, rename = "splitDns")]
        split_dns: bool,
    },
    Release {
        id: String,
        generation: u64,
    },
}
fn main() -> anyhow::Result<()> {
    let mut registry = Reservations::default();
    let mut input = io::stdin().lock();
    loop {
        let mut bytes = Vec::new();
        // Bounded framing, including malformed peers that omit the newline.
        loop {
            let buffer = input.fill_buf()?;
            if buffer.is_empty() {
                return Ok(());
            }
            let n = buffer
                .iter()
                .position(|b| *b == b'\n')
                .map_or(buffer.len(), |n| n + 1);
            anyhow::ensure!(bytes.len() + n <= 2 * 1024 * 1024, "message_too_large");
            bytes.extend_from_slice(&buffer[..n]);
            input.consume(n);
            if bytes.last() == Some(&b'\n') {
                break;
            }
        }
        let result = (|| -> anyhow::Result<serde_json::Value> {
            match serde_json::from_slice::<Request>(&bytes)? {
                Request::PasswordChallengeFields { challenge } => {
                    Ok(serde_json::to_value(termterm_vpn_core::password_challenge_fields(&challenge))?)
                }
                Request::ValidateProfile { profile } => {
                    profile.validate()?;
                    if profile.family == termterm_vpn_core::Family::OpenVpn {
                        termterm_vpn_core::import::openvpn_embedded(&profile.config)?;
                    }
                    Ok(serde_json::Value::Null)
                }
                Request::WireGuardConfig { profile } => {
                    profile.validate()?;
                    anyhow::ensure!(profile.family == termterm_vpn_core::Family::WireGuard, "wireguard_profile_required");
                    termterm_vpn_core::wireguard::parse(&profile.config)?.engine_config()
                }
                Request::Reserve {
                    id,
                    generation,
                    plan,
                    split_dns,
                } => registry.reserve(&id, generation, plan, split_dns).map(|()| serde_json::Value::Null),
                Request::Release { id, generation } => {
                    anyhow::ensure!(registry.release(&id, generation), "stale_session");
                    Ok(serde_json::Value::Null)
                }
            }
        })();
        println!(
            "{}",
            match result {
                Ok(result) => serde_json::json!({"ok": true, "result": result}),
                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
            }
        );
        io::stdout().flush()?;
    }
}
