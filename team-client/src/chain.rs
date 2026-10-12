//! Root-scoped connection plans. Decrypted dependencies never become ordinary
//! resource records or frontend IPC responses.
use crate::{
    connection::Target,
    keys::{self, Identity},
    transport::{OpenCredential, TeamClient},
};
use anyhow::{ensure, Context, Result};
use reqwest::Method;
use serde_json::{json, Value};
use uuid::Uuid;

pub struct ConnectionPlan {
    pub target_id: Uuid,
    pub steps: Vec<Target>,
}
pub struct PreparedChain {
    pub id: Uuid,
    pub preview: Value,
    source_revision: Option<i64>,
    route_unchanged: bool,
    envelopes: Value,
    change: crate::resources::PreparedResourceChange,
}
fn context(team: Uuid, root: Uuid, hop: Uuid, revision: i64) -> String {
    format!("termterm-chain-v2:{team}:{root}:{hop}:{revision}")
}
fn open_dependency_envelope(
    team: Uuid,
    root: Uuid,
    id: Uuid,
    row: &mut Value,
    identity: &Identity,
) -> Result<()> {
    if id != root && row["directEnvelope"] != true {
        let revision = row["revision"]
            .as_i64()
            .context("Dependency revision missing")?;
        let key = keys::unwrap(
            identity,
            row["dependencyEnvelope"]
                .as_str()
                .context("Dependency key missing")?,
            &context(team, root, id, revision),
        )?;
        row["envelope"] = json!(keys::wrap(
            &key,
            &identity.public_key(),
            &format!("termterm-resource-v2:{team}:{id}:{revision}")
        )?);
    }
    Ok(())
}
impl TeamClient {
    pub async fn connection_profiles(&self, root: Uuid, identity: &Identity) -> Result<Value> {
        let plan = self.authorize_plan(root).await?;
        let (team, _, _) = self.actor()?;
        let mut result = Vec::new();
        for value in plan["steps"]
            .as_array()
            .context("Connection steps missing")?
        {
            let mut row = value.clone();
            let id = serde_json::from_value(row["id"].clone())?;
            open_dependency_envelope(team, root, id, &mut row, identity)?;
            let host = crate::resources::open(team, &row, identity)?.content;
            result.push(json!({"id":id,"name":host.name,"usernamePolicy":host.username_policy,"username":host.username,"proxy":host.proxy}));
        }
        Ok(json!(result))
    }

