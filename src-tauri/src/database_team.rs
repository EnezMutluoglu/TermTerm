//! Team SQL has a fresh, server-verified lease, separate from terminal connect.
//! No offline fallback, client-supplied profiles or arbitrary monitoring SQL.
use super::*;
use crate::team::database as access;
use std::sync::atomic::Ordering;

#[derive(Clone)]
pub(super) struct Binding {
    vault: String,
    record: String,
    revision: i64,
    acl_revision: i64,
    // A shared CA is public PEM content, never a path supplied by another device.
    ca: Option<Arc<tempfile::NamedTempFile>>,
}
impl Binding {
    fn from(auth: &access::SqlAuthorization) -> Self {
        Self {
            vault: auth.vault.clone(),
            record: auth.record.id.clone(),
            revision: auth.revision,
            acl_revision: auth.acl_revision,
            ca: None,
        }
    }
}

// Exhaustive and fail-closed: adding a helper operation does not grant Team access.
fn permission(op: &str) -> Result<&'static str> {
    Ok(match op {
        "open" | "schemas" | "tables" | "columns" | "capabilities" | "detail" | "routines" | "metadataOptions" | "status" => "sql_access",
        "query" | "transaction" | "browse" | "plan" | "lobRead" => "sql_run",
        "check" | "evidence" => "sql_monitor",
        "rowEditOpen" | "rowEditPreview" | "rowEditApply" | "rowEditDiscard" |
        "rowDeletePreview" | "rowDeleteApply" | "rowDeleteDiscard" |
        "rowInsertMetadata" | "rowInsertPreview" | "rowInsertApply" | "rowInsertDiscard" |
        "changeOptions" | "changePreview" | "changeApply" | "changeDiscard" |
        "provisionPreview" | "provisionApply" | "provisionDiscard" | "provisionInspect" |
        "extensionEnvironment" | "extensionPreview" | "extensionApply" | "extensionDiscard" => "sql_manage",
        "history" | "snapshotCapture" | "snapshotList" | "snapshotGet" | "snapshotDelete" => anyhow::bail!("Team SQL metrik / şema geçmişi henüz desteklenmiyor; sonuçlar yalnız bu oturumda tutulur"),
        _ => anyhow::bail!("Team SQL işlemi desteklenmiyor"),
    })
}

pub(super) async fn validate(
    state: &Shared,
    binding: &Binding,
    permission: &str,
    audit: bool,
) -> Result<()> {
    let auth = access::authorize(state, &binding.record, permission, audit).await?;
    ensure!(
        auth.vault == binding.vault
            && auth.revision == binding.revision
            && auth.acl_revision == binding.acl_revision,
        "Team bağlantısı veya yetkiler değişti; SQL oturumunu yeniden açın"
    );
    Ok(())
}

async fn close(bridge: &Bridge, id: &str) {
    // Remove ownership first so concurrent calls cannot reuse this session.
    bridge.sessions.lock().await.remove(id);
    bridge.session_records.lock().await.remove(id);
    bridge.team_bindings.lock().await.remove(id);
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        bridge.request(json!({"op":"close","sessionId":id})),
    )
    .await;
}

fn watch(app: &AppHandle, state: &Shared, bridge: &Arc<Bridge>, id: String) {
    let app = app.clone();
    let state = Arc::downgrade(state);
    let bridge = Arc::downgrade(bridge);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let (Some(state), Some(bridge)) = (state.upgrade(), bridge.upgrade()) else {
                break;
            };
            let binding = bridge.team_bindings.lock().await.get(&id).cloned();
            let Some(binding) = binding else { break };
            if let Err(error) = validate(&state, &binding, "sql_access", false).await {
                use tauri::Emitter;
                let _=app.emit("database-session-closed",json!({"sessionId":id,"message":format!("Team SQL oturumu kapatıldı: {error}")}));
                close(&bridge, &id).await;
                break;
            }
        }
    });
}

