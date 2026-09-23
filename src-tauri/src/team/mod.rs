//! PostgreSQL-only Team accounts. Tokens and connection material stay in Rust.
pub mod cache;
pub mod crypto;
#[cfg(test)] mod tests;
use crate::{model::{Record,VaultInfo},state::Shared,sync::{self,SyncProfile}};
use anyhow::{anyhow,ensure,Context,Result};
use cache::{Cache,CachedRecord,Draft};
use serde::{Serialize,Deserialize};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{path::PathBuf,time::Duration};
use tauri::{AppHandle,Emitter,Manager,State};
use uuid::Uuid;
use zeroize::{Zeroizing,Zeroize};

#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
struct Auth { profile:SyncProfile,user_id:String,username:String,device_id:String,access:String,refresh:String,delivery:String,identity:Vec<u8>,expires:i64 }
impl Drop for Auth {fn drop(&mut self){self.profile.password.zeroize();self.access.zeroize();self.refresh.zeroize();self.delivery.zeroize();self.identity.zeroize();}}
pub struct Account { cache:Cache,auth:Auth,vault:Option<(String,String,String)>,online:bool,error:Option<String> }
fn err(e:anyhow::Error)->String{format!("{e:#}")}
fn now()->i64{chrono::Utc::now().timestamp_millis()}
fn cache_path(app:&AppHandle,p:&SyncProfile,login:&str)->Result<PathBuf>{
 let id=hex::encode(Sha256::digest(format!("{}:{}:{}:{}",p.host,p.port,p.database,login.to_lowercase())));
 Ok(app.path().app_data_dir()?.join("team").join(format!("{id}.ttteam")))
}
fn with<T>(s:&Shared,f:impl FnOnce(&mut Account)->Result<T>)->Result<T>{let mut g=s.team.lock().map_err(|_|anyhow!("Team lock"))?; f(g.as_mut().ok_or_else(||anyhow!("Team hesabına giriş yapın"))?)}
pub fn active(s:&Shared)->bool{s.team.lock().map(|s|s.is_some()).unwrap_or(true)}
fn identity(a:&Auth)->Result<Zeroizing<[u8;32]>>{let mut key=Zeroizing::new([0;32]);ensure!(a.identity.len()==32,"Invalid local identity");key.copy_from_slice(&a.identity);Ok(key)}
fn auth_error(e:&anyhow::Error)->bool{e.chain().filter_map(|e|e.downcast_ref::<tokio_postgres::Error>()).any(|e|e.as_db_error().is_some_and(|e|matches!(e.code().code(),"28000"|"28P01"|"42501")))}
async fn connect_checked(profile:&SyncProfile)->Result<tokio_postgres::Client>{
 let client=sync::connect(profile).await?;
 let ready:bool=client.query_one("SELECT to_regclass('termterm_team.users') IS NOT NULL",&[]).await?.get(0);
 ensure!(ready,"Team şeması kurulmamış. Ayrı migration hesabıyla migrations/team betiklerini çalıştırın; uygulama hesabı şema oluşturamaz.");
 let privileged:bool=client.query_one("SELECT rolsuper OR rolbypassrls OR rolcreaterole OR has_table_privilege(current_user,'termterm_team.users','SELECT') OR pg_has_role(current_user,'termterm_team_executor','MEMBER') FROM pg_roles WHERE rolname=current_user",&[]).await?.get(0);
 ensure!(!privileged,"Team bağlantısı sınırlı uygulama rolü gerektirir; migration / tablo sahibi / superuser hesabı kullanılamaz");
 Ok(client)
}
async fn raw_rpc(auth:&Auth,action:&str,body:&Value)->Result<Value>{
 let client=connect_checked(&auth.profile).await?;
 let row=tokio::time::timeout(Duration::from_secs(25),client.query_one("SELECT termterm_team.rpc($1,$2,$3)",&[&auth.access,&action,&body])).await.context("Team request timed out")??;
 Ok(row.get(0))
}
async fn rpc(s:&Shared,action:&str,body:&Value)->Result<Value>{
 let _rpc=s.team_rpc_gate.lock().await;
 let mut auth=with(s,|a|Ok(a.auth.clone()))?;
 if auth.expires<=now()+30_000 {
  let client=connect_checked(&auth.profile).await?;
  let refreshed:Value=client.query_one("SELECT termterm_team.refresh_session($1)",&[&auth.refresh]).await?.get(0);
  auth.access=refreshed["accessToken"].as_str().ok_or_else(||anyhow!("Invalid session"))?.into();auth.refresh=refreshed["refreshToken"].as_str().unwrap_or_default().into();auth.expires=now()+3_600_000;
  with(s,|a|{a.auth=auth.clone();a.cache.set("auth",&auth)})?;
 }
 let result=raw_rpc(&auth,action,body).await;
 match &result {
  Ok(_)=>{with(s,|a|{a.online=true;a.error=None;Ok(())})?;}
  Err(e)=>{with(s,|a|{a.online=auth_error(e);a.error=Some(err(anyhow!(e.to_string())));Ok(())})?;}
 }
 result
}
fn public_status(a:&Account)->Result<Value>{
 let (pending,oldest,verified)=if let Some((v,_,_))=&a.vault {let p=a.cache.pending(v)?;(p.len(),p.first().map(|p|p.client_at.clone()),a.cache.get::<i64>(&format!("verified:{v}"))?)} else {(0,None,None)};
 Ok(json!({"user":{"id":a.auth.user_id,"username":a.auth.username},"online":a.online,"error":a.error,"vaultId":a.vault.as_ref().map(|v|&v.0),"pending":pending,"oldestPending":oldest,"verifiedAt":verified,"canEditOffline":verified.is_some_and(|t|cache::editable_at(t,now())),"authRejected":a.cache.get::<bool>("authRejected")?.unwrap_or(false),"overview":a.cache.get::<Value>("overview")?,"cachePath":a.cache.path}))
}
#[tauri::command]
pub fn team_status(state:State<Shared>)->std::result::Result<Option<Value>,String>{
 let g=state.team.lock().map_err(|_|"Team lock")?;g.as_ref().map(public_status).transpose().map_err(err)
}
#[tauri::command]
pub async fn team_auth(app:AppHandle,state:State<'_,Shared>,profile:SyncProfile,login:String,email:String,password:String,register:bool)->std::result::Result<Value,String>{
 let state=state.inner().clone(); let _gate=state.team_gate.lock().await;
 let password=Zeroizing::new(password);
 let result:Result<Value>=async {
  ensure!(password.chars().count()>=12 && password.len()<=72,"Parola en az 12 karakter, en fazla 72 UTF-8 baytı olmalı");
  let path=cache_path(&app,&profile,&login)?;
  let pw=password.clone();let path_clone=path.clone();
  let existing=tauri::async_runtime::spawn_blocking(move||if path_clone.exists(){Cache::open(&path_clone,&pw,false).map(Some)}else{Ok(None)}).await??;
  let device=existing.as_ref().map(|c|c.get::<String>("device")).transpose()?.flatten().unwrap_or_else(||Uuid::new_v4().to_string());
  let client=connect_checked(&profile).await?;
  let device_uuid=Uuid::parse_str(&device)?;
  let reply:Value=if register {
   let pw=password.clone();let(public,private)=tauri::async_runtime::spawn_blocking(move||crypto::identity(&pw)).await??;
   client.query_one("SELECT termterm_team.register_account($1,$2,$3,$4,$5,$6,$7)",&[&login,&email,&&*password,&hex::decode(public)?,&hex::decode(private)?,&device_uuid,&"TermTerm Desktop"]).await?.get(0)
  } else {client.query_one("SELECT termterm_team.login_account($1,$2,$3,$4)",&[&login,&&*password,&device_uuid,&"TermTerm Desktop"]).await?.get(0)};
  if let Some(e)=reply["error"].as_str(){if let Some(cache)=&existing{cache.set("authRejected",&true)?;}anyhow::bail!("{e}");}
  let cache=if let Some(cache)=existing{cache}else{let pw=password.clone();tauri::async_runtime::spawn_blocking(move||Cache::open(&path,&pw,true)).await??};
  cache.set("device",&device)?;
  let encoded=reply["privateIdentity"].as_str().ok_or_else(||anyhow!("Missing identity"))?.to_string();let pw=password.clone();
  let identity=tauri::async_runtime::spawn_blocking(move||crypto::unlock_identity(&pw,&encoded)).await??;
  let auth=Auth{profile,user_id:reply["userId"].as_str().unwrap_or_default().into(),username:reply["username"].as_str().unwrap_or_default().into(),device_id:device,access:reply["accessToken"].as_str().unwrap_or_default().into(),refresh:reply["refreshToken"].as_str().unwrap_or_default().into(),delivery:reply["deliveryToken"].as_str().unwrap_or_default().into(),identity:identity.to_vec(),expires:now()+3_600_000};
  let overview=raw_rpc(&auth,"overview",&json!({})).await?;
  cache.set("auth",&auth)?;cache.set("overview",&overview)?;cache.set("authRejected",&false)?;
  crate::commands::close_connections(&state).await.map_err(|e|anyhow!(e))?;
  *state.vault.lock().map_err(|_|anyhow!("Vault lock"))?=None;
  *state.team.lock().map_err(|_|anyhow!("Team lock"))?=Some(Account{cache,auth,vault:None,online:true,error:None});
  start_watch(&app,&state);with(&state,public_status_mut)
 }.await; result.map_err(err)
}
fn public_status_mut(a:&mut Account)->Result<Value>{public_status(a)}
#[tauri::command]
pub async fn team_offline(app:AppHandle,state:State<'_,Shared>,profile:SyncProfile,login:String,password:String)->std::result::Result<Value,String>{
 let state=state.inner().clone();let _gate=state.team_gate.lock().await;
 let result:Result<Value>=async{
  let path=cache_path(&app,&profile,&login)?;
  let cache=tauri::async_runtime::spawn_blocking(move||Cache::open(&path,&password,false)).await??;
  ensure!(!cache.get::<bool>("authRejected")?.unwrap_or(false),"Sunucu erişimi reddetti; çevrimdışı giriş yapılamaz");
  let auth=cache.get::<Auth>("auth")?.ok_or_else(||anyhow!("Önce çevrimiçi giriş yapın"))?;
  crate::commands::close_connections(&state).await.map_err(|e|anyhow!(e))?;
  *state.vault.lock().map_err(|_|anyhow!("Vault lock"))?=None;
  let vault=cache.get::<(String,String,String)>("selectedVault")?;
  *state.team.lock().map_err(|_|anyhow!("Team lock"))?=Some(Account{cache,auth,vault,online:false,error:Some("Çevrimdışı: yeni erişim iptalleri henüz doğrulanamadı".into())});
  start_watch(&app,&state);with(&state,public_status_mut)
 }.await;result.map_err(err)
}
#[tauri::command]
pub async fn team_logout(state:State<'_,Shared>)->std::result::Result<(),String>{
 let _gate=state.team_gate.lock().await;
 crate::commands::close_connections(&state).await?;
 let _=flush_activity(&state).await;
 let _=rpc(&state,"logout",&json!({})).await;
 let _=with(&state,|a|a.cache.set("authRejected",&true));
 if let Some(t)=state.team_task.lock().map_err(|_|"Team lock")?.take(){t.abort();}
 *state.team.lock().map_err(|_|"Team lock")?=None;Ok(())
}
fn start_watch(app:&AppHandle,s:&Shared){
 let app=app.clone();let state=s.clone();
 if let Ok(mut task)=s.team_task.lock(){if let Some(old)=task.take(){old.abort();}
 *task=Some(tokio::spawn(async move{
  loop {tokio::time::sleep(Duration::from_secs(15)).await;let _gate=state.team_gate.lock().await;
   if !active(&state){break;}
   if let Err(e)=synchronize(&app,&state).await {
    let rejected=auth_error(&e);let session_invalid=e.chain().filter_map(|e|e.downcast_ref::<tokio_postgres::Error>()).any(|e|e.as_db_error().is_some_and(|e|matches!(e.code().code(),"28000"|"28P01")));let _=with(&state,|a|{a.online=false;a.error=Some(err(e));if rejected{if let Some((v,_,_))=a.vault.clone(){a.cache.revoke(&v)?;}if session_invalid{a.cache.set("authRejected",&true)?;}}Ok(())});
    if rejected{let _=crate::commands::close_connections(&state).await;}
    if rejected{if let Ok(vault)=info(&state){let _=app.emit("team-vault",vault);}}
   }
   if let Ok(status)=with(&state,public_status_mut){let _=app.emit("team-status",status);}
  }
 }));}
}
async fn load_snapshot(app:&AppHandle,s:&Shared)->Result<()> {
 let selected=with(s,|a|Ok(a.vault.clone()))?;let Some((v,_,_))=selected else{return Ok(())};
 let response=rpc(s,"records",&json!({"vaultId":v})).await?;
 let revoked=with(s,|a|a.cache.apply_permissions(&v,response["records"].as_array().ok_or_else(||anyhow!("Invalid permission response"))?))?;
 if revoked{crate::commands::close_connections(s).await.map_err(|e|anyhow!(e))?;}
 let key=with(s,|a|identity(&a.auth))?;
 let dependencies=rpc(s,"dependency_cache",&json!({"vaultId":v})).await?;
 let dependencies=dependencies.as_array().ok_or_else(||anyhow!("Invalid dependency cache"))?.iter().map(|r|crypto::decrypt(&key,&v,r)).collect::<Result<Vec<_>>>()?;
 let mut records=vec![];
 for row in response["records"].as_array().ok_or_else(||anyhow!("Invalid Team snapshot"))? {
  let record=crypto::decrypt(&key,&v,row)?;
  records.push(CachedRecord{record,revision:row["revision"].as_i64().unwrap_or(0),permissions:serde_json::from_value(row["permissions"].clone())?,deleted:row["deleted"]==true});
  with(s,|a|a.cache.set(&format!("recipients:{v}:{}",row["id"].as_str().unwrap_or_default()),&row["recipients"]))?;
 }
 for path in response["paths"].as_array().into_iter().flatten(){let label=dependencies.iter().find(|r|r.id==path["id"].as_str().unwrap_or_default()).and_then(|r|r.data["label"].as_str()).unwrap_or("Erişim yolu");records.push(CachedRecord{record:Record{id:path["id"].as_str().unwrap_or_default().into(),kind:"group".into(),data:json!({"label":label,"groupId":path["parentId"],"_teamPathOnly":true}),updated_at:0},revision:0,permissions:vec![],deleted:false});}
 let revoked=with(s,|a|{a.cache.set(&format!("acl:{v}"),&response["aclRevision"])?;a.cache.set(&format!("recipients:{v}:"),&response["rootRecipients"])?;a.cache.snapshot(&v,&records,now())})?;
 if revoked{crate::commands::close_connections(s).await.map_err(|e|anyhow!(e))?;with(s,|a|a.cache.purge_connection_material(&v))?;}
 with(s,|a|{a.cache.set(&format!("dependencies:{v}"),&dependencies)?;a.cache.set(&format!("revoked:{v}"),&false)})?;
 let info=with(s,|a|{let(_,name,_)=a.vault.as_ref().unwrap();a.cache.info(&v,name,&a.auth.device_id)})?;
 app.emit("team-vault",&info)?;Ok(())
}
fn links(record:&Record)->Vec<Value>{let mut links=vec![];if let Some(id)=record.data["credentialId"].as_str().filter(|s|!s.is_empty()){links.push(json!({"targetId":id,"relation":"identity"}));}for id in record.data["chain"].as_array().into_iter().flatten(){links.push(json!({"targetId":id,"relation":"chain"}));}links}
async fn prepared(s:&Shared,draft:&Draft)->Result<Value>{
 let recipient=rpc(s,"recipients",&json!({"vaultId":draft.vault_id,"recordId":if draft.expected_revision>0{Some(&draft.record.id)}else{None},"parentId":draft.record.data["groupId"]})).await?;
 let mut body=crypto::encrypt(&draft.vault_id,&draft.record,draft.expected_revision+1,&recipient["recipients"])?;
 let object=body.as_object_mut().unwrap();
 for(k,v)in json!({"vaultId":draft.vault_id,"recordId":draft.record.id,"operationId":draft.operation_id,"sequence":draft.sequence,"expectedRevision":draft.expected_revision,"kind":draft.record.kind,"parentId":draft.record.data["groupId"],"clientAt":draft.client_at,"deleted":draft.deleted,"restoreFrom":draft.restore_from,"aclRevision":recipient["aclRevision"],"links":links(&draft.record)}).as_object().unwrap(){object.insert(k.clone(),v.clone());}
 Ok(body)
}
async fn synchronize(app:&AppHandle,s:&Shared)->Result<Value>{
 let overview=rpc(s,"overview",&json!({})).await?;with(s,|a|a.cache.set("overview",&overview))?;
 let selected=with(s,|a|Ok(a.vault.clone()))?;let Some((v,_,_))=selected else{return Ok(overview)};
 // Verify fresh permissions BEFORE preparing any recipient envelopes.
 if let Err(e)=load_snapshot(app,s).await {
  if auth_error(&e) {
   deliver_revoked(s,&v).await?;
   with(s,|a|a.cache.revoke(&v))?;
   crate::commands::close_connections(s).await.map_err(|e|anyhow!(e))?;
  }
  return Err(e);
 }
 let drafts=with(s,|a|a.cache.pending(&v))?;
 let mut previous=std::collections::HashMap::new();
 for mut draft in drafts {
  if let Some(rev)=with(s,|a|a.cache.last_applied(&v,&draft.record.id,draft.sequence,draft.expected_revision))? {draft.expected_revision=rev;}
  if let Some(rev)=previous.get(&draft.record.id){draft.expected_revision=*rev;}
  let body=match prepared(s,&draft).await {Ok(body)=>body,Err(e) if auth_error(&e)=>{deliver_revoked(s,&v).await?;break;},Err(e)=>return Err(e)};
  with(s,|a|a.cache.set(&format!("prepared:{}",draft.operation_id),&body))?;
  let result=rpc(s,"apply",&body).await?;
  with(s,|a|a.cache.receipt(&draft.operation_id,&result))?;
  if result["status"]=="applied"{previous.insert(draft.record.id.clone(),result["revision"].as_i64().unwrap_or(0));}else{break;}
 }
 load_snapshot(app,s).await?;flush_activity(s).await?;Ok(overview)
}
async fn deliver_revoked(s:&Shared,v:&str)->Result<()> {
 let(auth,drafts)=with(s,|a|Ok((a.auth.clone(),a.cache.pending(v)?)))?;
 let client=sync::connect(&auth.profile).await?;
 for draft in drafts {
  let body=with(s,|a|{
   if let Some(body)=a.cache.get::<Value>(&format!("prepared:{}",draft.operation_id))?{return Ok(body);}
   let scope=if draft.expected_revision>0{draft.record.id.as_str()}else{draft.record.data["groupId"].as_str().unwrap_or_default()};
   let recipients=a.cache.get::<Value>(&format!("recipients:{v}:{scope}"))?.ok_or_else(||anyhow!("Bekleyen kayıt teslimi için önceki alıcı anahtarları eksik"))?;
   let mut body=crypto::encrypt(v,&draft.record,draft.expected_revision+1,&recipients)?;
   for(k,value)in json!({"vaultId":v,"recordId":draft.record.id,"operationId":draft.operation_id,"sequence":draft.sequence,"expectedRevision":draft.expected_revision,"clientAt":draft.client_at,"kind":draft.record.kind,"parentId":draft.record.data["groupId"],"deleted":draft.deleted}).as_object().unwrap(){body[k]=value.clone();}Ok(body)
  })?;
  let result:Value=client.query_one("SELECT termterm_team.deliver_pending($1,$2)",&[&auth.delivery,&body]).await?.get(0);
  with(s,|a|a.cache.receipt(&draft.operation_id,&result))?;
 }
 Ok(())
}
#[tauri::command]
pub async fn team_request(app:AppHandle,state:State<'_,Shared>,action:String,body:Value)->std::result::Result<Value,String>{
 let _gate=state.team_gate.lock().await;
 let result:Result<Value>=async{
  if action=="sync"{return synchronize(&app,&state).await;}
  if action=="acl_set" {
   let preview=rpc(&state,"acl_preview",&body).await?;let key=with(&state,|a|identity(&a.auth))?;let vault=body["vaultId"].as_str().ok_or_else(||anyhow!("Vault missing"))?;
   let mut rewrap=vec![];
   for record in preview.as_array().ok_or_else(||anyhow!("Permission preview missing"))? {
    let history=rpc(&state,"history",&json!({"vaultId":vault,"recordId":record["id"]})).await?;
    for mut row in history.as_array().into_iter().flatten().cloned(){row["id"]=record["id"].clone();rewrap.extend(crypto::rewrap(&key,vault,&row,&record["recipients"])?);}
   }
   let mut body=body;body["rewrap"]=json!(rewrap);let result=rpc(&state,"acl_set",&body).await?;synchronize(&app,&state).await?;return Ok(result);
  }
  ensure!(["overview","create_team","create_vault","vaults","search_users","members","member_set","delegate","audit","maintenance","conflicts","move_preview"].contains(&action.as_str()),"Bu Team işlemi arayüze açık değil");
  // Encrypted conflict payloads are decrypted natively and secrets are redacted.
  let mut response=rpc(&state,&action,&body).await?;
  if action=="vaults" {with(&state,|a|a.cache.set(&format!("vault-list:{}",body["teamId"].as_str().unwrap_or_default()),&response))?;}
  if action=="conflicts" {let key=with(&state,|a|identity(&a.auth))?;for conflict in response.as_array_mut().into_iter().flatten(){let mut row=conflict["candidate"].clone();row["id"]=row["recordId"].clone();row["revision"]=json!(row["expectedRevision"].as_i64().unwrap_or(0)+1);let record=crypto::decrypt(&key,body["vaultId"].as_str().unwrap_or_default(),&row)?;conflict["candidateRecord"]=serde_json::to_value(crypto::split(&record).0)?;conflict.as_object_mut().unwrap().remove("candidate");}}
  Ok(response)
 }.await;result.map_err(err)
}
#[tauri::command]
pub async fn team_open_vault(app:AppHandle,state:State<'_,Shared>,vault_id:String,name:String,team_id:String)->std::result::Result<VaultInfo,String>{
 let _gate=state.team_gate.lock().await;
 let result:Result<VaultInfo>=async{
  crate::commands::close_connections(&state).await.map_err(|e|anyhow!(e))?;
  let known=if with(&state,|a|Ok(a.online))?{rpc(&state,"vaults",&json!({"teamId":team_id})).await?}else{with(&state,|a|Ok(a.cache.get::<Value>(&format!("vault-list:{team_id}"))?.unwrap_or(json!([]))))?};
  ensure!(known.as_array().is_some_and(|list|list.iter().any(|v|v["id"]==vault_id&&v["name"]==name)),"Bu kasa hesaba atanmış veya çevrimdışı indirilmiş değil");
  with(&state,|a|{let selected=(vault_id,name,team_id);a.cache.set("selectedVault",&selected)?;a.vault=Some(selected);Ok(())})?;
  if with(&state,|a|Ok(a.online))?{synchronize(&app,&state).await?;}
  info(&state)
 }.await;result.map_err(err)
}
#[tauri::command]
pub async fn team_decide(app:AppHandle,state:State<'_,Shared>,conflict_id:String,current_revision:i64,choice:String,merged:Option<Record>)->std::result::Result<Value,String>{
 let _gate=state.team_gate.lock().await;
 let result:Result<Value>=async{
  let(v,key)=with(&state,|a|Ok((a.vault.as_ref().ok_or_else(||anyhow!("Ortak kasa seçin"))?.0.clone(),identity(&a.auth)?)))?;
  ensure!(["server","candidate","merge"].contains(&choice.as_str()),"Geçersiz karar");
  let conflicts=rpc(&state,"conflicts",&json!({"vaultId":v})).await?;
  let conflict=conflicts.as_array().and_then(|cs|cs.iter().find(|c|c["id"]==conflict_id)).ok_or_else(||anyhow!("Karar artık beklemiyor veya yetkiniz yok"))?;
  let id=conflict["recordId"].as_str().ok_or_else(||anyhow!("Missing record id"))?;
  let mut decision=json!({"vaultId":v,"conflictId":conflict_id,"currentRevision":current_revision,"choice":choice});
  let mut operation=None;
  if choice!="server" {
   let mut row=conflict["candidate"].clone();row["id"]=json!(id);row["revision"]=json!(row["expectedRevision"].as_i64().unwrap_or(0)+1);
   let candidate=crypto::decrypt(&key,&v,&row)?;
   let record=if choice=="merge"{let merged=merged.ok_or_else(||anyhow!("Birleştirilmiş kayıt eksik"))?;ensure!(merged.id==candidate.id&&merged.kind==candidate.kind,"Kayıt kimliği değiştirilemez");crypto::merge(crypto::split(&merged).0,&crypto::split(&candidate).1)}else{candidate};
   // A decision is an online transaction, never inserted in the offline edit queue.
   let draft=with(&state,|a|{if current_revision>0{a.cache.permitted(&v,id,"edit")?;}else if let Some(parent)=record.data["groupId"].as_str().filter(|s|!s.is_empty()){a.cache.permitted(&v,parent,"edit")?;}else{ensure!(a.cache.get::<Value>(&format!("recipients:{v}:"))?.and_then(|v|v.as_array().cloned()).is_some_and(|v|!v.is_empty()),"Kök klasörde oluşturma izni yok");}let sequence:i64=a.cache.conn.query_row("SELECT coalesce(max(sequence),0)+1 FROM journal",[],|r|r.get(0))?;Ok(Draft{operation_id:Uuid::new_v4().to_string(),sequence,vault_id:v.clone(),record,expected_revision:current_revision,deleted:row["deleted"]==true,client_at:chrono::Utc::now().to_rfc3339(),restore_from:None})})?;
   decision["change"]=prepared(&state,&draft).await?;
   with(&state,|a|{a.cache.record_online_receipt(&draft,&json!({"status":"online"}))?;a.cache.set(&format!("decision:{}",draft.operation_id),&decision)})?;
   operation=Some(draft);
  }
  let result=rpc(&state,"decide",&decision).await?;
  if let Some(draft)=operation {with(&state,|a|a.cache.record_online_receipt(&draft,&result))?;}
  load_snapshot(&app,&state).await?;Ok(result)
 }.await;result.map_err(err)
}
pub fn info(s:&Shared)->Result<VaultInfo>{with(s,|a|{let(v,name,_)=a.vault.as_ref().ok_or_else(||anyhow!("Ortak kasa seçin"))?;a.cache.info(v,name,&a.auth.device_id)})}
#[tauri::command]
pub async fn team_move(app:AppHandle,state:State<'_,Shared>,record_id:String,parent_id:Option<String>,acl_revision:i64,record_revision:i64)->std::result::Result<VaultInfo,String>{
 let _gate=state.team_gate.lock().await;
 let result:Result<VaultInfo>=async{
  let(v,key,mut record)=with(&state,|a|{let v=a.vault.as_ref().ok_or_else(||anyhow!("Kasa seçin"))?.0.clone();Ok((v.clone(),identity(&a.auth)?,a.cache.permitted(&v,&record_id,"manage")?.record))})?;
  let preview=rpc(&state,"move_preview",&json!({"vaultId":v,"recordId":record_id,"parentId":parent_id})).await?;
  ensure!(preview["aclRevision"]==acl_revision&&preview["recordRevision"]==record_revision,"Kayıt veya yetkiler değişti; taşıma önizlemesini yenileyin");
  let mut rewrap=vec![];
  for affected in preview["after"].as_array().ok_or_else(||anyhow!("Invalid preview"))? {
   let rows=rpc(&state,"history",&json!({"vaultId":v,"recordId":affected["id"]})).await?;
   for mut row in rows.as_array().into_iter().flatten().cloned(){row["id"]=affected["id"].clone();rewrap.extend(crypto::rewrap(&key,&v,&row,&affected["recipients"])?);}
  }
  record.data["groupId"]=json!(parent_id.clone().unwrap_or_default());
  let users=preview["after"].as_array().and_then(|a|a.iter().find(|r|r["id"]==record_id)).ok_or_else(||anyhow!("Missing move recipients"))?;
  let sequence=with(&state,|a|Ok(a.cache.conn.query_row("SELECT coalesce(max(sequence),0)+1 FROM journal",[],|r|r.get(0))?))?;
  let draft=Draft{operation_id:Uuid::new_v4().to_string(),sequence,vault_id:v.clone(),record:record.clone(),expected_revision:record_revision,deleted:false,client_at:chrono::Utc::now().to_rfc3339(),restore_from:None};
  let mut body=crypto::encrypt(&v,&record,record_revision+1,&users["recipients"])?;
  for(k,value)in json!({"operationId":draft.operation_id,"sequence":sequence,"vaultId":v,"recordId":record_id,"kind":record.kind,"parentId":parent_id,"expectedRevision":record_revision,"clientAt":draft.client_at,"deleted":false,"links":links(&record),"aclRevision":acl_revision,"movePreviewRevision":acl_revision,"rewrap":rewrap}).as_object().unwrap(){body[k]=value.clone();}
  with(&state,|a|{a.cache.record_online_receipt(&draft,&json!({"status":"online"}))?;a.cache.set(&format!("prepared:{}",draft.operation_id),&body)})?;
  let response=rpc(&state,"apply",&body).await?;with(&state,|a|a.cache.receipt(&draft.operation_id,&response))?;
  ensure!(response["status"]=="applied","Taşıma çakıştı; karar kuyruğunu inceleyin");
  load_snapshot(&app,&state).await?;info(&state)
 }.await;result.map_err(err)
}
pub fn save(s:&Shared,records:Vec<Record>,deleted:bool)->Result<VaultInfo>{
 with(s,|a|{
  let(v,_,team)=a.vault.clone().ok_or_else(||anyhow!("Ortak kasa seçin"))?;
  if records.iter().all(|r|r.kind=="settings") {
   ensure!(!deleted,"Yerel tercih kaydı silinemez");a.cache.set("preferences",&records)?;
   return a.cache.info(&v,&a.vault.as_ref().unwrap().1,&a.auth.device_id);
  }
  if records.iter().all(|r|r.kind=="knownHost") {
   let mut known=a.cache.get::<Vec<Record>>(&format!("known:{v}"))?.unwrap_or_default();
   for record in records {known.retain(|r|r.id!=record.id);if !deleted{known.push(record);}}
   a.cache.set(&format!("known:{v}"),&known)?;
   return a.cache.info(&v,&a.vault.as_ref().unwrap().1,&a.auth.device_id);
  }
  let overview=a.cache.get::<Value>("overview")?.unwrap_or_default();let owner=overview["teams"].as_array().is_some_and(|ts|ts.iter().any(|t|t["id"]==team&&t["ownerId"]==a.auth.user_id));
  a.cache.enqueue_batch(&v,records,deleted,now(),owner)?;
  let(_,name,_)=a.vault.as_ref().unwrap();a.cache.info(&v,name,&a.auth.device_id)
 })
}
#[tauri::command]
pub async fn team_save(app:AppHandle,state:State<'_,Shared>,record:Record,deleted:bool)->std::result::Result<VaultInfo,String>{
 let _gate=state.team_gate.lock().await;let result=save(&state,vec![record],deleted).map_err(err)?;
 let _=synchronize(&app,&state).await;Ok(result)
}
#[tauri::command]
pub async fn team_history(state:State<'_,Shared>,record_id:String)->std::result::Result<Value,String>{
 let _gate=state.team_gate.lock().await;
 let result:Result<Value>=async{
  let(v,key)=with(&state,|a|Ok((a.vault.as_ref().ok_or_else(||anyhow!("Select vault"))?.0.clone(),identity(&a.auth)?)))?;
  let mut rows=rpc(&state,"history",&json!({"vaultId":v,"recordId":record_id})).await?;
  for row in rows.as_array_mut().into_iter().flatten(){row["id"]=json!(record_id);let record=crypto::decrypt(&key,&v,row)?;row["record"]=serde_json::to_value(crypto::split(&record).0)?;for field in ["payload","secrets","envelopes"]{row.as_object_mut().unwrap().remove(field);}}
  Ok(rows)
 }.await;result.map_err(err)
}
#[tauri::command]
pub async fn team_checkout(app:AppHandle,state:State<'_,Shared>,record_id:String,revision:Option<i64>,permanent:bool)->std::result::Result<Value,String>{
 let _gate=state.team_gate.lock().await;
 let result:Result<Value>=async{
  let(v,key)=with(&state,|a|Ok((a.vault.as_ref().ok_or_else(||anyhow!("Select vault"))?.0.clone(),identity(&a.auth)?)))?;
  if let Some(revision)=revision {
   let rows=rpc(&state,"history",&json!({"vaultId":v,"recordId":record_id})).await?;
   let mut row=rows.as_array().and_then(|a|a.iter().find(|r|r["revision"]==revision)).ok_or_else(||anyhow!("Sürüm bulunamadı"))?.clone();row["id"]=json!(record_id);
   let record=crypto::decrypt(&key,&v,&row)?;
   if permanent {
    let draft=with(&state,|a|a.cache.enqueue(&v,record,false,now(),Some(revision),false))?;
    let body=prepared(&state,&draft).await?;let result=rpc(&state,"apply",&body).await?;with(&state,|a|a.cache.receipt(&draft.operation_id,&result))?;load_snapshot(&app,&state).await?;return Ok(result);
   }
   rpc(&state,"pin",&json!({"vaultId":v,"recordId":record_id,"revision":revision,"purpose":"temporary"})).await?;
   with(&state,|a|a.cache.set(&format!("checkout:{v}:{record_id}"),&Some(record)))?;
   Ok(json!({"temporary":true,"revision":revision}))
  }else{
   with(&state,|a|a.cache.set(&format!("checkout:{v}:{record_id}"),&Option::<Record>::None))?;
   rpc(&state,"unpin",&json!({"vaultId":v,"recordId":record_id,"purpose":"temporary"})).await?;Ok(json!({"temporary":false}))
  }
 }.await;result.map_err(err)
}
pub async fn connection_records(s:&Shared,id:&str)->Result<(Vec<Record>,String)>{
 let (v,online)=with(s,|a|{let v=a.vault.as_ref().ok_or_else(||anyhow!("Ortak kasa seçin"))?.0.clone();ensure!(!a.cache.get::<bool>("authRejected")?.unwrap_or(false)&&!a.cache.get::<bool>(&format!("revoked:{v}"))?.unwrap_or(false),"Team erişimi reddedildi");a.cache.permitted(&v,id,"connect")?;Ok((v,a.online))})?;
 let mut records=if online {
  let rows=match rpc(s,"connection_records",&json!({"vaultId":v,"recordId":id})).await {Ok(rows)=>Some(rows),Err(e) if !auth_error(&e)&&!e.chain().filter_map(|e|e.downcast_ref::<tokio_postgres::Error>()).any(|e|e.as_db_error().is_some())=>None,Err(e)=>return Err(e)};
  let key=with(s,|a|identity(&a.auth))?;let records=if let Some(rows)=rows{rows.as_array().ok_or_else(||anyhow!("Invalid connection response"))?.iter().map(|r|crypto::decrypt(&key,&v,r)).collect::<Result<Vec<_>>>()?}else{with(s,|a|Ok(a.cache.get::<Vec<Record>>(&format!("dependencies:{v}"))?.unwrap_or_default()))?};
  with(s,|a|a.cache.set(&format!("connection:{v}:{id}"),&records))?;records
 }else{with(s,|a|a.cache.get::<Vec<Record>>(&format!("dependencies:{v}")))?.ok_or_else(||anyhow!("Bu bağlantı çevrimdışı kullanım için daha önce indirilmedi"))?};
 if let Some(Some(record))=with(s,|a|a.cache.get::<Option<Record>>(&format!("checkout:{v}:{id}")))?{if let Some(current)=records.iter_mut().find(|r|r.id==id){*current=record;}}
 // Portable Team material never implicitly reads arbitrary device-local key paths.
 for r in &mut records{for field in ["keyPath","certificatePath"]{if r.data[field].as_str().is_some_and(|s|!s.is_empty()){anyhow::bail!("Team bağlantısı taşınabilir anahtar içeriği gerektirir; yerel anahtar yolu kullanılamaz");}}}
 Ok((records,v))
}
pub fn local_known(s:&Shared,vault:&str)->Result<Vec<Record>>{with(s,|a|Ok(a.cache.get::<Vec<Record>>(&format!("known:{vault}"))?.unwrap_or_default()))}
pub fn trust_known(s:&Shared,vault:&str,record:Record)->Result<()>{with(s,|a|{let mut records=a.cache.get::<Vec<Record>>(&format!("known:{vault}"))?.unwrap_or_default();records.push(record);a.cache.set(&format!("known:{vault}"),&records)})}
pub fn export_records(s:&Shared,ids:&[String],secrets:bool)->Result<Vec<Record>> {
 with(s,|a|{
  let(v,_,_)=a.vault.as_ref().ok_or_else(||anyhow!("Ortak kasa seçin"))?;
  ensure!(!a.cache.get::<bool>("authRejected")?.unwrap_or(false),"Team erişimi reddedildi");
  let all=a.cache.records(v)?;
  let selected=all.into_iter().filter(|r|!r.deleted && r.permissions.iter().any(|p|p=="read") && (ids.is_empty()||ids.contains(&r.record.id))).collect::<Vec<_>>();
  ensure!(ids.iter().all(|id|selected.iter().any(|r|&r.record.id==id)),"Seçilen kayıt erişilebilir değil");
  selected.into_iter().map(|r|{ensure!(r.permissions.iter().any(|p|p=="export"),"Dışa aktarma yetkisi yok: {}",r.record.data["label"]);if secrets {ensure!(r.permissions.iter().any(|p|p=="reveal"),"Sırları dışa aktarmak için ayrıca sırları görüntüleme izni gerekir");Ok(r.record)}else{Ok(crypto::split(&r.record).0)}}).collect()
 })
}

