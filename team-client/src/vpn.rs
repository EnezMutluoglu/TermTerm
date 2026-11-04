//! Typed VPN configuration; shared configuration never contains a personal username or password.
use crate::{keys::{self, Identity, Purpose}, transport::{OpenCredential, TeamClient}};
use anyhow::{ensure, Context, Result};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use termterm_vpn_core::{Family, Profile};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

#[derive(Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct Details {
    pub version: u32, pub label: String, pub family: Family, pub source: String,
    pub server: String, pub group_id: Option<String>, pub realm: String,
}
impl Details {
    pub(crate) fn from_profile(p:&Profile)->Self {Self{version:p.version,label:p.label.clone(),family:p.family,source:p.source.clone(),server:p.server.clone(),group_id:p.group_id.clone(),realm:p.realm.clone()}}

    fn profile(self, config: String) -> Profile {
        Profile {version:self.version,label:self.label,family:self.family,source:self.source,server:self.server,
            group_id:self.group_id,realm:self.realm,config,username:String::new(),password:String::new(),remember:false}
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct Authentication { pub username: String, pub password: String, pub remember: bool }
impl Drop for Authentication { fn drop(&mut self) {self.username.zeroize();self.password.zeroize();} }
#[derive(Deserialize)]
#[serde(tag="kind",rename_all="camelCase",rename_all_fields="camelCase",deny_unknown_fields)]
pub enum Action {
    List,
    Delete {id:Uuid, revision:i64},
    Binding {resource_id:Uuid, #[serde(default)] personal:bool},
    Bind {resource_id:Uuid, expected_revision:i64, vpn_id:Option<Uuid>, #[serde(default)] personal:bool},
    Authenticate {id:Uuid, authentication:Authentication},
    Reveal {id:Uuid},
    Export {id:Uuid, path:String},
}
pub(crate) fn recipients(options:&Value)->Result<Vec<crate::sharing::ShareRecipient>> {options["recipients"].as_array().context("Invalid recipients")?.iter().filter(|r|["view","connect","edit","reveal","export"].iter().any(|p|r[p]==true)).map(|r|Ok(crate::sharing::ShareRecipient{account_id:serde_json::from_value(r["accountId"].clone())?,connect:r["connect"].as_bool().unwrap_or(false),reveal:r["reveal"].as_bool().unwrap_or(false),export:r["export"].as_bool().unwrap_or(false),view:r["view"].as_bool(),edit:r["edit"].as_bool()})).collect::<Result<Vec<_>>>()}
pub(crate) fn validate_configuration(profile:&Profile)->Result<()> {profile.validate()?;if profile.family==Family::OpenVpn {termterm_vpn_core::import::openvpn_embedded(&profile.config)?;}Ok(())}
fn decode_requirement(value:&Value)->Result<Option<Uuid>> {Ok(serde_json::from_value(value.get("vpnId").context("Required VPN proof missing")?.clone())?)}
impl TeamClient {
    pub(crate) async fn vpn_access(&self,id:Uuid)->Result<Value> {self.request(Method::GET,&format!("v1/vpn/{id}/authorize"),None,true).await}
    pub async fn vpn_authorize(&self,id:Uuid)->Result<()> {
        self.request(Method::GET,&format!("v1/vpn/{id}/authorize"),None,true).await?; Ok(())
    }
    pub async fn required_vpn_for_plan(&self,root:Uuid,hop:Uuid)->Result<Option<Uuid>> {
        decode_requirement(&self.request(Method::GET,&format!("v1/vpn/required/{root}/{hop}"),None,true).await?)
    }
    pub async fn required_vpn(&self,host:Uuid)->Result<Option<Uuid>> {
        decode_requirement(&self.request(Method::GET,&format!("v1/vpn/required/{host}"),None,true).await?)
    }
    pub async fn vpn_authentication(&self,id:Uuid,identity:&Identity)->Result<Authentication> {
        let value=self.request(Method::GET,&format!("v1/vpn/{id}/auth"),None,true).await?;
        if value["credentialId"].is_null() {return Ok(Authentication::default());}
        let credential=self.open_private_credential(serde_json::from_value(value["credentialId"].clone())?,identity).await?;
        ensure!(credential.kind=="vpn_auth","Invalid VPN authentication kind");
        Ok(serde_json::from_slice(&credential.secret)?)
    }
    pub async fn save_vpn_authentication(&self,id:Uuid,authentication:&Authentication,identity:&Identity)->Result<()> {
        self.vpn_authorize(id).await?;
        ensure!(authentication.username.len()<=4096 && authentication.password.len()<=4096 && !authentication.username.contains('\0') && !authentication.password.contains('\0'),"Invalid VPN authentication");
        ensure!(authentication.remember || authentication.password.is_empty(),"Password requires remember permission");
        let previous=self.request(Method::GET,&format!("v1/vpn/{id}/auth"),None,true).await?;
        let (credential_id,revision)=if previous["credentialId"].is_null() {(Uuid::new_v4(),0)} else {
            let c=serde_json::from_value(previous["credentialId"].clone())?;
            let old=self.open_private_credential(c,identity).await?;
            ensure!(old.kind=="vpn_auth","Invalid VPN authentication kind"); (c,old.revision)
        };
        let secret=Zeroizing::new(serde_json::to_vec(authentication)?);
        self.save_private_credential(credential_id,revision,"vpn_auth",br#"{"label":"VPN authentication"}"#,&secret,identity).await?;
        self.request(Method::POST,&format!("v1/vpn/{id}/auth"),Some(&json!({"credentialId":credential_id})),true).await?; Ok(())
    }
    pub async fn open_vpn(&self,id:Uuid,purpose:&str,identity:&Identity)->Result<OpenCredential> {
        let private=self.private_credentials().await?;
        let own=private.as_array().context("Invalid credential list")?.iter().any(|c|c["id"]==id.to_string() && c["kind"]=="vpn");
        let credential=if own {self.open_private_credential(id,identity).await?} else {self.open_shared_credential(id,purpose,None,identity).await?};
        ensure!(credential.kind=="vpn","Invalid VPN configuration kind"); Ok(credential)
    }
    pub fn decode_vpn(credential:&OpenCredential)->Result<Profile> {
        ensure!(credential.kind=="vpn","Invalid VPN configuration kind");
        let details:Details=serde_json::from_slice(&credential.data)?;
        let profile=details.profile(String::from_utf8(credential.secret.to_vec())?);
        profile.validate()?;
        if profile.family==Family::OpenVpn {termterm_vpn_core::import::openvpn_embedded(&profile.config)?;}
        Ok(profile)
    }
    pub async fn vpn_profiles(&self,identity:&Identity)->Result<Vec<Value>> {
        let (team,_,_)=self.actor()?;
        let private=self.private_credentials().await?;
        let shared=self.shared_credentials().await?;
        let mut profiles=vec![];
        for (row,visibility) in private.as_array().context("Invalid credentials")?.iter().map(|r|(r,"private"))
            .chain(shared.as_array().context("Invalid shared credentials")?.iter().map(|r|(r,"shared"))) {
            if row["kind"]!="vpn" {continue;}
            let id:Uuid=serde_json::from_value(row["id"].clone())?;
            let revision=row["revision"].as_i64().context("Invalid revision")?;
            let data=keys::decrypt(team,id,revision,Purpose::Data,row["encryptedData"].as_str().context("Missing VPN metadata")?,row["dataEnvelope"].as_str().context("VPN recovery required")?,identity)?;
            let details:Details=serde_json::from_slice(&data)?;
            let mut view=serde_json::to_value(details)?;
            let connect=visibility=="private" || row["permissions"].as_array().is_some_and(|p|p.iter().any(|p|p=="connect"));
            let auth=if connect {self.vpn_authentication(id,identity).await?} else {Authentication::default()};
            view["id"]=json!(id);view["revision"]=json!(revision);view["visibility"]=json!(visibility);
            view["permissions"]=if visibility=="private" {json!(["read","connect","edit","reveal","export"])} else {row["permissions"].clone()};
            view["hasConfig"]=json!(matches!(view["family"].as_str(),Some("openVpn"|"wireGuard")));
            view["username"]=json!(auth.username);view["hasPassword"]=json!(!auth.password.is_empty());view["remember"]=json!(auth.remember);
            view["canRemove"]=json!(visibility=="private" || row["canRemove"]==true);view["canUpdate"]=json!(visibility=="private" || row["canUpdate"]==true);view["scopeId"]=row["scopeId"].clone();
            profiles.push(view);
        }
        Ok(profiles)
    }
    pub async fn save_vpn_configuration(&self,id:Uuid,expected:i64,profile:&Profile,identity:&Identity)->Result<i64> {
        profile.validate()?;
        let details=Details {version:profile.version,label:profile.label.clone(),family:profile.family,source:profile.source.clone(),server:profile.server.clone(),group_id:profile.group_id.clone(),realm:profile.realm.clone()};
        let data=Zeroizing::new(serde_json::to_vec(&details)?);
        if expected==0 {return self.save_private_credential(id,0,"vpn",&data,profile.config.as_bytes(),identity).await;}
        let private=self.private_credentials().await?;
        if private.as_array().context("Invalid credentials")?.iter().any(|r|r["id"]==id.to_string() && r["kind"]=="vpn") {
            return self.save_private_credential(id,expected,"vpn",&data,profile.config.as_bytes(),identity).await;
        }
        let options=self.shared_update_options(id).await?;
        let recipients=recipients(&options)?;
        let plan=self.prepare_shared_update(id,expected,None,&recipients).await?;
        let source=OpenCredential{kind:"vpn".into(),revision:expected,data,secret:Zeroizing::new(profile.config.as_bytes().to_vec())};
        let receipt=self.finish_shared_source(&plan,identity,&source).await?;
        receipt["revision"].as_i64().context("Invalid VPN save receipt")
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn missing_requirement_proof_never_means_direct_connection() {
        assert!(decode_requirement(&json!({})).is_err());assert!(decode_requirement(&json!({"vpnId":"invalid"})).is_err());
        assert_eq!(decode_requirement(&json!({"vpnId":null})).unwrap(),None);
    }
    #[test] fn shared_details_cannot_contain_credentials_or_otp() {
        let details=json!({"version":1,"label":"Fixture","family":"fortinet","source":"manual","server":"https://vpn.example","groupId":null,"realm":""});
        assert!(serde_json::from_value::<Details>(details.clone()).is_ok());
        for field in ["username","password","otp","config"] {let mut value=details.clone();value[field]=json!("secret");assert!(serde_json::from_value::<Details>(value).is_err());}
        let mut auth=json!({"username":"user","password":"fixture","remember":true});auth["otp"]=json!("123456");assert!(serde_json::from_value::<Authentication>(auth).is_err());
    }
}