pub(super) async fn authorize_file(
    state: &Shared,
    record: Option<&str>,
    session: Option<&str>,
    writing: bool,
) -> Result<Binding> {
    let owned = if let Some(session) = session.filter(|s| !s.is_empty()) {
        let bridge = state
            .database
            .lock()
            .await
            .clone()
            .ok_or_else(|| anyhow!("SQL oturumu kapalı"))?;
        let binding = bridge
            .team_bindings
            .lock()
            .await
            .get(session)
            .cloned()
            .ok_or_else(|| anyhow!("Team SQL oturumu yok"))?;
        if let Some(record) = record {
            ensure!(record == binding.record, "SQL kayıt / oturum uyuşmazlığı");
        }
        Some(binding)
    } else {
        None
    };
    let record = owned
        .as_ref()
        .map(|b| b.record.as_str())
        .or(record)
        .ok_or_else(|| anyhow!("Team SQL dosya işlemi bağlantı kimliği gerektirir"))?;
    let auth = access::authorize(
        state,
        record,
        if writing { "sql_export" } else { "sql_run" },
        true,
    )
    .await?;
    let binding = Binding::from(&auth);
    if let Some(owned) = owned {
        ensure!(
            owned.vault == binding.vault
                && owned.revision == binding.revision
                && owned.acl_revision == binding.acl_revision,
            "Team SQL bağlantısı değişti"
        );
    }
    Ok(binding)
}

