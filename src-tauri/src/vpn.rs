//! Encrypted personal/Team VPN profiles and the authenticated broker bridge.
use crate::{crypto, model::Record, state::Shared, vault::Vault};
use anyhow::{anyhow, ensure, Context, Result};
use rusqlite::params;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
};
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use tauri::Emitter;
use tauri::{AppHandle, Manager, State};
use termterm_vpn_core::{Family, Profile};
use uuid::Uuid;
use zeroize::Zeroizing;

type Api<T> = std::result::Result<T, String>;
const DEV_ID: &str = "local.termterm.desktop.vpndev";
fn enabled_application(identifier: &str, version: Option<&str>) -> bool {
    if cfg!(feature = "vpn-candidate") {
        identifier == "local.termterm.desktop" && version == Some("0.4.4")
    } else {
        identifier == DEV_ID
    }
}
fn broker_hello(hello: &Value) -> bool {
    let identity = if cfg!(feature = "vpn-candidate") { "local.termterm.desktop.broker" } else { "local.termterm.desktop.vpndev.broker" };
    hello["ok"] == true && matches!(hello["result"]["version"].as_u64(), Some(1 | 2))
        && ((!cfg!(feature = "vpn-candidate") && hello["result"]["labOnly"] == true)
            || (hello["result"]["labOnly"] == false && hello["result"]["mode"] == "authenticatedNative" && hello["result"]["identity"] == identity))
}
#[derive(Default)]
pub struct Runtime {
    imports: Mutex<HashMap<String, (String, Zeroizing<String>)>>,
    pub(super) sessions: Mutex<HashMap<String, Value>>,
    pub(super) generation: AtomicU64,
    credential_attempts: Mutex<HashMap<String, u64>>,
    team_remember: Mutex<HashMap<String, bool>>,
    team_watchers: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    pub(super) closing: AtomicBool,
    pub(super) gate: tokio::sync::Mutex<()>,
    pub(super) sso: crate::vpn_sso::Runtime,
    owner: Mutex<Option<TeamOwner>>,
    pub(super) lifecycle: crate::vpn_lifecycle::Runtime,
    leases: Mutex<HashMap<String, BoundVpn>>,
    revoked_leases: Mutex<HashMap<String, u64>>,
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    client: tokio::sync::Mutex<Option<BrokerClient>>,
}
#[derive(Clone)]
struct TeamOwner { id: String, context: String, epoch: u64 }
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BoundVpn {
    pub id: String,
    pub generation: u64,
    pub interface_index: u32,
    #[serde(default)] pub interface_name: Option<String>,
    #[serde(default)] pub interface_luid: Option<String>,
    #[serde(default)] pub interface_guid: Option<String>,
    pub address_families: Vec<String>,
    #[serde(default)] pub addresses: Vec<String>,
    #[serde(skip)] pub owner_scope: String,
    #[serde(skip)] pub team_epoch: Option<u64>,
}
impl BoundVpn {
    fn validate(&self) -> Result<()> {
        ensure!(Uuid::parse_str(&self.id).is_ok() && self.generation > 0 && self.interface_index > 0, "Invalid VPN network lease");
        ensure!(!self.address_families.is_empty() && self.address_families.iter().all(|family| matches!(family.as_str(), "ipv4" | "ipv6")), "Unsupported VPN address family");
        ensure!(self.interface_name.as_ref().is_none_or(|name| !name.is_empty() && name.len() <= 64 && name.bytes().all(|b|b.is_ascii_alphanumeric() || b == b'_' || b == b'-')), "Invalid VPN interface name");
        ensure!(self.addresses.len() <= 8 && self.addresses.iter().all(|value| value.split_once('/').is_some_and(|(ip,prefix)| ip.parse::<std::net::IpAddr>().is_ok_and(|ip|prefix.parse::<u8>().is_ok_and(|prefix|prefix <= if ip.is_ipv4() {32} else {128})))), "Invalid VPN interface addresses");
        Ok(())
    }
}

pub(crate) fn bound_active(app: &AppHandle, state: &Shared, lease: &BoundVpn) -> bool {
    if state.vpn.closing.load(Ordering::SeqCst) || dev(app, state).ok().as_deref() != Some(lease.owner_scope.as_str()) { return false; }
    if lease.team_epoch.is_some_and(|epoch|app.state::<termterm_team_client::desktop::DesktopState>().current_epoch() != epoch) { return false; }
    state.vpn.leases.lock().ok().is_some_and(|leases|leases.get(&lease.id).is_some_and(|current| current.generation == lease.generation))
        && state.vpn.sessions.lock().ok().is_some_and(|sessions|sessions.get(&lease.id).is_some_and(|session|session["generation"] == lease.generation && session["state"] == "connected" && session["vaultId"] == lease.owner_scope))
}

pub(crate) async fn ensure_team_required(app: &AppHandle, state: &Shared, host: Uuid, epoch: u64) -> Result<Option<BoundVpn>> {
    ensure_team_required_for_plan(app,state,host,host,epoch).await
}
pub(crate) async fn ensure_team_required_for_plan(app: &AppHandle, state: &Shared, root: Uuid, host: Uuid, epoch: u64) -> Result<Option<BoundVpn>> {
    let desktop = app.state::<termterm_team_client::desktop::DesktopState>();
    ensure!(desktop.current_epoch() == epoch, "Team session changed");
    let Some(profile) = desktop.required_vpn_for_plan(root, host, epoch).await? else { return Ok(None); };
    let (_, context) = desktop.vpn_scope().await?;
    let owner = authorize(app, state, Some(&context)).await?;
    let session = state.vpn.sessions.lock().map_err(|_| anyhow!("VPN lock"))?.values().find(|session|session["profileId"] == profile.to_string() && session["vaultId"] == owner && !terminal(session)).cloned();
    let session = if let Some(session) = session { session } else {
        vpn_connect(app.clone(), app.state::<Shared>(), profile.to_string(), Some(context)).await.map_err(anyhow::Error::msg)?
    };
    let id = session["id"].as_str().context("Invalid required VPN session")?.to_owned();
    let generation = session["generation"].as_u64().context("Invalid required VPN generation")?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(300);
    loop {
        ensure!(desktop.current_epoch() == epoch && !state.vpn.closing.load(Ordering::SeqCst), "Team session closed");
        let current = state.vpn.sessions.lock().map_err(|_| anyhow!("VPN lock"))?.get(&id).cloned().context("Required VPN closed")?;
        ensure!(current["generation"] == generation && !terminal(&current) && current["state"] != "disconnecting", "Required VPN failed or cancelled");
        if current["state"] == "connected" {
            desktop.check_vpn(profile, epoch).await?;
            ensure!(desktop.required_vpn_for_plan(root, host, epoch).await? == Some(profile), "Host VPN requirement changed");
            let lease = state.vpn.leases.lock().map_err(|_| anyhow!("VPN lease lock"))?.get(&id).cloned().context("Required VPN broker does not support interface leases")?;
            ensure!(bound_active(app, state, &lease), "Stale required VPN lease");
            return Ok(Some(lease));
        }
        ensure!(tokio::time::Instant::now() < deadline, "Required VPN authentication timed out");
        tokio::select! { _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}, _ = desktop.cancelled(epoch) => return Err(anyhow!("Team session closed")) }
    }
}

fn error(e: anyhow::Error) -> String {
    format!("{e:#}")
}
fn personal(app: &AppHandle, state: &Shared) -> Result<String> {
    ensure!(!app.state::<termterm_team_client::desktop::DesktopState>().active(), "VPN personal vault required");
    ensure!(
        !state.vpn.closing.load(Ordering::SeqCst),
        "VPN kasa kapanÄ±yor; kasayÄ± yeniden aÃ§Ä±n"
    );
    ensure!(
        enabled_application(&app.config().identifier, app.config().version.as_deref()),
        "VPN yalnÄ±z ayrÄ± geliÅŸtirme uygulamasÄ±nda kullanÄ±labilir"
    );
    ensure!(!crate::team::active(state), "VPN kiÅŸisel kasa gerektiriyor");
    Ok(state
        .vault
        .lock()
        .map_err(|_| anyhow!("Vault lock"))?
        .as_ref()
        .ok_or_else(|| anyhow!("Vault locked"))?
        .id
        .clone())
}

