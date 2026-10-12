//! Narrow native IPC facade; all service sessions/crypto identities stay in Rust.
use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use termterm_team_client::{
    desktop::{DesktopAction, DesktopState},
    transport::ServiceProfile,
};
pub mod sftp;
pub mod signin;
pub mod terminal;
pub mod tunnel;
pub use terminal::{NativeConnections, TrustState};

#[tauri::command]
pub async fn team_v2_compatibility(
    app: AppHandle,
    state: State<'_, DesktopState>,
    profile: ServiceProfile,
) -> Result<Value, String> {
    state.configure_cache(app.path().app_data_dir().map_err(|e|e.to_string())?.join("team-v2-cache"))
        .map_err(|e|e.to_string())?;
    state.compatibility(profile).await.map_err(|e|e.to_string())
}

#[tauri::command]
pub async fn team_v2_login(
    app: AppHandle,
    state: State<'_, DesktopState>,
    connections: State<'_, NativeConnections>,
    profile: ServiceProfile,
    username: String,
    password: String,
    remember: Option<bool>,
) -> Result<Value, String> {
    let password = zeroize::Zeroizing::new(password);
    connections.clear_trust();
    state
        .configure_cache(
            app.path()
                .app_data_dir()
                .map_err(|e| e.to_string())?
                .join("team-v2-cache"),
        )
        .map_err(|e| e.to_string())?;
    let mut view = state.login(profile.clone(), username.clone(), password.to_string())
        .await.map_err(|e| e.to_string())?;
    if view["activationRequired"] == false {
        let root = app.path().app_data_dir().map_err(|e|e.to_string())?.join("team-v2-signin");
        let stored = tauri::async_runtime::spawn_blocking(move || signin::save(&root,profile,username,&password,remember.unwrap_or(false)))
            .await.map_err(|e|e.to_string())?;
        if let Err(e) = stored { view["rememberWarning"] = serde_json::json!(e.to_string()); }
    }
    Ok(view)
}
#[tauri::command]
pub async fn team_v2_submit_drafts(
    app: AppHandle,
    state: State<'_, DesktopState>,
    connections: State<'_, NativeConnections>,
    profile: ServiceProfile,
    username: String,
    password: String,
) -> Result<Value, String> {
    connections.clear_trust();
    state
        .configure_cache(
            app.path()
                .app_data_dir()
                .map_err(|e| e.to_string())?
                .join("team-v2-cache"),
        )
        .map_err(|e| e.to_string())?;
    state
        .submit_drafts(profile, username, password)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn team_v2_action(
    state: State<'_, DesktopState>,
    action: DesktopAction,
) -> Result<Value, String> {
    if let DesktopAction::SaveCredential {
        credential_kind,
        secret,
        passphrase,
        ..
    } = &action
    {
        if credential_kind == "ssh_key" {
            crate::ssh_compat::decode_private_key(secret, passphrase.as_deref().unwrap_or(""))
                .map_err(|_| "SSH anahtarı veya anahtar parolası geçersiz".to_string())?;
        }
    }
    state.call(action).await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn team_v2_logout(
    state: State<'_, DesktopState>,
    connections: State<'_, NativeConnections>,
) -> Result<(), String> {
    let result = state.logout().map_err(|e| e.to_string());
    connections.clear_trust();
    result
}
