//! Explicit private-to-shared copies. Secrets stay in Rust until an authorized
//! reveal/export action; connection consumers receive non-serializable material.
use crate::{
    keys::{self, Identity, Purpose},
    transport::{OpenCredential, TeamClient},
};
use anyhow::{ensure, Context, Result};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShareRecipient {
    pub account_id: Uuid,
    pub connect: bool,
    pub reveal: bool,
    pub export: bool,
}
impl TeamClient {
    pub async fn shared_update_options(&self, id: Uuid) -> Result<Value> {
        self.request(
            Method::GET,
            &format!("v1/credentials/shared/{id}/update-options"),
            None,
            true,
        )
        .await
    }
    pub async fn prepare_shared_update(
        &self,
        id: Uuid,
        expected: i64,
        source: Option<Uuid>,
        recipients: &[ShareRecipient],
    ) -> Result<Value> {
        self.request(
            Method::POST,
            &format!("v1/credentials/shared/{id}/updates"),
            Some(&json!({"expectedRevision":expected,"sourceId":source,"recipients":recipients})),
            true,
        )
        .await
    }
    pub async fn cancel_shared_update(&self, plan: Uuid) -> Result<Value> {
        self.request(
            Method::POST,
            &format!("v1/credentials/updates/{plan}"),
            None,
            true,
        )
        .await
    }
    pub async fn finish_shared_update(&self, plan: Uuid, identity: &Identity) -> Result<Value> {
        let p = self
            .request(
                Method::GET,
                &format!("v1/credentials/updates/{plan}"),
                None,
                true,
            )
            .await?;
        if p["status"] == "applied" {
            return Ok(p);
        }
        let id: Uuid = serde_json::from_value(p["credentialId"].clone())?;
        let expected = p["expectedRevision"]
            .as_i64()
            .context("Missing expected revision")?;
        let source = if p["sourceId"].is_null() {
            let value = self
                .open_shared_credential(id, "reveal", None, identity)
                .await?;
            ensure!(
                value.revision == expected,
                "Shared credential changed; preview again"
            );
            value
        } else {
            let source_id: Uuid = serde_json::from_value(p["sourceId"].clone())?;
            let value = self.open_private_credential(source_id, identity).await?;
            ensure!(
                Some(value.revision) == p["sourceRevision"].as_i64(),
                "Private source changed; preview again"
            );
            value
        };
        self.finish_shared_source(&p, identity, &source).await
    }
    pub(crate) async fn finish_shared_source(&self,p:&Value,identity:&Identity,source:&OpenCredential)->Result<Value> {
        if p["status"]=="applied" {return Ok(p.clone());}
        let plan:Uuid=serde_json::from_value(p["id"].clone())?;
        let id:Uuid=serde_json::from_value(p["credentialId"].clone())?;
        let expected=p["expectedRevision"].as_i64().context("Missing expected revision")?;
        let revision = expected.checked_add(1).context("Revision exhausted")?;
        let (team, _, _) = self.actor()?;
        let recovery = p["recoveryPublicKey"]
            .as_str()
            .context("Missing recovery key")?;
        let data = keys::encrypt(
            team,
            id,
            revision,
            Purpose::Data,
            &source.data,
            &identity.public_key(),
            recovery,
        )?;
        let secret = keys::encrypt(
            team,
            id,
            revision,
            Purpose::Secret,
            &source.secret,
            &identity.public_key(),
            recovery,
        )?;
        let envelopes=p["recipients"].as_array().context("Recipients missing")?.iter().map(|r|->Result<Value>{
            let key=r["publicKey"].as_str().context("Recipient key missing")?;
            Ok(json!({"accountId":r["accountId"],"identityVersion":r["identityVersion"],
                "data":keys::rewrap(team,id,revision,Purpose::Data,&data.user_envelope,identity,key)?,
                "secret":keys::rewrap(team,id,revision,Purpose::Secret,&secret.user_envelope,identity,key)?}))
        }).collect::<Result<Vec<_>>>()?;
        self.request(Method::POST,&format!("v1/credentials/updates/{plan}/finish"),Some(&json!({
            "encryptedData":data.ciphertext,"encryptedSecret":secret.ciphertext,"recoveryData":data.recovery_envelope,
            "recoverySecret":secret.recovery_envelope,"envelopes":envelopes})),true).await
    }
    pub async fn share_candidates(&self, scope: Uuid) -> Result<Value> {
        self.request(
            Method::GET,
            &format!("v1/credentials/share/candidates/{scope}"),
            None,
            true,
        )
        .await
    }
    pub async fn prepare_share(
        &self,
        source: Uuid,
        scope: Uuid,
        recipients: &[ShareRecipient],
    ) -> Result<Value> {
        self.request(
            Method::POST,
            "v1/credentials/share/plans",
            Some(&json!({"sourceId":source,"scopeId":scope,"recipients":recipients})),
            true,
        )
        .await
    }
    pub async fn cancel_share(&self, plan: Uuid) -> Result<Value> {
        self.request(
            Method::POST,
            &format!("v1/credentials/share/plans/{plan}"),
            None,
            true,
        )
        .await
    }
    pub async fn finish_share(&self, plan: Uuid, identity: &Identity) -> Result<Value> {
        let p = self
            .request(
                Method::GET,
                &format!("v1/credentials/share/plans/{plan}"),
                None,
                true,
            )
            .await?;
        if p["status"] == "applied" {
            return Ok(json!({"credentialId":p["credentialId"],"revision":p["revision"]}));
        }
        let source: Uuid = serde_json::from_value(p["sourceId"].clone())?;
        let id: Uuid = serde_json::from_value(p["credentialId"].clone())?;
        let source = self.open_private_credential(source, identity).await?;
        ensure!(
            Some(source.revision) == p["sourceRevision"].as_i64(),
            "Source changed; create a new sharing preview"
        );
        let (team, _, _) = self.actor()?;
        let recovery = p["recoveryPublicKey"]
            .as_str()
            .context("Recovery key missing")?;
        let data = keys::encrypt(
            team,
            id,
            1,
            Purpose::Data,
            &source.data,
            &identity.public_key(),
            recovery,
        )?;
        let secret = keys::encrypt(
            team,
            id,
            1,
            Purpose::Secret,
            &source.secret,
            &identity.public_key(),
            recovery,
        )?;
        let envelopes=p["recipients"].as_array().context("Recipients missing")?.iter().map(|r|->Result<Value>{
            let key=r["publicKey"].as_str().context("Recipient key missing")?;
            Ok(json!({"accountId":r["accountId"],"identityVersion":r["identityVersion"],
                "data":keys::rewrap(team,id,1,Purpose::Data,&data.user_envelope,identity,key)?,
                "secret":keys::rewrap(team,id,1,Purpose::Secret,&secret.user_envelope,identity,key)?}))
        }).collect::<Result<Vec<_>>>()?;
        self.request(Method::POST,&format!("v1/credentials/share/plans/{plan}/finish"),Some(&json!({
            "encryptedData":data.ciphertext,"encryptedSecret":secret.ciphertext,
            "recoveryData":data.recovery_envelope,"recoverySecret":secret.recovery_envelope,"envelopes":envelopes
        })),true).await
    }
    pub async fn shared_credentials(&self) -> Result<Value> {
        self.request(Method::GET, "v1/credentials/shared", None, true)
            .await
    }
    pub async fn remove_shared(&self, id: Uuid, revision: i64) -> Result<Value> {
        self.request(
            Method::POST,
            &format!("v1/credentials/shared/{id}/remove"),
            Some(&json!({"expectedRevision":revision})),
            true,
        )
        .await
    }
    pub async fn open_shared_credential(
        &self,
        id: Uuid,
        purpose: &str,
        host: Option<Uuid>,
        identity: &Identity,
    ) -> Result<OpenCredential> {
        ensure!(
            ["connect", "reveal", "export"].contains(&purpose),
            "Invalid credential purpose"
        );
        let value = self
            .request(
                Method::POST,
                &format!("v1/credentials/shared/{id}/read"),
                Some(&json!({"purpose":purpose,"hostId":host})),
                true,
            )
            .await?;
        self.decode_connection_credential(id, &value, identity)
    }
    pub(crate) fn decode_connection_credential(&self, id: Uuid, value: &Value, identity: &Identity) -> Result<OpenCredential> {
        let (team, _, version) = self.actor()?;
        let revision = value["revision"].as_i64().context("Invalid revision")?;
        let open = |purpose: Purpose, name: &str, field: &str| -> Result<_> {
            let envelope = value["envelopes"]
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .find(|e| e["purpose"] == name && e["identityVersion"] == version)
                })
                .and_then(|e| e["envelope"].as_str())
                .context("Shared identity recovery required")?;
            keys::decrypt(
                team,
                id,
                revision,
                purpose,
                value[field].as_str().context("Ciphertext missing")?,
                envelope,
                identity,
            )
        };
        Ok(OpenCredential {
            kind: value["kind"].as_str().context("Kind missing")?.into(),
            revision,
            data: open(Purpose::Data, "data", "encryptedData")?,
            secret: open(Purpose::Secret, "secret", "encryptedSecret")?,
        })
    }
    pub async fn open_connection_credential(
        &self,
        id: Uuid,
        host: Uuid,
        identity: &Identity,
    ) -> Result<OpenCredential> {
        let shared = self.shared_credentials().await?;
        if shared
            .as_array()
            .context("Invalid identity list")?
            .iter()
            .any(|c| c["id"] == id.to_string())
        {
            self.open_shared_credential(id, "connect", Some(host), identity)
                .await
        } else {
            self.open_private_credential(id, identity).await
        }
    }
}