pub(super) fn dev(app: &AppHandle, state: &Shared) -> Result<String> {
    ensure!(enabled_application(&app.config().identifier, app.config().version.as_deref()) && !state.vpn.closing.load(Ordering::SeqCst), "VPN session unavailable for this application build");
    let desktop = app.state::<termterm_team_client::desktop::DesktopState>();
    if desktop.active() {
        let owner = state.vpn.owner.lock().map_err(|_| anyhow!("VPN owner lock"))?;
        return owner.as_ref().filter(|owner| owner.epoch == desktop.current_epoch()).map(|owner| owner.id.clone()).context("Team VPN session changed; reopen VPN");
    }
    personal(app, state)
}
fn ensure_team_context(app: &AppHandle, context: Option<&str>) -> Result<()> {
    ensure!(app.state::<termterm_team_client::desktop::DesktopState>().active() == context.is_some(), "Team VPN context required; reopen VPN");
    Ok(())
}
async fn authorize(app: &AppHandle, state: &Shared, context: Option<&str>) -> Result<String> {
    ensure!(enabled_application(&app.config().identifier, app.config().version.as_deref()) && !state.vpn.closing.load(Ordering::SeqCst), "VPN session unavailable for this application build");
    let desktop = app.state::<termterm_team_client::desktop::DesktopState>();
    if !desktop.active() {
        ensure!(context.is_none(), "Team VPN session closed");
        ensure!(state.vpn.owner.lock().map_err(|_|anyhow!("VPN owner lock"))?.is_none(), "Previous Team VPN owner must close before opening personal VPN");
        return personal(app, state);
    }
    let (epoch, current) = desktop.vpn_scope().await?;
    ensure!(context.is_none_or(|context| context == current), "Team VPN session changed; reopen VPN");
    use sha2::Digest;
    let digest = sha2::Sha256::digest(current.as_bytes());
    let id = Uuid::from_bytes(digest[..16].try_into().unwrap()).to_string();
    let mut owner = state.vpn.owner.lock().map_err(|_| anyhow!("VPN owner lock"))?;
    ensure!(owner.as_ref().is_none_or(|owner| owner.id == id && owner.epoch == epoch), "Previous VPN owner must close before switching accounts");
    *owner = Some(TeamOwner { id: id.clone(), epoch, context: current });
    Ok(id)
}

pub fn profiles_raw(vault: &Vault) -> Result<Vec<Record>> {
    let exists: bool = vault.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='vpn_profiles')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(vec![]);
    }
    let mut query = vault
        .conn
        .prepare("SELECT id,payload FROM vpn_profiles ORDER BY rowid")?;
    let rows = query.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
    })?;
    let mut result = vec![];
    for row in rows {
        let (id, payload) = row?;
        ensure!(Uuid::parse_str(&id).is_ok(), "Invalid VPN profile id");
        let record: Record = serde_json::from_slice(&crypto::unseal(
            &vault.key,
            &payload,
            format!("vpn-profile:{id}").as_bytes(),
        )?)?;
        ensure!(
            record.id == id && record.kind == "vpn",
            "Invalid encrypted VPN profile"
        );
        serde_json::from_value::<Profile>(record.data.clone())?.validate()?;
        result.push(record);
    }
    Ok(result)
}

pub fn restore_profiles(vault: &mut Vault, profiles: &[Record]) -> Result<()> {
    if profiles.is_empty() {
        return Ok(());
    }
    let tx = vault.conn.transaction()?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS vpn_profiles(id TEXT PRIMARY KEY,payload BLOB NOT NULL)",
    )?;
    for record in profiles {
        ensure!(
            Uuid::parse_str(&record.id).is_ok() && record.kind == "vpn",
            "Invalid VPN profile"
        );
        let profile: Profile = serde_json::from_value(record.data.clone())?;
        profile.validate()?;
        if profile.family == Family::OpenVpn {
            termterm_vpn_core::import::openvpn_embedded(&profile.config)?;
        }
        let bytes = Zeroizing::new(serde_json::to_vec(record)?);
        let payload = crypto::seal(
            &vault.key,
            &bytes,
            format!("vpn-profile:{}", record.id).as_bytes(),
        )?;
        tx.execute("INSERT INTO vpn_profiles VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", params![record.id, payload])?;
    }
    // Older clients reject before opening read/write; never silently erase unfamiliar VPN data.
    tx.execute("UPDATE header SET version=MAX(version,2) WHERE id=1", [])?;
    tx.commit()?;
    Ok(())
}

fn view(record: &Record) -> Value {
    let mut data = record.data.clone();
    data["id"] = record.id.clone().into();
    data["hasPassword"] = (!data["password"].as_str().unwrap_or_default().is_empty()).into();
    data["hasConfig"] = (!data["config"].as_str().unwrap_or_default().is_empty()).into();
    if data["family"] == "openVpn" {
        if let Ok(auth) = termterm_vpn_core::import::openvpn_authentication(data["config"].as_str().unwrap_or_default()) { data["authentication"] = json!(auth); }
    }
    data.as_object_mut().unwrap().remove("password");
    data.as_object_mut().unwrap().remove("config");
    data
}

