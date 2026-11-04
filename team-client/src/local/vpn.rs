//! VPN cache/outbox is separate from resource candidates. No OTP or browser state is accepted.
use super::*;
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use termterm_vpn_core::Profile;
use uuid::Uuid;

const EDIT_WINDOW: i64=24*60*60*1000;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CachedVpn {
    pub id:Uuid, pub revision:i64, pub edit_revision:i64, pub generation:i64, pub verified_ms:i64,
    pub view:Value, pub profile:Option<Profile>, pub auth_id:Option<Uuid>, pub auth_revision:i64, pub edit_auth_revision:i64,
}
impl CachedVpn {
    pub fn permitted(&self,p:&str)->bool {self.view["permissions"].as_array().is_some_and(|a|a.iter().any(|x|x==p))}
    pub fn view(&self)->Value {let mut v=self.view.clone();v["revision"]=json!(self.edit_revision);v["localGeneration"]=json!(self.generation);v["cached"]=json!(true);v["pending"]=json!(self.edit_revision!=self.revision || self.edit_auth_revision!=self.auth_revision);if self.revision==0 {v["canRemove"]=json!(false);}v}
}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Draft {
    pub id:Uuid,pub device:Uuid,pub sequence:i64,pub vpn_id:Uuid,pub base_revision:i64,pub client_ms:i64,
    pub record:CachedVpn,pub configuration:bool,pub authentication:bool,pub auth_base:i64,
    pub creation_authority:Option<Uuid>,pub shared_plan:Option<Value>,pub delivery:Option<Value>,pub receipt:Option<Value>,
}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Requirement {root:Uuid,hop:Uuid,vpn:Option<Uuid>,plan_hash:String}
pub(super) fn migrate(tx:&Transaction<'_>)->Result<()> {
    tx.execute_batch("CREATE TABLE cached_vpn(id TEXT PRIMARY KEY,generation INTEGER NOT NULL,ciphertext BLOB NOT NULL);
      CREATE TABLE vpn_operations(id TEXT PRIMARY KEY,sequence INTEGER NOT NULL UNIQUE,vpn_id TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN ('pending','sending','applied','conflict','quarantined')),ciphertext BLOB NOT NULL);
      CREATE INDEX vpn_pending ON vpn_operations(sequence);
      CREATE TABLE vpn_requirements(root_id TEXT NOT NULL,hop_id TEXT NOT NULL,ciphertext BLOB NOT NULL,PRIMARY KEY(root_id,hop_id));
      PRAGMA user_version=9;")?;Ok(())
}
fn seal<T:Serialize>(key:&[u8;32],aad:&str,value:&T)->Result<Vec<u8>> {
    let bytes=Zeroizing::new(serde_json::to_vec(value)?);ensure!(bytes.len()<=4*1024*1024,"VPN cache entry too large");
    crypto::seal(key,&bytes,aad.as_bytes())
}
fn open<T:for<'a> Deserialize<'a>>(key:&[u8;32],aad:&str,bytes:&[u8])->Result<T>{Ok(serde_json::from_slice(&crypto::unseal(key,bytes,aad.as_bytes())?)?)}
fn read_record(tx:&Transaction<'_>,key:&[u8;32],scope:&str,id:Uuid)->Result<Option<CachedVpn>> {
    let row:Option<(i64,Vec<u8>)>=tx.query_row("SELECT generation,ciphertext FROM cached_vpn WHERE id=?1",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    row.map(|(generation,bytes)|{let r:CachedVpn=open(key,&format!("{scope}:vpn:{id}:{generation}"),&bytes)?;ensure!(r.id==id && r.generation==generation,"VPN cache metadata mismatch");Ok(r)}).transpose()
}
fn write_record(tx:&Transaction<'_>,key:&[u8;32],scope:&str,r:&CachedVpn)->Result<()> {
    let bytes=seal(key,&format!("{scope}:vpn:{}:{}",r.id,r.generation),r)?;
    tx.execute("INSERT INTO cached_vpn VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET generation=excluded.generation,ciphertext=excluded.ciphertext",params![r.id.to_string(),r.generation,bytes])?;Ok(())
}
fn write_draft(tx:&Transaction<'_>,key:&[u8;32],scope:&str,d:&Draft,state:&str)->Result<()> {
    let bytes=seal(key,&format!("{scope}:vpn-operation:{}:{}:{}:{state}",d.id,d.sequence,d.vpn_id),d)?;
    tx.execute("INSERT INTO vpn_operations VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET state=excluded.state,ciphertext=excluded.ciphertext",params![d.id.to_string(),d.sequence,d.vpn_id.to_string(),state,bytes])?;Ok(())
}
fn read_draft(tx:&Transaction<'_>,key:&[u8;32],scope:&str,id:Uuid)->Result<(Draft,String)> {
    let (sequence,vpn,state,bytes):(i64,String,String,Vec<u8>)=tx.query_row("SELECT sequence,vpn_id,state,ciphertext FROM vpn_operations WHERE id=?1",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    let d:Draft=open(key,&format!("{scope}:vpn-operation:{id}:{sequence}:{vpn}:{state}"),&bytes)?;
    ensure!(d.id==id && d.sequence==sequence && d.vpn_id.to_string()==vpn,"VPN draft metadata mismatch");Ok((d,state))
}
pub(super) fn revoke(tx:&Transaction<'_>,key:&[u8;32],scope:&str,id:Option<Uuid>)->Result<()> {
    let ids={let mut q=tx.prepare("SELECT id FROM vpn_operations WHERE (?1 IS NULL OR vpn_id=?1) AND state IN ('pending','sending')")?;let rows=q.query_map([id.map(|v|v.to_string())],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;rows};
    for id in ids {let (d,_)=read_draft(tx,key,scope,Uuid::parse_str(&id)?)?;write_draft(tx,key,scope,&d,"quarantined")?;}
    tx.execute("DELETE FROM cached_vpn WHERE ?1 IS NULL OR id=?1",[id.map(|v|v.to_string())])?;
    // An inaccessible profile must not leave an old verified-none route proof.
    tx.execute("DELETE FROM vpn_requirements",[])?;Ok(())
}
impl Store {
    pub(crate) fn cached_vpn(&mut self,id:Uuid)->Result<CachedVpn>{let tx=self.db.transaction()?;read_record(&tx,&self.key,&self.context,id)?.context("VPN not cached; connect online first")}
    pub(crate) fn vpn_status(&self)->Result<Value> {
        let (pending,conflicts,quarantined):(i64,i64,i64)=self.db.query_row("SELECT coalesce(sum(state IN ('pending','sending')),0),coalesce(sum(state='conflict'),0),coalesce(sum(state='quarantined'),0) FROM vpn_operations",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        Ok(json!({"pending":pending,"conflicts":conflicts,"quarantined":quarantined}))
    }
    pub(crate) fn reconcile_vpn(&mut self,visible:&std::collections::HashSet<Uuid>)->Result<()> {
        let ids={let mut q=self.db.prepare("SELECT id FROM cached_vpn")?;let rows=q.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;rows};
        for id in ids {let id=Uuid::parse_str(&id)?;if !visible.contains(&id) && self.cached_vpn(id)?.revision>0 {self.revoke_vpn(Some(id))?;}}Ok(())
    }
    pub(crate) fn cached_vpn_views(&mut self)->Result<Vec<Value>> {
        let tx=self.db.transaction()?;
        let ids={let mut q=tx.prepare("SELECT id FROM cached_vpn ORDER BY id")?;let rows=q.query_map([],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;rows};
        let mut views=vec![];
        for id in ids {let r=read_record(&tx,&self.key,&self.context,Uuid::parse_str(&id)?)?.context("VPN cache changed")?;if r.permitted("read") {views.push(r.view());}}
        Ok(views)
    }
    pub(crate) fn refresh_vpn(&mut self,mut record:CachedVpn)->Result<()> {
        ensure!(record.revision>0 && record.generation==0,"Invalid server VPN record");
        let tx=self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM vpn_operations WHERE vpn_id=?1 AND state IN ('pending','sending','conflict','quarantined'))",[record.id.to_string()],|r|r.get(0))?;
        let old=read_record(&tx,&self.key,&self.context,record.id)?;
        let lost=old.as_ref().is_some_and(|old|["read","connect","edit"].iter().any(|p|old.permitted(p) && !record.permitted(p)));
        if lost {revoke(&tx,&self.key,&self.context,Some(record.id))?;}
        if pending && !lost {if let Some(old)=old {
            record.profile=old.profile;record.revision=old.revision;record.edit_revision=old.edit_revision;record.generation=old.generation;
            record.auth_id=old.auth_id;record.auth_revision=old.auth_revision;record.edit_auth_revision=old.edit_auth_revision;
            for field in ["label","family","source","server","groupId","realm","username","remember","hasPassword","hasConfig"] {record.view[field]=old.view[field].clone();}
        }}
        write_record(&tx,&self.key,&self.context,&record)?;
        tx.commit()?;Ok(())
    }
    pub(crate) fn revoke_vpn(&mut self,id:Option<Uuid>)->Result<()> {
        let tx=self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;revoke(&tx,&self.key,&self.context,id)?;tx.commit()?;Ok(())
    }
    pub(crate) fn save_vpn_requirement(&mut self,root:Uuid,hop:Uuid,vpn:Option<Uuid>)->Result<()> {
        let plan=self.cached_connection(root)?;ensure!(plan.steps.iter().any(|s|s.id==hop),"Hop not in cached plan");
        let entry=self.read(&format!("connection:{root}"))?;
        let proof=Requirement{root,hop,vpn,plan_hash:hex::encode(Sha256::digest(&*entry.bytes))};
        let bytes=seal(&self.key,&format!("{}:vpn-required:{root}:{hop}",self.context),&proof)?;
        self.db.execute("INSERT INTO vpn_requirements VALUES(?1,?2,?3) ON CONFLICT(root_id,hop_id) DO UPDATE SET ciphertext=excluded.ciphertext",params![root.to_string(),hop.to_string(),bytes])?;Ok(())
    }
    pub(crate) fn cached_vpn_requirement(&self,root:Uuid,hop:Uuid)->Result<Option<Uuid>> {
        let plan=self.cached_connection(root)?;ensure!(plan.steps.iter().any(|s|s.id==hop),"Hop not in cached plan");
        let bytes:Vec<u8>=self.db.query_row("SELECT ciphertext FROM vpn_requirements WHERE root_id=?1 AND hop_id=?2",params![root.to_string(),hop.to_string()],|r|r.get(0)).optional()?.context("VPN requirement unknown; connect online first")?;
        let proof:Requirement=open(&self.key,&format!("{}:vpn-required:{root}:{hop}",self.context),&bytes)?;
        let entry=self.read(&format!("connection:{root}"))?;
        ensure!(proof.root==root && proof.hop==hop && proof.plan_hash==hex::encode(Sha256::digest(&*entry.bytes)),"VPN requirement proof changed; connect online first");
        if let Some(id)=proof.vpn {let row:Option<(i64,Vec<u8>)>=self.db.query_row("SELECT generation,ciphertext FROM cached_vpn WHERE id=?1",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;let (generation,bytes)=row.context("Required VPN not cached")?;let r:CachedVpn=open(&self.key,&format!("{}:vpn:{id}:{generation}",self.context),&bytes)?;ensure!(r.permitted("connect") && r.profile.is_some(),"permission_denied");}
        Ok(proof.vpn)
    }
    pub(crate) fn queue_vpn(&mut self,mut record:CachedVpn,configuration:bool,authentication:bool,now:i64,authority:Option<Uuid>)->Result<Uuid> {
        self.journal_status(now)?;
        ensure!(configuration || authentication,"Empty VPN operation");
        let tx=self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut clock=journal::clock(&tx,&self.key,&self.context,now)?;
        ensure!(clock.verified && clock.remaining_ms>0 && now>=record.verified_ms && now.saturating_sub(record.verified_ms)<EDIT_WINDOW,"VPN offline edit lease expired; reconnect first");
        ensure!((!configuration || record.permitted("edit")) && (!authentication || record.permitted("connect")),"permission_denied");
        ensure!(record.revision>0 || authority.is_some(),"Connect online before creating private VPN profiles offline");
        if let Some(profile)=&record.profile {crate::vpn::validate_configuration(profile)?;} else {anyhow::bail!("VPN configuration not cached");}
        let previous=read_record(&tx,&self.key,&self.context,record.id)?;
        if let Some(previous)=&previous {ensure!(record.generation==previous.generation && record.revision==previous.revision,"VPN local revision changed; refresh");}
        let blocked:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM vpn_operations WHERE vpn_id=?1 AND state IN ('conflict','quarantined'))",[record.id.to_string()],|r|r.get(0))?;
        ensure!(!blocked,"VPN draft requires a decision; encrypted local copy retained");
        let base=record.edit_revision;let auth_base=record.edit_auth_revision;
        if configuration {record.edit_revision=record.edit_revision.checked_add(1).context("VPN revision exhausted")?;}
        if authentication {record.auth_id=Some(record.auth_id.unwrap_or_else(Uuid::new_v4));record.edit_auth_revision=record.edit_auth_revision.checked_add(1).context("VPN auth revision exhausted")?;}
        clock.sequence=clock.sequence.checked_add(1).context("Device sequence exhausted")?;
        record.generation=record.generation.checked_add(1).context("VPN generation exhausted")?;
        let draft=Draft{id:Uuid::new_v4(),device:clock.device,sequence:clock.sequence,vpn_id:record.id,base_revision:base,client_ms:now,record:record.clone(),configuration,authentication,auth_base,creation_authority:authority,shared_plan:None,delivery:None,receipt:None};
        write_record(&tx,&self.key,&self.context,&record)?;write_draft(&tx,&self.key,&self.context,&draft,"pending")?;journal::write_clock(&tx,&self.key,&self.context,&clock)?;
        tx.commit()?;Ok(draft.id)
    }
    pub(crate) fn next_vpn_delivery(&mut self)->Result<Option<(Draft,String)>> {
        let tx=self.db.transaction()?;
        let id:Option<String>=tx.query_row("SELECT o.id FROM vpn_operations o WHERE o.state IN ('pending','sending') AND NOT EXISTS(SELECT 1 FROM vpn_operations x WHERE x.vpn_id=o.vpn_id AND x.sequence<o.sequence AND x.state<>'applied') ORDER BY sequence LIMIT 1",[],|r|r.get(0)).optional()?;
        id.map(|id|read_draft(&tx,&self.key,&self.context,Uuid::parse_str(&id)?)).transpose()
    }
    pub(crate) fn prepare_vpn_delivery(&mut self,id:Uuid,plan:Option<Value>,body:Option<Value>)->Result<()> {
        let tx=self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;let (mut d,state)=read_draft(&tx,&self.key,&self.context,id)?;
        ensure!(matches!(state.as_str(),"pending"|"sending"),"VPN draft not deliverable");
        if let Some(plan)=plan {if let Some(old)=&d.shared_plan {ensure!(*old==plan,"Cannot replace uncertain VPN plan");}d.shared_plan=Some(plan);}
        if let Some(body)=body {if let Some(old)=&d.delivery {ensure!(*old==body,"Cannot replace uncertain VPN ciphertext");}ensure!(body["operationId"]==id.to_string(),"VPN operation mismatch");d.delivery=Some(body);}
        write_draft(&tx,&self.key,&self.context,&d,if d.delivery.is_some(){"sending"}else{"pending"})?;tx.commit()?;Ok(())
    }
    pub(crate) fn acknowledge_vpn(&mut self,id:Uuid,receipt:Value)->Result<()> {
        let tx=self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;let (mut d,_)=read_draft(&tx,&self.key,&self.context,id)?;
        if let Some(old)=&d.receipt {ensure!(*old==receipt,"VPN receipt changed");return Ok(());}
        let state=receipt["status"].as_str().context("VPN receipt status missing")?;
        ensure!(matches!(state,"applied"|"conflict"|"quarantined"),"VPN receipt not final");
        if state=="applied" {if let Some(mut r)=read_record(&tx,&self.key,&self.context,d.vpn_id)? {r.revision=receipt["revision"].as_i64().context("VPN receipt revision missing")?;if let Some(id)=receipt["authId"].as_str(){r.auth_id=Some(Uuid::parse_str(id)?);r.auth_revision=receipt["authRevision"].as_i64().context("VPN auth receipt revision missing")?;}write_record(&tx,&self.key,&self.context,&r)?;}}
        if state=="quarantined" {revoke(&tx,&self.key,&self.context,Some(d.vpn_id))?;}
        d.receipt=Some(receipt.clone());write_draft(&tx,&self.key,&self.context,&d,state)?;tx.commit()?;Ok(())
    }
}

#[cfg(test)] mod tests {
    use super::*;
    use crate::{chain::ConnectionPlan,connection::{Target,Authentication},resources::ResourceContent};
    const NOW:i64=1_800_000_000_000;
    fn record()->CachedVpn {
        let profile=Profile{version:1,label:"Fixture".into(),family:termterm_vpn_core::Family::Fortinet,source:"manual".into(),server:"https://vpn.example".into(),group_id:None,username:"initial".into(),password:"initial-password".into(),realm:String::new(),config:String::new(),remember:true};
        CachedVpn{id:Uuid::new_v4(),revision:1,edit_revision:1,generation:0,verified_ms:NOW,view:json!({"label":"Fixture","visibility":"private","permissions":["read","connect","edit"]}),profile:Some(profile),auth_id:Some(Uuid::new_v4()),auth_revision:1,edit_auth_revision:1}
    }
    fn plan(root:Uuid,hop:Uuid)->ConnectionPlan {ConnectionPlan{target_id:root,steps:vec![hop,root].into_iter().map(|id|Target{id,protocol:"ssh".into(),label:"fixture".into(),address:"localhost".into(),port:22,username:None,authentication:Authentication::Prompt,proxy:None,proxy_auth:None}).collect()}}
    #[test] fn atomic_auth_config_restart_overlay_and_exact_delivery()->Result<()> {
        let dir=tempfile::tempdir()?;let identity=Identity::generate();let mut store=Store::open(dir.path(),"vpn-account-a",&identity)?;
        let original=record();store.refresh_vpn(original.clone())?;store.authorize_offline_edits(NOW)?;
        let mut edited=original.clone();let p=edited.profile.as_mut().unwrap();p.label="Local edit".into();p.username="personal-user".into();p.password="vpn-secret-marker".into();
        edited.view["label"]=json!("Local edit");
        let operation=store.queue_vpn(edited,true,true,NOW+1,None)?;
        let body=json!({"operationId":operation,"encryptedConfig":"fixture-ciphertext","auth":"fixture-auth"});
        let shared_plan=json!({"id":Uuid::new_v4(),"recipients":[]});
        store.prepare_vpn_delivery(operation,Some(shared_plan.clone()),Some(body.clone()))?;
        assert!(store.prepare_vpn_delivery(operation,None,Some(json!({"operationId":operation,"encryptedConfig":"different"}))).is_err());
        let mut fresh=original.clone();fresh.revision=2;fresh.edit_revision=2;fresh.view["label"]=json!("Remote edit");fresh.verified_ms=NOW+2;
        store.refresh_vpn(fresh)?;
        assert_eq!(store.cached_vpn(original.id)?.view["label"],"Local edit");
        drop(store);
        let mut store=Store::open(dir.path(),"vpn-account-a",&identity)?;
        let (draft,state)=store.next_vpn_delivery()?.unwrap();assert_eq!(state,"sending");assert_eq!(draft.delivery,Some(body));assert_eq!(draft.shared_plan,Some(shared_plan));
        assert_eq!(draft.record.profile.as_ref().unwrap().password,"vpn-secret-marker");assert_eq!(draft.record.profile.as_ref().unwrap().username,"personal-user");
        let r=store.cached_vpn(original.id)?;assert_eq!(r.revision,1);assert_eq!(r.edit_revision,2);assert_eq!(r.profile.as_ref().unwrap().label,"Local edit");
        let receipt=json!({"status":"applied","operationId":operation,"revision":2,"authId":original.auth_id,"authRevision":2});
        store.acknowledge_vpn(operation,receipt.clone())?;store.acknowledge_vpn(operation,receipt)?;
        assert_eq!(store.vpn_status()?["pending"],0);assert_eq!(store.cached_vpn(original.id)?.auth_revision,2);
        assert!(store.queue_vpn(original.clone(),true,true,NOW+3,None).is_err());
        for entry in std::fs::read_dir(dir.path())? {let bytes=std::fs::read(entry?.path())?;assert!(!bytes.windows(b"vpn-secret-marker".len()).any(|w|w==b"vpn-secret-marker"));}
        assert!(Store::open(dir.path(),"vpn-account-b",&identity)?.cached_vpn(original.id).is_err());Ok(())
    }
    #[test] fn edit_lease_expires_but_connect_survives_and_rollback_persists()->Result<()> {
        let dir=tempfile::tempdir()?;let identity=Identity::generate();let mut store=Store::open(dir.path(),"lease",&identity)?;
        let r=record();store.refresh_vpn(r.clone())?;store.authorize_offline_edits(NOW)?;
        assert!(store.queue_vpn(r.clone(),true,true,NOW+EDIT_WINDOW,None).is_err());
        assert!(store.cached_vpn(r.id)?.permitted("connect"));assert!(store.cached_vpn(r.id)?.profile.is_some());
        drop(store);let mut store=Store::open(dir.path(),"lease",&identity)?;
        assert!(store.queue_vpn(r.clone(),true,true,NOW+1,None).is_err());
        store.authorize_offline_edits(NOW+EDIT_WINDOW+2)?;
        assert!(store.queue_vpn(r.clone(),true,true,NOW-1,None).is_err());
        drop(store);let mut store=Store::open(dir.path(),"lease",&identity)?;
        assert!(store.queue_vpn(r,true,true,NOW+EDIT_WINDOW+3,None).is_err());Ok(())
    }
    #[test] fn failed_draft_commit_rolls_back_configuration_authentication_and_sequence()->Result<()> {
        let dir=tempfile::tempdir()?;let identity=Identity::generate();let mut store=Store::open(dir.path(),"disk-failure",&identity)?;
        let r=record();store.refresh_vpn(r.clone())?;store.authorize_offline_edits(NOW)?;
        store.db.execute_batch("CREATE TEMP TRIGGER vpn_disk_full BEFORE INSERT ON vpn_operations BEGIN SELECT RAISE(ABORT,'database or disk is full');END;")?;
        let mut edited=r.clone();edited.profile.as_mut().unwrap().password="must-not-commit".into();
        assert!(store.queue_vpn(edited,true,true,NOW+1,None).is_err());
        let retained=store.cached_vpn(r.id)?;assert!(retained.generation==0 && retained.profile.unwrap().password=="initial-password");assert!(store.vpn_status()?["pending"]==0);
        store.db.execute_batch("DROP TRIGGER vpn_disk_full")?;store.queue_vpn(r,true,true,NOW+2,None)?;assert!(store.next_vpn_delivery()?.unwrap().0.sequence==1);Ok(())
    }
    #[test] fn explicit_none_proof_is_required_and_denial_purges_material_preserving_drafts()->Result<()> {
        let dir=tempfile::tempdir()?;let identity=Identity::generate();let mut store=Store::open(dir.path(),"proof",&identity)?;
        let root=Uuid::new_v4();let hop=Uuid::new_v4();store.save_connection(&plan(root,hop))?;
        assert!(store.cached_vpn_requirement(root,hop).is_err());
        store.save_vpn_requirement(root,hop,None)?;assert_eq!(store.cached_vpn_requirement(root,hop)?,None);
        let r=record();store.refresh_vpn(r.clone())?;store.authorize_offline_edits(NOW)?;
        store.save_vpn_requirement(root,hop,Some(r.id))?;assert_eq!(store.cached_vpn_requirement(root,hop)?,Some(r.id));
        assert!(store.cached_vpn_requirement(root,Uuid::new_v4()).is_err());
        let mut changed=plan(root,hop);changed.steps[0].address="changed-endpoint.example".into();store.save_connection(&changed)?;
        assert!(store.cached_vpn_requirement(root,hop).is_err());store.save_vpn_requirement(root,hop,Some(r.id))?;
        store.queue_vpn(r.clone(),true,true,NOW+1,None)?;store.revoke_vpn(Some(r.id))?;
        assert!(store.cached_vpn(r.id).is_err());assert!(store.cached_vpn_requirement(root,hop).is_err());assert!(store.next_vpn_delivery()?.is_none());assert_eq!(store.vpn_status()?["quarantined"],1);
        assert_eq!(store.journal_status(NOW+2)?.pending,0);Ok(())
    }
    #[test] fn cache8_upgrade_preserves_existing_resource_journal_and_creation_requires_authority()->Result<()> {
        let dir=tempfile::tempdir()?;let identity=Identity::generate();let mut store=Store::open(dir.path(),"upgrade",&identity)?;
        let host=Uuid::new_v4();let content=ResourceContent{kind:"host".into(),parent_id:None,name:"retained".into(),address:Some("localhost".into()),protocol:Some("ssh".into()),port:Some(22),username_policy:Some("personal".into()),username:None,deleted:false,document:None,proxy:None};
        store.refresh_cached_resource(journal::CachedResource{navigation_only:false,id:host,vault_id:Uuid::new_v4(),server_revision:1,generation:0,permissions:journal::Permissions{read:true,edit:true,connect:true},write_capability:Some(Uuid::new_v4()),verified_at_ms:NOW,content:content.clone()})?;
        store.authorize_offline_edits(NOW)?;store.queue_edit(host,0,content,NOW+1)?;
        let before:Vec<u8>=store.db.query_row("SELECT ciphertext FROM journal_operations",[],|r|r.get(0))?;
        store.db.execute_batch("DROP TABLE vpn_requirements;DROP TABLE vpn_operations;DROP TABLE cached_vpn;PRAGMA user_version=8;")?;drop(store);
        let mut store=Store::open(dir.path(),"upgrade",&identity)?;assert_eq!(store.db.query_row::<i64,_,_>("PRAGMA user_version",[],|r|r.get(0))?,9);
        let after:Vec<u8>=store.db.query_row("SELECT ciphertext FROM journal_operations",[],|r|r.get(0))?;assert_eq!(before,after);assert_eq!(store.journal_status(NOW+2)?.pending,1);
        let mut r=record();r.revision=0;r.edit_revision=0;
        assert!(store.queue_vpn(r.clone(),true,true,NOW+3,None).is_err());
        store.queue_vpn(r,true,true,NOW+3,Some(Uuid::new_v4()))?;assert_eq!(store.vpn_status()?["pending"],1);assert_eq!(store.journal_status(NOW+4)?.pending,1);Ok(())
    }
}
