use crate::state::Shared;
use anyhow::{anyhow, ensure, Result};
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tauri::{AppHandle, Manager, State};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{oneshot, Mutex},
};

type Reply = oneshot::Sender<std::result::Result<Value, String>>;
#[path = "database_team.rs"]
mod team_access;
#[tauri::command]
pub async fn database_file(
    state: State<'_, Shared>,
    action: String,
    path: String,
    content: Option<String>,
    record_id: Option<String>,
    session_id: Option<String>,
) -> std::result::Result<String, String> {
    let run = async {
        let file_epoch=state.database_epoch.load(std::sync::atomic::Ordering::SeqCst);
        let team_file = if crate::team::active(&state) {
            Some(team_access::authorize_file(&state, record_id.as_deref(), session_id.as_deref(), action != "read").await?)
        } else { None };
        ensure!(team_file.is_some() ||
            state
                .vault
                .lock()
                .map_err(|_| anyhow!("Vault lock"))?
                .is_some(),
            "Kasa kilitli"
        );
        let target = PathBuf::from(path);
        let extension = target
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        ensure!(
            if action == "writeBytes" { ["txt", "bin"].contains(&extension.as_str()) } else { ["sql", "csv", "json"].contains(&extension.as_str()) },
            "SQL/CSV/JSON metin işlemleri; tam değer için TXT/BIN dosyası seçin"
        );
        if action == "read" {
            ensure!(
                tokio::fs::metadata(&target).await?.len() <= 5_000_000,
                "Dosya 5 MB sınırını aşıyor"
            );
            return Ok(tokio::fs::read_to_string(target).await?);
        }
        ensure!(action == "write" || action == "writeBytes", "Geçersiz dosya işlemi");
        let text = content.ok_or_else(|| anyhow!("İçerik yok"))?;
        let bytes = if action == "writeBytes" {
            use base64::Engine;
            ensure!(text.len() <= 11_184_812, "Değer 8 MiB sınırını aşıyor");
            let decoded=base64::engine::general_purpose::STANDARD.decode(text)?;
            ensure!(decoded.len() <= 8*1024*1024, "Değer 8 MiB sınırını aşıyor");
            decoded
        } else {
            ensure!(text.len() <= 10_000_000, "Dosya 10 MB sınırını aşıyor");
            text.into_bytes()
        };
        let temporary = target.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        tokio::fs::write(&temporary, bytes).await?;
        if let Some(binding) = team_file {
            if let Err(error) = team_access::validate(&state, &binding, "sql_export", false).await {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(error);
            }
        }
        if state.database_epoch.load(std::sync::atomic::Ordering::SeqCst)!=file_epoch {
            let _=tokio::fs::remove_file(&temporary).await;
            anyhow::bail!("Kasa değişti; dosya kaydı iptal edildi");
        }
        if let Err(e) = tokio::fs::rename(&temporary, &target).await {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(e.into());
        }
        Ok(String::new())
    };
    run.await.map_err(|e: anyhow::Error| e.to_string())
}
pub struct Bridge {
    app: AppHandle,
    child: Mutex<Child>,
    input: Mutex<ChildStdin>,
    pending: Arc<Mutex<HashMap<String, Reply>>>,
    sessions: Mutex<HashMap<String, String>>,
    session_records: Mutex<HashMap<String, String>>,
    tunnels: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    team_bindings: Mutex<HashMap<String, team_access::Binding>>,
}
impl Bridge {
    async fn start(app: &AppHandle) -> Result<Arc<Self>> {
        let bundled = app.path().resource_dir()?.join("database-helper");
        let root = if bundled.join("runtime").is_dir() {
            bundled
        } else if cfg!(debug_assertions) {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../database-helper")
        } else {
            app.path().resource_dir()?.join("database-helper")
        };
        let java = root
            .join("runtime/bin")
            .join(if cfg!(windows) { "java.exe" } else { "java" });
        ensure!(
            java.is_file(),
            "SQL çalışma zamanı eksik: pnpm db:prepare çalıştırın"
        );
        let separator = if cfg!(windows) { ";" } else { ":" };
        let cp = format!(
            "{}{separator}{}",
            root.join("out").display(),
            root.join("lib/*").display()
        );
        let mut command = Command::new(java);
        command
            .args(["-Xmx256m", "-Dfile.encoding=UTF-8", "-cp", &cp, "Main"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command.spawn()?;
        let input = child.stdin.take().ok_or_else(|| anyhow!("SQL stdin"))?;
        let output = child.stdout.take().ok_or_else(|| anyhow!("SQL stdout"))?;
        let pending: Arc<Mutex<HashMap<String, Reply>>> = Arc::new(Mutex::new(HashMap::new()));
        let queue = pending.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(output).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    if let Some(tx) = queue.lock().await.remove(v["id"].as_str().unwrap_or("")) {
                        let answer = if v.get("error").is_some() {
                            Err(v["error"].to_string())
                        } else {
                            Ok(v["result"].clone())
                        };
                        let _ = tx.send(answer);
                    }
                }
            }
            for (_, tx) in queue.lock().await.drain() {
                let _ = tx.send(Err(
                    json!({"state":"unknown_outcome","message":"SQL bağlantı süreci kapandı; yazma işleminin sonucu bilinmiyor"}).to_string(),
                ));
            }
        });
        Ok(Arc::new(Self {
            app: app.clone(),
            child: Mutex::new(child),
            input: Mutex::new(input),
            pending,
            sessions: Mutex::new(HashMap::new()),
            session_records: Mutex::new(HashMap::new()),
            tunnels: Mutex::new(HashMap::new()),
            team_bindings: Mutex::new(HashMap::new()),
        }))
    }
    async fn request(&self, mut request: Value) -> Result<Value> {
        let id = uuid::Uuid::new_v4().to_string();
        request["id"] = json!(id);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);
        let mut line = serde_json::to_vec(&request)?;
        line.push(b'\n');
        if let Err(e) = self.input.lock().await.write_all(&line).await {
            self.pending.lock().await.remove(&id);
            return Err(anyhow!(json!({"state":"unknown_outcome","message":format!("SQL iletişimi kesildi: {e}")}).to_string()));
        }
        match tokio::time::timeout(std::time::Duration::from_secs(620), rx).await {
            Ok(r) => r?.map_err(|e| anyhow!(e)),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(anyhow!(json!({"state":"unknown_outcome","message":"SQL yanıt zaman aşımı; yazma sonucu bilinmiyor, otomatik tekrar yapılmadı"}).to_string()))
            }
        }
    }
}
pub async fn close_all(state: &Shared) {
    state
        .database_suspended
        .store(true, std::sync::atomic::Ordering::SeqCst);
    state
        .database_epoch
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if let Some(bridge) = state.database.lock().await.take() {
        use tauri::Emitter;
        for id in bridge.team_bindings.lock().await.keys() {
            let _=bridge.app.emit("database-session-closed",json!({"sessionId":id,"message":"Kasa veya Team yetkileri değişti; SQL oturumu kapatıldı"}));
        }
        for (_, task) in bridge.tunnels.lock().await.drain() {
            task.abort();
        }
        let _ = bridge.child.lock().await.kill().await;
    }
}

