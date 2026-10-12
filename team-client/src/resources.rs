//! Resource encryption and typed HTTPS operations, independent of personal credentials.
use crate::{
    keys::{self, Identity},
    transport::TeamClient,
    vault_crypto as crypto,
};
use anyhow::{ensure, Context, Result};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceContent {
    pub kind: String,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub address: Option<String>,
    pub protocol: Option<String>,
    pub port: Option<u16>,
    pub username_policy: Option<String>,
    pub username: Option<String>,
    pub deleted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<TeamDocument>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<ProxyConfig>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxyConfig {
    pub kind: String,
    pub host: String,
    pub port: u16,
    pub authenticate: bool,
}
impl ProxyConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(matches!(self.kind.as_str(), "http" | "socks5"), "Unsupported proxy type");
        ensure!(self.port > 0 && !self.host.is_empty() && self.host.len() <= 253
            && !self.host.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c,'/'|'@'|'\\')),
            "Invalid proxy address or port");
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum TeamDocument {
    Snippet { command: String },
    Workspace { layout: WorkspaceLayout, entries: Vec<WorkspaceEntry> },
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceLayout { Tabs, Grid }
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceEntry {
    pub host_id: Uuid,
    pub startup_command: String,
    pub stats_enabled: bool,
}
impl TeamDocument {
    pub fn host_ids(&self) -> Option<Vec<Uuid>> {
        match self { Self::Workspace { entries, .. } => Some(entries.iter().map(|e|e.host_id).collect()), _ => None }
    }
    fn validate(&self) -> Result<()> {
        let valid_command=|s:&str|s.len()<=16384 && !s.contains('\0');
        match self {
            Self::Snippet {command} => ensure!(!command.trim().is_empty() && valid_command(command), "Command required; maximum 16384 UTF-8 bytes, no NUL"),
            Self::Workspace { entries,.. } => {
                ensure!(!entries.is_empty() && entries.len()<=16,"Workspace requires 1–16 targets");
                let mut ids=std::collections::HashSet::new();
                for e in entries { ensure!(ids.insert(e.host_id) && valid_command(&e.startup_command), "Duplicate target or invalid startup command"); }
            }
        }
        ensure!(serde_json::to_vec(self)?.len()<=32768,"Document exceeds 32768 UTF-8 bytes");
        Ok(())
    }
}
impl ResourceContent {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.trim().is_empty() && self.name.len() <= 2048,
            "Resource name required (maximum 2048 bytes)"
        );
        if let Some(proxy) = &self.proxy {
            ensure!(self.kind == "host" && matches!(self.protocol.as_deref(), Some("ssh" | "mosh")), "Proxy requires SSH or Mosh");
            proxy.validate()?;
        }
        match self.kind.as_str() {
            "folder" => ensure!(
                self.address.is_none()
                    && self.protocol.is_none()
                    && self.port.is_none()
                    && self.username_policy.is_none()
                    && self.username.is_none(),
                "Folder cannot contain host fields"
            ),
            "host" => {
                ensure!(
                    self.address
                        .as_ref()
                        .is_some_and(|x| !x.trim().is_empty() && x.len() <= 2048),
                    "Host address required"
                );
                ensure!(
                    matches!(
                        self.protocol.as_deref(),
                        Some("ssh" | "mosh" | "telnet" | "serial")
                    ) && self.port.is_some_and(|p| p > 0),
                    "Invalid protocol/port"
                );
                ensure!(
                    matches!(
                        self.username_policy.as_deref(),
                        Some("shared_required" | "personal")
                    ),
                    "Username policy required"
                );
                ensure!(
                    self.username_policy.as_deref() != Some("shared_required")
                        || self.username.as_ref().is_some_and(|x| !x.trim().is_empty()),
                    "Shared username required"
                );
                ensure!(
                    self.username.as_ref().is_none_or(|x| x.len() <= 2048),
                    "Username too long"
                );
            }
            "snippet" | "workspace" => {
                ensure!(self.address.is_none() && self.protocol.is_none() && self.port.is_none() && self.username_policy.is_none() && self.username.is_none(),"Document cannot contain host fields");
                let d=self.document.as_ref().context("Document missing")?;
                ensure!(matches!((self.kind.as_str(),d),("snippet",TeamDocument::Snippet {..}) | ("workspace",TeamDocument::Workspace {..})),"Document kind mismatch");
                d.validate()?;
            }
            _ => anyhow::bail!("Unsupported resource kind"),
        }
        ensure!(matches!(self.kind.as_str(),"snippet"|"workspace") || self.document.is_none(),"Unexpected document");
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct PreparedResourceChange(pub(crate) Value);
impl PreparedResourceChange {
    pub fn operation_id(&self) -> Result<Uuid> {
        Ok(serde_json::from_value(self.0["operationId"].clone())?)
    }
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeReceipt {
    pub operation_id: Uuid,
    pub status: String,
    pub revision: Option<i64>,
    pub conflict_id: Option<Uuid>,
}
pub struct OpenResource {
    pub id: Uuid,
    pub revision: i64,
    pub content: ResourceContent,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictCandidate {
    pub id: Uuid,
    pub resource_id: Uuid,
    pub expected_revision: i64,
    pub content: Option<ResourceContent>,
    pub error: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictPage {
    pub conflicts: Vec<ConflictCandidate>,
    pub next_after: Option<Uuid>,
}
fn base(team: Uuid, id: Uuid, revision: i64) -> String {
    format!("termterm-resource-v2:{team}:{id}:{revision}")
}
fn seal_field(key: &[u8; 32], base: &str, field: &str, value: &[u8]) -> Result<String> {
    Ok(hex::encode(crypto::seal(
        key,
        value,
        format!("{base}:{field}").as_bytes(),
    )?))
}
pub(crate) fn encrypt_change(
    team: Uuid,
    vault: Uuid,
    id: Uuid,
    expected: i64,
    timestamp: (i64, &str),
    content: &ResourceContent,
    preparation: &Value,
) -> Result<PreparedResourceChange> {
    let (sequence, client_at) = timestamp;
    content.validate()?;
    ensure!(
        (0..i64::MAX).contains(&expected) && sequence > 0,
        "Invalid revision/sequence"
    );
    let revision = expected + 1;
    let context = base(team, id, revision);
    let key = crypto::random_key();
    let recipients = preparation["recipients"]
        .as_array()
        .context("Missing recipients")?;
    ensure!(!recipients.is_empty(), "No authorized recipients");
    let envelopes=recipients.iter().map(|r|->Result<Value>{Ok(json!({"accountId":r["accountId"],"identityVersion":r["identityVersion"],
        "envelope":keys::wrap(&key,r["publicKey"].as_str().context("Missing recipient key")?,&context)?}))}).collect::<Result<Vec<_>>>()?;
    let snapshot = Zeroizing::new(serde_json::to_vec(content)?);
    let dependencies = preparation.get("dependencyRecipients").map(|value| -> Result<Vec<Value>> {
        let rows=value.as_array().context("Invalid dependency recipients")?;
        ensure!(rows.len()<=15000,"Too many dependency recipients");
        rows.iter().map(|r| {
            let root:Uuid=serde_json::from_value(r["rootId"].clone())?;
            let context=format!("termterm-chain-v2:{team}:{root}:{id}:{revision}");
            Ok(json!({"rootId":root,"accountId":r["accountId"],"identityVersion":r["identityVersion"],
                "envelope":keys::wrap(&key,r["publicKey"].as_str().context("Missing dependency recipient key")?,&context)?}))
        }).collect()
    }).transpose()?;
    Ok(PreparedResourceChange(
        json!({"operationId":Uuid::new_v4(),"sequence":sequence,"vaultId":vault,"resourceId":id,
        "expectedRevision":expected,"aclRevision":preparation["aclRevision"],"writeCapability":preparation["writeCapability"],"clientAt":client_at,
        "kind":content.kind,"parentId":content.parent_id,"deleted":content.deleted,"protocol":content.protocol,"port":content.port,"usernamePolicy":content.username_policy,
        "encryptedName":seal_field(&key,&context,"name",content.name.as_bytes())?,
        "encryptedAddress":content.address.as_ref().map(|x|seal_field(&key,&context,"address",x.as_bytes())).transpose()?,
        "encryptedUsername":content.username.as_ref().map(|x|seal_field(&key,&context,"username",x.as_bytes())).transpose()?,
        "encryptedDocument":content.document.as_ref().map(|d|seal_field(&key,&context,"document",&serde_json::to_vec(d)?)).transpose()?,
        "workspaceHostIds":content.document.as_ref().and_then(TeamDocument::host_ids),
        "snapshot":seal_field(&key,&context,"snapshot",&snapshot)?,"envelopes":envelopes,"dependencyEnvelopes":dependencies}),
    ))
}
pub(crate) fn open(team: Uuid, value: &Value, identity: &Identity) -> Result<OpenResource> {
    let id: Uuid = serde_json::from_value(value["id"].clone())?;
    let revision = value["revision"]
        .as_i64()
        .context("Invalid resource revision")?;
    ensure!(revision > 0, "Invalid revision");
    let context = base(team, id, revision);
    let key = keys::unwrap(
        identity,
        value["envelope"]
            .as_str()
            .context("Resource key unavailable; permission/identity refresh required")?,
        &context,
    )?;
    let bytes = crypto::unseal(
        &key,
        &hex::decode(value["snapshot"].as_str().context("Missing snapshot")?)?,
        format!("{context}:snapshot").as_bytes(),
    )?;
    let content: ResourceContent = serde_json::from_slice(&bytes)?;
    content.validate()?;
    let plain = serde_json::to_value(&content)?;
    for field in [
        "kind",
        "parentId",
        "deleted",
        "protocol",
        "port",
        "usernamePolicy",
    ] {
        ensure!(
            plain[field] == value[field],
            "Resource control metadata mismatch: {field}"
        );
    }
    for (plain_field, cipher_field) in [
        ("name", "encryptedName"),
        ("address", "encryptedAddress"),
        ("username", "encryptedUsername"),
    ] {
        if let Some(expected) = plain[plain_field].as_str() {
            let data = crypto::unseal(
                &key,
                &hex::decode(
                    value[cipher_field]
                        .as_str()
                        .context("Missing encrypted field")?,
                )?,
                format!("{context}:{plain_field}").as_bytes(),
            )?;
            ensure!(
                data.as_slice() == expected.as_bytes(),
                "Resource field mismatch"
            );
        } else {
            ensure!(value[cipher_field].is_null(), "Unexpected encrypted field");
        }
    }
    if let Some(document)=&content.document {
        let encrypted=value["encryptedDocument"].as_str().context("Document ciphertext missing")?;
        let bytes=crypto::unseal(&key,&hex::decode(encrypted)?,format!("{context}:document").as_bytes())?;
        ensure!(serde_json::from_slice::<TeamDocument>(&bytes)?==*document,"Document mismatch");
        ensure!(serde_json::to_value(document.host_ids())?==value["workspaceHostIds"],"Workspace target metadata mismatch");
    } else {
        ensure!(value["encryptedDocument"].is_null() && value["workspaceHostIds"].is_null(),"Unexpected document metadata");
    }
    Ok(OpenResource {
        id,
        revision,
        content,
    })
}
impl TeamClient {
    pub async fn pending_conflicts(
        &self,
        identity: &Identity,
        after: Option<Uuid>,
    ) -> Result<ConflictPage> {
        let (team, _, _) = self.actor()?;
        let path = format!(
            "v1/conflicts?limit=100{}",
            after.map(|id| format!("&after={id}")).unwrap_or_default()
        );
        let response = self.request(Method::GET, &path, None, true).await?;
        let mut conflicts = Vec::new();
        for row in response["conflicts"]
            .as_array()
            .context("Invalid conflict response")?
        {
            let id: Uuid = serde_json::from_value(row["id"].clone())?;
            let resource_id: Uuid = serde_json::from_value(row["resourceId"].clone())?;
            let revision = row["expectedRevision"]
                .as_i64()
                .context("Invalid revision")?;
            let decrypted = (|| -> Result<ResourceContent> {
                ensure!(
                    (0..i64::MAX).contains(&revision),
                    "Invalid candidate revision"
                );
                let context = base(team, resource_id, revision + 1);
                let key = keys::unwrap(
                    identity,
                    row["envelope"].as_str().context("Missing candidate key")?,
                    &context,
                )?;
                let bytes = crypto::unseal(
                    &key,
                    &hex::decode(
                        row["encryptedCandidate"]
                            .as_str()
                            .context("Missing candidate")?,
                    )?,
                    format!("{context}:snapshot").as_bytes(),
                )?;
                let content: ResourceContent = serde_json::from_slice(&bytes)?;
                content.validate()?;
                Ok(content)
            })();
            let (content, error) = match decrypted {
                Ok(c) => (Some(c), None),
                Err(_) => (
                    None,
                    Some(
                        "Candidate could not be decrypted; the current record is unchanged".into(),
                    ),
                ),
            };
            conflicts.push(ConflictCandidate {
                id,
                resource_id,
                expected_revision: revision,
                content,
                error,
            });
        }
        Ok(ConflictPage {
            conflicts,
            next_after: serde_json::from_value(response["nextAfter"].clone())?,
        })
    }
    pub async fn decide_conflict(
        &self,
        id: Uuid,
        current_revision: i64,
        replacement: Option<&PreparedResourceChange>,
    ) -> Result<Value> {
        let body = json!({"currentRevision":current_revision,"choice":if replacement.is_some(){"replacement"}else{"server"},"change":replacement});
        self.request(
            Method::POST,
            &format!("v1/conflicts/{id}/decision"),
            Some(&body),
            true,
        )
        .await
    }
    pub async fn create_vault(&self, id: Uuid, name: &str, identity: &Identity) -> Result<Value> {
        ensure!(
            !name.trim().is_empty() && name.len() <= 2048,
            "Vault name required"
        );
        let (team, _, version) = self.actor()?;
        let key = crypto::random_key();
        let context = format!("termterm-vault-v2:{team}:{id}");
        self.request(Method::POST,"v1/vaults",Some(&json!({"id":id,"identityVersion":version,
            "encryptedName":seal_field(&key,&context,"name",name.as_bytes())?,"envelope":keys::wrap(&key,&identity.public_key(),&context)?})),true).await
    }
    pub async fn vaults(&self) -> Result<Value> {
        self.request(Method::GET, "v1/vaults", None, true).await
    }
    pub async fn prepare_resource(
        &self,
        vault: Uuid,
        id: Uuid,
        expected: i64,
        sequence: i64,
        client_at: &str,
        content: &ResourceContent,
    ) -> Result<PreparedResourceChange> {
        let (team, _, _) = self.actor()?;
        let preparation = self
            .request(
                Method::POST,
                "v1/resources/recipients",
                Some(&json!({"vaultId":vault,"resourceId":id,"parentId":content.parent_id})),
                true,
            )
            .await?;
        encrypt_change(
            team,
            vault,
            id,
            expected,
            (sequence, client_at),
            content,
            &preparation,
        )
    }
    pub async fn save_resource(&self, change: &PreparedResourceChange) -> Result<ChangeReceipt> {
        Ok(serde_json::from_value(
            self.request(Method::POST, "v1/resources/changes", Some(&change.0), true)
                .await?,
        )?)
    }
    pub async fn open_resource(&self, id: Uuid, identity: &Identity) -> Result<OpenResource> {
        let (team, _, _) = self.actor()?;
        let value = self
            .request(Method::GET, &format!("v1/resources/{id}"), None, true)
            .await?;
        open(team, &value, identity)
    }
    pub async fn resource_page(&self, vault: Uuid, after: Option<Uuid>) -> Result<Value> {
        let path = format!(
            "v1/vaults/{vault}/resources?limit=100{}",
            after.map(|a| format!("&after={a}")).unwrap_or_default()
        );
        self.request(Method::GET, &path, None, true).await
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn proxy_metadata_rejects_secrets_and_invalid_endpoints() {
        use super::ProxyConfig;
        for kind in ["http", "socks5"] {
            let p=ProxyConfig{kind:kind.into(),host:"proxy.example".into(),port:1080,authenticate:true};
            assert!(p.validate().is_ok());
            for host in ["", "proxy\r\nInjected", "user@proxy", "http://proxy", "proxy path"] {
                assert!(ProxyConfig{host:host.into(),..p.clone()}.validate().is_err());
            }
            assert!(ProxyConfig{port:0,..p}.validate().is_err());
        }
        assert!(serde_json::from_value::<ProxyConfig>(serde_json::json!({"kind":"http","host":"proxy","port":80,"authenticate":true,"password":"must not enter host metadata"})).is_err());
    }
    use super::*;
    #[test]
    fn shared_resource_crypto_is_bound_to_identity_fields_and_revision() {
        let team = Uuid::new_v4();
        let vault = Uuid::new_v4();
        let id = Uuid::new_v4();
        let identity = Identity::generate();
        let other = Identity::generate();
        let content = ResourceContent {
            document: None, proxy: None,
            kind: "host".into(),
            parent_id: None,
            name: "Türkçe üretim".into(),
            address: Some("127.0.0.1".into()),
            protocol: Some("ssh".into()),
            port: Some(22),
            username_policy: Some("shared_required".into()),
            username: Some("company_admin".into()),
            deleted: false,
        };
        let preparation = json!({"aclRevision":1,"writeCapability":Uuid::new_v4(),"recipients":[{"accountId":Uuid::new_v4(),"identityVersion":1,"publicKey":identity.public_key()}]});
        let change = encrypt_change(
            team,
            vault,
            id,
            0,
            (1, "2026-10-06T12:00:00Z"),
            &content,
            &preparation,
        )
        .unwrap();
        let mut row = change.0.clone();
        row["id"] = json!(id);
        row["revision"] = json!(1);
        row["envelope"] = row["envelopes"][0]["envelope"].clone();
        assert_eq!(open(team, &row, &identity).unwrap().content, content);
        assert!(open(team, &row, &other).is_err());
        assert!(open(Uuid::new_v4(), &row, &identity).is_err());
        row["port"] = json!(2222);
        assert!(open(team, &row, &identity).is_err());
        row["port"] = json!(22);
        row["revision"] = json!(2);
        assert!(open(team, &row, &identity).is_err());
    }
    #[test]
    fn document_ciphertext_targets_and_limits_are_authenticated() {
        let identity=Identity::generate();let team=Uuid::new_v4();let id=Uuid::new_v4();let host=Uuid::new_v4();
        let mut c=ResourceContent{kind:"workspace".into(),parent_id:None,name:"İşlem alanı".into(),address:None,port:None,protocol:None,username_policy:None,username:None,deleted:false,
            proxy:None, document:Some(TeamDocument::Workspace{layout:WorkspaceLayout::Grid,entries:vec![WorkspaceEntry{host_id:host,startup_command:"printf 'Türkçe'".into(),stats_enabled:false}]})};
        let p=json!({"aclRevision":1,"writeCapability":Uuid::new_v4(),"recipients":[{"accountId":Uuid::new_v4(),"identityVersion":1,"publicKey":identity.public_key()}]});
        let change=encrypt_change(team,Uuid::new_v4(),id,0,(1,"2026-10-08T00:00:00Z"),&c,&p).unwrap();
        let mut row=change.0;row["id"]=json!(id);row["revision"]=json!(1);row["envelope"]=row["envelopes"][0]["envelope"].clone();
        assert_eq!(open(team,&row,&identity).unwrap().content,c);
        assert!(!row.to_string().contains("printf"));
        row["workspaceHostIds"]=json!([Uuid::new_v4()]);assert!(open(team,&row,&identity).is_err());
        row["workspaceHostIds"]=json!([host]);row["encryptedDocument"]=json!("00".repeat(100));assert!(open(team,&row,&identity).is_err());
        if let Some(TeamDocument::Workspace{entries,..})=&mut c.document {entries.push(entries[0].clone());}
        assert!(c.validate().is_err());
        c.kind="snippet".into();c.document=Some(TeamDocument::Snippet{command:"echo test".into()});assert!(c.validate().is_ok());
        c.document=Some(TeamDocument::Snippet{command:"bad\0command".into()});assert!(c.validate().is_err());
        c.document=Some(TeamDocument::Snippet{command:"ş".repeat(8193)});assert!(c.validate().is_err());
        c.kind="host".into();assert!(c.validate().is_err());
    }

}