    pub async fn prepare_chain(
        &self,
        root: Uuid,
        hops: &[Uuid],
        identity: &Identity,
        sequence: i64,
        client_at: &str,
    ) -> Result<PreparedChain> {
        ensure!(hops.len() <= 15 && !hops.contains(&root), "Invalid chain");
        let result = self
            .request(
                Method::POST,
                &format!("v1/connections/{root}/chain"),
                Some(&json!({"hops":hops})),
                true,
            )
            .await?;
        let (team, _, _) = self.actor()?;
        let id = serde_json::from_value(result["planId"].clone())?;
        let resources = result["resources"]
            .as_array()
            .context("Chain sources missing")?;
        let recipients = result["recipients"]
            .as_array()
            .context("Chain recipients missing")?;
        ensure!(
            resources.len() == hops.len() && recipients.len() <= 1000,
            "Invalid chain preview"
        );
        let mut envelopes = Vec::new();
        let mut labels = Vec::new();
        for hop in hops {
            let row = resources
                .iter()
                .find(|r| r["id"] == hop.to_string())
                .context("Chain source missing")?;
            let open = crate::resources::open(team, row, identity)?;
            let key = keys::unwrap(
                identity,
                row["envelope"].as_str().context("Source key missing")?,
                &format!("termterm-resource-v2:{team}:{hop}:{}", open.revision),
            )?;
            for recipient in recipients {
                envelopes.push(json!({"hopId":hop,"accountId":recipient["accountId"],"identityVersion":recipient["identityVersion"],
                    "envelope":keys::wrap(&key,recipient["publicKey"].as_str().context("Recipient key missing")?,&context(team,root,*hop,open.revision))?}));
            }
            labels.push(json!({"id":hop,"name":open.content.name,"revision":open.revision}));
        }
        let row = self
            .request(Method::GET, &format!("v1/resources/{root}"), None, true)
            .await?;
        let open = crate::resources::open(team, &row, identity)?;
        ensure!(
            row["revision"] == result["targetRevision"],
            "Chain source changed; preview again"
        );
        let change = self
            .prepare_resource(
                serde_json::from_value(row["vaultId"].clone())?,
                root,
                open.revision,
                sequence,
                client_at,
                &open.content,
            )
            .await?;
        Ok(PreparedChain {
            id,
            source_revision: None,
            route_unchanged: false,
            preview: json!({"planId":id,"targetId":root,"hops":labels,
            "recipientCount":recipients.len(),"expiresInSeconds":900}),
            envelopes: json!(envelopes),
            change,
        })
    }
    pub async fn prepare_host_restore(
        &self,
        id: Uuid,
        source: i64,
        current: i64,
        sequence: i64,
        client_at: &str,
        identity: &Identity,
    ) -> Result<PreparedChain> {
        let hops: Vec<Uuid> = serde_json::from_value(
            self.request(
                Method::GET,
                &format!("v1/connections/{id}/history/{source}"),
                None,
                true,
            )
            .await?,
        )?;
        let row = self
            .request(Method::GET, &format!("v1/resources/{id}"), None, true)
            .await?;
        let present = crate::resources::open(self.actor()?.0, &row, identity)?;
        ensure!(
            present.revision == current,
            "Resource changed; preview again"
        );
        let previous = self
            .history(id, identity)
            .await?
            .into_iter()
            .find(|v| v.revision == source)
            .context("History unavailable")?
            .content
            .context("Historical content cannot be decrypted")?;
        ensure!(
            previous.kind == "host"
                && !previous.deleted
                && previous.parent_id == present.content.parent_id,
            "Historical references require a move/recovery preview"
        );
        let current_hops: Vec<Uuid> = serde_json::from_value(
            self.request(
                Method::GET,
                &format!("v1/connections/{id}/history/{current}"),
                None,
                true,
            )
            .await?,
        )?;
        let change = self
            .prepare_resource(
                serde_json::from_value(row["vaultId"].clone())?,
                id,
                current,
                sequence,
                client_at,
                &previous,
            )
            .await?;
        let mut plan = if current_hops == hops {
            let plan_id = Uuid::new_v4();
            PreparedChain {
                id: plan_id,
                source_revision: Some(source),
                route_unchanged: true,
                envelopes: json!([]),
                change,
                preview: json!({"planId":plan_id,"hops":hops.iter().map(|id|json!({"id":id,"name":id})).collect::<Vec<_>>(),"routeUnchanged":true}),
            }
        } else {
            let mut plan = self
                .prepare_chain(id, &hops, identity, sequence, client_at)
                .await?;
            ensure!(
                plan.change.0["expectedRevision"] == current,
                "Resource changed; preview again"
            );
            plan.change = change;
            plan
        };
        plan.source_revision = Some(source);
        plan.preview["sourceRevision"] = json!(source);
        plan.preview["currentRevision"] = json!(current);
        plan.preview["proxy"] = json!(previous.proxy);
        Ok(plan)
    }
    pub async fn finish_chain(&self, plan: &PreparedChain) -> Result<Value> {
        if plan.route_unchanged {
            return self
                .request(
                    Method::POST,
                    "v1/resources/restore",
                    Some(&json!({"sourceRevision":plan.source_revision,"change":plan.change})),
                    true,
                )
                .await;
        }
        self.request(
            Method::POST,
            &format!("v1/connection-plans/{}/finish", plan.id),
            Some(&json!({"envelopes":plan.envelopes,"change":plan.change,"sourceRevision":plan.source_revision})),
            true,
        )
        .await
    }
    pub async fn authorize_plan(&self, root: Uuid) -> Result<Value> {
        self.request(
            Method::GET,
            &format!("v1/connections/{root}/plan"),
            None,
            true,
        )
        .await
    }
    pub async fn resolve_plan(&self, root: Uuid, identity: &Identity) -> Result<ConnectionPlan> {
        let plan = self.authorize_plan(root).await?;
        let rows = plan["steps"]
            .as_array()
            .context("Connection steps missing")?;
        ensure!(
            plan["targetId"] == root.to_string()
                && !rows.is_empty()
                && rows.len() <= 16
                && rows.last().unwrap()["id"] == root.to_string(),
            "Invalid connection plan"
        );
        let (team, _, _) = self.actor()?;
        let mut seen = std::collections::HashSet::new();
        let mut steps = Vec::new();
        for row in rows {
            let mut row = row.clone();
            let id: Uuid = serde_json::from_value(row["id"].clone())?;
            ensure!(seen.insert(id), "Connection cycle");
            open_dependency_envelope(team, root, id, &mut row, identity)?;
            steps.push(self.resolve_step(root, id, row, identity).await?);
        }
        Ok(ConnectionPlan {
            target_id: root,
            steps,
        })
    }
    pub(crate) async fn open_plan_credential(
        &self,
        root: Uuid,
        hop: Uuid,
        id: Uuid,
        identity: &Identity,
    ) -> Result<OpenCredential> {
        let value = self
            .request(
                Method::POST,
                &format!("v1/connections/{root}/credential"),
                Some(&json!({"hopId":hop,"credentialId":id})),
                true,
            )
            .await?;
        self.decode_connection_credential(id, &value, identity)
    }
}
