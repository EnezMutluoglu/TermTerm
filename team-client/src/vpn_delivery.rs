//! Bounded typed VPN delivery. Exact ciphertext is durable before the first submission.
use crate::{keys::Identity,local::{Store,vpn::{CachedVpn,Draft}},transport::{OpenCredential,TeamClient},vpn::Details};
use anyhow::{ensure,Context,Result};
use reqwest::Method;
use serde_json::{json,Value};
use uuid::Uuid;
use zeroize::Zeroizing;

impl TeamClient {
    pub(crate) async fn cache_vpn_record(&self,view:Value,identity:&Identity,now:i64)->Result<CachedVpn> {
        let id=serde_json::from_value(view["id"].clone())?;
        let revision=view["revision"].as_i64().context("VPN revision missing")?;
        let permitted=|p:&str|view["permissions"].as_array().is_some_and(|a|a.iter().any(|x|x==p));
        let mut profile=if permitted("connect") || permitted("edit") {Some(Self::decode_vpn(&self.open_vpn(id,if permitted("connect"){"vpn"}else{"edit"},identity).await?)?)}else{None};
        let mut auth_id=None;let mut auth_revision=0;
        if permitted("connect") {
            let binding=self.request(Method::GET,&format!("v1/vpn/{id}/auth"),None,true).await?;
            auth_id=serde_json::from_value(binding.get("credentialId").context("VPN authentication binding missing")?.clone())?;
            if let Some(a)=auth_id {
                let c=self.open_private_credential(a,identity).await?;ensure!(c.kind=="vpn_auth","Invalid VPN authentication kind");auth_revision=c.revision;
                let mut auth:crate::vpn::Authentication=serde_json::from_slice(&c.secret)?;
                let p=profile.as_mut().context("VPN config missing")?;
                p.username=std::mem::take(&mut auth.username);p.password=std::mem::take(&mut auth.password);p.remember=auth.remember;
            }
        }
        Ok(CachedVpn{id,revision,edit_revision:revision,generation:0,verified_ms:now,view,profile,auth_id,auth_revision,edit_auth_revision:auth_revision})
    }
    pub(crate) async fn provision_vpn_creation(&self,store:&mut Store)->Result<Uuid> {
        let value=self.request(Method::GET,"v1/vpn/create-authority",None,true).await?;
        let id=serde_json::from_value(value["authority"].clone())?;
        let entry=store.read("vpn-create-authority")?;
        ensure!(store.replace("vpn-create-authority",entry.revision,&serde_json::to_vec(&id)?)?,"VPN creation authority changed");Ok(id)
    }
    async fn prepare_vpn_body(&self,d:&Draft,store:&mut Store,identity:&Identity)->Result<Value> {
        ensure!(self.device==Some(d.device),"VPN journal device changed");
        let profile=d.record.profile.as_ref().context("VPN draft configuration missing")?;
        let metadata=Zeroizing::new(serde_json::to_vec(&Details::from_profile(profile))?);
        let mut shared_plan=d.shared_plan.clone();
        let config=if !d.configuration {Value::Null} else if d.record.view["visibility"]=="shared" {
            if shared_plan.is_none() {
                let options=self.shared_update_options(d.vpn_id).await?;
                ensure!(options["revision"]==d.base_revision,"revision_conflict");
                let recipients=crate::vpn::recipients(&options)?;
                let plan=self.prepare_shared_update(d.vpn_id,d.base_revision,None,&recipients).await?;
                store.prepare_vpn_delivery(d.id,Some(plan.clone()),None)?;shared_plan=Some(plan);
            }
            let source=OpenCredential{kind:"vpn".into(),revision:d.base_revision,data:metadata,secret:Zeroizing::new(profile.config.as_bytes().to_vec())};
            self.prepare_shared_source(shared_plan.as_ref().context("VPN plan missing")?,identity,&source)?
        } else {self.prepare_private_credential(d.vpn_id,d.base_revision,"vpn",&metadata,profile.config.as_bytes(),identity).await?};
        let auth=if d.authentication {
            let authentication=crate::vpn::Authentication{username:profile.username.clone(),password:profile.password.clone(),remember:profile.remember};
            let bytes=Zeroizing::new(serde_json::to_vec(&authentication)?);
            self.prepare_private_credential(d.record.auth_id.context("VPN auth id missing")?,d.auth_base,"vpn_auth",br#"{"label":"VPN authentication"}"#,&bytes,identity).await?
        } else {Value::Null};
        Ok(json!({"operationId":d.id,"deviceId":d.device,"sequence":d.sequence,"vpnId":d.vpn_id,"baseRevision":d.base_revision,"creationAuthority":d.creation_authority,
            "config":config,"auth":auth,"authId":d.record.auth_id,"authBase":d.auth_base,"sharedPlan":shared_plan.as_ref().and_then(|p|p["id"].as_str())}))
    }
    pub(crate) async fn deliver_next_vpn(&self,store:&mut Store,identity:&Identity)->Result<Option<Value>> {
        let Some((d,_))=store.next_vpn_delivery()? else{return Ok(None)};
        let result=async {
            let receipt=self.request(Method::GET,&format!("v1/vpn/operations/{}",d.id),None,true).await?;
            if receipt["found"]==true && receipt["status"]!="prepared" {store.acknowledge_vpn(d.id,receipt.clone())?;return Ok(Some(receipt));}
            let body=if let Some(body)=&d.delivery {body.clone()}else {
                ensure!(receipt["found"]==false,"Prepared VPN receipt without durable ciphertext");
                let body=self.prepare_vpn_body(&d,store,identity).await?;
                store.prepare_vpn_delivery(d.id,None,Some(body.clone()))?;body
            };
            let prepared=self.request(Method::POST,"v1/vpn/operations",Some(&body),true).await?;
            if prepared["status"]!="prepared" {store.acknowledge_vpn(d.id,prepared.clone())?;return Ok(Some(prepared));}
            let receipt=self.request(Method::POST,&format!("v1/vpn/operations/{}",d.id),None,true).await?;
            store.acknowledge_vpn(d.id,receipt.clone())?;Ok(Some(receipt))
        }.await;
        if result.as_ref().is_err_and(|e:&anyhow::Error|e.to_string()=="revision_conflict") {
            let receipt=json!({"found":true,"operationId":d.id,"status":"conflict"});
            store.acknowledge_vpn(d.id,receipt.clone())?;return Ok(Some(receipt));
        }
        if result.as_ref().is_err_and(|e:&anyhow::Error|matches!(e.to_string().as_str(),"permission_denied"|"authentication_denied")) {store.revoke_vpn(Some(d.vpn_id))?;}
        // A denied/conflicted VPN draft stays encrypted locally; the resource candidate API cannot carry its secrets.
        result
    }
}
