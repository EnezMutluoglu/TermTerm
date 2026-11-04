use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use termterm_vpn_core::Profile;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum Request {
    Hello { #[serde(rename="requestId")] request_id: u64, #[serde(rename="vaultId")] vault_id: Uuid },
    Connect { #[serde(rename="requestId")] request_id: u64, id: Uuid, generation: u64, #[serde(rename="profileId")] profile_id: Uuid, profile: Profile },
    Answer { #[serde(rename="requestId")] request_id: u64, id: Uuid, generation: u64, #[serde(rename="challengeId")] challenge_id: u64, values: BTreeMap<String, String> },
    SsoAnswer { #[serde(rename="requestId")] request_id: u64, id: Uuid, generation: u64, #[serde(rename="challengeId")] challenge_id: u64, result: SsoResult },
    Disconnect { #[serde(rename="requestId")] request_id: u64, id: Uuid, generation: u64 },
    Quiesced { #[serde(rename="requestId")] request_id: u64, id: Uuid, generation: u64 },
    Sessions { #[serde(rename="requestId")] request_id: u64 },
    Close { #[serde(rename="requestId")] request_id: u64 },
    NetworkReceipt { #[serde(rename="requestId")] request_id: u64, id: Uuid, generation: u64 },
}
impl Request {
    pub fn request_id(&self) -> u64 {
        match self { Self::Hello { request_id, .. } | Self::Connect { request_id, .. } | Self::Answer { request_id, .. } | Self::SsoAnswer { request_id, .. } | Self::Disconnect { request_id, .. } | Self::Quiesced {request_id,..} | Self::Sessions { request_id } | Self::Close { request_id } | Self::NetworkReceipt {request_id,..} => *request_id }
    }
}
#[derive(Debug, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct EngineEvent { pub kind: String, pub session_id: Uuid, pub generation: u64, #[serde(default)] pub detail: Value }
pub fn validate_fixture_profile(profile: &Profile) -> Result<()> {
    profile.validate()?;
    if profile.family == termterm_vpn_core::Family::OpenVpn {
        for remote in termterm_vpn_core::import::openvpn_remotes(&profile.config)? {
            ensure!(matches!(remote.host.as_str(),"127.0.0.1" | "::1") && [31194,31195,31198,31199].contains(&remote.port)
                && matches!(remote.protocol.as_str(),"tcp-client" | "tcp" | "tcp4-client" | "tcp4" | "tcp6-client" | "tcp6"),"validation_requires_fixed_loopback_fixture");
        }
    } else if profile.family == termterm_vpn_core::Family::WireGuard {
        let config=termterm_vpn_core::wireguard::parse(&profile.config)?;
        ensure!(config.peers.iter().all(|peer|peer.endpoint=="127.0.0.1:31820"),"validation_requires_fixed_loopback_fixture");
    } else {
        ensure!(profile.family == termterm_vpn_core::Family::Cisco && profile.server == "https://127.0.0.1:31443", "validation_requires_fixed_loopback_fixture");
    }
    Ok(())
}
pub fn validate_progress(detail: Value) -> Result<()> {
    #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Progress { name: String, error: bool }
    let progress: Progress = serde_json::from_value(detail)?;
    ensure!(!progress.error && !progress.name.is_empty() && progress.name.len() <= 256,"invalid_progress_event"); Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct Challenge {
    pub id: u64, pub fields: Vec<Field>, #[serde(default)] pub message: String, #[serde(default)] pub error: String,
    #[serde(default, skip_serializing_if="Option::is_none")] pub kind: Option<String>,
    #[serde(default, skip_serializing_if="Option::is_none")] pub provider: Option<String>,
    #[serde(default, skip_serializing_if="Option::is_none")] pub stage: Option<String>,
    #[serde(default, skip_serializing_if="Option::is_none")] pub private: Option<SsoChallenge>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct SsoChallenge { pub url: String, pub expected_origin: String, pub expected_path: String }
impl Drop for SsoChallenge {
    fn drop(&mut self) { use zeroize::Zeroize; self.url.zeroize(); }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct SsoResult { pub uri: String, pub username: String, pub token: String, pub token_name: String, pub status: u16, pub auth_status: String }
impl Drop for SsoResult {
    fn drop(&mut self) { use zeroize::Zeroize; self.uri.zeroize(); self.username.zeroize(); self.token.zeroize(); }
}
impl SsoChallenge {
    pub fn validate(&self) -> Result<()> {
        termterm_vpn_core::sso::validate_start(&self.url)?;
        ensure!(termterm_vpn_core::sso::origin(&self.expected_origin)? == self.expected_origin
            && self.expected_path == termterm_vpn_core::sso::CALLBACK_PATH, "invalid_sso_challenge"); Ok(())
    }
    pub fn answer(&self, result: &SsoResult) -> Result<()> {
        self.validate()?;
        termterm_vpn_core::sso::validate_result_uri(&result.uri, &self.expected_origin)?;
        ensure!((200..300).contains(&result.status) && result.auth_status == "1"
            && matches!(result.token_name.as_str(), "prelogin-cookie" | "portal-userauthcookie"), "invalid_sso_result");
        for value in [&result.username, &result.token] {
            ensure!(!value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control), "invalid_sso_result");
        } Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field { pub name: String, pub label: String, pub secret: bool, pub purpose: String, #[serde(default)] pub choices: Vec<Choice>, #[serde(default, skip_serializing_if="String::is_empty")] pub value: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice { pub value: String, pub label: String }
impl Challenge {
    pub fn gateway_origin(&self, values: &BTreeMap<String, String>) -> Result<Option<String>> {
        let Some(field) = self.fields.iter().find(|field|field.name == "gateway" && field.purpose == "choice") else {return Ok(None)};
        let selected = values.get("gateway").ok_or_else(||anyhow::anyhow!("invalid_gateway_choice"))?;
        ensure!(field.choices.iter().any(|choice|&choice.value == selected), "invalid_gateway_choice");
        Ok(Some(termterm_vpn_core::sso::origin(&format!("https://{selected}"))?))
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.id > 0 && self.fields.len() <= 32 && self.message.len() <= 4096 && self.error.len() <= 4096, "invalid_challenge");
        if let Some(private) = &self.private {
            ensure!(self.kind.as_deref() == Some("sso") && self.provider.as_deref() == Some("globalProtect") && self.fields.is_empty(), "invalid_sso_challenge");
            ensure!(self.stage.as_deref().is_none_or(|stage|matches!(stage,"portal" | "gateway")), "invalid_sso_stage");
            private.validate()?;
        } else { ensure!(self.kind.is_none() && self.provider.is_none() && self.stage.is_none(), "invalid_challenge"); }
        let mut names = std::collections::HashSet::new();
        for field in &self.fields {
            ensure!(!field.name.is_empty() && field.name.len() <= 256 && field.label.len() <= 4096 && field.choices.len() <= 256 && names.insert(&field.name), "invalid_challenge_field");
            ensure!(field.choices.iter().all(|c| c.value.len() <= 4096 && c.label.len() <= 4096), "invalid_challenge_choice");
        }
        Ok(())
    }
    pub fn answer(&self, values: &BTreeMap<String, String>) -> Result<()> {
        ensure!(self.private.is_none(), "sso_result_required");
        ensure!(values.len() == self.fields.len(), "invalid_answer_fields");
        for field in &self.fields {
            let value = values.get(&field.name).ok_or_else(|| anyhow::anyhow!("invalid_answer_fields"))?;
            ensure!(value.len() <= 4096 && !value.contains('\0'), "invalid_answer");
            ensure!(field.choices.is_empty() || field.choices.iter().any(|c| &c.value == value), "invalid_choice");
        }
        Ok(())
    }
}
pub fn read_sync(reader: &mut impl std::io::BufRead) -> Result<Value> {
    let mut bytes = Vec::new();
    loop {
        let available = reader.fill_buf()?; ensure!(!available.is_empty(), "control_channel_closed");
        let position = available.iter().position(|v| *v == b'\n'); let count = position.map_or(available.len(), |v| v + 1);
        ensure!(bytes.len() + count <= crate::CONTROL_LIMIT, "control_message_too_large");
        bytes.extend_from_slice(&available[..count]); reader.consume(count);
        if position.is_some() { return Ok(serde_json::from_slice(&bytes)?); }
    }
}
pub async fn read_async<R: tokio::io::AsyncBufRead + Unpin>(reader: &mut R) -> Result<Request> {
    use tokio::io::AsyncBufReadExt;
    let mut bytes = Vec::new();
    loop {
        let available = reader.fill_buf().await?; ensure!(!available.is_empty(), "control_channel_closed");
        let position = available.iter().position(|v| *v == b'\n'); let count = position.map_or(available.len(), |v| v + 1);
        ensure!(bytes.len() + count <= crate::CONTROL_LIMIT, "control_message_too_large");
        bytes.extend_from_slice(&available[..count]); reader.consume(count);
        if position.is_some() { return Ok(serde_json::from_slice(&bytes)?); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_receipt_and_quiescence_are_strict_production_commands() {
        let raw=serde_json::json!({"action":"networkReceipt","requestId":1,"id":Uuid::new_v4(),"generation":1});
        assert!(serde_json::from_value::<Request>(raw.clone()).is_ok());
        let mut bad=raw;bad["untrusted"]=true.into();assert!(serde_json::from_value::<Request>(bad).is_err());
        let raw=serde_json::json!({"action":"quiesced","requestId":2,"id":Uuid::new_v4(),"generation":1});assert!(serde_json::from_value::<Request>(raw.clone()).is_ok());let mut bad=raw;bad["execute"]="command".into();assert!(serde_json::from_value::<Request>(bad).is_err());
    }
    #[test]
    fn sso_results_are_private_typed_and_bound_to_the_callback() {
        let raw = serde_json::json!({"id":1,"kind":"sso","provider":"globalProtect","fields":[],"private":{"url":"https://idp.example/saml?SAMLRequest=synthetic","expectedOrigin":"https://vpn.example","expectedPath":"/SAML20/SP/ACS"}});
        let challenge: Challenge = serde_json::from_value(raw.clone()).unwrap(); challenge.validate().unwrap();
        for stage in ["portal", "gateway"] {
            let mut staged = raw.clone(); staged["stage"] = stage.into();
            serde_json::from_value::<Challenge>(staged).unwrap().validate().unwrap();
        }
        let mut bad = raw.clone(); bad["stage"] = "connected".into();
        assert!(serde_json::from_value::<Challenge>(bad).unwrap().validate().is_err());
        assert!(challenge.answer(&BTreeMap::new()).is_err());
        let result = serde_json::json!({"uri":"https://vpn.example/SAML20/SP/ACS","username":"fixture-user","token":"fixture-token","tokenName":"prelogin-cookie","status":200,"authStatus":"1"});
        for name in ["prelogin-cookie", "portal-userauthcookie"] {
            let mut value = result.clone(); value["tokenName"] = name.into();
            challenge.private.as_ref().unwrap().answer(&serde_json::from_value(value).unwrap()).unwrap();
        }
        for (key, value) in [("uri", "https://idp.example/SAML20/SP/ACS"), ("uri", "https://vpn.example:444/SAML20/SP/ACS"), ("uri", "https://vpn.example/SAML20/SP/ACS?x=y"), ("tokenName", "passwd"), ("authStatus", "0"), ("token", ""), ("token", "bad\r\nheader")] {
            let mut bad = result.clone(); bad[key] = value.into();
            assert!(challenge.private.as_ref().unwrap().answer(&serde_json::from_value(bad).unwrap()).is_err());
        }
        let mut bad = result.clone(); bad["status"] = 302.into();
        assert!(challenge.private.as_ref().unwrap().answer(&serde_json::from_value(bad).unwrap()).is_err());
        let mut bad = result; bad["token"] = "x".repeat(4097).into();
        assert!(challenge.private.as_ref().unwrap().answer(&serde_json::from_value(bad).unwrap()).is_err());
        let mut bad = raw.clone(); bad["fields"] = serde_json::json!([{"name":"password","label":"Password","secret":true,"purpose":"secret"}]);
        assert!(serde_json::from_value::<Challenge>(bad).unwrap().validate().is_err());
        let mut bad = raw; bad.as_object_mut().unwrap().remove("private");
        assert!(serde_json::from_value::<Challenge>(bad).unwrap().validate().is_err());
        let ordinary: Challenge = serde_json::from_value(serde_json::json!({"id":2,"fields":[]})).unwrap(); ordinary.validate().unwrap();
        assert!(serde_json::to_value(ordinary).unwrap().get("private").is_none());
    }
    #[test]
    fn gateway_origin_changes_only_to_an_offered_choice() {
        let raw = serde_json::json!({"id":2,"fields":[{"name":"gateway","label":"Gateway","secret":false,"purpose":"choice","choices":[{"value":"gateway.example:444","label":"Gateway"}]}]});
        let challenge: Challenge = serde_json::from_value(raw.clone()).unwrap();
        let values = BTreeMap::from([("gateway".into(),"gateway.example:444".into())]);
        assert_eq!(challenge.gateway_origin(&values).unwrap().as_deref(),Some("https://gateway.example:444"));
        assert!(challenge.gateway_origin(&BTreeMap::from([("gateway".into(),"other.example".into())])).is_err());
        let mut empty = raw; empty["fields"][0]["choices"] = serde_json::json!([]);
        assert!(serde_json::from_value::<Challenge>(empty).unwrap().gateway_origin(&values).is_err());
    }
    #[test]
    fn bounded_frames_reject_truncation_and_unknown_control_fields() {
        assert!(read_sync(&mut std::io::Cursor::new(b"{}" )).is_err());
        assert!(read_sync(&mut std::io::Cursor::new(vec![b' '; crate::CONTROL_LIMIT + 1])).is_err());
        assert!(serde_json::from_value::<Request>(serde_json::json!({"action":"close","requestId":1,"execute":"cmd.exe"})).is_err());
        assert!(serde_json::from_str::<Request>("{\"action\":\"close\",\"requestId\":1,\"requestId\":2}").is_err());
    }
    #[test]
    fn ordinary_openvpn_progress_does_not_fail_a_connecting_session() {
        for name in ["RESOLVE","WAIT","CONNECTING","GET_CONFIG"] {
            validate_progress(serde_json::json!({"name":name,"error":false})).unwrap();
        }
        assert!(validate_progress(serde_json::json!({"name":"AUTH_FAILED","error":true})).is_err());
    }
    #[test]
    fn validation_targets_cannot_escape_disposable_loopback_fixtures() {
        let mut profile=Profile {version:1,label:"Disposable fixture".into(),family:termterm_vpn_core::Family::OpenVpn,source:"manual".into(),server:String::new(),group_id:None,username:String::new(),password:String::new(),realm:String::new(),config:"client\nproto tcp-client\nremote 127.0.0.1 31194\n".into(),remember:false};
        validate_fixture_profile(&profile).unwrap();
        for config in ["proto tcp-client\nremote attacker.example 31194\n","proto tcp-client\nremote 127.0.0.1 443\n","proto udp\nremote 127.0.0.1 31194\n","proto tcp-client\nremote 127.0.0.1 31194\nremote 192.0.2.1 31195\n"] {
            profile.config=config.into();assert!(validate_fixture_profile(&profile).is_err());
        }
        profile.family=termterm_vpn_core::Family::Cisco;profile.config.clear();profile.server="https://127.0.0.1:31443".into();validate_fixture_profile(&profile).unwrap();
        for server in ["https://127.0.0.1:443","https://attacker.example:31443","https://127.0.0.1:31443/other"] {profile.server=server.into();assert!(validate_fixture_profile(&profile).is_err());}
        profile.server="https://127.0.0.1:31443".into();profile.family=termterm_vpn_core::Family::Fortinet;assert!(validate_fixture_profile(&profile).is_err());
    }
}
