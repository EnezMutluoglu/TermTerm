//! Owner-approved ACL changes; staged rewrapping does not publish permissions.
use crate::{
    keys::{self, Identity},
    transport::TeamClient,
};
use anyhow::{ensure, Context, Result};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessPermission {
    Read,
    Connect,
    Edit,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessEffect {
    Allow,
    Deny,
    Clear,
}
#[derive(Serialize, Deserialize)]
pub struct AccessRule {
    pub permission: AccessPermission,
    pub effect: AccessEffect,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessPreview {
    pub id: Uuid,
    pub affected_resources: u64,
    pub required_keys: u64,
    pub changes: Vec<AccessRule>,
    pub expires_in_seconds: u64,
}
pub struct AccessProgress {
    pub staged: u64,
    pub next_after: Option<i64>,
}
#[derive(Deserialize, Serialize)]
pub struct AccessImpact {
    pub id: Uuid,
    pub kind: String,
    pub before: Vec<AccessPermission>,
    pub after: Vec<AccessPermission>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessImpactPage {
    pub records: Vec<AccessImpact>,
    pub next_after: Option<Uuid>,
}

impl TeamClient {
    pub async fn delegations(&self) -> Result<Value> {
        self.request(Method::GET, "v1/access/delegations", None, true)
            .await
    }
    pub async fn set_delegation(
        &self,
        account: Uuid,
        resource: Uuid,
        permissions: &[AccessPermission],
        expected_revision: i64,
    ) -> Result<Value> {
        self.request(Method::POST, "v1/access/delegations", Some(&json!({
            "accountId":account,"resourceId":resource,"permissions":permissions,"expectedRevision":expected_revision
        })), true).await
    }
    pub async fn access_impact(&self, plan: Uuid, after: Option<Uuid>) -> Result<AccessImpactPage> {
        let query = after.map(|id| format!("?after={id}")).unwrap_or_default();
        let value = self
            .request(
                Method::GET,
                &format!("v1/access/plans/{plan}/impact{query}"),
                None,
                true,
            )
            .await?;
        Ok(serde_json::from_value(value)?)
    }
    pub async fn prepare_access(
        &self,
        account: Uuid,
        resource: Uuid,
        changes: &[AccessRule],
    ) -> Result<AccessPreview> {
        let result = self
            .request(
                Method::POST,
                "v1/access/plans",
                Some(&json!({"accountId":account,"resourceId":resource,"changes":changes})),
                true,
            )
            .await?;
        Ok(serde_json::from_value(result)?)
    }
    /// Bounded, resumable batch. The caller remains free to cancel before final approval.
    pub async fn stage_access_keys(
        &self,
        plan: Uuid,
        after: i64,
        identity: &Identity,
    ) -> Result<AccessProgress> {
        ensure!(after >= 0, "Invalid page cursor");
        let (team, _, _) = self.actor()?;
        let result = self
            .request(
                Method::GET,
                &format!("v1/access/plans/{plan}/keys?after={after}"),
                None,
                true,
            )
            .await?;
        let key = result["publicKey"]
            .as_str()
            .context("Recipient key unavailable")?;
        let rows = result["keys"].as_array().context("Key page missing")?;
        ensure!(rows.len() <= 20, "Key page exceeds limit");
        let items = rows
            .iter()
            .map(|row| -> Result<Value> {
                let id: Uuid = serde_json::from_value(row["id"].clone())?;
                let context = match row["kind"].as_str() {
                    Some("vault") => format!("termterm-vault-v2:{team}:{id}"),
                    Some("resource" | "dependency") => {
                        let revision = row["revision"].as_i64().context("Missing revision")?;
                        ensure!(revision > 0, "Invalid revision");
                        if row["sourceDependency"]==true {
                            let root:Uuid=serde_json::from_value(row["rootId"].clone())?;
                            format!("termterm-chain-v2:{team}:{root}:{id}:{revision}")
                        } else {format!("termterm-resource-v2:{team}:{id}:{revision}")}
                    }
                    _ => anyhow::bail!("Unknown key purpose"),
                };
                let source = row["envelope"].as_str().context("Source key unavailable")?;
                let secret = keys::unwrap(identity, source, &context)?;
                let destination=if row["kind"]=="dependency" {
                    let root:Uuid=serde_json::from_value(row["rootId"].clone())?;
                    let revision=row["revision"].as_i64().context("Missing revision")?;
                    format!("termterm-chain-v2:{team}:{root}:{id}:{revision}")
                } else {context};
                Ok(json!({"ordinal":row["ordinal"],"envelope":keys::wrap(&secret,key,&destination)?}))
            })
            .collect::<Result<Vec<_>>>()?;
        let count = items.len() as u64;
        if count > 0 {
            self.request(
                Method::POST,
                &format!("v1/access/plans/{plan}/keys"),
                Some(&json!(items)),
                true,
            )
            .await?;
        }
        Ok(AccessProgress {
            staged: count,
            next_after: result["nextAfter"].as_i64(),
        })
    }
    /// Kept separate from preparation/staging: no permission changes before this call.
    pub async fn finish_access(&self, plan: Uuid, cancel: bool) -> Result<Value> {
        self.request(
            Method::POST,
            &format!("v1/access/plans/{plan}/finish"),
            Some(&json!({"cancel":cancel})),
            true,
        )
        .await
    }
}
