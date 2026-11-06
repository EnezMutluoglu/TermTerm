//! Explicit debug driver: authenticates with the audited engine and exits before tunnel setup.
use super::*;
use serde_json::json;
use std::{
    io::{BufReader, Write},
    sync::Arc,
};
use tauri::Emitter;
use termterm_vpn_windows_broker::{
    protocol::{Challenge, EngineEvent},
    windows::process::{self, Child},
};

#[derive(Default)]
pub(super) struct Runtime {
    engines: Mutex<HashMap<String, Engine>>,
    epoch: std::sync::atomic::AtomicU64,
}
struct Engine {
    generation: u64,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    child: Arc<Mutex<Child>>,
    input: Arc<Mutex<std::fs::File>>,
}
impl Drop for Engine {
    fn drop(&mut self) {
        if let Ok(child) = self.child.lock() {
            let _ = child.terminate();
        }
    }
}
pub(super) fn clear(state: &Shared) {
    state
        .vpn
        .sso
        .auth
        .epoch
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if let Ok(mut all) = state.vpn.sso.auth.engines.lock() {
        all.clear();
    }
}

fn current(state: &Shared, epoch: u64, vault: &str) -> bool {
    state
        .vpn
        .sso
        .auth
        .epoch
        .load(std::sync::atomic::Ordering::SeqCst)
        == epoch
        && !state.vpn.closing.load(std::sync::atomic::Ordering::SeqCst)
        && state
            .vault
            .lock()
            .ok()
            .is_some_and(|v| v.as_ref().is_some_and(|v| v.id == vault))
}
fn cache(state: &Shared, epoch: u64, vault: &str, public: &mut Value) -> Result<bool> {
    if !current(state, epoch, vault) {
        return Ok(false);
    }
    let id = public["id"]
        .as_str()
        .context("Invalid auth test session")?
        .to_owned();
    let mut sessions = state
        .vpn
        .sessions
        .lock()
        .map_err(|_| anyhow!("VPN session lock"))?;
    super::super::vpn::retain_failure(sessions.get(&id), public);
    sessions.insert(id, public.clone());
    Ok(true)
}

pub(crate) async fn request(state: &Shared, request: &Value) -> Option<Result<Value>> {
    let id = request["id"].as_str()?;
    let cached = match state.vpn.sessions.lock() {
        Ok(sessions) => sessions.get(id).cloned()?,
        Err(_) => return Some(Err(anyhow!("VPN session lock"))),
    };
    if cached["authOnly"] != true {return None;}
    let scoped = (|| -> Result<()> {
        let vault = state.vault.lock().map_err(|_|anyhow!("Vault lock"))?;
        ensure!(vault.as_ref().is_some_and(|vault|cached["vaultId"] == vault.id)
            && cached["generation"] == request["generation"], "Stale VPN authentication test");
        Ok(())
    })();
    if let Err(error) = scoped {return Some(Err(error));}
    let (input, wire) = {
        let all = match state.vpn.sso.auth.engines.lock() {
            Ok(all) => all,
            Err(_) => return Some(Err(anyhow!("VPN auth test lock"))),
        };
        // The first cancellation can remove the engine before the pane's second close arrives.
        let Some(engine) = all.get(id) else {
            return Some(if request["action"] == "disconnect" {Ok(Value::Null)} else {Err(anyhow!("VPN authentication test is no longer active"))});
        };
        if request["generation"] != engine.generation {
            return Some(Err(anyhow!("Stale VPN authentication test")));
        }
        let input = engine.input.clone();
        let action = request["action"].as_str().unwrap_or_default();
        if action == "disconnect" {
            engine.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
            drop(all);
            if let Ok(mut all) = state.vpn.sso.auth.engines.lock() {
                all.remove(id);
            }
            return Some(Ok(Value::Null));
        }
        if !matches!(action, "answer" | "ssoAnswer") {
            return Some(Err(anyhow!("Unsupported authentication test request")));
        }
        let frame = engine_frame(request);
        let wire = Zeroizing::new(format!("{frame}\n"));
        (input, wire)
    };
    Some(
        tokio::task::spawn_blocking(move || -> Result<Value> {
            input
                .lock()
                .map_err(|_| anyhow!("VPN authentication pipe lock"))?
                .write_all(wire.as_bytes())?;
            Ok(Value::Null)
        })
        .await
        .map_err(anyhow::Error::from)
        .and_then(|r| r),
    )
}

