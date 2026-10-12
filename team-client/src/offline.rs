//! Bounded outbox delivery. The desktop schedules at most one worker per
//! identity. No retry loop, plaintext export or device impersonation is hidden
//! here; receipts are recovered before accessing current resource contents.
use crate::{
    local::Store,
    resources::{ChangeReceipt, PreparedResourceChange},
    transport::TeamClient,
};
use anyhow::{ensure, Context, Result};
use reqwest::Method;
use serde_json::Value;
use uuid::Uuid;

pub enum Delivery {
    Empty,
    Applied(Uuid),
    Conflict(Uuid),
    Quarantined(Uuid),
}
impl TeamClient {
    pub async fn reconcile_journal(&self, store: &mut Store) -> Result<usize> {
        let mut count = 0;
        for id in store.conflict_operations()? {
            let row = self
                .request(
                    Method::GET,
                    &format!("v1/operations/{id}/receipt"),
                    None,
                    true,
                )
                .await?;
            ensure!(
                row["operationId"].as_str() == Some(&id.to_string()),
                "Decision receipt mismatch"
            );
            if matches!(
                row["decisionStatus"].as_str(),
                Some("resolved" | "rejected")
            ) {
                store.settle_decision(id)?;
                count += 1;
            }
        }
        store.prune_journal_receipts()?;
        Ok(count)
    }
    pub async fn offline_edit_capability(
        &self,
        vault: Uuid,
        id: Uuid,
        parent: Option<Uuid>,
    ) -> Result<Uuid> {
        let preparation = self
            .request(
                Method::POST,
                "v1/resources/recipients",
                Some(&serde_json::json!({"vaultId":vault,"resourceId":id,"parentId":parent})),
                true,
            )
            .await?;
        Ok(serde_json::from_value(
            preparation["writeCapability"].clone(),
        )?)
    }
    pub async fn operation_receipt(&self, id: Uuid) -> Result<Option<ChangeReceipt>> {
        let row = self
            .request(
                Method::GET,
                &format!("v1/operations/{id}/receipt"),
                None,
                true,
            )
            .await?;
        if row["found"] == false {
            return Ok(None);
        }
        ensure!(row["found"] == true, "Invalid operation receipt");
        let receipt: ChangeReceipt = serde_json::from_value(row)?;
        ensure!(receipt.operation_id == id, "Receipt operation mismatch");
        Ok(Some(receipt))
    }
    pub async fn deliver_next(&mut self, store: &mut Store) -> Result<Delivery> {
        let Some((mut draft, state)) = store.next_delivery()? else {
            return Ok(Delivery::Empty);
        };
        ensure!(
            self.device == Some(draft.device),
            "Sign in with the journal device before delivery"
        );
        self.renew_if_needed().await?;
        if let Some(scope) = draft.creation_scope {
            let result=self.request(Method::POST,"v1/offline/create-claim",Some(&serde_json::json!({"scope":scope,"resourceId":draft.resource_id,"vaultId":draft.vault_id,"parentId":draft.content.parent_id})),true).await?;
            let cap: Uuid = serde_json::from_value(result["writeCapability"].clone())?;
            store.bind_creation_receipt(draft.id, cap)?;
            draft.creation_scope = None;
            draft.write_capability = Some(cap);
        }
        if let Some(receipt) = self.operation_receipt(draft.id).await? {
            let result = if receipt.status == "applied" {
                Delivery::Applied(draft.id)
            } else {
                Delivery::Conflict(draft.id)
            };
            store.acknowledge_delivery(draft.id, serde_json::to_value(receipt)?)?;
            return Ok(result);
        }
        if state == "quarantined" {
            let body = if let Some(body) = draft.decision_delivery {
                body
            } else {
                let capability = draft
                    .write_capability
                    .context("Offline edit receipt missing")?;
                let preparation=self.request(Method::POST,"v1/offline/decisions/prepare",Some(&serde_json::json!({"capability":capability,"resourceId":draft.resource_id})),true).await?;
                let timestamp = chrono::DateTime::from_timestamp_millis(draft.client_at_ms)
                    .context("Invalid operation time")?
                    .to_rfc3339();
                let mut prepared = crate::resources::encrypt_change(
                    self.actor()?.0,
                    draft.vault_id,
                    draft.resource_id,
                    draft.expected_revision,
                    (draft.sequence, &timestamp),
                    &draft.content,
                    &preparation,
                )?;
                prepared.0["operationId"] = Value::String(draft.id.to_string());
                store.prepare_decision_delivery(draft.id, prepared.0.clone())?;
                prepared.0
            };
            let receipt: ChangeReceipt = serde_json::from_value(
                self.request(Method::POST, "v1/offline/decisions", Some(&body), true)
                    .await?,
            )?;
            ensure!(
                receipt.status == "conflict",
                "Decision delivery cannot apply changes"
            );
            store.acknowledge_delivery(draft.id, serde_json::to_value(receipt)?)?;
            return Ok(Delivery::Conflict(draft.id));
        }
        let body = if let Some(body) = draft.delivery {
            // A previous request may have failed before arrival. Verify today's
            // edit authority before replaying; revoked drafts take the narrow
            // decision path with freshly encrypted current-recipient envelopes.
            match self
                .offline_edit_capability(draft.vault_id, draft.resource_id, draft.content.parent_id)
                .await
            {
                Ok(_) => {}
                Err(e) if e.to_string() == "permission_denied" => {
                    store.revoke_cached_resource(draft.resource_id)?;
                    return Ok(Delivery::Quarantined(draft.id));
                }
                Err(e) => return Err(e),
            }
            // A lost response can only retry the exact operation. The server
            // checks its receipt before CAS and refuses stale ACL publication.
            body
        } else {
            let timestamp = chrono::DateTime::from_timestamp_millis(draft.client_at_ms)
                .context("Invalid operation time")?
                .to_rfc3339();
            let prepared = self
                .prepare_resource(
                    draft.vault_id,
                    draft.resource_id,
                    draft.expected_revision,
                    draft.sequence,
                    &timestamp,
                    &draft.content,
                )
                .await;
            let mut prepared = match prepared {
                Ok(v) => v,
                Err(e) if e.to_string() == "permission_denied" => {
                    store.revoke_cached_resource(draft.resource_id)?;
                    return Ok(Delivery::Quarantined(draft.id));
                }
                Err(e) => return Err(e),
            };
            prepared.0["operationId"] = Value::String(draft.id.to_string());
            store.prepare_delivery(draft.id, prepared.0.clone())?;
            prepared.0
        };
        let result = self.save_resource(&PreparedResourceChange(body)).await;
        let receipt = match result {
            Ok(r) => r,
            Err(e) if e.to_string() == "permission_denied" => {
                store.revoke_cached_resource(draft.resource_id)?;
                return Ok(Delivery::Quarantined(draft.id));
            }
            Err(e) => return Err(e), // unknown outcomes remain durable, never 'saved'
        };
        let result = if receipt.status == "applied" {
            Delivery::Applied(draft.id)
        } else {
            Delivery::Conflict(draft.id)
        };
        store.acknowledge_delivery(draft.id, serde_json::to_value(receipt)?)?;
        Ok(result)
    }
}
