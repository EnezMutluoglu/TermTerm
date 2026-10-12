//! Public connection preferences and an OS-protected, scope-bound password.
//! The password is consumed by Rust only, never returned through IPC.
use super::DesktopState;
use anyhow::{ensure, Context, Result};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};
use termterm_team_client::transport::ServiceProfile;
use zeroize::Zeroizing;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Saved {
    format: u32,
    pub profile: ServiceProfile,
    pub username: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Protected { binding: String, password: String }
impl Drop for Protected {
    fn drop(&mut self) { use zeroize::Zeroize; self.password.zeroize(); }
}
fn path(root: &Path) -> PathBuf { root.join("team-signin.ttsettings") }
fn validate(saved: &Saved) -> Result<()> {
    ensure!(saved.format == 1, "Unsupported Team sign-in preferences");
    saved.profile.cache_scope(uuid::Uuid::nil())?;
    ensure!(!saved.username.trim().is_empty() && saved.username.len() <= 320, "Invalid Team username");
    ensure!(saved.profile.ca_pem.as_ref().is_none_or(|s| s.len() <= 65536), "Invalid CA size");
    Ok(())
}
fn read(root: &Path) -> Result<Option<Saved>> {
    let file = path(root);
    if !file.try_exists()? { return Ok(None); }
    let db = rusqlite::Connection::open(file)?;
    let text: Option<String> = db.query_row("SELECT document FROM signin WHERE id=1", [], |r| r.get(0)).optional()?;
    text.map(|text| { let saved: Saved = serde_json::from_str(&text)?; validate(&saved)?; Ok(saved) }).transpose()
}
fn password(root: &Path, saved: &Saved) -> Result<Option<Zeroizing<String>>> {
    let Some(value) = crate::remember::load_optional(root, &path(root))? else { return Ok(None); };
    let protected: Protected = serde_json::from_str(&value)?;
    if protected.binding != serde_json::to_string(saved)? { return Ok(None); }
    Ok(Some(Zeroizing::new(protected.password.clone())))
}
pub fn save(root: &Path, profile: ServiceProfile, username: String, password: &str, enabled: bool) -> Result<()> {
    let saved = Saved { format: 1, profile, username };
    validate(&saved)?;
    std::fs::create_dir_all(root)?;
    let db = rusqlite::Connection::open(path(root))?;
    db.execute_batch("PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS signin(id INTEGER PRIMARY KEY CHECK(id=1),document TEXT NOT NULL);")?;
    let binding = serde_json::to_string(&saved)?;
    db.execute("INSERT INTO signin VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET document=excluded.document", [&binding])?;
    let protected = Zeroizing::new(serde_json::to_string(&Protected { binding, password: password.into() })?);
    crate::remember::save(root, &path(root), &protected, enabled)
}
fn root(app: &AppHandle) -> Result<PathBuf> { Ok(app.path().app_data_dir()?.join("team-v2-signin")) }
#[tauri::command]
pub async fn team_v2_saved_login(app: AppHandle) -> Result<Value, String> {
    let root = root(&app).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || -> Result<Value> {
        let Some(saved) = read(&root)? else { return Ok(Value::Null); };
        let (remembered, warning) = match password(&root, &saved) {
            Ok(p) => (p.is_some(), None), Err(e) => (false, Some(e.to_string())),
        };
        Ok(json!({"profile":saved.profile,"username":saved.username,"remembered":remembered,"warning":warning}))
    }).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn team_v2_forget_login(app: AppHandle) -> Result<(), String> {
    let root = root(&app).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        if path(&root).try_exists()? {
            crate::remember::save(&root, &path(&root), "", false)?;
            rusqlite::Connection::open(path(&root))?.execute("DELETE FROM signin", [])?;
        }
        Ok(())
    }).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn team_v2_login_remembered(app: AppHandle, state: State<'_, DesktopState>, connections: State<'_, super::NativeConnections>) -> Result<Value, String> {
    let root = root(&app).map_err(|e| e.to_string())?;
    let (saved, secret) = tauri::async_runtime::spawn_blocking(move || -> Result<_> {
        let saved = read(&root)?.context("Kayıtlı Team bağlantısı yok")?;
        let secret = password(&root, &saved)?.context("Kayıtlı parola yok; parolanızla giriş yapın")?;
        Ok((saved, secret))
    }).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    connections.clear_trust();
    state.configure_cache(app.path().app_data_dir().map_err(|e| e.to_string())?.join("team-v2-cache")).map_err(|e| e.to_string())?;
    state.login(saved.profile, saved.username, secret.to_string()).await.map_err(|e| {
        if e.to_string() == "authentication_denied" {
            "Kayıtlı giriş reddedildi. Hesabınızın durumunu kontrol edin veya yeni parolanızla giriş yapın.".into()
        } else { e.to_string() }
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Child process for remembered Team restart proof"]
    fn saved_signin_child() {
        let root = PathBuf::from(std::env::var_os("TERMTERM_SIGNIN_TEST_ROOT").unwrap());
        let saved = read(&root).unwrap().unwrap();
        assert_eq!(&**password(&root,&saved).unwrap().as_ref().unwrap(), "Disposable-Secret-123!");
    }
    #[test]
    fn saved_signin_is_bound_and_never_plaintext() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let profile = ServiceProfile {service_url:"https://example.invalid".into(),team_id:uuid::Uuid::new_v4(),ca_pem:None};
        save(dir.path(), profile.clone(), "operator".into(), "Disposable-Secret-123!", true)?;
        let status = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact","team_v2::signin::tests::saved_signin_child","--ignored","--test-threads=1"])
            .env("TERMTERM_SIGNIN_TEST_ROOT",dir.path()).status()?;
        assert!(status.success());
        let saved = read(dir.path())?.unwrap();
        assert_eq!(&**password(dir.path(), &saved)?.as_ref().unwrap(), "Disposable-Secret-123!");
        assert!(!std::fs::read(path(dir.path()))?.windows(21).any(|w| w == b"Disposable-Secret-123!"));
        let mut other = read(dir.path())?.unwrap(); other.profile.team_id = uuid::Uuid::new_v4();
        assert!(password(dir.path(), &other)?.is_none());
        save(dir.path(), profile, "operator".into(), "", false)?;
        assert!(password(dir.path(), &read(dir.path())?.unwrap())?.is_none());
        Ok(())
    }
}