fn engine_frame(request: &Value) -> Value {
    let mut frame = request.clone();
    frame["sessionId"] = request["id"].clone();
    frame["id"] = request["challengeId"].clone();
    frame.as_object_mut().unwrap().remove("challengeId");
    frame
}

fn finish(public: &mut Value, failed: bool, cancelled: bool) {
    if cancelled {
        public["state"] = "disconnected".into();
        public.as_object_mut().unwrap().remove("authenticationVerified");
        public.as_object_mut().unwrap().remove("error");
    } else if failed {
        public["state"] = "failed".into();
        public["error"] = match public["authStage"].as_str() {
            Some("portal") => "Portal SSO giriş testi tamamlanamadı; tünel açılmadı.",
            Some("gatewaySelection") => "Gateway seçimi sonrası giriş testi tamamlanamadı; tünel açılmadı.",
            Some("gateway") => "Gateway SSO giriş testi tamamlanamadı; tünel açılmadı.",
            _ => "SSO giriş testi tamamlanamadı; tünel açılmadı.",
        }.into();
    } else if public["authenticationVerified"] != true {
        public["state"] = "disconnected".into();
    }
    public.as_object_mut().unwrap().remove("challenge");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn repeated_cancellation_keeps_auth_only_ownership_after_engine_removal() {
        let temp = tempfile::tempdir().unwrap();
        let vault = crate::vault::Vault::create(&temp.path().join("cancel.ttvault"),"fixture","disposable-password").unwrap();
        let vault_id = vault.id.clone();
        let state = std::sync::Arc::new(crate::state::AppState::default());
        *state.vault.lock().unwrap() = Some(vault);
        let disconnect = json!({"action":"disconnect","id":"auth-only","generation":7});
        // A close can remove the engine either before or after its reader publishes the terminal state.
        for phase in ["challenge", "disconnected", "failed"] {
            let cached = json!({"id":"auth-only","vaultId":vault_id,"generation":7,"state":phase,"authOnly":true,"error":"retained fixture failure"});
            state.vpn.sessions.lock().unwrap().insert("auth-only".into(),cached.clone());
            for _ in 0..2 {
                assert!(request(&state,&disconnect).await.unwrap().is_ok(),"known auth-only cancellation must never fall through to the broker");
            }
            assert_eq!(state.vpn.sessions.lock().unwrap()["auth-only"],cached);
        }
        let mut stale = disconnect.clone(); stale["generation"] = 6.into();
        assert!(request(&state,&stale).await.unwrap().is_err());
        for action in ["answer", "ssoAnswer"] {
            let mut answer = disconnect.clone(); answer["action"] = action.into();
            assert!(request(&state,&answer).await.unwrap().is_err(),"finished auth-only commands must stay off the broker path");
        }
        state.vpn.sessions.lock().unwrap().get_mut("auth-only").unwrap()["vaultId"] = "other-vault".into();
        assert!(request(&state,&disconnect).await.unwrap().is_err());
        state.vpn.sessions.lock().unwrap().get_mut("auth-only").unwrap()["authOnly"] = false.into();
        assert!(request(&state,&disconnect).await.is_none(),"ordinary sessions retain their broker route");
        state.vpn.sessions.lock().unwrap().clear();
        assert!(request(&state,&disconnect).await.is_none());
    }
    #[test]
    fn cancellation_is_distinct_from_phase_failure_and_auth_only_success() {
        for stage in ["portal", "gatewaySelection", "gateway"] {
            let mut public = json!({"state":"challenge","authOnly":true,"authStage":stage,"challenge":{"id":3}});
            finish(&mut public,true,true);
            assert_eq!(public["state"],"disconnected");
            assert!(public.get("error").is_none() && public.get("challenge").is_none());
            finish(&mut public,true,false);
            assert_eq!(public["state"],"failed");
            assert!(public["error"].as_str().unwrap().contains(if stage == "portal" {"Portal"} else {"Gateway"}));
        }
        let mut verified = json!({"state":"disconnected","authOnly":true,"authenticationVerified":true});
        finish(&mut verified,false,false);
        assert_eq!(verified["state"],"disconnected");
        assert_eq!(verified["authenticationVerified"],true);
    }
    #[test]
    fn reply_wire_keeps_session_and_challenge_identities_distinct() {
        for action in ["answer", "ssoAnswer"] {
            let wire = engine_frame(
                &json!({"action":action,"id":"fixture-session","generation":7,"challengeId":3,"result":{"token":"fixture"}}),
            );
            assert_eq!(wire["sessionId"], "fixture-session");
            assert_eq!(wire["generation"], 7);
            assert_eq!(wire["id"], 3);
            assert!(wire.get("challengeId").is_none());
        }
    }
    #[tokio::test]
    async fn delayed_reader_cannot_republish_after_the_same_vault_is_reactivated() {
        let temp = tempfile::tempdir().unwrap();
        let vault = crate::vault::Vault::create(
            &temp.path().join("epoch.ttvault"),
            "fixture",
            "disposable-password",
        )
        .unwrap();
        let vault_id = vault.id.clone();
        let state = std::sync::Arc::new(crate::state::AppState::default());
        *state.vault.lock().unwrap() = Some(vault);
        let old_epoch = state
            .vpn
            .sso
            .auth
            .epoch
            .load(std::sync::atomic::Ordering::SeqCst);
        crate::vpn::close_all(&state).await.unwrap();
        crate::vpn::activate(&state);
        let shared = state.clone();
        let old_vault = vault_id.clone();
        let delayed = tokio::task::spawn_blocking(move || {
            let _gate = shared.vpn.gate.blocking_lock();
            let mut old =
                json!({"id":"old-session","vaultId":old_vault,"generation":1,"state":"challenge"});
            cache(&shared, old_epoch, &old_vault, &mut old).unwrap()
        });
        assert!(!tokio::time::timeout(Duration::from_secs(1), delayed)
            .await
            .unwrap()
            .unwrap());
        assert!(state.vpn.sessions.lock().unwrap().is_empty());
        let epoch = state
            .vpn
            .sso
            .auth
            .epoch
            .load(std::sync::atomic::Ordering::SeqCst);
        state
            .vpn
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert!(
            current(&state, epoch, &vault_id),
            "independent connections do not invalidate a vault lifecycle"
        );
    }
}

