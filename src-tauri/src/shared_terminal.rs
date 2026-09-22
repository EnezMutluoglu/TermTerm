use crate::{
    crypto,
    state::{SessionInput, Shared},
    sync::{self, SyncProfile},
};
use anyhow::{anyhow, ensure, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Listener};
use tokio::sync::mpsc;
use uuid::Uuid;

fn event(app: &AppHandle, id: &str, kind: &str, detail: Value) {
    let _ = app.emit(
        "session-event",
        json!({"id":id,"kind":kind,"detail":detail}),
    );
}
fn aad(id: Uuid, lease: Uuid, kind: &str) -> String {
    format!("terminal:{id}:{lease}:{kind}")
}
pub async fn list(state: &Shared, p: &SyncProfile) -> Result<Vec<Value>> {
    let id = {
        let g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        Uuid::parse_str(&g.as_ref().ok_or_else(|| anyhow!("Vault locked"))?.id)?
    };
    let c = sync::connect(p).await?;
    let s = sync::schema(p)?;
    Ok(c.query(&format!("SELECT id,owner::text,writer::text FROM {s}.terminal_sessions WHERE vault_id=$1 AND expires_at>now()"),&[&id]).await?.iter().map(|r|json!({"id":r.get::<_,Uuid>(0),"owner":r.get::<_,String>(1),"writer":r.get::<_,String>(2)})).collect())
}
pub async fn control(p: &SyncProfile, id: &str, writer: &str, finish: bool) -> Result<()> {
    let c = sync::connect(p).await?;
    let s = sync::schema(p)?;
    c.execute(
        &format!("SELECT {s}.control_terminal($1,$2,$3,$4)"),
        &[&Uuid::parse_str(id)?, &writer, &Uuid::new_v4(), &finish],
    )
    .await?;
    Ok(())
}
pub async fn start(
    app: AppHandle,
    state: Shared,
    p: SyncProfile,
    local_id: Option<String>,
    remote_id: Option<String>,
) -> Result<Value> {
    let (vault_id, key) = {
        let g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        let v = g.as_ref().ok_or_else(|| anyhow!("Vault locked"))?;
        (Uuid::parse_str(&v.id)?, v.key.clone())
    };
    let c = sync::connect(&p).await?;
    let schema = sync::schema(&p)?;
    let owner = local_id.is_some();
    let share_id = remote_id
        .map(|s| Uuid::parse_str(&s))
        .transpose()?
        .unwrap_or_else(Uuid::new_v4);
    let session_id = local_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let (input_tx, mut input_rx) = mpsc::channel::<SessionInput>(128);
    let owner_sender = if owner {
        Some(
            state
                .sessions
                .lock()
                .map_err(|_| anyhow!("Session lock"))?
                .get(&session_id)
                .cloned()
                .ok_or_else(|| anyhow!("Open a terminal first"))?,
        )
    } else {
        None
    };
    if owner {
        let already = state
            .shared_writers
            .lock()
            .map_err(|_| anyhow!("Share lock"))?
            .contains_key(&session_id);
        ensure!(!already, "This terminal is already shared");
        c.execute(
            &format!("SELECT {schema}.open_terminal($1,$2,$3)"),
            &[&vault_id, &share_id, &Uuid::new_v4()],
        )
        .await?;
    }
    let row=c.query_opt(&format!("SELECT lease FROM {schema}.terminal_sessions WHERE id=$1 AND vault_id=$2 AND expires_at>now()"),&[&share_id,&vault_id]).await?.ok_or_else(||anyhow!("Shared terminal is unavailable"))?;
    let mut lease: Uuid = row.get(0);
    let mut cursor:i64=c.query_one(&format!("SELECT COALESCE(MAX(seq),0)::bigint FROM {schema}.terminal_frames WHERE session_id=$1"),&[&share_id]).await?.get(0);
    if !owner {
        let mut sessions = state.sessions.lock().map_err(|_| anyhow!("Session lock"))?;
        ensure!(sessions.len() < 16, "Maximum 16 active sessions");
        sessions.insert(session_id.clone(), input_tx);
        event(&app, &session_id, "connected", Value::Null);
    }
    if owner {
        state
            .shared_writers
            .lock()
            .map_err(|_| anyhow!("Share lock"))?
            .insert(session_id.clone(), true);
    }
    let (output_tx, mut output_rx) = mpsc::channel::<Vec<u8>>(128);
    let matched = session_id.clone();
    let listener = if owner {
        Some(app.listen("session-event", move |e| {
            if let Ok(value) = serde_json::from_str::<Value>(e.payload()) {
                if value["id"] == matched && value["kind"] == "data" {
                    if let Some(data) = value["detail"]
                        .as_str()
                        .and_then(|s| STANDARD.decode(s).ok())
                    {
                        let _ = output_tx.try_send(data);
                    }
                }
            }
        }))
    } else {
        None
    };
    let result = json!({"id":share_id,"sessionId":session_id,"owner":owner});
    let worker = state.clone();
    let task_key = share_id.to_string();
    let task = tokio::spawn(async move {
        struct Unlisten(AppHandle, Option<tauri::EventId>);
        impl Drop for Unlisten {
            fn drop(&mut self) {
                if let Some(id) = self.1 {
                    self.0.unlisten(id);
                }
            }
        }
        let _listener = Unlisten(app.clone(), listener);
        let state = worker;
        let run:Result<()>=async{
    let mut timer=tokio::time::interval(std::time::Duration::from_millis(200));let mut heartbeat=std::time::Instant::now();
    loop{tokio::select!{
      _=timer.tick()=>{
       if owner&&heartbeat.elapsed().as_secs()>=2{ensure!(state.sessions.lock().map_err(|_|anyhow!("Session lock"))?.contains_key(&session_id),"Owner terminal closed");c.execute(&format!("SELECT {schema}.renew_terminal($1)"),&[&share_id]).await?;heartbeat=std::time::Instant::now();}
       let row=c.query_opt(&format!("SELECT lease,writer::text,owner::text FROM {schema}.terminal_sessions WHERE id=$1 AND expires_at>now()"),&[&share_id]).await?.ok_or_else(||anyhow!("Shared terminal ended"))?;
       lease=row.get(0);let writer:String=row.get(1);if owner{state.shared_writers.lock().map_err(|_|anyhow!("Share lock"))?.insert(session_id.clone(),writer==p.username);}
       let _=app.emit("share-event",json!({"id":share_id,"sessionId":session_id,"writer":writer,"owner":row.get::<_,String>(2)}));
       let frames=c.query(&format!("SELECT seq,kind,payload,lease FROM {schema}.terminal_frames WHERE session_id=$1 AND seq>$2 AND expires_at>now() ORDER BY seq LIMIT 512"),&[&share_id,&cursor]).await?;
       for row in frames{cursor=row.get(0);let kind:String=row.get(1);let frame_lease:Uuid=row.get(3);if kind=="input"&&frame_lease!=lease{continue;}let payload:Vec<u8>=row.get(2);let data=crypto::unseal(&key,&payload,aad(share_id,frame_lease,&kind).as_bytes())?;
        if owner&&kind=="input"{owner_sender.as_ref().unwrap().send(SessionInput::Data(data.to_vec())).await?;}else if !owner&&kind=="output"{event(&app,&session_id,"data",json!(STANDARD.encode(&*data)));}else if kind=="close"{return Ok(());}
       }
      },
      data=output_rx.recv(),if owner=>{if let Some(data)=data{let body=crypto::seal(&key,&data,aad(share_id,lease,"output").as_bytes())?;c.execute(&format!("SELECT {schema}.send_terminal_frame($1,$2,'output',$3)"),&[&share_id,&lease,&body]).await?;}},
      input=input_rx.recv(),if !owner=>{match input{Some(SessionInput::Data(data))=>{let body=crypto::seal(&key,&data,aad(share_id,lease,"input").as_bytes())?;if let Err(e)=c.execute(&format!("SELECT {schema}.send_terminal_frame($1,$2,'input',$3)"),&[&share_id,&lease,&body]).await{event(&app,&session_id,"status",json!(format!("Read only / control changed: {}",e)));}},Some(SessionInput::Resize(..))=>{},_=>break,}}
    }}Ok(())
   }.await;
        if let Err(e) = run {
            if !owner {
                event(&app, &session_id, "error", json!(format!("{:#}", e)));
            }
            let _ = app.emit(
                "share-event",
                json!({"id":share_id,"ended":true,"error":e.to_string()}),
            );
        }
        if owner {
            let _ = control(&p, &share_id.to_string(), &p.username, true).await;
        } else {
            state.sessions.lock().unwrap().remove(&session_id);
            event(&app, &session_id, "closed", Value::Null);
        }
        state.shared_writers.lock().unwrap().remove(&session_id);
    });
    state
        .shares
        .lock()
        .map_err(|_| anyhow!("Share lock"))?
        .insert(task_key, task);
    Ok(result)
}
