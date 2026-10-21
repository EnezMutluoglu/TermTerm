//! Isolated lab driver for the shared route reservation policy.
use serde::Deserialize;
use std::io::{self, BufRead, Write};
use termterm_vpn_core::network::{NetworkPlan, Reservations};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
enum Request {
    Reserve {
        id: String,
        generation: u64,
        plan: NetworkPlan,
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
            anyhow::ensure!(bytes.len() + n <= 256 * 1024, "message_too_large");
            bytes.extend_from_slice(&buffer[..n]);
            input.consume(n);
            if bytes.last() == Some(&b'\n') {
                break;
            }
        }
        let result = (|| -> anyhow::Result<()> {
            match serde_json::from_slice::<Request>(&bytes)? {
                Request::Reserve {
                    id,
                    generation,
                    plan,
                } => registry.reserve(&id, generation, plan, false),
                Request::Release { id, generation } => {
                    anyhow::ensure!(registry.release(&id, generation), "stale_session");
                    Ok(())
                }
            }
        })();
        println!(
            "{}",
            match result {
                Ok(()) => serde_json::json!({"ok": true}),
                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
            }
        );
        io::stdout().flush()?;
    }
}