pub(crate) async fn start(app: &AppHandle, state: &Shared, profile_id: String) -> Result<Value> {
    let _gate = state.vpn.gate.lock().await;
    let vault_id = super::super::vpn::dev(app, state)?;
    let epoch = state
        .vpn
        .sso
        .auth
        .epoch
        .load(std::sync::atomic::Ordering::SeqCst);
    super::super::vpn::ensure_idle(state, &profile_id)?;
    let profile = {
        let vault = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        super::super::vpn::profiles_raw(vault.as_ref().context("Vault locked")?)?
            .into_iter()
            .find(|r| r.id == profile_id)
            .context("VPN profile not found")?
            .data
    };
    ensure!(
        profile["family"] == "globalProtect",
        "This authentication test requires GlobalProtect"
    );
    let directory = std::env::current_exe()?
        .parent()
        .context("VPN engine directory unavailable")?
        .to_owned();
    // Bundle verification is immutable; this never invokes the privileged broker readiness path.
    super::super::vpn_bundle::inspect(&directory)?;
    let child = process::auth_engine(&directory.join("termterm-openconnect.exe"))?;
    let input = Arc::new(Mutex::new(child.input.try_clone()?));
    let reader = BufReader::new(child.output.try_clone()?);
    let child = Arc::new(Mutex::new(child));
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let id = uuid::Uuid::new_v4().to_string();
    let generation = state
        .vpn
        .generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        + 1;
    let session = json!({"id":id,"generation":generation,"vaultId":vault_id,"profileId":profile_id,"state":"connecting","authOnly":true});
    let initial = Zeroizing::new(format!(
        "{}\n",
        json!({"sessionId":id,"generation":generation,"protocol":"gp","server":profile["server"],"authOnly":true})
    ));
    input
        .lock()
        .map_err(|_| anyhow!("VPN authentication pipe lock"))?
        .write_all(initial.as_bytes())?;
    state
        .vpn
        .sessions
        .lock()
        .map_err(|_| anyhow!("VPN session lock"))?
        .insert(id.clone(), session.clone());
    state
        .vpn
        .sso
        .auth
        .engines
        .lock()
        .map_err(|_| anyhow!("VPN auth test lock"))?
        .insert(
            id.clone(),
            Engine {
                generation,
                cancelled: cancelled.clone(),
                child: child.clone(),
                input: input.clone(),
            },
        );
    let app = app.clone();
    let shared = state.clone();
    let mut public = session.clone();
    tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let outcome = (|| -> Result<()> {
            loop {
                let event: EngineEvent = serde_json::from_value(
                    termterm_vpn_windows_broker::protocol::read_sync(&mut reader)?,
                )?;
                ensure!(
                    event.session_id.to_string() == id && event.generation == generation,
                    "Stale authentication engine event"
                );
                let _gate = shared.vpn.gate.blocking_lock();
                if !current(&shared, epoch, &vault_id) {
                    break;
                }
                match event.kind.as_str() {
                    "connecting" => {
                        public["state"] = "connecting".into();
                        public.as_object_mut().unwrap().remove("challenge");
                    }
                    "challenge" => {
                        let challenge: Challenge = serde_json::from_value(event.detail)?;
                        challenge.validate()?;
                        public["state"] = "challenge".into();
                        public["challenge"] = serde_json::to_value(challenge)?;
                        public["authStage"] = if public["challenge"]["fields"].as_array().is_some_and(|fields|fields.iter().any(|field|field["name"] == "gateway" && field["purpose"] == "choice")) {
                            "gatewaySelection".into()
                        } else { public["challenge"]["stage"].clone() };
                        super::session(&app, &shared, &mut public)?;
                    }
                    "authenticated" => {
                        public["state"] = "disconnected".into();
                        public["authenticationVerified"] = true.into();
                        public.as_object_mut().unwrap().remove("challenge");
                        break;
                    }
                    "failed" => return Err(anyhow!("GlobalProtect authentication failed")),
                    _ => return Err(anyhow!("Unexpected authentication engine event")),
                }
                if cache(&shared, epoch, &vault_id, &mut public)? {
                    let _ = app.emit_to("main", "vpn-event", public.clone());
                }
            }
            Ok(())
        })();
        let _gate = shared.vpn.gate.blocking_lock();
        finish(&mut public, outcome.is_err(), cancelled.load(std::sync::atomic::Ordering::SeqCst));
        super::cancel(&shared, &id, generation, None);
        if let Ok(mut all) = shared.vpn.sso.auth.engines.lock() {
            all.remove(&id);
        }
        if cache(&shared, epoch, &vault_id, &mut public).unwrap_or(false) {
            let _ = app.emit_to("main", "vpn-event", public);
        }
        if let Ok(child) = child.lock() {
            let _ = child.terminate();
        }
    });
    Ok(session)
}
