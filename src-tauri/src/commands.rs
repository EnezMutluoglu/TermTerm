use crate::{
    connections, files, imports,
    model::{Record, VaultInfo},
    state::{SessionInput, Shared},
    sync::{self, SyncProfile},
    tunnels,
    vault::{self, Vault},
};
use anyhow::{anyhow, ensure, Result};
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
type Api<T> = std::result::Result<T, String>;
#[tauri::command]
pub async fn agent_identities(kind: String) -> Api<Vec<Value>> {
    crate::keys::identities(&kind).await.map_err(err)
}
#[tauri::command]
pub async fn key_install(
    app: AppHandle,
    state: State<'_, Shared>,
    host_id: String,
    public_key: String,
) -> Api<()> {
    crate::keys::install(&app, &state, &host_id, &public_key)
        .await
        .map_err(err)
}
#[tauri::command]
pub fn sync_status(state: State<Shared>) -> Api<bool> {
    Ok(state
        .sync_task
        .lock()
        .map_err(|_| "Sync lock")?
        .as_ref()
        .is_some_and(|t| !t.is_finished()))
}
fn err(e: anyhow::Error) -> String {
    format!("{:#}", e)
}
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Api<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}
async fn with_vault_async<T: Send + 'static>(
    state: Shared,
    f: impl FnOnce(&mut Vault) -> Result<T> + Send + 'static,
) -> Api<T> {
    blocking(move || {
        let mut guard = state
            .vault
            .lock()
            .map_err(|_| anyhow!("Vault lock failed"))?;
        f(guard
            .as_mut()
            .ok_or_else(|| anyhow!("Unlock a vault first"))?)
    })
    .await
}
fn recent(app: &AppHandle, path: &str) {
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("recent.json"), json!({"path":path}).to_string());
    }
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> Api<Value> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let recent = std::fs::read_to_string(dir.join("recent.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok());
    Ok(
        json!({"version":env!("CARGO_PKG_VERSION"),"defaultVaultPath":dir.join("Personal.ttvault").to_string_lossy(),"home":crate::platform::home(),"recent":recent,"platform":crate::platform::info()}),
    )
}
#[tauri::command]
pub async fn vault_create(
    app: AppHandle,
    state: State<'_, Shared>,
    path: String,
    name: String,
    password: String,
) -> Api<VaultInfo> {
    let vault = blocking(move || Vault::create(&PathBuf::from(path), &name, &password)).await?;
    let result = vault.info().map_err(err)?;
    close_connections(&state).await?;
    *state.vault.lock().map_err(|_| "Vault lock")? = Some(vault);
    recent(&app, &result.path);
    Ok(result)
}
#[tauri::command]
pub async fn vault_open(
    app: AppHandle,
    state: State<'_, Shared>,
    path: String,
    password: String,
) -> Api<VaultInfo> {
    let vault = blocking(move || Vault::open(&PathBuf::from(path), &password)).await?;
    let result = vault.info().map_err(err)?;
    close_connections(&state).await?;
    *state.vault.lock().map_err(|_| "Vault lock")? = Some(vault);
    recent(&app, &result.path);
    Ok(result)
}
#[tauri::command]
pub async fn vault_info(state: State<'_, Shared>) -> Api<VaultInfo> {
    with_vault_async(state.inner().clone(), move |v| v.info()).await
}
#[tauri::command]
pub async fn vault_remember(
    app: AppHandle,
    state: State<'_, Shared>,
    password: String,
    enabled: bool,
) -> Api<()> {
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?;
    with_vault_async(state.inner().clone(), move |v| {
        if enabled {
            let envelope: crate::crypto::Envelope = serde_json::from_slice(&v.envelope()?)?;
            let key = envelope.unlock(&password, v.id.as_bytes())?;
            ensure!(key == v.key, "Incorrect vault password");
        }
        crate::remember::save(&root, &v.path, &password, enabled)
    })
    .await
}
#[tauri::command]
pub async fn vault_open_remembered(
    app: AppHandle,
    state: State<'_, Shared>,
    path: String,
) -> Api<VaultInfo> {
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let target = PathBuf::from(&path);
    let password = blocking(move || crate::remember::load(&root, &target)).await?;
    vault_open(app, state, path, password.to_string()).await
}
#[tauri::command]
pub async fn vault_remember_status(app: AppHandle, path: String) -> Api<bool> {
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?;
    blocking(move || Ok(crate::remember::load_optional(&root, &PathBuf::from(path))?.is_some())).await
}
#[tauri::command]
pub async fn vault_try_open_remembered(
    app: AppHandle,
    state: State<'_, Shared>,
    path: String,
) -> Api<Option<VaultInfo>> {
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let candidate = blocking(move || {
        let path = PathBuf::from(path);
        crate::remember::load_optional(&root, &path)?
            .map(|password| Vault::open(&path, &password))
            .transpose()
    }).await?;
    let Some(vault) = candidate else { return Ok(None) };
    let result = vault.info().map_err(err)?;
    close_connections(&state).await?;
    *state.vault.lock().map_err(|_| "Vault lock")? = Some(vault);
    recent(&app, &result.path);
    Ok(Some(result))
}
#[tauri::command]
pub async fn vault_lock(state: State<'_, Shared>) -> Api<()> {
    close_connections(&state).await?;
    *state.vault.lock().map_err(|_| "Vault lock")? = None;
    Ok(())
}
pub(crate) async fn close_connections(state: &Shared) -> Api<()> {
    crate::operations::cancel_all(state);
    crate::transfers::cancel_all(state);
    crate::metrics::stop_all(state);
    if let Some(task) = state.bridge.lock().map_err(|_| "Bridge lock")?.take() {
        task.abort();
    }
    for (_, task) in state.shares.lock().map_err(|_| "Share lock")?.drain() {
        task.abort();
    }
    state
        .shared_writers
        .lock()
        .map_err(|_| "Share lock")?
        .clear();
    if let Some(task) = state.sync_task.lock().map_err(|_| "Sync lock")?.take() {
        task.abort();
    }
    let senders = state
        .sessions
        .lock()
        .map_err(|_| "Session lock")?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for sender in senders {
        let _ = sender.send(SessionInput::Close).await;
    }
    state.prompts.lock().map_err(|_| "Prompt lock")?.clear();
    // A closing session commits its encrypted log before removing itself. Keep
    // the current vault alive until that completes, including on vault switch.
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if state.sessions.lock().map(|s| s.is_empty()).unwrap_or(true) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    state.sftp.lock().await.clear();
    let edits = std::mem::take(&mut *state.edits.lock().await);
    for edit in edits.into_values() {
        let _ = tokio::fs::remove_file(&edit.local).await;
        if let Some(parent) = edit.local.parent() {
            let _ = tokio::fs::remove_dir(parent).await;
        }
    }
    for (_, t) in state.tunnels.lock().map_err(|_| "Tunnel lock")?.drain() {
        t.abort();
    }
    Ok(())
}
#[tauri::command]
pub fn session_metrics_set(
    app: AppHandle,
    state: State<Shared>,
    session_id: String,
    enabled: bool,
) -> Api<()> {
    crate::metrics::set(&app, &state, &session_id, enabled).map_err(err)
}
#[tauri::command]
pub async fn records_save(state: State<'_, Shared>, mut records: Vec<Record>) -> Api<VaultInfo> {
    for r in &mut records {
        r.updated_at = chrono::Utc::now().timestamp_millis();
    }
    with_vault_async(state.inner().clone(), move |v| {
        v.put(&records)?;
        v.info()
    })
    .await
}
#[tauri::command]
pub async fn records_delete(state: State<'_, Shared>, ids: Vec<String>) -> Api<VaultInfo> {
    with_vault_async(state.inner().clone(), move |v| {
        let records = v.records()?;
        for r in &records {
            if ids.contains(&r.id) {
                continue;
            }
            for k in ["groupId", "credentialId", "hostId"] {
                ensure!(
                    !r.data[k]
                        .as_str()
                        .is_some_and(|x| ids.iter().any(|id| id == x)),
                    "{} is still referenced by {}",
                    k,
                    r.data["label"].as_str().unwrap_or("a record")
                );
            }
            for field in ["chain", "hostIds", "inventoryGroups"] {
                ensure!(
                    !r.data[field].as_array().is_some_and(|a| a
                        .iter()
                        .any(|id| id.as_str().is_some_and(|s| ids.iter().any(|i| i == s)))),
                    "Record is still referenced by {} ({field})",
                    r.data["label"].as_str().unwrap_or("another record")
                );
            }
        }
        v.delete(&ids)?;
        v.info()
    })
    .await
}
#[tauri::command]
pub async fn vault_copy(state: State<'_, Shared>, path: String) -> Api<()> {
    let s = state.inner().clone();
    blocking(move || {
        let g = s.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        g.as_ref()
            .ok_or_else(|| anyhow!("Vault locked"))?
            .portable_copy(&PathBuf::from(path))
    })
    .await
}
#[tauri::command]
pub async fn backup_create(
    state: State<'_, Shared>,
    path: String,
    password: String,
    include_profiles: bool,
    ids: Vec<String>,
) -> Api<()> {
    let s = state.inner().clone();
    blocking(move || {
        let g = s.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        g.as_ref().ok_or_else(|| anyhow!("Vault locked"))?.backup(
            &PathBuf::from(path),
            &password,
            include_profiles,
            &ids,
        )
    })
    .await
}
#[tauri::command]
pub async fn backup_preview(path: String, password: String) -> Api<vault::Backup> {
    blocking(move || vault::read_backup(&PathBuf::from(path), &password)).await
}
#[tauri::command]
pub async fn backup_restore(
    app: AppHandle,
    state: State<'_, Shared>,
    source: String,
    password: String,
    path: String,
    new_password: String,
    index: usize,
) -> Api<VaultInfo> {
    let v = blocking(move || {
        let backup = vault::read_backup(&PathBuf::from(source), &password)?;
        let data = backup
            .vaults
            .get(index)
            .ok_or_else(|| anyhow!("Invalid vault selection"))?;
        let mut v = Vault::create(&PathBuf::from(path), &data.name, &new_password)?;
        v.put(&vault::restored_records(&data.records)?)?;
        Ok(v)
    })
    .await?;
    let info = v.info().map_err(err)?;
    close_connections(&state).await?;
    *state.vault.lock().map_err(|_| "Vault lock")? = Some(v);
    recent(&app, &info.path);
    Ok(info)
}
#[tauri::command]
pub async fn import_preview(
    app: AppHandle,
    state: State<'_, Shared>,
    path: String,
    format: String,
    password: Option<String>,
    mapping: Option<std::collections::HashMap<String, String>>,
    encoding: Option<String>,
    operation_id: Option<String>,
) -> Api<imports::ImportPreview> {
    let op = crate::operations::Operation::start(&app, &state, operation_id, "import-preview")
        .map_err(err)?;
    let worker = op.clone();
    let result = blocking(move || {
        worker.check()?;
        worker.progress("reading", 0, 0);
        let preview = crate::archive::preview_encoded(
            &PathBuf::from(path),
            &format,
            &password.unwrap_or_default(),
            mapping,
            encoding.as_deref(),
        )?;
        worker.check()?;
        Ok(preview)
    })
    .await;
    op.finish(&state, result.as_ref().err().cloned());
    result
}
#[tauri::command]
pub fn operation_cancel(state: State<Shared>, id: String) -> Api<bool> {
    crate::operations::cancel(&state, &id).map_err(err)
}
#[tauri::command]
pub async fn backup_bundle(
    state: State<'_, Shared>,
    path: String,
    password: String,
    sources: Vec<crate::archive::Source>,
    ids: Vec<String>,
    include_profiles: bool,
    include_files: bool,
) -> Api<crate::archive::Report> {
    let state = state.inner().clone();
    blocking(move || {
        let g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        crate::archive::bundle(
            g.as_ref().ok_or_else(|| anyhow!("Vault locked"))?,
            &PathBuf::from(path),
            &password,
            sources,
            &ids,
            include_profiles,
            include_files,
        )
    })
    .await
}
#[tauri::command]
pub async fn csv_headers(path: String, encoding: Option<String>) -> Api<Vec<String>> {
    blocking(move || {
        let text = imports::read_text_encoded(std::path::Path::new(&path), encoding.as_deref())?;
        Ok(csv::Reader::from_reader(text.as_bytes())
            .headers()?
            .iter()
            .map(str::to_string)
            .collect())
    })
    .await
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportApplyResult {
    vault: VaultInfo,
    added: usize,
    updated: usize,
    skipped: usize,
    failed: usize,
    items: Vec<imports::MergeItem>,
}
#[tauri::command]
pub async fn import_apply(
    app: AppHandle,
    state: State<'_, Shared>,
    records: Vec<Record>,
    policy: String,
    vault_id: String,
    operation_id: Option<String>,
) -> Api<ImportApplyResult> {
    let op = crate::operations::Operation::start(&app, &state, operation_id, "import-apply")
        .map_err(err)?;
    let worker = op.clone();
    let shared = state.inner().clone();
    let result = blocking(move || {
        worker.check()?;
        let mut guard = shared.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        let v = guard.as_mut().ok_or_else(|| anyhow!("Vault locked"))?;
        ensure!(
            v.id == vault_id,
            "Target vault changed; preview the import again"
        );
        let existing = v.records()?;
        worker.check()?;
        worker.progress("matching", 0, records.len());
        let merged = imports::merge_report(&existing, &records, &policy)?;
        imports::validate(&existing, &merged.records)?;
        worker.progress("encrypting", 0, merged.records.len());
        v.put_checked(&merged.records, || worker.check(), || worker.commit())?;
        Ok(ImportApplyResult {
            vault: v.info()?,
            added: merged.added,
            updated: merged.updated,
            skipped: merged.skipped,
            failed: 0,
            items: merged.items,
        })
    })
    .await;
    op.finish(&state, result.as_ref().err().cloned());
    result
}
#[tauri::command]
pub async fn export_preview(state: State<'_, Shared>, format: String, secrets: bool) -> Api<Value> {
    with_vault_async(state.inner().clone(), move |v| {
        let (text, warnings) = imports::export(&v.records()?, &format, secrets)?;
        Ok(json!({"text":text,"warnings":warnings}))
    })
    .await
}
#[tauri::command]
pub async fn export_write(
    state: State<'_, Shared>,
    path: String,
    format: String,
    secrets: bool,
) -> Api<()> {
    with_vault_async(state.inner().clone(), move |v| {
        use std::io::Write;
        let (text, _) = imports::export(&v.records()?, &format, secrets)?;
        let mut f = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        Ok(())
    })
    .await
}
#[tauri::command]
pub async fn key_import(path: String, passphrase: String) -> Api<Value> {
    blocking(move||{let meta=std::fs::metadata(&path)?;ensure!(meta.len()<1024*1024,"Key file is too large");let text=std::fs::read_to_string(path)?;let key=crate::ssh_compat::decode_private_key(&text,&passphrase)?;let public=key.public_key().to_openssh()?;Ok(json!({"privateKey":text,"passphrase":passphrase,"publicKey":public,"fingerprint":key.public_key().fingerprint(russh::keys::HashAlg::Sha256).to_string()}))}).await
}
#[tauri::command]
pub async fn key_generate() -> Api<Value> {
    blocking(move||{let key=russh::keys::PrivateKey::random(&mut russh::keys::key::safe_rng(),russh::keys::Algorithm::Ed25519)?;Ok(json!({"privateKey":key.to_openssh(russh::keys::ssh_key::LineEnding::LF)?.to_string(),"publicKey":key.public_key().to_openssh()?,"fingerprint":key.public_key().fingerprint(russh::keys::HashAlg::Sha256).to_string()}))}).await
}
#[tauri::command]
pub fn session_start(
    app: AppHandle,
    state: State<Shared>,
    host_id: Option<String>,
    shell: Option<String>,
) -> Api<String> {
    connections::start(app, state.inner().clone(), host_id, shell).map_err(err)
}
#[tauri::command]
pub async fn session_input(
    state: State<'_, Shared>,
    id: String,
    data: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
    close: Option<bool>,
) -> Api<()> {
    if data.is_some()
        && state
            .shared_writers
            .lock()
            .map_err(|_| "Share lock")?
            .get(&id)
            == Some(&false)
    {
        return Err("Shared terminal input is controlled by another member".into());
    }
    let sender = state
        .sessions
        .lock()
        .map_err(|_| "Session lock")?
        .get(&id)
        .cloned()
        .ok_or("Session closed")?;
    let msg = if close == Some(true) {
        SessionInput::Close
    } else if let (Some(cols), Some(rows)) = (cols, rows) {
        SessionInput::Resize(cols.clamp(1, 500), rows.clamp(1, 300))
    } else {
        SessionInput::Data(data.unwrap_or_default().into_bytes())
    };
    sender.send(msg).await.map_err(|e| e.to_string())
}
#[tauri::command]
pub fn prompt_answer(state: State<Shared>, id: String, answers: Vec<String>) -> Api<()> {
    if let Some(tx) = state.prompts.lock().map_err(|_| "Prompt lock")?.remove(&id) {
        let _ = tx.send(answers);
    }
    Ok(())
}
#[tauri::command]
pub async fn sftp_connect(
    app: AppHandle,
    state: State<'_, Shared>,
    id: String,
    host_id: String,
) -> Api<String> {
    files::connect(&app, &state, &id, &host_id)
        .await
        .map_err(err)
}
#[tauri::command]
pub async fn sftp_disconnect(state: State<'_, Shared>, id: String) -> Api<()> {
    state.sftp.lock().await.remove(&id);
    Ok(())
}
#[tauri::command]
pub async fn file_edit(
    app: AppHandle,
    state: State<'_, Shared>,
    endpoint: files::Endpoint,
) -> Api<Value> {
    files::edit_begin(&app, &state, endpoint).await.map_err(err)
}
#[tauri::command]
pub async fn file_edit_save(
    app: AppHandle,
    state: State<'_, Shared>,
    id: String,
    overwrite: bool,
    close: bool,
) -> Api<()> {
    files::edit_save(&app, &state, &id, overwrite, close)
        .await
        .map_err(err)
}
#[tauri::command]
pub async fn file_list(
    state: State<'_, Shared>,
    endpoint: files::Endpoint,
) -> Api<Vec<files::FileEntry>> {
    files::list(&state, &endpoint).await.map_err(err)
}
#[tauri::command]
pub async fn file_action(
    state: State<'_, Shared>,
    endpoint: files::Endpoint,
    action: String,
    target: Option<String>,
    permissions: Option<u32>,
) -> Api<()> {
    files::action(&state, &endpoint, &action, target, permissions)
        .await
        .map_err(err)
}
#[tauri::command]
pub async fn file_transfer(
    app: AppHandle,
    state: State<'_, Shared>,
    id: String,
    source: files::Endpoint,
    dest: files::Endpoint,
    overwrite: bool,
) -> Api<u64> {
    files::copy(&app, &state, &id, source, dest, overwrite)
        .await
        .map_err(err)
}
#[tauri::command]
pub fn transfer_control(
    app: AppHandle,
    state: State<Shared>,
    id: String,
    action: String,
) -> Api<bool> {
    crate::transfers::control(&app, &state, &id, &action).map_err(err)
}
#[tauri::command]
pub async fn tunnel_start(
    app: AppHandle,
    state: State<'_, Shared>,
    id: String,
    tunnel: tunnels::Tunnel,
) -> Api<()> {
    tunnels::start(app, state.inner().clone(), id, tunnel)
        .await
        .map_err(err)
}
#[tauri::command]
pub fn tunnel_stop(state: State<Shared>, id: String) -> Api<()> {
    if let Some(task) = state.tunnels.lock().map_err(|_| "Tunnel lock")?.remove(&id) {
        task.abort();
    }
    Ok(())
}
#[tauri::command]
pub async fn sync_test(profile: SyncProfile) -> Api<Value> {
    sync::test(&profile).await.map_err(err)
}
#[tauri::command]
pub async fn sync_watch(
    app: AppHandle,
    state: State<'_, Shared>,
    profile: SyncProfile,
    enabled: bool,
) -> Api<()> {
    sync::watch(app, state.inner().clone(), profile, enabled)
        .await
        .map_err(err)
}
#[tauri::command]
pub async fn sync_prepare(profile: SyncProfile) -> Api<()> {
    sync::prepare(&profile).await.map_err(err)
}
#[tauri::command]
pub async fn sync_preview(state: State<'_, Shared>, profile: SyncProfile) -> Api<Value> {
    sync::preview(&state, &profile).await.map_err(err)
}
#[tauri::command]
pub async fn sync_bind(state: State<'_, Shared>, profile: SyncProfile, token: String) -> Api<()> {
    sync::bind(&state, &profile, &token).await.map_err(err)
}
#[tauri::command]
pub async fn sync_list(profile: SyncProfile) -> Api<Vec<Value>> {
    sync::list_remote(&profile).await.map_err(err)
}
#[tauri::command]
pub async fn sync_upload(state: State<'_, Shared>, profile: SyncProfile) -> Api<()> {
    sync::upload_new(&state, &profile).await.map_err(err)
}
#[tauri::command]
pub async fn sync_run(state: State<'_, Shared>, profile: SyncProfile) -> Api<sync::SyncReport> {
    sync::synchronize(&state, &profile).await.map_err(err)
}
#[tauri::command]
pub async fn sync_resolve(
    state: State<'_, Shared>,
    profile: SyncProfile,
    id: String,
    choice: String,
    revision: i64,
) -> Api<()> {
    sync::resolve(&state, &profile, &id, &choice, revision)
        .await
        .map_err(err)
}
#[tauri::command]
pub async fn sync_download(
    app: AppHandle,
    state: State<'_, Shared>,
    profile: SyncProfile,
    id: String,
    password: String,
    path: String,
) -> Api<VaultInfo> {
    let v = sync::download(&profile, &id, &password, &PathBuf::from(path))
        .await
        .map_err(err)?;
    let info = v.info().map_err(err)?;
    close_connections(&state).await?;
    *state.vault.lock().map_err(|_| "Vault lock")? = Some(v);
    recent(&app, &info.path);
    Ok(info)
}
#[tauri::command]
pub async fn team_list(state: State<'_, Shared>, profile: SyncProfile) -> Api<Vec<Value>> {
    sync::members(&state, &profile).await.map_err(err)
}
#[tauri::command]
pub async fn share_list(state: State<'_, Shared>, profile: SyncProfile) -> Api<Vec<Value>> {
    crate::shared_terminal::list(&state, &profile)
        .await
        .map_err(err)
}
#[tauri::command]
pub async fn share_start(
    app: AppHandle,
    state: State<'_, Shared>,
    profile: SyncProfile,
    local_id: Option<String>,
    remote_id: Option<String>,
) -> Api<Value> {
    crate::shared_terminal::start(app, state.inner().clone(), profile, local_id, remote_id)
        .await
        .map_err(err)
}
#[tauri::command]
pub async fn share_control(
    profile: SyncProfile,
    id: String,
    writer: String,
    finish: bool,
) -> Api<()> {
    crate::shared_terminal::control(&profile, &id, &writer, finish)
        .await
        .map_err(err)
}
#[tauri::command]
pub async fn team_set(
    state: State<'_, Shared>,
    profile: SyncProfile,
    username: String,
    role: String,
    password: String,
) -> Api<()> {
    sync::set_member(&state, &profile, &username, &role, &password)
        .await
        .map_err(err)
}
#[tauri::command]
pub fn serial_ports() -> Api<Vec<String>> {
    serialport::available_ports()
        .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn lab_profile() -> Api<Value> {
    #[cfg(not(debug_assertions))]
    {
        return Err("Lab helpers are unavailable in production builds".into());
    }
    #[cfg(debug_assertions)]
    {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let p = root.join(".lab/connection.json");
        let mut v: Value = serde_json::from_slice(&std::fs::read(p).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if let Some(obj) = v.as_object_mut() {
            obj.remove("accounts");
            obj.insert(
                "caPath".into(),
                root.join(".lab/ca.crt")
                    .to_string_lossy()
                    .to_string()
                    .into(),
            );
            obj.insert("label".into(), "Local WSL lab".into());
        }
        Ok(v)
    }
}
#[tauri::command]
pub async fn cloud_discover(
    provider: String,
    profile: String,
    region: String,
    token: String,
) -> Api<imports::ImportPreview> {
    crate::integrations::discover(&provider, &profile, &region, &token)
        .await
        .map_err(err)
}
#[tauri::command]
pub async fn api_bridge(app: AppHandle, state: State<'_, Shared>, enabled: bool) -> Api<Value> {
    crate::integrations::bridge(app, state.inner().clone(), enabled)
        .await
        .map_err(err)
}