#[tauri::command]
pub async fn vpn_status(app: AppHandle, state: State<'_, Shared>) -> Api<Value> {
    let _gate = state.vpn.gate.lock().await;
    authorize(&app, &state, None).await.map_err(error)?;
    // Windows status performs only immutable bundle reads, never client creation.
    // Do not hold profile mutation / vault shutdown behind a cold debug hash scan.
    #[cfg(target_os = "windows")]
    drop(_gate);
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let result = client(&app, &state).await.map(|_| ());
    #[cfg(target_os = "windows")]
    let inventory = desktop_bundle().await;
    #[cfg(target_os = "windows")]
    let bundle = match &inventory {
        Ok(count) => json!({"complete":true,"filesChecked":count}),
        Err(error) => json!({"complete":false,"reason":error.to_string()}),
    };
    #[cfg(not(target_os = "windows"))]
    let bundle = Value::Null;
    #[cfg(target_os = "windows")]
    let result: Result<()> = {
        let readiness = termterm_vpn_windows_broker::windows::readiness();
        if readiness.available {
            inventory.map(|_| ())
        } else {
            Err(anyhow!(readiness.reason))
        }
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let result: Result<()> = Err(anyhow!(
        "Bu platformda VPN Ã§alÄ±ÅŸma zamanÄ± henÃ¼z doÄŸrulanmadÄ±"
    ));
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let lab_only = state.vpn.client.lock().await.as_ref().is_none_or(|client|client.lab_only);
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let lab_only = !cfg!(feature = "vpn-candidate");
    Ok(
        json!({"available":result.is_ok(),"platform":std::env::consts::OS,"reason":result.err().map(|e|e.to_string()).unwrap_or_default(),"bundle":bundle,"labOnly":lab_only,"maxConnections":3,"supportedFamilies":["openVpn","wireGuard","fortinet","globalProtect"],"authTestAvailable":cfg!(all(windows,feature="e2e",debug_assertions))}),
    )
}

#[tauri::command]
pub fn vpn_profiles(app: AppHandle, state: State<'_, Shared>) -> Api<Vec<Value>> {
    let vault_id = personal(&app, &state).map_err(error)?;
    let guard = state.vault.lock().map_err(|_| "Vault lock")?;
    let vault = guard.as_ref().ok_or("Vault locked")?;
    if vault.id != vault_id { return Err("Vault changed".into()); }
    profiles_raw(vault)
        .map(|records| records.iter().map(view).collect())
        .map_err(error)
}

fn read_import_preview(path: &std::path::Path, app_identifier: &str) -> Result<(Family, termterm_vpn_core::import::Preview, Option<Value>)> {
    let text = Zeroizing::new(std::fs::read_to_string(path)?);
    let (family, preview) = termterm_vpn_core::import::configuration(
        &text,
        path.parent().context("VPN import directory unavailable")?,
    )?;
    let diagnostics = (app_identifier == DEV_ID).then(|| {
        use sha2::Digest;
        json!({"sha256":hex::encode(sha2::Sha256::digest(text.as_bytes())),"byteLength":text.len()})
    });
    Ok((family, preview, diagnostics))
}

#[tauri::command]
pub async fn vpn_import(app: AppHandle, state: State<'_, Shared>, path: String, team_context: Option<String>) -> Api<Value> {
    let _gate = state.vpn.gate.lock().await;
    let vault_id = authorize(&app, &state, team_context.as_deref()).await.map_err(error)?;
    ensure_team_context(&app, team_context.as_deref()).map_err(error)?;
    let path = std::path::Path::new(&path);
    let result = (|| -> Result<Value> {
        ensure!(
            path.extension()
                .is_some_and(|v| v.eq_ignore_ascii_case("ovpn") || v.eq_ignore_ascii_case("conf")),
            "Bir .ovpn dosyasÄ± seÃ§in"
        );
        ensure!(
            std::fs::metadata(path)?.len() <= 1024 * 1024,
            "VPN dosyasÄ± 1 MiB sÄ±nÄ±rÄ±nÄ± aÅŸÄ±yor"
        );
        let (family, preview, diagnostics) = read_import_preview(path, &app.config().identifier)?;
        let authentication = if family == Family::OpenVpn {Some(termterm_vpn_core::import::openvpn_authentication(&preview.config)?)} else {None};
        let token = Uuid::new_v4().to_string();
        if !preview.blocked && preview.missing_files.is_empty() {
            if family == Family::OpenVpn { termterm_vpn_core::import::openvpn_embedded(&preview.config)?; }
            ensure!(dev(&app, &state)? == vault_id, "VPN owner changed");
            let mut imports = state
                .vpn
                .imports
                .lock()
                .map_err(|_| anyhow!("VPN import lock"))?;
            ensure!(!state.vpn.closing.load(Ordering::SeqCst), "Vault closing");
            ensure!(
                imports.len() < 16,
                "En Ã§ok 16 bekleyen import; kasayÄ± kilitleyerek temizleyin"
            );
            imports.insert(token.clone(), (vault_id, Zeroizing::new(preview.config)));
        }
        let mut result = json!({"token":token,"family":family,"warnings":preview.warnings,"missingFiles":preview.missing_files,"blocked":preview.blocked});
        if let Some(diagnostics) = diagnostics { result["diagnostics"] = diagnostics; }
        if let Some(authentication) = authentication { result["authentication"] = json!(authentication); }
        Ok(result)
    })();
    result.map_err(error)
}

#[tauri::command]
pub async fn vpn_profile_save(
    app: AppHandle,
    state: State<'_, Shared>,
    profile: Value,
    import_token: Option<String>,
    team_context: Option<String>,
    revision: Option<i64>,
) -> Api<Value> {
    let _gate = state.vpn.gate.lock().await;
    let vault_id = authorize(&app, &state, team_context.as_deref()).await.map_err(error)?;
    if let Some(context) = team_context.as_deref() {
        let mut data = json!({"version":1,"label":"","family":"openVpn","source":"manual","server":"","groupId":null,"username":"","password":"","realm":"","config":"","remember":false});
        for key in ["version","label","family","source","server","groupId","username","password","realm","remember"] {
            if let Some(value) = profile.get(key) { data[key] = value.clone(); }
        }
        if let Some(token) = &import_token {
            let imports = state.vpn.imports.lock().map_err(|_| "VPN import lock")?;
            let (owner, text) = imports.get(token).ok_or("Import expired; choose the file again")?;
            if owner != &vault_id { return Err("VPN import owner changed".into()); }
            let (family, _) = termterm_vpn_core::import::configuration(text,std::path::Path::new(".")).map_err(error)?;
            if data["family"] != serde_json::to_value(family).map_err(|e|e.to_string())? { return Err("VPN import family mismatch".into()); }
            data["config"] = text.to_string().into();
        }
        let id = profile["id"].as_str().filter(|id| !id.is_empty()).map(Uuid::parse_str).transpose().map_err(|e|e.to_string())?;
        if let Some(id) = id { ensure_idle(&state, &id.to_string()).map_err(error)?; }
        let parsed: Profile = serde_json::from_value(data).map_err(|e|e.to_string())?;
        let result = app.state::<termterm_team_client::desktop::DesktopState>().save_vpn(context, id, revision.or_else(||profile["revision"].as_i64()).unwrap_or(0), parsed).await.map_err(error)?;
        if let Some(token) = &import_token { state.vpn.imports.lock().map_err(|_| "VPN import lock")?.remove(token); }
        return Ok(result);
    }
    ensure_team_context(&app, None).map_err(error)?;
    (|| -> Result<Value> {
        let mut guard = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        let vault = guard.as_mut().context("Vault locked")?;
        ensure!(vault.id == vault_id, "Vault changed");
        let id = profile["id"].as_str().filter(|s| !s.is_empty()).map(str::to_owned).unwrap_or_else(|| Uuid::new_v4().to_string());
        ensure!(Uuid::parse_str(&id).is_ok(), "Invalid profile id");
        ensure!(!state.vpn.sessions.lock().map_err(|_| anyhow!("VPN lock"))?.values().any(|s|s["profileId"] == id && !terminal(s)), "Ã–nce VPN baÄŸlantÄ±sÄ±nÄ± kapatÄ±n");
        let old = profiles_raw(vault)?.into_iter().find(|r| r.id == id);
        let mut data = old.as_ref().map(|r| r.data.clone()).unwrap_or_else(||json!({"version":1,"label":"","family":"openVpn","source":"manual","server":"","groupId":null,"username":"","password":"","realm":"","config":"","remember":true}));
        let fields = profile.as_object().context("Invalid VPN profile")?;
        for (key, value) in fields {
            ensure!(["id","version","label","family","source","server","groupId","username","password","realm","remember","hasConfig","hasPassword"].contains(&key.as_str()), "Unsupported VPN field: {key}");
            if !["id","hasConfig","hasPassword"].contains(&key.as_str()) { data[key] = value.clone(); }
        }
        if old.as_ref().is_some_and(|r|r.data["family"] != data["family"]) { data["config"] = "".into(); }
        if let Some(token) = &import_token {
            let imports = state.vpn.imports.lock().map_err(|_| anyhow!("VPN import lock"))?;
            let (owner, text) = imports.get(token).context("Import sÃ¼resi doldu; dosyayÄ± yeniden seÃ§in")?;
            let (family, _) = termterm_vpn_core::import::configuration(text, std::path::Path::new("."))?;
            ensure!(*owner == vault_id && data["family"] == serde_json::to_value(family)?, "Invalid import owner/family");
            data["config"] = text.to_string().into();
        }
        if data["remember"] != true { data["password"] = "".into(); }
        let parsed: Profile = serde_json::from_value(data.clone())?;
        parsed.validate()?;
        ensure!(matches!(parsed.family, Family::OpenVpn | Family::WireGuard | Family::Fortinet | Family::GlobalProtect), "Unsupported VPN family");
        if parsed.family == Family::OpenVpn { termterm_vpn_core::import::openvpn_embedded(&parsed.config)?; }
        if parsed.family == Family::WireGuard { let (family, preview) = termterm_vpn_core::import::configuration(&parsed.config, std::path::Path::new("."))?; ensure!(family == Family::WireGuard && !preview.blocked, "Desteklenmeyen WireGuard ayarı"); }
        let record = Record { id, kind: "vpn".into(), data, updated_at: chrono::Utc::now().timestamp_millis() };
        restore_profiles(vault, std::slice::from_ref(&record))?;
        if let Some(token) = import_token { state.vpn.imports.lock().map_err(|_|anyhow!("VPN import lock"))?.remove(&token); }
        Ok(view(&record))
    })().map_err(error)
}

#[tauri::command]
pub async fn vpn_profile_delete(app: AppHandle, state: State<'_, Shared>, id: String, team_context: Option<String>, revision: Option<i64>) -> Api<()> {
    let _gate = state.vpn.gate.lock().await;
    let vault_id = authorize(&app, &state, team_context.as_deref()).await.map_err(error)?;
    ensure_idle(&state, &id).map_err(error)?;
    if let Some(context) = team_context {
        use termterm_team_client::{desktop::DesktopAction, vpn::Action};
        app.state::<termterm_team_client::desktop::DesktopState>().call(DesktopAction::Vpn { context: Some(context), action: Action::Delete { id: Uuid::parse_str(&id).map_err(|e|e.to_string())?, revision: revision.ok_or("VPN revision required")? } }).await.map_err(error)?;
        return Ok(());
    }
    ensure_team_context(&app, None).map_err(error)?;
    let guard = state.vault.lock().map_err(|_| "Vault lock")?;
    let vault = guard.as_ref().ok_or("Vault locked")?;
    if vault.id != vault_id {
        return Err("Vault changed".into());
    }
    if !profiles_raw(vault)
        .map_err(error)?
        .iter()
        .any(|r| r.id == id)
    {
        return Err("VPN bulunamadÄ±".into());
    }
    vault
        .conn
        .execute("DELETE FROM vpn_profiles WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn terminal(session: &Value) -> bool {
    matches!(session["state"].as_str(), Some("failed" | "disconnected"))
}
pub(super) fn ensure_idle(state: &Shared, profile_id: &str) -> Result<()> {
    ensure!(
        !state
            .vpn
            .sessions
            .lock()
            .map_err(|_| anyhow!("VPN lock"))?
            .values()
            .any(|s| s["profileId"] == profile_id && !terminal(s)),
        "Ã–nce VPN baÄŸlantÄ±sÄ±nÄ± kapatÄ±n"
    );
    Ok(())
}
#[tauri::command]
pub fn vpn_sessions(app: AppHandle, state: State<'_, Shared>) -> Api<Vec<Value>> {
    let vault_id = dev(&app, &state).map_err(error)?;
    Ok(state
        .vpn
        .sessions
        .lock()
        .map_err(|_| "VPN lock")?
        .values()
        .filter(|s| s["vaultId"] == vault_id)
        .map(|session| public_session(&state, session.clone()))
        .collect())
}

#[tauri::command]
pub async fn vpn_connect(
    app: AppHandle,
    state: State<'_, Shared>,
    profile_id: String,
    team_context: Option<String>,
) -> Api<Value> {
    let _gate = state.vpn.gate.lock().await;
    let vault_id = authorize(&app, &state, team_context.as_deref()).await.map_err(error)?;
    ensure_team_context(&app, team_context.as_deref()).map_err(error)?;
    ensure_idle(&state, &profile_id).map_err(error)?;
    let profile = if team_context.is_some() {
        let desktop = app.state::<termterm_team_client::desktop::DesktopState>();
        let (_, profile) = desktop.vpn_material(Uuid::parse_str(&profile_id).map_err(|e|e.to_string())?, desktop.current_epoch()).await.map_err(error)?;
        serde_json::to_value(profile).map_err(|e|e.to_string())?
    } else {
        let vault = state.vault.lock().map_err(|_| "Vault lock")?;
        profiles_raw(vault.as_ref().ok_or("Vault locked")?)
            .map_err(error)?
            .into_iter()
            .find(|r| r.id == profile_id)
            .ok_or("VPN bulunamadÄ±")?
            .data
    };
    let id = Uuid::new_v4().to_string();
    let generation = state.vpn.generation.fetch_add(1, Ordering::SeqCst) + 1;
    // Cached credentials are answered inside the broker, before a public challenge is emitted.
    if profile["remember"] == true && profile["password"].as_str().is_some_and(|p| !p.is_empty()) {
        state.vpn.credential_attempts.lock().map_err(|_| "VPN lock")?.insert(id.clone(), generation);
    }
    if team_context.is_some() { state.vpn.team_remember.lock().map_err(|_| "VPN lock")?.insert(id.clone(), profile["remember"] == true); }
    let result = rpc(&app, &state, json!({"action":"connect","id":id,"generation":generation,"profileId":profile_id,"profile":profile})).await.map_err(error)?;
    if dev(&app, &state).map_err(error)? != vault_id { return Err("VPN owner changed".into()); }
    let reply=cache_connect_reply(&state,id.clone(),result).map_err(error)?;
    if team_context.is_some() {
        let epoch=app.state::<termterm_team_client::desktop::DesktopState>().current_epoch();
        watch_team(app,state.inner().clone(),id,generation,Uuid::parse_str(&profile_id).map_err(|e|e.to_string())?,epoch).await.map_err(error)?;
    }
    Ok(reply)
}
async fn watch_team(app: AppHandle, state: Shared, id: String, generation: u64, profile: Uuid, epoch: u64) -> Result<()> {
    let tx=state.vpn.client.lock().await.as_ref().context("VPN broker unavailable")?.tx.clone();
    let shared=state.clone();
    let identity=id.clone();
    let task=tokio::spawn(async move {
        let desktop=app.state::<termterm_team_client::desktop::DesktopState>();
        loop {
            let cancelled=tokio::select! { _=desktop.cancelled(epoch)=>true, _=tokio::time::sleep(std::time::Duration::from_secs(10))=>false };
            if shared.vpn.closing.load(Ordering::SeqCst) || !shared.vpn.sessions.lock().ok().is_some_and(|sessions|sessions.get(&identity).is_some_and(|session|session["generation"]==generation && !terminal(session))) {break;}
            if !cancelled && desktop.check_vpn(profile,epoch).await.is_ok() {continue;}
            let _=invalidate_lease(&shared,&identity,generation);
            if let Ok(mut sessions)=shared.vpn.sessions.lock() {
                if let Some(session)=sessions.get_mut(&identity).filter(|session|session["generation"]==generation) {
                    session["state"]="disconnecting".into();
                    session["error"]="Team VPN authorization changed; dependent connections are closing".into();
                    let _=app.emit("vpn-event",public_session(&shared,session.clone()));
                }
            }
            if crate::vpn_lifecycle::quiesce(&shared,&identity,generation).await.is_err() {break;}
            // Keep the original account's transport; a later account must never receive this stop.
            let (send,_)=tokio::sync::oneshot::channel();
            let _=tx.send((json!({"action":"disconnect","id":identity,"generation":generation}),send)).await;
            break;
        }
    });
    let mut watchers=state.vpn.team_watchers.lock().map_err(|_|anyhow!("VPN watcher lock"))?;
    watchers.retain(|_,task|!task.is_finished());
    if let Some(previous)=watchers.insert(id,task) {previous.abort();}
    Ok(())
}
pub(super) fn cache_connect_reply(state: &Shared,id: String,mut result: Value) -> Result<Value> {
    if let Some(challenge) = result.get_mut("challenge").and_then(Value::as_object_mut) { challenge.remove("private"); }
    let mut sessions = state.vpn.sessions.lock().map_err(|_| anyhow!("VPN lock"))?;
    Ok(public_session(state, sessions.entry(id).or_insert(result).clone()))
}
pub(super) fn public_session(state: &Shared, mut session: Value) -> Value {
    if let Ok(owner) = state.vpn.owner.lock() {
        if let Some(owner) = owner.as_ref().filter(|owner| session["vaultId"] == owner.id) {
            session["vaultId"] = owner.context.clone().into();
        }
    }
    session
}
#[tauri::command]
pub async fn vpn_disconnect(
    app: AppHandle,
    state: State<'_, Shared>,
    id: String,
    generation: u64,
) -> Api<()> {
    let _gate = state.vpn.gate.lock().await;
    dev(&app, &state).map_err(error)?;
    #[cfg(all(windows,feature="e2e",debug_assertions))]
    if let Some(result) = crate::vpn_sso::auth_only::request(&state,&json!({"action":"disconnect","id":id,"generation":generation})).await {
        result.map_err(error)?;
        crate::vpn_sso::cancel(&state, &id, generation, None);
        return finish_disconnect(&state,&id,_gate).await;
    }
    scoped(&state, &id, generation).map_err(error)?;
    crate::vpn_sso::cancel(&state, &id, generation, None);
    rpc(
        &app,
        &state,
        json!({"action":"disconnect","id":id,"generation":generation}),
    )
    .await
    .map_err(error)?;
    finish_disconnect(&state,&id,_gate).await
}
async fn finish_disconnect(state: &Shared,id: &str,gate: tokio::sync::MutexGuard<'_,()>) -> Api<()> {
    drop(gate);
    for _ in 0..120 {
        if state
            .vpn
            .sessions
            .lock()
            .map_err(|_| "VPN lock")?
            .get(id)
            .is_some_and(terminal)
        {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err("VPN kapatma işlemi için bekleme süresi doldu; temizliğin tamamlandığı doğrulanamadı.".into())
}
pub(super) fn scoped(state: &Shared, id: &str, generation: u64) -> Result<()> {
    let vault_id = owner_id(state)?;
    let sessions = state.vpn.sessions.lock().map_err(|_| anyhow!("VPN lock"))?;
    let session = sessions.get(id).context("VPN session not found")?;
    ensure!(
        session["vaultId"] == vault_id && session["generation"] == generation && !terminal(session),
        "Stale VPN session"
    );
    Ok(())
}
pub(super) fn owner_id(state: &Shared) -> Result<String> {
    if let Some(owner) = state.vpn.owner.lock().map_err(|_| anyhow!("VPN owner lock"))?.as_ref() { return Ok(owner.id.clone()); }
    Ok(state
        .vault
        .lock()
        .map_err(|_| anyhow!("Vault lock"))?
        .as_ref()
        .context("Vault locked")?
        .id
        .clone())
}
#[tauri::command]
pub async fn vpn_answer(
    app: AppHandle,
    state: State<'_, Shared>,
    id: String,
    generation: u64,
    challenge_id: u64,
    values: Value,
) -> Api<()> {
    let _gate = state.vpn.gate.lock().await;
    dev(&app, &state).map_err(error)?;
    scoped(&state, &id, generation).map_err(error)?;
    let session = state.vpn.sessions.lock().map_err(|_| "VPN lock")?.get(&id).cloned().ok_or("VPN session not found")?;
    if session["challenge"]["id"] != challenge_id { return Err("Stale VPN challenge".into()); }
    let credentials = first_credential_answer(&mut *state.vpn.credential_attempts.lock().map_err(|_| "VPN lock")?, &id, generation, &session["challenge"], &values);
    rpc(&app, &state, json!({"action":"answer","id":id,"generation":generation,"challengeId":challenge_id,"values":values})).await.map_err(error)?;
    if let Some((username, password)) = credentials {
        let team_owner = state.vpn.owner.lock().map_err(|_| "VPN owner lock")?.clone();
        if let Some(owner) = team_owner {
            if state.vpn.team_remember.lock().map_err(|_| "VPN lock")?.get(&id) == Some(&true) {
                use termterm_team_client::{desktop::DesktopAction, vpn::{Action, Authentication}};
                app.state::<termterm_team_client::desktop::DesktopState>().call(DesktopAction::Vpn { context: Some(owner.context), action: Action::Authenticate { id: Uuid::parse_str(session["profileId"].as_str().ok_or("Invalid VPN profile")?).map_err(|e|e.to_string())?, authentication: Authentication { username: username.to_string(), password: password.to_string(), remember: true } } }).await.map_err(error)?;
            }
            return Ok(());
        }
        let mut guard = state.vault.lock().map_err(|_| "Vault lock")?;
        let vault = guard.as_mut().ok_or("Vault locked")?;
        if session["vaultId"] != vault.id { return Err("Vault changed".into()); }
        save_answer_credentials(vault, &session["profileId"], &username, &password).map_err(error)?;
    }
    Ok(())
}
fn save_answer_credentials(vault: &mut Vault, profile_id: &Value, username: &str, password: &str) -> Result<()> {
    if let Some(mut record) = profiles_raw(vault)?.into_iter().find(|r| *profile_id == r.id && r.data["remember"] == true) {
        record.data["username"] = username.into();
        record.data["password"] = password.into();
        record.updated_at = chrono::Utc::now().timestamp_millis();
        restore_profiles(vault, &[record])?;
    }
    Ok(())
}

fn first_credential_answer(attempts: &mut HashMap<String, u64>, id: &str, generation: u64, challenge: &Value, values: &Value) -> Option<(Zeroizing<String>, Zeroizing<String>)> {
    if termterm_vpn_core::private_key_challenge(challenge) { return None; }
    let fields = challenge["fields"].as_array()?;
    if !fields.iter().any(|f| f["secret"] == true || matches!(f["purpose"].as_str(), Some("secret" | "password" | "otp" | "keyPassword"))) { return None; }
    if attempts.get(id) == Some(&generation) { return None; }
    // Consume even a rejected/OTP form or opted-out attempt: later forms cannot replace a password.
    attempts.insert(id.to_owned(), generation);
    termterm_vpn_core::password_challenge_fields(challenge)?;
    remembered_answer(&challenge["fields"], values)
}
fn remembered_answer(fields: &Value, values: &Value) -> Option<(Zeroizing<String>, Zeroizing<String>)> {
    let (user, password) = termterm_vpn_core::password_fields(fields)?;
    let username = values[user].as_str()?;
    let password = values[password].as_str()?;
    if username.len() > 4096 || password.is_empty() || password.len() > 4096 { return None; }
    Some((Zeroizing::new(username.to_owned()), Zeroizing::new(password.to_owned())))
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
struct BrokerClient {
    tx: tokio::sync::mpsc::Sender<(Value, tokio::sync::oneshot::Sender<Result<Value>>)>,
    task: tokio::task::JoinHandle<()>,
    lab_only: bool,
}
#[cfg(target_os = "windows")]
async fn desktop_bundle() -> Result<usize> {
    let executable = std::env::current_exe()?;
    let directory = executable.parent().context("VPN helper directory unavailable")?.to_path_buf();
    tokio::task::spawn_blocking(move || crate::vpn_bundle::inspect(&directory)).await?
}
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
trait BrokerTransport: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> BrokerTransport for T {}
#[cfg(target_os = "linux")]
fn require_same_lab_namespace(hello: &Value) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let namespace = std::fs::metadata("/proc/self/ns/net")?;
    ensure!(
        hello["result"]["networkNamespace"]["device"] == namespace.dev()
            && hello["result"]["networkNamespace"]["inode"] == namespace.ino(),
        "VPN uygulamasÄ± ve laboratuvar servisi aynÄ± izole aÄŸ namespace'inde Ã§alÄ±ÅŸmalÄ±"
    );
    Ok(())
}
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
async fn client(app: &AppHandle, state: &Shared) -> Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    use std::os::unix::fs::MetadataExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut current = state.vpn.client.lock().await;
    if current.as_ref().is_some_and(|c| !c.task.is_finished()) {
        return Ok(());
    }
    let vault_id = dev(app, state)?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let mut stream: Box<dyn BrokerTransport> = {
        let path = if cfg!(feature = "vpn-candidate") {
            #[cfg(target_os = "linux")] let path = "/run/termterm-vpn/broker.sock";
            #[cfg(target_os = "macos")] let path = "/var/run/termterm-vpn/broker.sock";
            std::ffi::OsString::from(path)
        } else { std::env::var_os("TERMTERM_VPN_LAB_SOCKET").unwrap_or_else(|| {
            #[cfg(target_os = "linux")] let path = "/run/termterm-vpn-dev/broker.sock";
            #[cfg(target_os = "macos")] let path = "/var/run/termterm-vpn-dev/broker.sock";
            path.into()
        }) };
        let path = std::path::Path::new(&path);
        ensure!(path.is_absolute(), "VPN broker path must be absolute");
        let parent = path.parent().context("Invalid broker path")?.metadata()?;
        ensure!(
            parent.uid() == 0 && parent.mode() & 0o022 == 0,
            "VPN socket directory must be root owned/private"
        );
        let stream = tokio::net::UnixStream::connect(path).await?;
        ensure!(
            stream.peer_cred()?.uid() == 0,
            "VPN broker must run as root"
        );
        Box::new(stream)
    };
    #[cfg(target_os = "windows")]
    let mut stream: Box<dyn BrokerTransport> = {
        let readiness = termterm_vpn_windows_broker::windows::readiness();
        ensure!(readiness.available, "{}", readiness.reason);
        desktop_bundle().await?;
        let executable = std::env::current_exe()?;
        let helper = executable
            .parent()
            .context("VPN helper directory unavailable")?
            .join("termterm-vpn-broker.exe");
        Box::new(termterm_vpn_windows_broker::windows::connect(&helper).await?)
    };
    stream
        .write_all(
            format!(
                "{}\n",
                json!({"action":"hello","requestId":0,"vaultId":vault_id})
            )
            .as_bytes(),
        )
        .await?;
    let mut initial = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let byte = stream.read_u8().await?;
            initial.push(byte);
            ensure!(initial.len() <= 4096, "Invalid broker handshake");
            if byte == b'\n' {
                break;
            }
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    let hello: Value = serde_json::from_slice(&initial)?;
    ensure!(
        broker_hello(&hello),
        "Unsupported VPN broker"
    );
    #[cfg(target_os = "linux")]
    if hello["result"]["labOnly"] == true { require_same_lab_namespace(&hello)?; }
    #[cfg(target_os = "macos")]
    ensure!(hello["result"]["labOnly"] == false, "macOS requires an authenticated native broker");
    let lease_capable = hello["result"]["version"] == 2 && hello["result"]["networkLease"] == true;
    let lab_only = hello["result"]["labOnly"] == true;
    let (tx, mut rx) =
        tokio::sync::mpsc::channel::<(Value, tokio::sync::oneshot::Sender<Result<Value>>)>(32);
    let app = app.clone();
    let shared = state.clone();
    let task = tokio::spawn(async move {
        let mut pending = HashMap::new();
        let mut next = 1u64;
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 8192];
        let outcome: Result<()> = async {
            loop {
                tokio::select! {
                    command = rx.recv() => {
                        let Some((mut value, reply)) = command else { break; };
                        value["requestId"] = next.into();
                        pending.insert(next, reply);
                        next += 1;
                        let wire = Zeroizing::new(format!("{value}\n"));
                        stream.write_all(wire.as_bytes()).await?;
                    }
                    read = stream.read(&mut buffer) => {
                        let count = read?;
                        if count == 0 { break; }
                        bytes.extend_from_slice(&buffer[..count]);
                        ensure!(bytes.len() <= 2*1024*1024, "VPN event too large");
                        while let Some(end) = bytes.iter().position(|b|*b == b'\n') {
                            let value: Value = serde_json::from_slice(&bytes[..end])?;
                            bytes.drain(..=end);
                            if value["kind"] == "reply" {
                                if let Some(reply) = value["requestId"].as_u64().and_then(|id|pending.remove(&id)) {
                                    let result = if value["ok"] == true { Ok(value["result"].clone()) } else { Err(anyhow!(value["error"].as_str().unwrap_or("VPN broker error").to_owned())) };
                                    let _ = reply.send(result);
                                }
                            } else if value["kind"] == "networkLease" {
                                ensure!(lease_capable, "Unexpected VPN network lease from an old broker");
                                let mut lease: BoundVpn = serde_json::from_value(value["lease"].clone())?;
                                ensure!(value["id"] == lease.id && value["generation"] == lease.generation, "VPN lease generation mismatch");
                                cache_lease(&shared,&vault_id,&mut lease)?;
                            } else if value["kind"] == "quiesceRequired" {
                                ensure!(lease_capable, "Unexpected VPN quiesce request");
                                let id = value["id"].as_str().context("Invalid VPN quiesce id")?;
                                let generation = value["generation"].as_u64().context("Invalid VPN quiesce generation")?;
                                invalidate_lease(&shared,id,generation)?;
                                crate::vpn_lifecycle::quiesce(&shared,id,generation).await?;
                                stream.write_all(format!("{}\n",json!({"action":"quiesced","id":id,"generation":generation,"requestId":next})).as_bytes()).await?;
                                next += 1;
                            } else if value["kind"] == "session" {
                                let mut session = value["session"].clone();
                                ensure!(session["vaultId"] == vault_id, "VPN event vault mismatch");
                                if matches!(session["state"].as_str(),Some("disconnecting" | "failed" | "disconnected")) {
                                    let id=session["id"].as_str().context("Invalid VPN session")?;
                                    let generation=session["generation"].as_u64().context("Invalid VPN generation")?;
                                    invalidate_lease(&shared,id,generation)?;
                                    crate::vpn_lifecycle::quiesce(&shared,id,generation).await?;
                                }
                                if let Err(error) = crate::vpn_sso::session(&app, &shared, &mut session) {
                                    let id = session["id"].as_str().context("Invalid VPN session")?.to_owned();
                                    let generation = session["generation"].as_u64().context("Invalid VPN generation")?;
                                    crate::vpn_sso::cancel(&shared,&id,generation,None);
                                    session["state"] = "failed".into();
                                    session["error"] = error.to_string().into();
                                    session.as_object_mut().unwrap().remove("challenge");
                                    let wire = format!("{}\n",json!({"action":"disconnect","id":id,"generation":generation,"requestId":next}));
                                    next += 1;
                                    stream.write_all(wire.as_bytes()).await?;
                                }
                                if let Some(id) = session["id"].as_str() {
                                    let id = id.to_owned();
                                    let mut sessions = shared.vpn.sessions.lock().map_err(|_|anyhow!("VPN lock"))?;
                                    retain_failure(sessions.get(&id),&mut session);
                                    sessions.insert(id, session.clone());
                                    let _ = app.emit("vpn-event", public_session(&shared, session));
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        }.await;
        let message = outcome
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "VPN broker disconnected".into());
        crate::vpn_sso::clear(&shared);
        let leases = shared.vpn.leases.lock().map(|leases|leases.values().filter(|lease|lease.owner_scope == vault_id).cloned().collect::<Vec<_>>()).unwrap_or_default();
        for lease in leases {
            let _=invalidate_lease(&shared,&lease.id,lease.generation);
            let _=crate::vpn_lifecycle::quiesce(&shared,&lease.id,lease.generation).await;
        }
        for (_, reply) in pending {
            let _ = reply.send(Err(anyhow!(message.clone())));
        }
        if let Ok(mut sessions) = shared.vpn.sessions.lock() {
            for session in sessions
                .values_mut()
                .filter(|s| s["vaultId"] == vault_id && !terminal(s))
            {
                session["state"] = "failed".into();
                session["error"] = message.clone().into();
                let _ = app.emit("vpn-event", public_session(&shared, session.clone()));
            }
        };
    });
    *current = Some(BrokerClient { tx, task, lab_only });
    Ok(())
}

pub(super) fn retain_failure(previous: Option<&Value>, incoming: &mut Value) {
    if let Some(previous) = previous.filter(|previous| previous["state"] == "failed"
        && previous["generation"] == incoming["generation"] && previous["vaultId"] == incoming["vaultId"]
        && matches!(incoming["state"].as_str(),Some("disconnecting" | "disconnected" | "failed"))
        && previous["error"].as_str().is_some_and(|error|!error.is_empty())) {
        incoming["state"] = "failed".into();
        incoming["error"] = previous["error"].clone();
        incoming.as_object_mut().unwrap().remove("authenticationVerified");
    }
}
fn invalidate_lease(state: &Shared, id: &str, generation: u64) -> Result<()> {
    let mut revoked=state.vpn.revoked_leases.lock().map_err(|_|anyhow!("VPN lease lock"))?;
    let previous=revoked.entry(id.to_owned()).or_default();
    *previous=(*previous).max(generation);
    let mut leases=state.vpn.leases.lock().map_err(|_|anyhow!("VPN lease lock"))?;
    if leases.get(id).is_some_and(|lease|lease.generation <= generation) {leases.remove(id);}
    Ok(())
}
fn cache_lease(state: &Shared, owner: &str, lease: &mut BoundVpn) -> Result<()> {
    lease.validate()?;
    lease.owner_scope=owner.to_owned();
    lease.team_epoch=state.vpn.owner.lock().map_err(|_|anyhow!("VPN owner lock"))?.as_ref().map(|owner|owner.epoch);
    let revoked=state.vpn.revoked_leases.lock().map_err(|_|anyhow!("VPN lease lock"))?;
    if !revoked.get(&lease.id).is_some_and(|generation|*generation>=lease.generation) {
        state.vpn.leases.lock().map_err(|_|anyhow!("VPN lease lock"))?.insert(lease.id.clone(),lease.clone());
    }
    Ok(())
}

fn request_timeout(action:Option<&str>)->std::time::Duration {
    // Cleanup includes durable undo of up to 1,024 routes and asynchronous Windows removal.
    std::time::Duration::from_secs(match action {Some("disconnect")=>60,Some("close")=>185,_=>10})
}
pub(super) async fn rpc(app: &AppHandle, state: &Shared, request: Value) -> Result<Value> {
    #[cfg(all(windows,feature="e2e",debug_assertions))]
    if let Some(result) = crate::vpn_sso::auth_only::request(state,&request).await { return result; }
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        client(app, state).await?;
        let tx = state
            .vpn
            .client
            .lock()
            .await
            .as_ref()
            .context("VPN broker unavailable")?
            .tx
            .clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        let timeout=request_timeout(request["action"].as_str());
        tx.send((request, send))
            .await
            .map_err(|_| anyhow!("VPN broker closed"))?;
        tokio::time::timeout(timeout, receive).await??
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (app, state, request);
        Err(anyhow!(
            "Windows VPN yardÄ±mcÄ± servisi ve imzalÄ± TUN sÃ¼rÃ¼cÃ¼sÃ¼ henÃ¼z bu pakette yok"
        ))
    }
}

pub async fn close_all(state: &Shared) -> Result<()> {
    let _gate = state.vpn.gate.lock().await;
    state.vpn.closing.store(true, Ordering::SeqCst);
    for (_,task) in state.vpn.team_watchers.lock().map_err(|_|anyhow!("VPN watcher lock"))?.drain() {task.abort();}
    let leases=state.vpn.leases.lock().map_err(|_|anyhow!("VPN lease lock"))?.values().cloned().collect::<Vec<_>>();
    for lease in leases {
        invalidate_lease(state,&lease.id,lease.generation)?;
        crate::vpn_lifecycle::quiesce(state,&lease.id,lease.generation).await?;
    }
    crate::vpn_sso::clear(state);
    state
        .vpn
        .imports
        .lock()
        .map_err(|_| anyhow!("VPN import lock"))?
        .clear();
    state.vpn.team_remember.lock().map_err(|_| anyhow!("VPN lock"))?.clear();
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    if let Some(mut client) = state.vpn.client.lock().await.take() {
        let (reply, _) = tokio::sync::oneshot::channel();
        let _ = client.tx.send((json!({"action":"close"}), reply)).await;
        if tokio::time::timeout(request_timeout(Some("close")), &mut client.task)
            .await
            .is_err()
        {
            client.task.abort();
            return Err(anyhow!("VPN broker cleanup timed out"));
        }
    }
    state
        .vpn
        .sessions
        .lock()
        .map_err(|_| anyhow!("VPN lock"))?
        .clear();
    *state.vpn.owner.lock().map_err(|_| anyhow!("VPN owner lock"))? = None;
    Ok(())
}

pub fn activate(state: &Shared) {
    if let Ok(mut attempts) = state.vpn.credential_attempts.lock() { attempts.clear(); }
    if let Ok(mut revoked) = state.vpn.revoked_leases.lock() { revoked.clear(); }
    state.vpn.closing.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    #[test]
    fn vpn_build_contract_rejects_other_versions_and_cross_mode_brokers() {
        use super::*;
        let candidate = cfg!(feature = "vpn-candidate");
        assert_eq!(enabled_application(DEV_ID, Some("0.4.3-dev.4")), !candidate);
        assert_eq!(enabled_application("local.termterm.desktop", Some("0.4.4")), candidate);
        assert!(!enabled_application("local.termterm.desktop", Some("0.4.2")));
        assert!(!enabled_application("local.termterm.desktop", Some("0.4.5")));
        let lab = json!({"ok":true,"result":{"version":2,"labOnly":true}});
        assert_eq!(broker_hello(&lab), !candidate);
        for (identity, accepted) in [("local.termterm.desktop.broker", candidate), ("local.termterm.desktop.vpndev.broker", !candidate), ("foreign.broker", false)] {
            let hello = json!({"ok":true,"result":{"version":2,"labOnly":false,"mode":"authenticatedNative","identity":identity}});
            assert_eq!(broker_hello(&hello), accepted);
        }
    }
    use super::*;
    #[test]
    fn personal_profile_view_exposes_authentication_hints_without_key_material() {
        let record = Record {id:Uuid::new_v4().to_string(),kind:"vpn".into(),updated_at:0,data:json!({"family":"openVpn","password":"Disposable account password","config":"client\n<cert>\npublic certificate fixture\n</cert>\n<key>\n-----BEGIN ENCRYPTED PRIVATE KEY-----\npublic key fixture\n</key>\n"})};
        let public = view(&record);
        assert_eq!(public["authentication"],json!({"certificate":true,"usernamePassword":false,"privateKeyPassword":true,"staticChallenge":false}));
        assert!(public.get("config").is_none() && public.get("password").is_none());
        assert!(!public.to_string().contains("public key fixture"));
    }
    #[test]
    fn import_diagnostics_fingerprint_the_same_raw_buffer_that_is_parsed_only_in_vpn_dev() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test.ovpn");
        std::fs::write(&path, "\u{feff}# tanı\r\nclient\r\nremote vpn.example 1194\r\n<ca>\r\nQUJD\r\n</ca>\r\nscript-security 2\r\n").unwrap();
        let (family, preview, diagnostics) = read_import_preview(&path, DEV_ID).unwrap();
        assert_eq!(family, Family::OpenVpn);
        assert!(preview.blocked);
        assert_eq!(preview.warnings, ["Satır 7: script-security desteklenmiyor; yürütülmedi"]);
        assert_eq!(preview.config, "client\nremote vpn.example 1194\n<ca>\nQUJD\n</ca>\n");
        assert_eq!(diagnostics, Some(json!({"sha256":"2c79ce148e98d550274138c01503301630f32678ca6e2fdacbb58e852560feaa","byteLength":83})));
        for identifier in ["local.termterm.desktop", "local.termterm.desktop.teamdev", ""] {
            assert!(read_import_preview(&path, identifier).unwrap().2.is_none());
        }
    }
    fn lease(generation:u64)->BoundVpn {
        BoundVpn {id:"00000000-0000-0000-0000-000000000001".into(),generation,interface_index:7,interface_name:Some("ttv0123456789ab".into()),interface_luid:None,interface_guid:None,address_families:vec!["ipv4".into()],addresses:vec!["10.97.0.2/32".into()],owner_scope:String::new(),team_epoch:None}
    }
    #[test]
    fn late_receipts_cannot_revive_quiesced_generations_or_remove_newer_receipts() {
        let state=std::sync::Arc::new(crate::state::AppState::default());
        let mut current=lease(2);
        cache_lease(&state,"owner",&mut current).unwrap();
        invalidate_lease(&state,&current.id,1).unwrap();
        assert_eq!(state.vpn.leases.lock().unwrap()[&current.id].generation,2);
        invalidate_lease(&state,&current.id,2).unwrap();
        cache_lease(&state,"owner",&mut current).unwrap();
        assert!(state.vpn.leases.lock().unwrap().is_empty());
        current.generation=3;
        cache_lease(&state,"new-owner",&mut current).unwrap();
        assert_eq!(state.vpn.leases.lock().unwrap()[&current.id].owner_scope,"new-owner");
        let mut invalid=current.clone();invalid.interface_index=0;assert!(cache_lease(&state,"new-owner",&mut invalid).is_err());
        invalid=current;invalid.addresses=vec!["10.0.0.1/99".into()];assert!(invalid.validate().is_err());
    }
    #[test]
    fn cleanup_has_a_separate_budget_without_extending_connection_or_authentication_requests() {
        use std::time::Duration;
        assert_eq!(request_timeout(Some("disconnect")),Duration::from_secs(60));
        assert_eq!(request_timeout(Some("close")),request_timeout(Some("disconnect"))*3+Duration::from_secs(5));
        for action in [None,Some("connect"),Some("answer"),Some("hello"),Some("status")] {assert_eq!(request_timeout(action),Duration::from_secs(10));}
    }
    #[tokio::test]
    async fn disconnect_wait_releases_the_gate_needed_by_its_terminal_reader() {
        let state=std::sync::Arc::new(crate::state::AppState::default());
        state.vpn.sessions.lock().unwrap().insert("fixture".into(),json!({"state":"disconnecting"}));
        let gate=state.vpn.gate.lock().await;
        let shared=state.clone();
        let reader=tokio::spawn(async move {
            let _gate=shared.vpn.gate.lock().await;
            shared.vpn.sessions.lock().unwrap().get_mut("fixture").unwrap()["state"]="disconnected".into();
        });
        tokio::time::timeout(std::time::Duration::from_secs(1),finish_disconnect(&state,"fixture",gate)).await.unwrap().unwrap();
        reader.await.unwrap();
    }
    #[test]
    fn rejected_session_keeps_its_reason_through_cleanup_without_affecting_other_sessions() {
        let failure = json!({"id":"rejected","generation":3,"vaultId":"fixture","state":"failed","error":"Embedded POST is unavailable"});
        let mut sessions = HashMap::from([("rejected",failure),("other",json!({"state":"connected"}))]);
        for state in ["disconnecting","disconnected"] {
            let mut incoming = json!({"id":"rejected","generation":3,"vaultId":"fixture","state":state,"authenticationVerified":true});
            retain_failure(sessions.get("rejected"),&mut incoming);
            assert_eq!(incoming["state"],"failed");
            assert_eq!(incoming["error"],"Embedded POST is unavailable");
            assert!(incoming.get("authenticationVerified").is_none());
            sessions.insert("rejected",incoming);
        }
        assert_eq!(sessions["other"]["state"],"connected");
        let mut fresh = json!({"generation":4,"vaultId":"fixture","state":"disconnected"});
        retain_failure(sessions.get("rejected"),&mut fresh);
        assert_eq!(fresh["state"],"disconnected");
        assert!(fresh.get("error").is_none());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_client_rejects_missing_or_other_broker_network_namespace() {
        use std::os::unix::fs::MetadataExt;
        let current = std::fs::metadata("/proc/self/ns/net").unwrap();
        let hello = json!({"result":{"networkNamespace":{"device":current.dev(),"inode":current.ino()}}});
        require_same_lab_namespace(&hello).unwrap();
        assert!(require_same_lab_namespace(&json!({"result":{}})).is_err());
        let mut other = hello;
        other["result"]["networkNamespace"]["inode"] = (current.ino() ^ 1).into();
        assert!(require_same_lab_namespace(&other).is_err());
    }
    #[tokio::test]
    async fn vault_close_clears_pending_imports_sessions_and_blocks_reconnect_until_activation() {
        let state = std::sync::Arc::new(crate::state::AppState::default());
        state.vpn.imports.lock().unwrap().insert(
            "token".into(),
            (
                "vault".into(),
                Zeroizing::new("private fixture import".into()),
            ),
        );
        state.vpn.sessions.lock().unwrap().insert(
            "session".into(),
            json!({"vaultId":"vault","state":"disconnected"}),
        );
        close_all(&state).await.unwrap();
        assert!(state.vpn.imports.lock().unwrap().is_empty());
        assert!(state.vpn.sessions.lock().unwrap().is_empty());
        assert!(state.vpn.closing.load(Ordering::SeqCst));
        activate(&state);
        assert!(!state.vpn.closing.load(Ordering::SeqCst));
    }
    #[test]
    fn encrypted_profiles_upgrade_only_vpn_vaults_and_do_not_leak_into_info() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("vpn.ttvault");
        let mut vault = Vault::create(&path, "test", "test-password").unwrap();
        let version = |v: &Vault| {
            v.conn
                .query_row("SELECT version FROM header", [], |r| r.get::<_, u32>(0))
                .unwrap()
        };
        assert_eq!(version(&vault), 1);
        assert!(profiles_raw(&vault).unwrap().is_empty());
        let ordinary = temp.path().join("ordinary.ttbackup");
        vault.backup(&ordinary, "test-password", true, &[]).unwrap();
        assert_eq!(
            crate::vault::read_backup(&ordinary, "test-password")
                .unwrap()
                .version,
            1
        );
        let record = Record::new(
            "vpn",
            json!({"version":1,"label":"Fixture","family":"fortinet","source":"manual","server":"https://vpn.example","groupId":null,"username":"alice","password":"private-vpn-password","realm":"","config":"","remember":true}),
        );
        restore_profiles(&mut vault, &[record.clone()]).unwrap();
        assert_eq!(version(&vault), 2);
        assert_ne!(
            version(&vault),
            crypto::FORMAT_VERSION,
            "v1 clients must reject this header before read/write"
        );
        assert!(vault.info().unwrap().records.is_empty());
        assert!(!serde_json::to_string(&view(&record))
            .unwrap()
            .contains("private-vpn-password"));
        assert!(view(&record).get("password").is_none());
        assert!(view(&record).get("config").is_none());
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes
            .windows(b"private-vpn-password".len())
            .any(|s| s == b"private-vpn-password"));
        drop(vault);
        let vault = Vault::open(&path, "test-password").unwrap();
        assert_eq!(profiles_raw(&vault).unwrap(), vec![record]);
        let backup = temp.path().join("vpn.ttbackup");
        vault.backup(&backup, "test-password", false, &[]).unwrap();
        let restored = crate::vault::read_backup(&backup, "test-password").unwrap();
        assert_eq!(restored.version, 2);
        assert_ne!(
            restored.version, 1,
            "v1 clients must reject the new backup payload"
        );
        assert_eq!(restored.vaults[0].vpn_profiles.len(), 1);
        let bundled = temp.path().join("vpn-bundle.ttbackup");
        crate::archive::bundle(&vault, &bundled, "test-password", vec![], &[], false, false)
            .unwrap();
        assert_eq!(
            crate::vault::read_backup(&bundled, "test-password")
                .unwrap()
                .vaults[0]
                .vpn_profiles,
            restored.vaults[0].vpn_profiles
        );
        let mut dest = Vault::create(
            &temp.path().join("restored.ttvault"),
            "restored",
            "test-password",
        )
        .unwrap();
        restore_profiles(&mut dest, &restored.vaults[0].vpn_profiles).unwrap();
        assert_eq!(
            profiles_raw(&dest).unwrap(),
            restored.vaults[0].vpn_profiles
        );
        let copy = temp.path().join("portable.ttvault");
        vault.portable_copy(&copy).unwrap();
        let copied = Vault::open(&copy, "test-password").unwrap();
        assert_eq!(version(&copied), 2);
        assert_eq!(
            profiles_raw(&copied).unwrap(),
            restored.vaults[0].vpn_profiles
        );
        assert!(Vault::inspect(&copy, "test-password").is_err());
        drop(copied);
        assert_eq!(
            Vault::inspect(&copy, "test-password").unwrap().vpn_profiles,
            restored.vaults[0].vpn_profiles
        );
    }
}

#[cfg(test)]
mod remembered_credentials_tests {
    use super::*;
    #[test]
    fn key_unlock_never_persists_or_consumes_account_credentials_but_otp_still_does() {
        let key = json!({"fields":[{"name":"privateKeyPassword","purpose":"keyPassword","secret":true}]});
        let account = json!({"fields":[{"name":"username","purpose":"username"},{"name":"password","purpose":"password","secret":true}]});
        let otp = json!({"fields":[{"name":"response","purpose":"otp","secret":true}]});
        let mut attempts = HashMap::new();
        assert!(first_credential_answer(&mut attempts,"key",1,&key,&json!({"privateKeyPassword":"Disposable key passphrase"})).is_none());
        assert!(attempts.is_empty());
        assert!(first_credential_answer(&mut attempts,"key",1,&account,&json!({"username":"fixture","password":"Disposable account password"})).is_some());
        assert!(first_credential_answer(&mut attempts,"key",1,&otp,&json!({"response":"123456"})).is_none());
        assert!(first_credential_answer(&mut attempts,"key",1,&account,&json!({"username":"fixture","password":"123456"})).is_none());
        for bad in [json!({"fields":[{"name":"password","purpose":"keyPassword","secret":true}]}),json!({"fields":[{"name":"privateKeyPassword","purpose":"keyPassword","secret":false}]})] {
            let mut attempts = HashMap::new();
            assert!(first_credential_answer(&mut attempts,"bad",1,&bad,&json!({})).is_none());
            assert!(first_credential_answer(&mut attempts,"bad",1,&account,&json!({"username":"fixture","password":"Disposable password"})).is_none());
        }
    }
    #[test]
    fn later_credential_shaped_otp_cannot_replace_initial_password_but_new_generation_can() {
        let mut attempts = HashMap::new();
        let form = json!({"fields":[{"name":"user","purpose":"text"},{"name":"passwd","purpose":"secret","label":"Password:"}]});
        let first = json!({"user":"fixture","passwd":"Initial password"});
        assert!(first_credential_answer(&mut attempts,"session",1,&form,&first).is_some());
        assert!(first_credential_answer(&mut attempts,"session",1,&form,&json!({"user":"fixture","passwd":"123456"})).is_none());
        assert!(first_credential_answer(&mut attempts,"session",2,&form,&first).is_some());
        let mut otp_form = form.clone(); otp_form["message"]="Enter verification token".into();
        assert!(first_credential_answer(&mut attempts,"other",1,&otp_form,&first).is_none());
        assert!(first_credential_answer(&mut attempts,"other",1,&form,&first).is_none());
        let explicit_otp=json!({"fields":[{"name":"username","purpose":"username"},{"name":"password","purpose":"password","label":"Verification token"}]});
        assert!(first_credential_answer(&mut attempts,"explicit-otp",1,&explicit_otp,&json!({"username":"fixture","password":"123456"})).is_none());
        assert!(first_credential_answer(&mut attempts,"explicit-otp",1,&form,&first).is_none());
        attempts.insert("cached".into(),1);
        assert!(first_credential_answer(&mut attempts,"cached",1,&form,&first).is_none());
    }
    #[test]
    fn saved_initial_credentials_survive_reopen_but_otp_and_opt_out_do_not_replace_them() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("remember.ttvault");
        let mut vault = Vault::create(&path,"Remember test","Disposable vault password").unwrap();
        let id = Uuid::new_v4().to_string();
        let record = Record {id:id.clone(),kind:"vpn".into(),updated_at:0,data:json!({"version":1,"label":"Remember","family":"fortinet","source":"manual","server":"https://vpn.example","groupId":null,"username":"","password":"","realm":"","config":"","remember":true})};
        restore_profiles(&mut vault,&[record]).unwrap();
        let fields = json!([{"name":"user","purpose":"text"},{"name":"passwd","purpose":"secret","label":"Password:"}]);
        let (user,password) = remembered_answer(&fields,&json!({"user":"fixture-user","passwd":"Disposable remembered password"})).unwrap();
        save_answer_credentials(&mut vault,&json!(id),&user,&password).unwrap();
        assert!(remembered_answer(&json!([{"name":"passwd","purpose":"secret","label":"Challenge:"}]),&json!({"passwd":"654321"})).is_none());
        drop(vault);
        let mut vault = Vault::open(&path,"Disposable vault password").unwrap();
        let mut records = profiles_raw(&vault).unwrap();
        assert_eq!(records[0].data["username"],"fixture-user");
        assert_eq!(records[0].data["password"],"Disposable remembered password");
        assert!(view(&records[0]).get("password").is_none());
        let bytes=std::fs::read(&path).unwrap();
        assert!(!bytes.windows(b"Disposable remembered password".len()).any(|w|w==b"Disposable remembered password"));
        records[0].data["remember"]=false.into(); records[0].data["password"]="".into();
        restore_profiles(&mut vault,&records).unwrap();
        save_answer_credentials(&mut vault,&json!(id),"other-user","Do not save").unwrap();
        assert_eq!(profiles_raw(&vault).unwrap()[0].data["password"],"");
    }
}
