use crate::{
    keys::{self, Identity},
    resources::ResourceContent,
    transport::TeamClient,
    vault_crypto as crypto,
};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub revision: i64,
    pub created_at: Value,
    pub actor_id: Value,
    pub content: Option<ResourceContent>,
    pub error: Option<String>,
}
impl TeamClient {
    pub async fn select_history(
        &self,
        id: Uuid,
        revision: Option<i64>,
        identity: &Identity,
    ) -> Result<Value> {
        if let Some(revision) = revision {
            let old = self
                .history(id, identity)
                .await?
                .into_iter()
                .find(|v| v.revision == revision)
                .context("History unavailable")?
                .content
                .context("Historical content cannot be decrypted")?;
            ensure!(
                old.kind == "host" && !old.deleted && old.protocol.as_deref() == Some("ssh"),
                "Historical connection requires a valid SSH host"
            );
        }
        self.request(
            reqwest::Method::POST,
            &format!("v1/resources/{id}/history/selection"),
            Some(&json!({"revision":revision})),
            true,
        )
        .await
    }
    pub async fn history_selections(&self) -> Result<Value> {
        self.request(reqwest::Method::GET, "v1/history/selections", None, true)
            .await
    }
    pub async fn history(&self, id: Uuid, identity: &Identity) -> Result<Vec<Version>> {
        let (team, _, _) = self.actor()?;
        let rows = self
            .request(
                reqwest::Method::GET,
                &format!("v1/resources/{id}/history"),
                None,
                true,
            )
            .await?;
        rows.as_array()
            .context("Invalid history")?
            .iter()
            .map(|row| {
                let revision = row["revision"].as_i64().context("Invalid revision")?;
                let open = (|| -> Result<ResourceContent> {
                    ensure!(revision > 0, "Invalid revision");
                    let context = format!("termterm-resource-v2:{team}:{id}:{revision}");
                    let key = keys::unwrap(
                        identity,
                        row["envelope"]
                            .as_str()
                            .context("Version key unavailable")?,
                        &context,
                    )?;
                    let bytes = crypto::unseal(
                        &key,
                        &hex::decode(row["snapshot"].as_str().context("Missing version")?)?,
                        format!("{context}:snapshot").as_bytes(),
                    )?;
                    let content: ResourceContent = serde_json::from_slice(&bytes)?;
                    content.validate()?;
                    Ok(content)
                })();
                let (content, error) = match open {
                    Ok(c) => (Some(c), None),
                    Err(e) => (None, Some(e.to_string())),
                };
                Ok(Version {
                    revision,
                    created_at: row["createdAt"].clone(),
                    actor_id: row["actorId"].clone(),
                    content,
                    error,
                })
            })
            .collect()
    }
    pub async fn restore_version(
        &self,
        id: Uuid,
        source: i64,
        current: i64,
        sequence: i64,
        client_at: &str,
        identity: &Identity,
    ) -> Result<Value> {
        let (team, _, _) = self.actor()?;
        let row = self
            .request(
                reqwest::Method::GET,
                &format!("v1/resources/{id}"),
                None,
                true,
            )
            .await?;
        let present = crate::resources::open(team, &row, identity)?;
        ensure!(
            present.revision == current,
            "Kayıt değişti; geçmişi yenileyip yeniden karşılaştırın"
        );
        if present.content.kind=="host" {
            let plan=self.prepare_host_restore(id,source,current,sequence,client_at,identity).await?;
            return self.finish_chain(&plan).await;
        }
        let versions = self.history(id, identity).await?;
        let old = versions
            .into_iter()
            .find(|v| v.revision == source)
            .context("Version unavailable")?
            .content
            .context("Historical content cannot be decrypted")?;
        ensure!(
            !old.deleted
                && old.kind == present.content.kind
                && old.parent_id == present.content.parent_id,
            "Historical references require a move/recovery preview"
        );
        let vault = serde_json::from_value(row["vaultId"].clone())?;
        let change = self
            .prepare_resource(vault, id, current, sequence, client_at, &old)
            .await?;
        self.request(
            reqwest::Method::POST,
            "v1/resources/restore",
            Some(&json!({"sourceRevision":source,"change":change})),
            true,
        )
        .await
    }
    pub async fn history_pins(&self, id: Uuid, release: bool) -> Result<Value> {
        self.request(
            if release {
                reqwest::Method::POST
            } else {
                reqwest::Method::GET
            },
            &format!("v1/resources/{id}/history/pins"),
            None,
            true,
        )
        .await
    }
}
