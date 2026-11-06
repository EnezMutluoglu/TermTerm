//! Stable-only updates from the owner's release feed. Development builds never install updates.
use crate::state::Shared;
use serde_json::{json, Value};
use std::{sync::Mutex, time::Duration};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};

pub const REPOSITORY: &str = "https://github.com/EnezMutluoglu/TermTerm";
pub fn channel() -> &'static str {
    build_channel(env!("CARGO_PKG_VERSION"), cfg!(feature = "vpn-candidate"), option_env!("TERMTERM_RELEASE_CHANNEL"))
}
fn build_channel(version: &str, candidate: bool, requested: Option<&'static str>) -> &'static str {
    if candidate { return if requested == Some("stable") { "stable" } else { "candidate" }; }
    if version.contains('-') { return "development"; }
    requested.unwrap_or("development")
}

#[derive(Default)]
pub struct UpdateState {
    pending: Mutex<Option<Update>>,
    gate: tokio::sync::Mutex<()>,
}

#[tauri::command]
pub fn update_info(app: AppHandle) -> Value {
    json!({"version":app.package_info().version.to_string(),"channel":channel(),"enabled":channel()=="stable", "repository":REPOSITORY,
        "platform":std::env::consts::OS, "administratorAppImage":administrator_appimage(),
        "packageSupported": !administrator_appimage() && (cfg!(any(windows, target_os="macos")) || std::env::var_os("APPIMAGE").is_some())})
}
#[cfg(any(test, all(target_os="linux", feature="vpn-candidate")))]
fn root_appdir_executable(path: &std::path::Path) -> bool { path == std::path::Path::new("/opt/termterm-appimage/usr/bin/termterm") }
fn administrator_appimage() -> bool {
    #[cfg(all(target_os="linux", feature="vpn-candidate"))]
    return std::env::current_exe().is_ok_and(|path| root_appdir_executable(&path));
    #[cfg(not(all(target_os="linux", feature="vpn-candidate")))]
    false
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
pub async fn update_check(app: AppHandle, state: State<'_, UpdateState>, minimum_version: Option<String>) -> Result<Value, String> {
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
            if let Some(minimum) = &minimum_version {
                if !termterm_team_client::compatibility::satisfies_minimum(&update.version, minimum).map_err(|e|e.to_string())? {
                    return Err("Team için gereken sürüm onaylı güncelleme kaynağında henüz yok. Yöneticinizle iletişime geçin.".into());
                }
            }
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
    minimum_version: Option<String>,
) -> Result<(), String> {
    if channel() != "stable" {
        return Err("Development builds cannot install updates.".into());
    }
    if administrator_appimage() {
        return Err("This root-owned AppImage requires an administrator to stop VPN, install the reviewed replacement and update the broker executable pin. In-app replacement is disabled.".into());
    }
    #[cfg(target_os = "linux")]
    if std::env::var_os("APPIMAGE").is_none() {
        return Err(
            "Update .deb/.rpm installations with the matching package from Releases and your package manager."
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
    if let Some(minimum) = &minimum_version {
        if !termterm_team_client::compatibility::satisfies_minimum(&update.version, minimum).map_err(|e|e.to_string())? {
            return Err("Seçilen güncelleme Team için gereken asgari sürümü karşılamıyor.".into());
        }
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
    // Also wake Team connection watchers before installing/restarting. This
    // drops only session state, never the persistent encrypted offline queue.
    use tauri::Manager;
    app.state::<termterm_team_client::desktop::DesktopState>().logout().map_err(|e|e.to_string())?;
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
    #[cfg(windows)]
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn production_candidate_updates_require_the_explicit_release_build_channel() {
        for requested in [None, Some("candidate"), Some("development"), Some("unknown")] {
            assert_eq!(build_channel("0.4.3", true, requested), "candidate");
        }
        assert_eq!(build_channel("0.4.3", true, Some("stable")), "stable");
        assert_eq!(build_channel("0.4.3-dev.4", false, Some("stable")), "development");
        assert_eq!(build_channel("0.4.2", false, None), "development");
        assert_eq!(build_channel("0.4.2", false, Some("stable")), "stable");
    }
    #[test]
    fn administrator_appimage_path_is_exact_and_does_not_authorize_transient_mounts() {
        assert!(root_appdir_executable(std::path::Path::new("/opt/termterm-appimage/usr/bin/termterm")));
        for path in ["/tmp/.mount_termterm/usr/bin/termterm", "/usr/bin/termterm", "/opt/termterm-appimage/usr/bin/termterm-other"] {
            assert!(!root_appdir_executable(std::path::Path::new(path)));
        }
    }
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
