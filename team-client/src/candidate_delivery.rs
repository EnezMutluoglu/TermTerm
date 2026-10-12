//! No login/session/host access. Only old, already-authorised drafts can be
//! handed to the team's current decision makers after account revocation.
use crate::{
    local::Store,
    resources::{self, ChangeReceipt},
    transport::{CandidateRoute, TeamClient},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Proof {
    proof: String,
    expires_at_ms: i64,
}

pub(crate) async fn provision(client: &TeamClient, store: &mut Store, now: i64) -> Result<()> {
    let old = store.read("delivery-proof")?;
    if old.revision > 0 {
        let proof: Proof = serde_json::from_slice(&old.bytes)?;
        if proof.expires_at_ms > now.saturating_add(86_400_000) {
            return Ok(());
        }
    }
    let mut value = client
        .request(
            reqwest::Method::POST,
            "v1/offline/delivery-token",
            None,
            true,
        )
        .await?;
    let proof = Proof {
        proof: serde_json::from_value(value["proof"].take())?,
        expires_at_ms: value["expiresAtMs"]
            .as_i64()
            .context("Invalid delivery expiry")?,
    };
    ensure!(
        proof.proof.len() == 64 && proof.proof.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid delivery proof"
    );
    let bytes = Zeroizing::new(serde_json::to_vec(&proof)?);
    ensure!(
        store.replace("delivery-proof", old.revision, &bytes)?,
        "Delivery cache changed"
    );
    Ok(())
}

pub(crate) async fn submit_one(
    client: &TeamClient,
    team: Uuid,
    store: &mut Store,
    now: i64,
) -> Result<Value> {
    let bytes = store.read("delivery-proof")?;
    ensure!(bytes.revision > 0, "candidate_delivery_unavailable");
    let proof: Proof = serde_json::from_slice(&bytes.bytes)?;
    ensure!(proof.expires_at_ms > now, "candidate_delivery_expired");
    // Close accessible state first. Quarantined drafts are not available to
    // normal browsing or connections; the private worker alone opens them.
    store.revoke_all_cached_resources()?;
    for id in store.conflict_operations()? {
        let row = client
            .candidate_request(&proof.proof, CandidateRoute::Receipt(id), None)
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
        }
    }
    let Some((mut draft, state)) = store.next_delivery()? else {
        return Ok(summary(store, now, 0)?);
    };
    ensure!(
        state == "quarantined",
        "Candidate delivery requires quarantine"
    );
    if let Some(scope) = draft.creation_scope {
        let result=client.candidate_request(&proof.proof,CandidateRoute::Claim,Some(&json!({"scope":scope,"resourceId":draft.resource_id,"vaultId":draft.vault_id,"parentId":draft.content.parent_id}))).await?;
        let cap = serde_json::from_value(result["writeCapability"].clone())?;
        store.bind_creation_receipt(draft.id, cap)?;
        draft.creation_scope = None;
        draft.write_capability = Some(cap);
    }
    let previous = client
        .candidate_request(&proof.proof, CandidateRoute::Receipt(draft.id), None)
        .await?;
    if previous["found"] == true {
        let receipt: ChangeReceipt = serde_json::from_value(previous)?;
        ensure!(receipt.operation_id == draft.id, "Receipt mismatch");
        store.acknowledge_delivery(draft.id, serde_json::to_value(receipt)?)?;
        return summary(store, now, 1);
    }
    let body = if let Some(body) = draft.decision_delivery {
        body
    } else {
        let cap = draft
            .write_capability
            .context("Offline edit receipt missing")?;
        let preparation = client
            .candidate_request(
                &proof.proof,
                CandidateRoute::Prepare,
                Some(&json!({"capability":cap,"resourceId":draft.resource_id})),
            )
            .await?;
        let timestamp = chrono::DateTime::from_timestamp_millis(draft.client_at_ms)
            .context("Invalid operation time")?
            .to_rfc3339();
        let mut change = resources::encrypt_change(
            team,
            draft.vault_id,
            draft.resource_id,
            draft.expected_revision,
            (draft.sequence, &timestamp),
            &draft.content,
            &preparation,
        )?;
        change.0["operationId"] = json!(draft.id);
        store.prepare_decision_delivery(draft.id, change.0.clone())?;
        change.0
    };
    let receipt: ChangeReceipt = serde_json::from_value(
        client
            .candidate_request(&proof.proof, CandidateRoute::Submit, Some(&body))
            .await?,
    )?;
    ensure!(
        receipt.operation_id == draft.id && receipt.status == "conflict",
        "Invalid candidate receipt"
    );
    store.acknowledge_delivery(draft.id, serde_json::to_value(receipt)?)?;
    summary(store, now, 1)
}
fn summary(store: &mut Store, now: i64, delivered: u64) -> Result<Value> {
    let status = store.journal_status(now)?;
    Ok(
        json!({"delivered":delivered,"remaining":status.pending+status.quarantined,"awaitingDecision":status.conflicts}),
    )
}
