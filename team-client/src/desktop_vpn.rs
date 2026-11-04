use super::*;
use crate::{vpn::{Action,Details},local::vpn::CachedVpn};
use termterm_vpn_core::Profile;

fn scope(s:&Session,epoch:u64)->Result<String>{Ok(format!("{}:vpn:{epoch}",s.client.local_scope()?))}
fn denied(s:&mut Session,id:Option<Uuid>,e:&anyhow::Error)->Result<()> {
    if let Some(store)=s.store.as_mut() {
        match e.to_string().as_str(){"authentication_denied"=>store.revoke_all_cached_resources()?,"permission_denied"=>store.revoke_vpn(id)?,_=>{}}
    } Ok(())
}
async fn renew(s:&mut Session)->Result<bool> {
    s.client.ensure_compatible()?;
    match s.renew().await {Ok(())=>{s.offline=false;Ok(true)},Err(e) if e.to_string()=="network_unavailable" && s.store.is_some()=>{s.offline=true;Ok(false)},Err(e)=>{denied(s,None,&e)?;Err(e)}}
}
async fn refresh(s:&mut Session)->Result<()> {
    let identity=s.identity.as_ref().context("Activate account first")?;
    let result:Result<()>=async {
        let views=s.client.vpn_profiles(identity).await?;
        let now=now_ms()?;let mut records=vec![];
        for view in views {records.push(s.client.cache_vpn_record(view,identity,now).await?);}
        let store=s.store.as_mut().context("Persistent VPN cache unavailable")?;
        s.client.provision_vpn_creation(store).await?;
        let visible=records.iter().map(|r|r.id).collect();
        for record in records {store.refresh_vpn(record)?;}
        store.reconcile_vpn(&visible)?;store.authorize_offline_edits(now)?;Ok(())
    }.await;
    match result {Ok(())=>{s.offline=false;Ok(())},Err(e) if e.to_string()=="network_unavailable" && s.store.is_some()=>{s.offline=true;Ok(())},Err(e)=>{denied(s,None,&e)?;Err(e)}}
}
async fn deliver(s:&mut Session)->Result<()> {
    if s.offline{return Ok(());}
    let identity=s.identity.as_ref().context("Activate account first")?;
    let store=s.store.as_mut().context("Persistent VPN cache unavailable")?;
    match s.client.deliver_next_vpn(store,identity).await {
        Ok(_)=>Ok(()),Err(e) if e.to_string()=="network_unavailable" || e.to_string().starts_with("unknown_outcome")=>Ok(()),Err(e)=>Err(e)
    }
}
fn projected_profile(record:&CachedVpn)->Result<Profile>{record.profile.clone().context("VPN configuration not cached; reopen online")}
fn update_view(record:&mut CachedVpn) {
    if let Some(profile)=&record.profile {
        let metadata=serde_json::to_value(Details::from_profile(profile)).expect("VPN metadata serializes");
        for field in ["version","label","family","source","server","groupId","realm"] {record.view[field]=metadata[field].clone();}
        record.view["username"]=json!(profile.username);record.view["hasPassword"]=json!(!profile.password.is_empty());record.view["remember"]=json!(profile.remember);record.view["hasConfig"]=json!(!profile.config.is_empty());
    }
}
pub(super) async fn execute(s:&mut Session,context:Option<&str>,action:Action,epoch:u64)->Result<Value> {
    let current=scope(s,epoch)?;
    ensure!(context==Some(current.as_str()) || (context.is_none() && matches!(action,Action::List)),"Team session changed; reopen VPN");
    match action {
        Action::List=>{if !s.offline {deliver(s).await?;refresh(s).await?;}},
        Action::Binding{resource_id,personal}=>return s.client.request(reqwest::Method::GET,&format!("v1/vpn/{}/{resource_id}",if personal {"personal-bindings"} else {"bindings"}),None,true).await,
        Action::Bind{resource_id,expected_revision,vpn_id,personal}=>return s.client.request(reqwest::Method::POST,&format!("v1/vpn/{}/{resource_id}",if personal {"personal-bindings"} else {"bindings"}),Some(&json!({"expectedRevision":expected_revision,"vpnId":vpn_id})),true).await,
        Action::Delete{id,revision}=>{
            s.client.request(reqwest::Method::POST,&format!("v1/vpn/{id}/remove"),Some(&json!({"expectedRevision":revision})),true).await?;
            if let Some(store)=s.store.as_mut(){store.revoke_vpn(Some(id))?;}refresh(s).await?;
        },
        Action::Authenticate{id,mut authentication}=>{
            if !s.offline {refresh(s).await?;}
            let now=now_ms()?;let store=s.store.as_mut().context("Persistent VPN cache unavailable")?;
            let mut record=store.cached_vpn(id)?;ensure!(record.permitted("connect"),"permission_denied");
            let mut profile=projected_profile(&record)?;
            if authentication.remember && authentication.password.is_empty(){authentication.password=profile.password.clone();}
            profile.username=std::mem::take(&mut authentication.username);profile.password=std::mem::take(&mut authentication.password);profile.remember=authentication.remember;
            crate::vpn::validate_configuration(&profile)?;record.profile=Some(profile);update_view(&mut record);
            store.queue_vpn(record,false,true,now,None)?;deliver(s).await?;
        },
        Action::Reveal{id}=>{
            let identity=s.identity.as_ref().context("Activate account first")?;
            let c=s.client.open_vpn(id,"reveal",identity).await?;
            return Ok(json!({"configuration":std::str::from_utf8(&c.secret)?}));
        },
        Action::Export{id,path}=>{
            use std::io::Write;
            let identity=s.identity.as_ref().context("Activate account first")?;
            let c=s.client.open_vpn(id,"export",identity).await?;
            let mut options=std::fs::OpenOptions::new();options.write(true).create_new(true);
            #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
            let mut file=options.open(path).context("Choose a new VPN export file")?;
            let profile=TeamClient::decode_vpn(&c)?;
            let bytes=Zeroizing::new(serde_json::to_vec_pretty(&profile)?);file.write_all(&bytes)?;file.sync_all()?;
            return Ok(json!({"exported":true}));
        },
    }
    let store=s.store.as_mut().context("Persistent VPN cache unavailable")?;
    Ok(json!({"context":current,"profiles":store.cached_vpn_views()?,"offline":s.offline,"journal":store.vpn_status()?}))
}
impl DesktopState {
    pub async fn vpn_scope(&self)->Result<(u64,String)> {
        let epoch=self.current_epoch();let session=self.session.lock().map_err(|_|anyhow::anyhow!("Session lock"))?.clone().context("Sign in first")?;
        let mut s=session.lock().await;renew(&mut s).await?;let current=scope(&s,epoch)?;
        ensure!(epoch==self.current_epoch(),"Session closed");Ok((epoch,current))
    }
    pub async fn vpn_material(&self,id:Uuid,epoch:u64)->Result<(String,Profile)> {
        ensure!(epoch==self.current_epoch(),"Session closed");
        let session=self.session.lock().map_err(|_|anyhow::anyhow!("Session lock"))?.clone().context("Sign in first")?;
        let mut s=session.lock().await;
        if renew(&mut s).await? {
            let result:Result<()>=async {
                let identity=s.identity.as_ref().context("Activate account first")?;
                let access=s.client.vpn_access(id).await?;
                let credential=s.client.open_vpn(id,"vpn",identity).await?;
                let profile=TeamClient::decode_vpn(&credential)?;
                let mut view=serde_json::to_value(Details::from_profile(&profile))?;
                for field in ["visibility","permissions","revision"] {view[field]=access[field].clone();}view["id"]=json!(id);
                let record=s.client.cache_vpn_record(view,identity,now_ms()?).await?;
                s.store.as_mut().context("Persistent VPN cache unavailable")?.refresh_vpn(record)?;Ok(())
            }.await;
            if let Err(e)=result {if e.to_string()!="network_unavailable" {denied(&mut s,Some(id),&e)?;return Err(e);}}
        }
        let record=s.store.as_mut().context("Persistent VPN cache unavailable")?.cached_vpn(id)?;
        ensure!(record.permitted("connect"),"permission_denied");let profile=projected_profile(&record)?;crate::vpn::validate_configuration(&profile)?;
        ensure!(epoch==self.current_epoch(),"Session closed");Ok((scope(&s,epoch)?,profile))
    }
    pub async fn check_vpn(&self,id:Uuid,epoch:u64)->Result<()> {
        ensure!(epoch==self.current_epoch(),"Session closed");let session=self.session.lock().map_err(|_|anyhow::anyhow!("Session lock"))?.clone().context("Sign in first")?;
        let mut s=session.lock().await;
        let result=if renew(&mut s).await? {s.client.vpn_authorize(id).await}else{Err(anyhow::anyhow!("network_unavailable"))};
        match result {Ok(())=>{},Err(e) if e.to_string()=="network_unavailable"=>{let r=s.store.as_mut().context("Persistent VPN cache unavailable")?.cached_vpn(id)?;ensure!(r.permitted("connect") && r.profile.is_some(),"permission_denied");},Err(e)=>{denied(&mut s,Some(id),&e)?;return Err(e);}}
        ensure!(epoch==self.current_epoch(),"Session closed");Ok(())
    }
    pub async fn required_vpn_for_plan(&self,root:Uuid,hop:Uuid,epoch:u64)->Result<Option<Uuid>> {
        self.vpn_requirement(root,hop,epoch,true).await
    }
    pub async fn required_vpn(&self,host:Uuid,epoch:u64)->Result<Option<Uuid>> {
        self.vpn_requirement(host,host,epoch,false).await
    }
    async fn vpn_requirement(&self,root:Uuid,hop:Uuid,epoch:u64,plan:bool)->Result<Option<Uuid>> {
        ensure!(epoch==self.current_epoch(),"Session closed");let session=self.session.lock().map_err(|_|anyhow::anyhow!("Session lock"))?.clone().context("Sign in first")?;
        let mut s=session.lock().await;
        let result=if renew(&mut s).await? {if plan{s.client.required_vpn_for_plan(root,hop).await}else{s.client.required_vpn(hop).await}}else{Err(anyhow::anyhow!("network_unavailable"))};
        let required=match result {
            Ok(vpn)=>{if let Some(store)=s.store.as_mut(){store.save_vpn_requirement(root,hop,vpn)?;}vpn},
            Err(e) if e.to_string()=="network_unavailable"=>s.store.as_ref().context("Persistent VPN cache unavailable")?.cached_vpn_requirement(root,hop)?,
            Err(e)=>{denied(&mut s,None,&e)?;return Err(e);}
        };
        ensure!(epoch==self.current_epoch(),"Session closed");Ok(required)
    }
    pub async fn save_vpn(&self,context:&str,id:Option<Uuid>,revision:i64,mut profile:Profile)->Result<Value> {
        let epoch=self.current_epoch();let session=self.session.lock().map_err(|_|anyhow::anyhow!("Session lock"))?.clone().context("Sign in first")?;
        let mut s=session.lock().await;
        ensure!(epoch==self.current_epoch() && context==scope(&s,epoch)?,"Team session changed; reopen VPN");
        if renew(&mut s).await? {refresh(&mut s).await?;}
        let now=now_ms()?;let store=s.store.as_mut().context("Persistent VPN cache unavailable")?;
        let id=id.unwrap_or_else(Uuid::new_v4);
        let mut record=if revision==0 {CachedVpn{id,revision:0,edit_revision:0,generation:0,verified_ms:now,view:json!({"id":id,"visibility":"private","permissions":["read","connect","edit","reveal","export"],"canRemove":true,"canUpdate":true}),profile:None,auth_id:None,auth_revision:0,edit_auth_revision:0}}else{store.cached_vpn(id)?};
        ensure!(record.edit_revision==revision,"VPN local revision changed; refresh");ensure!(record.permitted("edit"),"permission_denied");
        if let Some(previous)=&record.profile {
            if profile.config.is_empty(){profile.config=previous.config.clone();}
            if profile.remember && profile.password.is_empty(){profile.password=previous.password.clone();}
        }
        crate::vpn::validate_configuration(&profile)?;
        let authentication=record.permitted("connect");
        if !authentication {profile.username.clear();profile.password.clear();profile.remember=false;}
        record.profile=Some(profile);update_view(&mut record);
        let authority=if revision==0 {Some(serde_json::from_slice::<Uuid>(&store.read("vpn-create-authority")?.bytes).context("Connect online before creating VPN profiles offline")?)}else{None};
        store.queue_vpn(record,true,authentication,now,authority)?;deliver(&mut s).await?;
        ensure!(epoch==self.current_epoch(),"Session closed");
        let mut view=s.store.as_mut().context("Persistent VPN cache unavailable")?.cached_vpn(id)?.view();view["journal"]=s.store.as_ref().context("Persistent VPN cache unavailable")?.vpn_status()?;Ok(view)
    }
}
