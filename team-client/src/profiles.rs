//! Pure resolution, called once per already-authorized hop. No implicit fallback list.
use crate::{
    keys::{self, Identity},
    transport::TeamClient,
    vault_crypto as crypto,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProtectedUsername {
    version: u32,
    ciphertext: String,
    envelope: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileSelection {
    pub host_id: Option<Uuid>,
    pub expected_revision: i64,
    pub method: String,
    pub credential_id: Option<Uuid>,
    pub username: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenProfile {
    pub host_id: Option<Uuid>,
    pub revision: i64,
    pub method: String,
    pub credential_id: Option<Uuid>,
    pub username: Option<String>,
    pub unavailable: bool,
}
#[derive(Serialize)]
pub struct ProfileList {
    pub default: Option<OpenProfile>,
    pub hosts: Vec<OpenProfile>,
}
pub(crate) fn context(team: Uuid, account: Uuid, host: Option<Uuid>, revision: i64) -> String {
    format!(
        "termterm-profile-v2:{team}:{account}:{}:{revision}",
        host.map(|v| v.to_string())
            .unwrap_or_else(|| "default".into())
    )
}
fn protect_username(identity: &Identity, aad: &str, username: &str) -> Result<String> {
    ensure!(
        !username.trim().is_empty()
            && username.len() <= 512
            && !username.chars().any(char::is_control),
        "Invalid username"
    );
    let key = crypto::random_key();
    Ok(hex::encode(serde_json::to_vec(&ProtectedUsername {
        version: 1,
        ciphertext: hex::encode(crypto::seal(&key, username.as_bytes(), aad.as_bytes())?),
        envelope: keys::wrap(&key, &identity.public_key(), aad)?,
    })?))
}
#[cfg(test)]
fn open_username(identity: &Identity, aad: &str, value: &str) -> Result<String> {
    open_username_envelope(identity, aad, value, None)
}
pub(crate) fn open_username_envelope(identity: &Identity, aad: &str, value: &str, envelope: Option<&str>) -> Result<String> {
    let p: ProtectedUsername = serde_json::from_slice(&hex::decode(value)?)?;
    ensure!(p.version == 1, "Unsupported profile encryption version");
    let key = keys::unwrap(identity, envelope.unwrap_or(&p.envelope), aad)?;
    let data = crypto::unseal(&key, &hex::decode(p.ciphertext)?, aad.as_bytes())?;
    Ok(String::from_utf8(data.to_vec())?)
}
fn open_profile(
    team: Uuid,
    account: Uuid,
    identity: &Identity,
    value: &Value,
) -> Result<OpenProfile> {
    let host = serde_json::from_value(value["hostId"].clone())?;
    let revision = value["revision"]
        .as_i64()
        .context("Invalid profile revision")?;
    ensure!(revision > 0, "Invalid profile revision");
    let method = value["method"]
        .as_str()
        .context("Missing profile method")?
        .to_string();
    let opened = value["encryptedUsername"]
        .as_str()
        .map(|p| open_username_envelope(identity, &context(team, account, host, revision), p, value["userEnvelope"].as_str()))
        .transpose();
    let unavailable = opened.is_err();
    Ok(OpenProfile {
        host_id: host,
        revision,
        method,
        credential_id: serde_json::from_value(value["credentialId"].clone())?,
        username: opened.unwrap_or(None),
        unavailable,
    })
}
impl TeamClient {
    pub async fn profile_history(&self,host:Option<Uuid>,identity:&Identity)->Result<Vec<OpenProfile>> {
        let rows=self.request(reqwest::Method::POST,"v1/profiles/history",Some(&json!({"hostId":host})),true).await?;
        let (team,account,_)=self.actor()?;
        rows.as_array().context("Invalid profile history")?.iter().map(|v|open_profile(team,account,identity,v)).collect()
    }
    pub async fn restore_profile(&self,host:Option<Uuid>,revision:i64,expected_revision:i64,identity:&Identity)->Result<Value> {
        let previous=self.profile_history(host,identity).await?.into_iter().find(|v|v.revision==revision).context("Profile version unavailable")?;
        ensure!(!previous.unavailable,"Archived username cannot be decrypted; re-enter it explicitly instead");
        self.save_profile(&ProfileSelection{host_id:host,expected_revision,method:previous.method,credential_id:previous.credential_id,username:previous.username},identity).await
    }
    pub async fn save_profile(
        &self,
        selection: &ProfileSelection,
        identity: &Identity,
    ) -> Result<Value> {
        let (team, account, _) = self.actor()?;
        ensure!(
            (0..i64::MAX).contains(&selection.expected_revision),
            "Invalid revision"
        );
        ensure!(
            matches!(
                selection.method.as_str(),
                "prompt" | "credential" | "inherit" | "keyboard_interactive"
            ),
            "Invalid authentication method"
        );
        ensure!(
            (selection.method == "credential") == selection.credential_id.is_some(),
            "Credential selection mismatch"
        );
        ensure!(
            selection.method != "inherit"
                || (selection.host_id.is_some() && selection.username.is_none()),
            "Only host profiles can inherit defaults"
        );
        let username = selection
            .username
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|p| {
                protect_username(
                    identity,
                    &context(
                        team,
                        account,
                        selection.host_id,
                        selection.expected_revision + 1,
                    ),
                    p,
                )
            })
            .transpose()?;
        let mut payload=json!({"hostId":selection.host_id,"expectedRevision":selection.expected_revision,"method":selection.method,"credentialId":selection.credential_id,"encryptedUsername":username});
        if let Some(encoded)=username.as_deref() {
            let crypto_context=self.request(reqwest::Method::GET,"v1/crypto/context",None,true).await?;
            let public=crypto_context["recovery"]["publicKey"].as_str().context("Configure the company recovery file before saving a personal username")?;
            let aad=context(team,account,selection.host_id,selection.expected_revision+1);
            let protected:ProtectedUsername=serde_json::from_slice(&hex::decode(encoded)?)?;
            let key=keys::unwrap(identity,&protected.envelope,&aad)?;
            payload["recoveryEnvelope"]=json!(keys::wrap(&key,public,&aad)?);
            payload["userEnvelope"]=json!(protected.envelope);
            payload["recoveryKeyId"]=crypto_context["recovery"]["keyId"].clone();
            payload["identityVersion"]=json!(self.actor()?.2);
        }
        self.request(reqwest::Method::PUT,"v1/profiles",Some(&payload),true).await
    }
    pub async fn open_profiles(&self, identity: &Identity) -> Result<ProfileList> {
        let (team, account, _) = self.actor()?;
        let rows = self.profiles().await?;
        let default = if rows["default"].is_null() {
            None
        } else {
            Some(open_profile(team, account, identity, &rows["default"])?)
        };
        let hosts = rows["hosts"]
            .as_array()
            .context("Invalid profile list")?
            .iter()
            .map(|v| open_profile(team, account, identity, v))
            .collect::<Result<Vec<_>>>()?;
        Ok(ProfileList { default, hosts })
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum Method {
    Prompt,
    KeyboardInteractive,
    Credential { id: Uuid },
}
#[derive(Clone)]
pub struct Profile {
    pub method: Method,
    pub username: Option<String>,
}
pub enum UsernamePolicy {
    SharedRequired(String),
    Personal,
}
pub struct Host {
    pub id: Uuid,
    pub policy: UsernamePolicy,
}
#[derive(PartialEq, Debug)]
pub struct Resolved {
    pub username: Option<String>,
    pub method: Method,
}
pub fn resolve(
    host: &Host,
    personal: Option<&Profile>,
    default: Option<&Profile>,
) -> Result<Resolved> {
    let username = match &host.policy {
        UsernamePolicy::SharedRequired(name) => {
            ensure!(!name.trim().is_empty(), "Shared username is missing");
            Some(name.clone())
        }
        UsernamePolicy::Personal => personal
            .and_then(|p| p.username.clone())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                default
                    .and_then(|p| p.username.clone())
                    .filter(|s| !s.trim().is_empty())
            }),
    };
    Ok(Resolved {
        username,
        method: personal
            .or(default)
            .map(|p| p.method.clone())
            .unwrap_or(Method::Prompt),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn usernames_are_bound_to_owner_host_and_revision() {
        let identity = Identity::generate();
        let team = Uuid::new_v4();
        let account = Uuid::new_v4();
        let host = Some(Uuid::new_v4());
        let aad = context(team, account, host, 1);
        let encrypted = protect_username(&identity, &aad, "şirket_admin").unwrap();
        assert_eq!(
            open_username(&identity, &aad, &encrypted).unwrap(),
            "şirket_admin"
        );
        for other in [
            context(team, account, host, 2),
            context(team, account, None, 1),
            context(team, Uuid::new_v4(), host, 1),
        ] {
            assert!(open_username(&identity, &other, &encrypted).is_err());
        }
        assert!(open_username(&Identity::generate(), &aad, &encrypted).is_err());
        assert!(protect_username(&identity, &aad, "name\nadmin").is_err());
    }
    #[test]
    fn precedence_locked_username_and_explicit_prompt() {
        let host = Host {
            id: Uuid::new_v4(),
            policy: UsernamePolicy::SharedRequired("company_admin".into()),
        };
        let defaults = Profile {
            method: Method::Credential { id: Uuid::new_v4() },
            username: Some("personal".into()),
        };
        let personal = Profile {
            method: Method::Prompt,
            username: Some("override".into()),
        };
        let result = resolve(&host, Some(&personal), Some(&defaults)).unwrap();
        assert_eq!(result.username.as_deref(), Some("company_admin"));
        assert_eq!(result.method, Method::Prompt);
        let host = Host {
            policy: UsernamePolicy::Personal,
            ..host
        };
        assert_eq!(
            resolve(&host, None, Some(&defaults)).unwrap().method,
            defaults.method
        );
        assert_eq!(
            resolve(&host, Some(&personal), Some(&defaults))
                .unwrap()
                .username
                .as_deref(),
            Some("override")
        );
        assert_eq!(
            resolve(&host, None, None).unwrap(),
            Resolved {
                username: None,
                method: Method::Prompt
            }
        );
    }
}