async fn forward(
    app: &AppHandle,
    state: &Shared,
    profile: &Value,
) -> Result<(u16, tokio::task::JoinHandle<()>)> {
    let id = uuid::Uuid::new_v4().to_string();
    let ssh = Arc::new(
        crate::connections::connect_ssh(
            app,
            state,
            &id,
            profile["sshHostId"].as_str().unwrap_or(""),
            None,
        )
        .await?,
    );
    let host = profile["host"]
        .as_str()
        .ok_or_else(|| anyhow!("Hedef adres yok"))?
        .to_owned();
    let port = profile["port"]
        .as_u64()
        .filter(|p| *p > 0 && *p <= 65535)
        .ok_or_else(|| anyhow!("Hedef port geçersiz"))? as u32;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let bound = listener.local_addr()?.port();
    let task = tokio::spawn(async move {
        let mut clients = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted=listener.accept()=>{let Ok((mut socket,_))=accepted else{break};let ssh=ssh.clone();let host=host.clone();clients.spawn(async move{
                    if let Ok(channel)=ssh.handle.channel_open_direct_tcpip(&host,port,"127.0.0.1",0).await{let mut remote=channel.into_stream();let _=tokio::io::copy_bidirectional(&mut socket,&mut remote).await;}
                });},
                _=clients.join_next(),if !clients.is_empty()=>{}
            }
        }
    });
    Ok((bound, task))
}
struct PendingTunnel(Option<tokio::task::JoinHandle<()>>);
impl Drop for PendingTunnel {
    fn drop(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
        }
    }
}

