//! Team-only ephemeral terminal transport. Each RPC rechecks the host ACL.
use super::*;
use crate::{crypto as cipher, state::SessionInput};
use base64::{engine::general_purpose::STANDARD, Engine};
use tauri::Listener;
use tokio::sync::mpsc;

fn vault(s: &Shared) -> Result<String> {
    with(s, |a| {
        Ok(a.vault
            .as_ref()
            .ok_or_else(|| anyhow!("Ortak kasa seçin"))?
            .0
            .clone())
    })
}
fn event(app: &AppHandle, id: &str, kind: &str, detail: Value) {
    let _ = app.emit(
        "session-event",
        json!({"id":id,"kind":kind,"detail":detail}),
    );
}
fn aad(id: &str, lease: &str, frame: &str, kind: &str) -> String {
    format!("termterm-team-terminal-v1:{id}:{lease}:{frame}:{kind}")
}
pub async fn list(s: &Shared) -> Result<Vec<Value>> {
    Ok(serde_json::from_value(
        rpc(s, "terminal_list", &json!({"vaultId":vault(s)?})).await?,
    )?)
}
pub async fn members(s: &Shared) -> Result<Vec<Value>> {
    let team = with(s, |a| {
        Ok(a.vault
            .as_ref()
            .ok_or_else(|| anyhow!("Ortak kasa seçin"))?
            .2
            .clone())
    })?;
    let data = rpc(s, "members", &json!({"teamId":team})).await?;
    Ok(data["members"].as_array().cloned().unwrap_or_default())
}
pub async fn control(s: &Shared, id: &str, writer: &str, finish: bool) -> Result<()> {
    rpc(
        s,
        "terminal_control",
        &json!({"vaultId":vault(s)?,"id":id,"writer":writer,"finish":finish}),
    )
    .await?;
    Ok(())
}
pub async fn start(
    app: AppHandle,
    s: Shared,
    local: Option<String>,
    remote: Option<String>,
) -> Result<Value> {
    ensure!(
        local.is_some() != remote.is_some(),
        "Paylaşılacak veya katılınacak terminali seçin"
    );
    let v = vault(&s)?;
    let owner = local.is_some();
    let sid = remote.unwrap_or_else(|| Uuid::new_v4().to_string());
    let id = local.unwrap_or_else(|| Uuid::new_v4().to_string());
    let username = with(&s, |a| Ok(a.auth.username.clone()))?;
    let owner_tx = if owner {
        Some(
            s.sessions
                .lock()
                .map_err(|_| anyhow!("Session lock"))?
                .get(&id)
                .cloned()
                .ok_or_else(|| anyhow!("Önce terminale bağlanın"))?,
        )
    } else {
        None
    };
    if owner {
        ensure!(
            !s.team_share_inputs
                .lock()
                .map_err(|_| anyhow!("Share lock"))?
                .contains_key(&id),
            "Terminal zaten paylaşılıyor"
        );
        let host = s
            .session_hosts
            .lock()
            .map_err(|_| anyhow!("Session lock"))?
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow!("Yalnız izinli Team host terminali paylaşılabilir"))?;
        let recipients = rpc(
            &s,
            "terminal_recipients",
            &json!({"vaultId":v,"recordId":host}),
        )
        .await?;
        let key = cipher::random_key();
        let context = format!("termterm-team-share:{sid}");
        let envelopes=recipients["members"].as_array().ok_or_else(||anyhow!("Üye anahtarları eksik"))?.iter().map(|user|Ok(json!({"userId":user["userId"],"envelope":crypto::wrap_key(&key,user["publicKey"].as_str().unwrap_or_default(),&context)?}))).collect::<Result<Vec<_>>>()?;
        rpc(&s,"terminal_open",&json!({"vaultId":v,"recordId":host,"id":sid,"lease":Uuid::new_v4().to_string(),"aclRevision":recipients["aclRevision"],"envelopes":envelopes})).await?;
    }
    let joined = rpc(&s, "terminal_join", &json!({"vaultId":v,"id":sid})).await?;
    let key = crypto::open_key(
        &*with(&s, |a| identity(&a.auth))?,
        joined["envelope"].as_str().unwrap_or_default(),
        &format!("termterm-team-share:{sid}"),
    )?;
    let mut lease = joined["lease"]
        .as_str()
        .ok_or_else(|| anyhow!("Lease missing"))?
        .to_string();
    let mut cursor = joined["cursor"].as_i64().unwrap_or(0);
    let (tx, mut input) = mpsc::channel::<SessionInput>(128);
    if owner {
        s.team_share_inputs
            .lock()
            .map_err(|_| anyhow!("Share lock"))?
            .insert(id.clone(), tx);
    } else {
        let mut sessions = s.sessions.lock().map_err(|_| anyhow!("Session lock"))?;
        ensure!(sessions.len() < 16, "En fazla 16 terminal");
        sessions.insert(id.clone(), tx);
        event(&app, &id, "connected", Value::Null);
    }
    let (out_tx, mut output) = mpsc::channel::<Vec<u8>>(128);
    let watched = id.clone();
    let listener = if owner {
        Some(app.listen("session-event", move |e| {
            if let Ok(value) = serde_json::from_str::<Value>(e.payload()) {
                if value["id"] == watched && value["kind"] == "data" {
                    if let Some(data) = value["detail"]
                        .as_str()
                        .and_then(|v| STANDARD.decode(v).ok())
                    {
                        let _ = out_tx.try_send(data);
                    }
                }
            }
        }))
    } else {
        None
    };
    let result = json!({"id":sid,"sessionId":id,"owner":owner});
    let task_id = sid.clone();
    let worker = s.clone();
    let task = tokio::spawn(async move {
        struct Cleanup {
            app: AppHandle,
            state: Shared,
            id: String,
            owner: bool,
            listener: Option<tauri::EventId>,
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                if let Some(listener) = self.listener {
                    self.app.unlisten(listener);
                }
                if let Ok(mut bindings) = self.state.team_share_inputs.lock() {
                    bindings.remove(&self.id);
                }
                if let Ok(mut writers) = self.state.shared_writers.lock() {
                    writers.remove(&self.id);
                }
                if !self.owner {
                    if let Ok(mut sessions) = self.state.sessions.lock() {
                        sessions.remove(&self.id);
                    }
                    event(&self.app, &self.id, "closed", Value::Null);
                }
            }
        }
        let cleanup = Cleanup {
            app: app.clone(),
            state: worker.clone(),
            id: id.clone(),
            owner,
            listener,
        };
        let run:Result<()>=async{
            let mut poll=tokio::time::interval(Duration::from_millis(250));poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {tokio::select!{
                _=poll.tick()=>{
                    if owner {ensure!(worker.sessions.lock().map_err(|_|anyhow!("Session lock"))?.contains_key(&id),"Sahip terminali kapandı");}
                    let row=rpc(&worker,"terminal_poll",&json!({"vaultId":v,"id":sid,"cursor":cursor})).await?;
                    let new_lease=row["lease"].as_str().ok_or_else(||anyhow!("Lease missing"))?;
                    if lease!=new_lease {while input.try_recv().is_ok(){}lease=new_lease.to_string();}
                    worker.shared_writers.lock().map_err(|_|anyhow!("Share lock"))?.insert(id.clone(),row["writer"]==username);
                    let _=app.emit("share-event",json!({"id":sid,"sessionId":id,"owner":row["owner"],"writer":row["writer"]}));
                    for frame in row["frames"].as_array().into_iter().flatten(){
                        if frame["lease"]!=lease{continue;}
                        let kind=frame["kind"].as_str().unwrap_or_default();let fid=frame["id"].as_str().unwrap_or_default();
                        let bytes=cipher::unseal(&key,&hex::decode(frame["payload"].as_str().unwrap_or_default())?,aad(&sid,&lease,fid,kind).as_bytes())?;
                        if owner&&kind=="input"{owner_tx.as_ref().unwrap().send(SessionInput::Data(bytes.to_vec())).await?;}
                        else if !owner&&kind=="output"{event(&app,&id,"data",json!(STANDARD.encode(&*bytes)));}
                    }
                    cursor=row["cursor"].as_i64().unwrap_or(cursor);
                },
                data=output.recv(),if owner=>{if let Some(data)=data {
                    send(&worker,&v,&sid,&lease,"output",&key,&data).await?;
                }},
                data=input.recv()=>{match data {
                    Some(SessionInput::Data(data))=>{
                        if let Err(error)=send(&worker,&v,&sid,&lease,"input",&key,&data).await {
                            if auth_error(&error){event(&app,&id,"status",json!("Yazma hakkı değişti; eski giriş gönderilmedi"));}
                            else {return Err(error);}
                        }
                    },
                    Some(SessionInput::Resize(..))=>{},
                    _=>break,
                }}
            }}Ok(())
        }.await;
        if owner {
            let _ = control(&worker, &sid, &username, true).await;
        }
        let _ = app.emit(
            "share-event",
            json!({"id":sid,"sessionId":id,"ended":true,"error":run.err().map(|e|e.to_string())}),
        );
        drop(cleanup);
        if let Ok(mut tasks) = worker.shares.lock() {
            tasks.remove(&sid);
        }
    });
    s.shares
        .lock()
        .map_err(|_| anyhow!("Share lock"))?
        .insert(task_id, task);
    Ok(result)
}
async fn send(
    s: &Shared,
    v: &str,
    id: &str,
    lease: &str,
    kind: &str,
    key: &[u8; 32],
    data: &[u8],
) -> Result<()> {
    // No retry queue: reconnection/control changes must never replay terminal input.
    let frame = Uuid::new_v4().to_string();
    let bytes = cipher::seal(key, data, aad(id, lease, &frame, kind).as_bytes())?;
    rpc(s,"terminal_send",&json!({"vaultId":v,"id":id,"lease":lease,"kind":kind,"frameId":frame,"payload":hex::encode(bytes)})).await?;
    Ok(())
}