pub(super) async fn handle(
    app: &AppHandle,
    state: &Shared,
    mut request: Value,
    epoch: u64,
) -> Result<Value> {
    ensure!(request.to_string().len() <= 1_100_000, "İstek çok büyük");
    let op = request["op"].as_str().unwrap_or("").to_owned();
    let id = request["sessionId"].as_str().unwrap_or("").to_owned();
    let vault = access::selected_vault(state)?;
    if op == "libraryLoad" {
        let record = access::load_document(state, request["recordId"].as_str().unwrap_or("")).await?;
        ensure!(state.database_epoch.load(Ordering::SeqCst) == epoch, "Kasa değişti; belge atıldı");
        return Ok(serde_json::to_value(record)?);
    }
    if op == "close" || op == "cancel" {
        let bridge = state
            .database
            .lock()
            .await
            .clone()
            .ok_or_else(|| anyhow!("SQL oturumu kapalı"))?;
        ensure!(
            bridge.sessions.lock().await.get(&id) == Some(&vault),
            "Bu Team kasasına ait SQL oturumu yok"
        );
        if op == "close" {
            close(&bridge, &id).await;
            return Ok(json!({"closed":true}));
        }
        let mut cancel = json!({"op":"cancel","sessionId":id});
        if request["operationId"].is_string() {
            cancel["operationId"] = request["operationId"].clone();
        }
        return bridge.request(cancel).await;
    }
    let required = permission(&op)?;
    if op != "open" && op != "check" {
        let bridge = state
            .database
            .lock()
            .await
            .clone()
            .ok_or_else(|| anyhow!("SQL oturumu kapalı"))?;
        let binding = bridge
            .team_bindings
            .lock()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow!("Team SQL oturumu yok; yeniden bağlanın"))?;
        ensure!(binding.vault == vault, "Team kasası değişti");
        if let Err(error) = validate(state, &binding, required, true).await {
            close(&bridge, &id).await;
            return Err(error);
        }
        request
            .as_object_mut()
            .ok_or_else(|| anyhow!("Geçersiz istek"))?
            .remove("profile");
        let result = bridge.request(request).await;
        // A denied/expired lease never returns an old query's data to the renderer.
        if let Err(error) = validate(state, &binding, required, false).await {
            close(&bridge, &id).await;
            anyhow::bail!(json!({"state":"unknown_outcome","message":format!("Team yetkisi değişti / doğrulanamadı; sonuç gösterilmedi, otomatik tekrar yapılmadı: {error}")}).to_string());
        }
        ensure!(
            state.database_epoch.load(Ordering::SeqCst) == epoch,
            "Kasa değişti; SQL sonucu atıldı"
        );
        return result;
    }
    let record_id = request["recordId"]
        .as_str()
        .ok_or_else(|| anyhow!("Bağlantı seçin"))?;
    let auth = access::authorize(state, record_id, required, true).await?;
    let mut binding = Binding::from(&auth);
    let mut profile = auth.record.data;
    ensure!(
        profile["sshHostId"].as_str().unwrap_or("").is_empty(),
        "Team SQL SSH tüneli henüz desteklenmiyor"
    );
    ensure!(
        profile["caPath"].as_str().unwrap_or("").is_empty(),
        "Team SQL yerel CA yolu kabul etmez; PEM sertifikasını bağlantıya ekleyin"
    );
    profile
        .as_object_mut()
        .ok_or_else(|| anyhow!("Geçersiz bağlantı"))?
        .remove("transportHost");
    profile.as_object_mut().unwrap().remove("transportPort");
    if let Some(pem) = profile["caPem"].as_str().filter(|p| !p.trim().is_empty()) {
        ensure!(
            pem.len() <= 262_144
                && pem.contains("-----BEGIN CERTIFICATE-----")
                && !pem.contains("PRIVATE KEY"),
            "CA için en fazla 256 KiB public PEM sertifikası gerekli"
        );
        use std::io::Write;
        let mut file = tempfile::NamedTempFile::new()?;
        file.write_all(pem.as_bytes())?;
        file.flush()?;
        profile["caPath"] = json!(file.path().to_string_lossy());
        binding.ca = Some(Arc::new(file));
    }
    let check = if op == "check" {
        let catalog: Vec<Value> =
            serde_json::from_str(include_str!("../../src/database/catalog.json"))?;
        let contracts: Value =
            serde_json::from_str(include_str!("../../src/database/actionInputs.json"))?;
        let action_id = request["actionId"]
            .as_str()
            .ok_or_else(|| anyhow!("Kontrol seçin"))?;
        let action = catalog
            .iter()
            .find(|a| a["id"] == action_id)
            .ok_or_else(|| anyhow!("Kontrol bulunamadı"))?;
        ensure!(
            action["engine"] == profile["engine"] && action["risk"] == "R0",
            "Motor veya risk uyuşmuyor"
        );
        ensure!(contracts[action_id].is_object(), "Kontrol sözleşmesi yok");
        Some(
            json!({"op":"check","action":action,"contract":contracts[action_id],"parameters":request.get("parameters").cloned().unwrap_or(json!({}))}),
        )
    } else {
        None
    };
    let bridge = {
        let mut slot = state.database.lock().await;
        if let Some(bridge) = slot.as_ref() {
            if bridge.child.lock().await.try_wait()?.is_some() {
                *slot = None;
            }
        }
        if slot.is_none() {
            *slot = Some(Bridge::start(app).await?);
        }
        slot.as_ref().unwrap().clone()
    };
    ensure!(
        state.database_epoch.load(Ordering::SeqCst) == epoch
            && !state.database_suspended.load(Ordering::SeqCst),
        "Kasa değişti"
    );
    let result = bridge
        .request(json!({"op":"open","profile":profile}))
        .await?;
    let id = result["sessionId"]
        .as_str()
        .ok_or_else(|| anyhow!("SQL oturumu yok"))?
        .to_owned();
    if let Err(error) = validate(state, &binding, required, false).await {
        close(&bridge, &id).await;
        return Err(error);
    }
    if state.database_epoch.load(Ordering::SeqCst) != epoch {
        close(&bridge, &id).await;
        anyhow::bail!("Kasa değişti");
    }
    bridge
        .sessions
        .lock()
        .await
        .insert(id.clone(), binding.vault.clone());
    bridge
        .session_records
        .lock()
        .await
        .insert(id.clone(), binding.record.clone());
    bridge
        .team_bindings
        .lock()
        .await
        .insert(id.clone(), binding.clone());
    watch(app, state, &bridge, id.clone());
    if let Some(mut check) = check {
        check["sessionId"] = json!(id);
        let response = bridge.request(check).await;
        let current = validate(state, &binding, required, false).await;
        close(&bridge, &id).await;
        current?;
        ensure!(
            state.database_epoch.load(Ordering::SeqCst) == epoch,
            "Kasa değişti"
        );
        let mut value = response?;
        value["resetEpoch"] = json!([
            result["capabilities"]["identity"],
            result["capabilities"]["instance"],
            result["capabilities"]["queryStatsReset"]
        ]);
        value["historyWarning"] = json!("Team SQL metrik geçmişi henüz saklanmıyor");
        return Ok(value);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sql_permissions_do_not_inherit_terminal_or_new_operations() {
        assert_eq!(permission("query").unwrap(), "sql_run");
        assert_eq!(permission("check").unwrap(), "sql_monitor");
        assert_eq!(permission("provisionApply").unwrap(), "sql_manage");
        assert_eq!(permission("rowDeleteApply").unwrap(), "sql_manage");
        assert!(permission("queryAsMonitor").is_err());
        assert!(permission("schemaSnapshot").is_err());
        assert!(permission("history").is_err());
    }
}