#[tauri::command]
pub async fn database_request(
    app: AppHandle,
    state: State<'_, Shared>,
    request: Value,
) -> std::result::Result<Value, String> {
    handle(&app, state.inner(), request)
        .await
        .map_err(|e| e.to_string())
}
async fn handle(app: &AppHandle, state: &Shared, mut request: Value) -> Result<Value> {
    ensure!(
        !state
            .database_suspended
            .load(std::sync::atomic::Ordering::SeqCst),
        "SQL bağlantıları kilitleniyor; kasayı yeniden açın"
    );
    let epoch = state
        .database_epoch
        .load(std::sync::atomic::Ordering::SeqCst);
    if crate::team::active(state) { return team_access::handle(app, state, request, epoch).await; }
    let (vault_id, records) = {
        let guard = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        let v = guard.as_ref().ok_or_else(|| anyhow!("Kasa kilitli"))?;
        (v.id.clone(), v.records()?)
    };
    let op = request["op"].as_str().unwrap_or("").to_owned();
    if op == "libraryLoad" {
        let record=records.iter().find(|r|Some(r.id.as_str())==request["recordId"].as_str() && crate::database_library::is_document(&r.kind)).ok_or_else(||anyhow!("SQL belgesi bulunamadı"))?;
        crate::database_library::validate(record)?;
        ensure!(records.iter().any(|r|r.kind=="databaseConnection" && r.id==record.data["connectionId"].as_str().unwrap_or("")), "SQL bağlantısı bulunamadı");
        return Ok(serde_json::to_value(record)?);
    }
    ensure!(
        [
            "open",
            "close",
            "cancel",
            "query",
            "schemas",
            "tables",
            "columns",
            "transaction",
            "status",
            "check", "capabilities", "detail", "browse", "routines", "plan", "history", "snapshotCapture", "snapshotList", "snapshotGet", "snapshotDelete", "rowEditOpen", "rowEditPreview", "rowEditApply", "rowEditDiscard", "metadataOptions", "evidence", "lobRead", "changeOptions", "changePreview", "changeApply", "changeDiscard", "provisionPreview", "provisionApply", "provisionDiscard", "provisionInspect", "extensionEnvironment", "extensionPreview", "extensionApply", "extensionDiscard", "rowDeletePreview", "rowDeleteApply", "rowDeleteDiscard", "rowInsertMetadata", "rowInsertPreview", "rowInsertApply", "rowInsertDiscard"
        ]
        .contains(&op.as_str()),
        "Geçersiz SQL işlemi"
    );
    ensure!(request.to_string().len() <= 1_100_000, "İstek çok büyük");
    if ["snapshotList","snapshotGet","snapshotDelete"].contains(&op.as_str()) {
        let record_id=request["recordId"].as_str().ok_or_else(||anyhow!("Bağlantı seçin"))?;
        ensure!(records.iter().any(|r|r.id==record_id&&r.kind=="databaseConnection"),"Bağlantı bulunamadı");
        let guard=state.vault.lock().map_err(|_|anyhow!("Vault lock"))?;
        let vault=guard.as_ref().filter(|v|v.id==vault_id).ok_or_else(||anyhow!("Kasa değişti"))?;
        return crate::database_snapshots::request(&app.path().app_data_dir()?.join("database-snapshots"),vault,record_id,&request);
    }
    if op=="history" {
        let record_id=request["recordId"].as_str().ok_or_else(||anyhow!("Bağlantı seçin"))?;
        ensure!(records.iter().any(|r|r.id==record_id&&r.kind=="databaseConnection"),"Bağlantı bulunamadı");
        let guard=state.vault.lock().map_err(|_|anyhow!("Vault lock"))?;let vault=guard.as_ref().ok_or_else(||anyhow!("Kasa kilitli"))?;
        return crate::database_history::read(&app.path().app_data_dir()?.join("database-metrics"),vault,record_id,chrono::Utc::now().timestamp_millis());
    }
    let check_parameters=request.get("parameters").cloned().unwrap_or(json!({}));
    let check_record_id=request["recordId"].as_str().unwrap_or("").to_owned();
    let check_action_id=request["actionId"].as_str().unwrap_or("").to_owned();
    let mut check_sql = None;
    let mut pending_tunnel = PendingTunnel(None);
    if op == "open" || op == "check" {
        let id = request["recordId"]
            .as_str()
            .ok_or_else(|| anyhow!("Bağlantı seçin"))?;
        let record = records
            .iter()
            .find(|r| r.id == id && r.kind == "databaseConnection")
            .ok_or_else(|| anyhow!("DB bağlantısı bulunamadı"))?;
        if op == "check" {
            let catalog:Vec<Value>=serde_json::from_str(include_str!("../../src/database/catalog.json"))?;
            let contracts:Value=serde_json::from_str(include_str!("../../src/database/actionInputs.json"))?;
            let action=catalog.iter().find(|a|a["id"]==check_action_id).ok_or_else(||anyhow!("Kontrol bulunamadı"))?;
            ensure!(action["engine"]==record.data["engine"]&&action["risk"]=="R0","Motor veya risk uyuşmuyor");
            let contract=&contracts[&check_action_id];ensure!(contract.is_object(),"Kontrol sözleşmesi yok");
            check_sql=Some(json!({"op":"check","action":action,"contract":contract,"parameters":request.get("parameters").cloned().unwrap_or(json!({}))}));
        }
        request = json!({"op":"open","profile":record.data});
        if !record.data["sshHostId"].as_str().unwrap_or("").is_empty() {
            let (port, task) = forward(app, state, &record.data).await?;
            pending_tunnel.0 = Some(task);
            request["profile"]["transportHost"] = json!("127.0.0.1");
            request["profile"]["transportPort"] = json!(port);
        } else {
            request["profile"]
                .as_object_mut()
                .unwrap()
                .remove("transportHost");
            request["profile"]
                .as_object_mut()
                .unwrap()
                .remove("transportPort");
        }
    }
    let bridge = {
        let mut slot = state.database.lock().await;
        if let Some(bridge)=slot.as_ref(){if bridge.child.lock().await.try_wait()?.is_some(){for (_,task) in bridge.tunnels.lock().await.drain(){task.abort();}*slot=None;}}
        if slot.is_none() {
            *slot = Some(Bridge::start(app).await?);
        }
        slot.as_ref().unwrap().clone()
    };
    // Check again after process startup so a concurrent vault lock cannot create a usable session.
    ensure!(
        !state
            .database_suspended
            .load(std::sync::atomic::Ordering::SeqCst)
            && state
                .database_epoch
                .load(std::sync::atomic::Ordering::SeqCst)
                == epoch
            && state
                .vault
                .lock()
                .map_err(|_| anyhow!("Vault lock"))?
                .as_ref()
                .is_some_and(|v| v.id == vault_id),
        "Kasa değişti"
    );
    let session_id = request["sessionId"].as_str().unwrap_or("").to_owned();
    if op != "open" && op != "check" {
        ensure!(
            bridge.sessions.lock().await.get(&session_id) == Some(&vault_id),
            "Bu kasaya ait SQL oturumu yok"
        );
    }
    if op=="snapshotCapture" {request["op"]=json!("schemaSnapshot");}
    let result = bridge.request(request).await?;
    ensure!(
        state
            .database_epoch
            .load(std::sync::atomic::Ordering::SeqCst)
            == epoch,
        "Kasa değişti; eski SQL sonucu atıldı"
    );
    if op=="snapshotCapture" {
        let record_id=bridge.session_records.lock().await.get(&session_id).cloned().ok_or_else(||anyhow!("SQL oturumu kapandı"))?;
        let guard=state.vault.lock().map_err(|_|anyhow!("Vault lock"))?;
        let vault=guard.as_ref().filter(|v|v.id==vault_id).ok_or_else(||anyhow!("Kasa değişti"))?;
        ensure!(vault.records()?.iter().any(|r|r.id==record_id&&r.kind=="databaseConnection"),"Bağlantı silindi");
        return crate::database_snapshots::save(&app.path().app_data_dir()?.join("database-snapshots"),vault,&record_id,result);
    }
    if let Some(mut check) = check_sql {
        let id=result["sessionId"].as_str().ok_or_else(||anyhow!("Kontrol oturumu yok"))?;
        check["sessionId"]=json!(id);
        let response=bridge.request(check).await;
        let _=bridge.request(json!({"op":"close","sessionId":id})).await;
        ensure!(state.database_epoch.load(std::sync::atomic::Ordering::SeqCst)==epoch,"Kasa değişti");
        let mut value=response?;
        value["resetEpoch"]=json!([result["capabilities"]["identity"],result["capabilities"]["instance"],result["capabilities"]["queryStatsReset"]]);
        if value["state"]=="succeeded" {
            let guard=state.vault.lock().map_err(|_|anyhow!("Vault lock"))?;
            if let Some(v)=guard.as_ref().filter(|v|v.id==vault_id){
                use sha2::{Digest,Sha256};
                let mut hash=Sha256::new();hash.update(&*v.key);hash.update(check_parameters.to_string());
                value["scopeHash"]=json!(format!("{:x}",hash.finalize()));
                if let Err(e)=crate::database_history::append(&app.path().app_data_dir()?.join("database-metrics"),v,&check_record_id,&check_action_id,&value,chrono::Utc::now().timestamp_millis()){value["historyWarning"]=json!(e.to_string());}
            }
        }
        return Ok(value);
    }
    if op == "open" {
        let id = result["sessionId"]
            .as_str()
            .ok_or_else(|| anyhow!("Oturum kimliği yok"))?
            .to_owned();
        bridge.sessions.lock().await.insert(id.clone(), vault_id);
        bridge.session_records.lock().await.insert(id.clone(), check_record_id);
        if let Some(task) = pending_tunnel.0.take() {
            bridge.tunnels.lock().await.insert(id, task);
        }
    }
    if op == "close" {
        bridge.sessions.lock().await.remove(&session_id);
        bridge.session_records.lock().await.remove(&session_id);
        if let Some(task) = bridge.tunnels.lock().await.remove(&session_id) {
            task.abort();
        }
    }
    Ok(result)
}
