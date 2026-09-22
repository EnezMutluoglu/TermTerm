//! Stable-only updates from the owner's release feed. Development builds never install updates.
use crate::state::Shared;
use serde_json::{json, Value};
use std::{sync::Mutex, time::Duration};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};

pub const REPOSITORY: &str = "https://github.com/EnezMutluoglu/TermTerm";
pub fn channel() -> &'static str {
    option_env!("TERMTERM_RELEASE_CHANNEL").unwrap_or("development")
}

#[derive(Default)]
pub struct UpdateState {
    pending: Mutex<Option<Update>>,
    gate: tokio::sync::Mutex<()>,
}

#[tauri::command]
pub fn update_info() -> Value {
    json!({"version":env!("CARGO_PKG_VERSION"),"channel":channel(),"enabled":channel()=="stable", "repository":REPOSITORY,
        "platform":std::env::consts::OS, "packageSupported": cfg!(any(windows, target_os="macos")) || std::env::var_os("APPIMAGE").is_some()})
}

fn approved_download(version: &str, url: &url::Url, raw: &Value) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()))
        && raw["approved"] == true
        && url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.path().starts_with(&format!(
            "/EnezMutluoglu/TermTerm/releases/download/v{version}/"
        ))
}

#[tauri::command]
pub async fn update_check(app: AppHandle, state: State<'_, UpdateState>) -> Result<Value, String> {
    if channel() != "stable" {
        return Err("Development builds do not receive stable updates.".into());
    }
    let _busy = state
        .gate
        .try_lock()
        .map_err(|_| "An update operation is already running")?;
    *state.pending.lock().map_err(|_| "Update lock")? = None;
    let update = app
        .updater_builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("Update check failed: {e}"))?;
    match update {
        Some(update) => {
            if !approved_download(&update.version, &update.download_url, &update.raw_json) {
                return Err(
                    "The feed does not identify an approved stable release from this repository."
                        .into(),
                );
            }
            let result = json!({"available":true,"version":update.version,"notes":update.body});
            *state.pending.lock().map_err(|_| "Update lock")? = Some(update);
            Ok(result)
        }
        None => Ok(json!({"available":false})),
    }
}

#[tauri::command]
pub async fn update_install(
    app: AppHandle,
    updates: State<'_, UpdateState>,
    state: State<'_, Shared>,
    version: String,
) -> Result<(), String> {
    if channel() != "stable" {
        return Err("Development builds cannot install updates.".into());
    }
    #[cfg(target_os = "linux")]
    if std::env::var_os("APPIMAGE").is_none() {
        return Err(
            "Update .deb installations with your package manager or the new .deb from Releases."
                .into(),
        );
    }
    let _busy = updates
        .gate
        .try_lock()
        .map_err(|_| "An update operation is already running")?;
    let mut update = updates
        .pending
        .lock()
        .map_err(|_| "Update lock")?
        .clone()
        .ok_or("Check for updates first")?;
    if update.version != version {
        return Err("The selected update has changed. Check again.".into());
    }
    if !state
        .operations
        .lock()
        .map_err(|_| "Operation lock")?
        .is_empty()
        || !state
            .transfers
            .lock()
            .map_err(|_| "Transfer lock")?
            .is_empty()
    {
        return Err("Finish or cancel active imports and transfers before updating.".into());
    }
    let mut received = 0u64;
    // Feed requests are small, but the installer includes offline prerequisites.
    // Do not carry the 20-second feed timeout into a large package download.
    update.timeout = Some(Duration::from_secs(900));
    let bytes = tokio::time::timeout(
        Duration::from_secs(900),
        update.download(
            |size, total| {
                received += size as u64;
                let _ = app.emit(
                    "update-progress",
                    json!({"received":received,"total":total,"phase":"downloading"}),
                );
            },
            || {},
        ),
    )
    .await
    .map_err(|_| "Update download timed out; the installed application is unchanged")?
    .map_err(|e| format!("Update download or signature verification failed: {e}"))?;
    // Download and signature validation completed before stopping user sessions.
    if !state
        .operations
        .lock()
        .map_err(|_| "Operation lock")?
        .is_empty()
        || !state
            .transfers
            .lock()
            .map_err(|_| "Transfer lock")?
            .is_empty()
    {
        return Err("An operation started during download. Finish it and retry.".into());
    }
    crate::commands::close_connections(&state).await?;
    *state.vault.lock().map_err(|_| "Vault lock")? = None;
    let _ = app.emit("update-progress", json!({"phase":"installing"}));
    let result = tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string());
    if result.is_err() {
        let _ = app.emit("update-progress", json!({"phase":"vault-locked"}));
    }
    result?;
    #[cfg(not(windows))]
    app.restart();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires TERMTERM_VERIFY_ARTIFACT pointing to a signed release installer"]
    fn release_signature_and_tamper_check() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        let path=std::env::var("TERMTERM_VERIFY_ARTIFACT").unwrap();
        let config:Value=serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let public=String::from_utf8(STANDARD.decode(config["plugins"]["updater"]["pubkey"].as_str().unwrap()).unwrap()).unwrap();
        let signature=String::from_utf8(STANDARD.decode(std::fs::read_to_string(format!("{path}.sig")).unwrap().trim()).unwrap()).unwrap();
        let public=minisign_verify::PublicKey::decode(&public).unwrap();
        let signature=minisign_verify::Signature::decode(&signature).unwrap();
        let mut bytes=std::fs::read(path).unwrap();
        public.verify(&bytes,&signature,true).unwrap();
        bytes[0]^=1;
        assert!(public.verify(&bytes,&signature,true).is_err());
    }
    #[test]
    fn only_approved_stable_assets_from_our_repo_are_accepted() {
        let approved = json!({"approved":true});
        let good = "https://github.com/EnezMutluoglu/TermTerm/releases/download/v0.3.2/setup.exe";
        assert!(approved_download(
            "0.3.2",
            &good.parse().unwrap(),
            &approved
        ));
        for bad in [
            good.replace("https:", "http:"),
            good.replace("github.com", "evil.test"),
            good.replace("TermTerm/", "Other/"),
            good.replace("v0.3.2/", "v0.3.1/"),
        ] {
            assert!(!approved_download(
                "0.3.2",
                &bad.parse().unwrap(),
                &approved
            ));
        }
        assert!(!approved_download(
            "0.3.2-beta.1",
            &good.parse().unwrap(),
            &approved
        ));
        assert!(!approved_download(
            "0.3.2",
            &good.parse().unwrap(),
            &json!({"approved":false})
        ));
    }
}