pub struct ConnectionAudit { state:std::sync::Weak<crate::state::AppState>,host:String,vault:String }
impl ConnectionAudit {pub fn record(&self,event:&str,bytes:u64){if let Some(s)=self.state.upgrade(){let _=activity(&s,&self.vault,&self.host,event,bytes);}}}
impl Drop for ConnectionAudit {fn drop(&mut self){if let Some(s)=self.state.upgrade(){let _=activity(&s,&self.vault,&self.host,"connection.closed",0);}}}
pub fn connection_opened(s:&Shared,vault:&str,host:&str)->Result<Option<ConnectionAudit>> {
 if !active(s){return Ok(None);} activity(s,vault,host,"connection.opened",0)?;
 Ok(Some(ConnectionAudit{state:std::sync::Arc::downgrade(s),host:host.into(),vault:vault.into()}))
}
pub fn activity(s:&Shared,vault:&str,host:&str,event:&str,bytes:u64)->Result<()> {
 if !active(s){return Ok(());}
 with(s,|a|{let mut queue=a.cache.get::<Vec<Value>>("auditQueue")?.unwrap_or_default();queue.push(json!({"vaultId":vault,"recordId":host,"event":event,"bytes":bytes,"operationId":Uuid::new_v4().to_string(),"clientAt":chrono::Utc::now().to_rfc3339()}));a.cache.set("auditQueue",&queue)})
}
async fn flush_activity(s:&Shared)->Result<()> {
 let queue=with(s,|a|Ok(a.cache.get::<Vec<Value>>("auditQueue")?.unwrap_or_default()))?;
 for event in &queue {rpc(s,"activity",event).await?;with(s,|a|{let mut pending=a.cache.get::<Vec<Value>>("auditQueue")?.unwrap_or_default();pending.retain(|p|p["operationId"]!=event["operationId"]);a.cache.set("auditQueue",&pending)})?;}
 Ok(())
}
