//! Durable, identity-bound outbox. A local edit and its immutable draft commit
//! together. Delivery preparation never replaces an earlier edit. Quarantine is
//! deliberately not an accessible resource: only the delivery worker reads it.
use super::*;
use crate::resources::ResourceContent;
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const EDIT_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
const MAX_PAYLOAD: usize = 2 * 1024 * 1024;

pub(super) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE journal_state(id INTEGER PRIMARY KEY CHECK(id=1), ciphertext BLOB NOT NULL);
         CREATE TABLE cached_resources(
           id TEXT PRIMARY KEY, vault_id TEXT NOT NULL, generation INTEGER NOT NULL,
           ciphertext BLOB NOT NULL);
         CREATE INDEX cached_resources_vault ON cached_resources(vault_id,id);
         CREATE TABLE journal_operations(
           id TEXT PRIMARY KEY, sequence INTEGER NOT NULL UNIQUE,
           resource_id TEXT NOT NULL, state TEXT NOT NULL
             CHECK(state IN ('pending','sending','applied','conflict','quarantined')),
           client_at_ms INTEGER NOT NULL, ciphertext BLOB NOT NULL);
         CREATE INDEX journal_pending ON journal_operations(sequence)
           WHERE state IN ('pending','sending','quarantined');
         CREATE INDEX journal_resource ON journal_operations(resource_id,sequence);
         PRAGMA user_version=2;",
    )?;
    Ok(())
}
pub(super) fn migrate_settled(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch("ALTER TABLE journal_operations RENAME TO journal_operations_v2;
      CREATE TABLE journal_operations(id TEXT PRIMARY KEY,sequence INTEGER NOT NULL UNIQUE,resource_id TEXT NOT NULL,
      state TEXT NOT NULL CHECK(state IN ('pending','sending','applied','conflict','quarantined','resolved')),client_at_ms INTEGER NOT NULL,ciphertext BLOB NOT NULL);
      INSERT INTO journal_operations SELECT * FROM journal_operations_v2;
      DROP TABLE journal_operations_v2;
      CREATE INDEX journal_pending ON journal_operations(sequence) WHERE state IN ('pending','sending','quarantined');
      CREATE INDEX journal_resource ON journal_operations(resource_id,sequence);
      PRAGMA user_version=3;")?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Clock {
    device: Uuid,
    sequence: i64,
    last_wall_ms: i64,
    remaining_ms: i64,
    verified: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Permissions {
    pub read: bool,
    pub connect: bool,
    pub edit: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CachedResource {
    #[serde(default)]
    pub navigation_only: bool,
    pub id: Uuid,
    pub vault_id: Uuid,
    pub server_revision: i64,
    pub generation: i64,
    pub permissions: Permissions,
    pub write_capability: Option<Uuid>,
    pub verified_at_ms: i64,
    pub content: ResourceContent,
}
#[derive(Serialize, Deserialize)]
pub struct Draft {
    #[serde(default)]
    pub(crate) creation_scope: Option<Uuid>,
    pub id: Uuid,
    pub device: Uuid,
    pub sequence: i64,
    pub resource_id: Uuid,
    pub vault_id: Uuid,
    pub expected_revision: i64,
    pub client_at_ms: i64,
    pub content: ResourceContent,
    // Exact first submitted ciphertext is retained across a lost response.
    // The worker must query/retry its receipt before preparing new recipients.
    pub(crate) delivery: Option<serde_json::Value>,
    pub(crate) decision_delivery: Option<serde_json::Value>,
    pub(crate) write_capability: Option<Uuid>,
    pub(crate) receipt: Option<serde_json::Value>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalStatus {
    pub pending: i64,
    pub conflicts: i64,
    pub quarantined: i64,
    pub oldest_pending_ms: Option<i64>,
    pub edit_remaining_ms: i64,
}

fn seal<T: Serialize>(key: &[u8; 32], aad: &str, value: &T) -> Result<Vec<u8>> {
    let bytes = Zeroizing::new(serde_json::to_vec(value)?);
    ensure!(bytes.len() <= MAX_PAYLOAD, "Offline entry too large");
    crypto::seal(key, &bytes, aad.as_bytes())
}
fn open<T: for<'a> Deserialize<'a>>(key: &[u8; 32], aad: &str, bytes: &[u8]) -> Result<T> {
    Ok(serde_json::from_slice(&crypto::unseal(
        key,
        bytes,
        aad.as_bytes(),
    )?)?)
}
fn clock(tx: &Transaction<'_>, key: &[u8; 32], scope: &str, now: i64) -> Result<Clock> {
    ensure!(now >= 0, "Invalid local clock");
    let row: Option<Vec<u8>> = tx
        .query_row("SELECT ciphertext FROM journal_state WHERE id=1", [], |r| {
            r.get(0)
        })
        .optional()?;
    let mut c: Clock = match row {
        Some(bytes) => open(key, &format!("{scope}:journal:clock"), &bytes)?,
        None => Clock {
            device: Uuid::new_v4(),
            sequence: 0,
            last_wall_ms: now,
            remaining_ms: 0,
            verified: false,
        },
    };
    // A backward clock, including across restart, closes the write lease. Only
    // a successful online permission refresh can grant a fresh lease.
    if now < c.last_wall_ms {
        c.remaining_ms = 0;
    } else {
        c.remaining_ms = c.remaining_ms.saturating_sub(now - c.last_wall_ms).max(0);
    }
    c.last_wall_ms = c.last_wall_ms.max(now);
    Ok(c)
}
fn write_clock(tx: &Transaction<'_>, key: &[u8; 32], scope: &str, c: &Clock) -> Result<()> {
    tx.execute("INSERT INTO journal_state VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET ciphertext=excluded.ciphertext",
        [seal(key, &format!("{scope}:journal:clock"), c)?])?;
    Ok(())
}
fn read_record(
    tx: &Transaction<'_>,
    key: &[u8; 32],
    scope: &str,
    id: Uuid,
) -> Result<Option<CachedResource>> {
    let row: Option<(String, i64, Vec<u8>)> = tx
        .query_row(
            "SELECT vault_id,generation,ciphertext FROM cached_resources WHERE id=?1",
            [id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    row.map(|(vault, generation, bytes)| {
        let result: CachedResource = open(
            key,
            &format!("{scope}:journal:resource:{id}:{vault}:{generation}"),
            &bytes,
        )?;
        ensure!(
            result.id == id
                && result.vault_id.to_string() == vault
                && result.generation == generation,
            "Cache metadata mismatch"
        );
        result.content.validate()?;
        Ok(result)
    })
    .transpose()
}
fn write_record(
    tx: &Transaction<'_>,
    key: &[u8; 32],
    scope: &str,
    r: &CachedResource,
) -> Result<()> {
    let bytes = seal(
        key,
        &format!(
            "{scope}:journal:resource:{}:{}:{}",
            r.id, r.vault_id, r.generation
        ),
        r,
    )?;
    tx.execute("INSERT INTO cached_resources VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET vault_id=excluded.vault_id,generation=excluded.generation,ciphertext=excluded.ciphertext",
        params![r.id.to_string(),r.vault_id.to_string(),r.generation,bytes])?;
    Ok(())
}
fn write_draft(
    tx: &Transaction<'_>,
    key: &[u8; 32],
    scope: &str,
    d: &Draft,
    state: &str,
) -> Result<()> {
    let bytes = seal(
        key,
        &format!(
            "{scope}:journal:operation:{}:{}:{}:{state}:{}",
            d.id, d.sequence, d.resource_id, d.client_at_ms
        ),
        d,
    )?;
    tx.execute("INSERT INTO journal_operations VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET state=excluded.state,ciphertext=excluded.ciphertext",
        params![d.id.to_string(),d.sequence,d.resource_id.to_string(),state,d.client_at_ms,bytes])?;
    Ok(())
}
fn read_draft(
    tx: &Transaction<'_>,
    key: &[u8; 32],
    scope: &str,
    id: Uuid,
) -> Result<(Draft, String)> {
    let (sequence,resource,state,time,bytes):(i64,String,String,i64,Vec<u8>)=tx.query_row(
        "SELECT sequence,resource_id,state,client_at_ms,ciphertext FROM journal_operations WHERE id=?1",[id.to_string()],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
    let d: Draft = open(
        key,
        &format!("{scope}:journal:operation:{id}:{sequence}:{resource}:{state}:{time}"),
        &bytes,
    )?;
    ensure!(
        d.id == id
            && d.sequence == sequence
            && d.resource_id.to_string() == resource
            && d.client_at_ms == time,
        "Journal metadata mismatch"
    );
    Ok((d, state))
}
impl Store {
    /// Call only after a successful authenticated authorization check. Partial
    /// page reads must not grant a new lease for unrelated cached resources.
    pub fn refresh_cached_resource(&mut self, mut record: CachedResource) -> Result<()> {
        ensure!(
            record.server_revision > 0 && record.generation == 0,
            "Invalid server resource"
        );
        record.content.validate()?;
        ensure!(
            record.permissions.read || record.permissions.connect || record.navigation_only,
            "No cache access"
        );
        ensure!(
            !record.navigation_only
                || (record.content.kind == "folder"
                    && !record.permissions.edit
                    && !record.permissions.connect
                    && !record.permissions.read
                    && record.write_capability.is_none()),
            "Navigation cache cannot grant resource permissions"
        );
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_operations WHERE resource_id=?1 AND state NOT IN ('applied','resolved'))",[record.id.to_string()],|r|r.get(0))?;
        let previous = read_record(&tx, &self.key, &self.context, record.id)?;
        if !record.permissions.connect
            || previous
                .as_ref()
                .is_none_or(|p| !super::connections::same_endpoint(&p.content, &record.content))
        {
            super::connections::invalidate_for(&tx, &self.key, &self.context, record.id)?;
        }
        if pending {
            let mut current = read_record(&tx, &self.key, &self.context, record.id)?
                .context("Pending resource is quarantined")?;
            ensure!(current.vault_id == record.vault_id, "Vault changed");
            current.permissions = record.permissions;
            current.navigation_only = record.navigation_only;
            current.write_capability = record.write_capability;
            current.verified_at_ms = record.verified_at_ms;
            // Never overwrite local work during refresh, even if remote won.
            write_record(&tx, &self.key, &self.context, &current)?;
        } else {
            if let Some(previous) = read_record(&tx, &self.key, &self.context, record.id)? {
                ensure!(previous.vault_id == record.vault_id, "Vault changed");
                // Re-reading the same version must not invalidate a preview or
                // a pending startup command. Actual content/permission changes
                // still advance the local CAS generation.
                let unchanged = previous.server_revision == record.server_revision
                    && previous.content == record.content
                    && previous.navigation_only == record.navigation_only
                    && previous.permissions.read == record.permissions.read
                    && previous.permissions.edit == record.permissions.edit
                    && previous.permissions.connect == record.permissions.connect;
                record.generation = if unchanged { previous.generation } else {
                    previous.generation.checked_add(1).context("Local generation exhausted")?
                };
            }
            write_record(&tx, &self.key, &self.context, &record)?;
        }
        tx.commit()?;
        Ok(())
    }
    /// The caller has completed a full permission refresh for this identity.
    pub fn authorize_offline_edits(&mut self, now_ms: i64) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut c = clock(&tx, &self.key, &self.context, now_ms)?;
        c.verified = true;
        c.remaining_ms = EDIT_WINDOW_MS;
        c.last_wall_ms = now_ms;
        write_clock(&tx, &self.key, &self.context, &c)?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn edit_authority_needs_refresh(&mut self, id: Uuid, now: i64) -> Result<bool> {
        let status = self.journal_status(now)?;
        let record = self.cached_resource(id)?.context("Resource not cached")?;
        Ok(status.edit_remaining_ms <= 0 || now < record.verified_at_ms
            || now.saturating_sub(record.verified_at_ms) >= EDIT_WINDOW_MS)
    }
    /// Only after an authenticated server permission check. Preserve the local
    /// draft/base revision so a concurrent server edit still reaches conflict CAS.
    pub(crate) fn renew_edit_authority(&mut self, id: Uuid, capability: Uuid, now: i64) -> Result<()> {
        let tx = self.db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut record = read_record(&tx, &self.key, &self.context, id)?.context("Resource not cached")?;
        ensure!(record.permissions.edit && !record.content.deleted && !record.navigation_only, "permission_denied");
        record.write_capability = Some(capability);
        record.verified_at_ms = now;
        let mut c = clock(&tx, &self.key, &self.context, now)?;
        c.verified = true;
        c.remaining_ms = EDIT_WINDOW_MS;
        c.last_wall_ms = now;
        write_record(&tx, &self.key, &self.context, &record)?;
        write_clock(&tx, &self.key, &self.context, &c)?;
        tx.commit()?;
        Ok(())
    }
    pub fn journal_status(&mut self, now_ms: i64) -> Result<JournalStatus> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let c = clock(&tx, &self.key, &self.context, now_ms)?;
        write_clock(&tx, &self.key, &self.context, &c)?;
        let result=tx.query_row("SELECT coalesce(sum(state IN ('pending','sending')),0),coalesce(sum(state='conflict'),0),coalesce(sum(state='quarantined'),0),min(CASE WHEN state NOT IN ('applied','resolved') THEN client_at_ms END) FROM journal_operations",[],|r|Ok(JournalStatus{pending:r.get(0)?,conflicts:r.get(1)?,quarantined:r.get(2)?,oldest_pending_ms:r.get(3)?,edit_remaining_ms:c.remaining_ms}))?;
        tx.commit()?;
        Ok(result)
    }
    pub fn cached_resource(&mut self, id: Uuid) -> Result<Option<CachedResource>> {
        let tx = self.db.transaction()?;
        read_record(&tx, &self.key, &self.context, id)
    }
    pub fn cached_page(&mut self, vault: Uuid, after: Option<Uuid>) -> Result<Vec<CachedResource>> {
        let tx = self.db.transaction()?;
        let ids: Vec<String> = {
            let mut q=tx.prepare("SELECT id FROM cached_resources WHERE vault_id=?1 AND (?2 IS NULL OR id>?2) ORDER BY id LIMIT 100")?;
            let rows = q
                .query_map(
                    params![vault.to_string(), after.map(|a| a.to_string())],
                    |r| r.get(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        ids.into_iter()
            .map(|id| {
                read_record(&tx, &self.key, &self.context, Uuid::parse_str(&id)?)
                    .and_then(|r| r.context("Cache changed"))
            })
            .collect()
    }
    pub fn reserve_sequence(&mut self, now_ms: i64) -> Result<i64> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut c = clock(&tx, &self.key, &self.context, now_ms)?;
        c.sequence = c
            .sequence
            .checked_add(1)
            .context("Device sequence exhausted")?;
        write_clock(&tx, &self.key, &self.context, &c)?;
        tx.commit()?;
        Ok(c.sequence)
    }
    pub fn journal_device(&mut self, now_ms: i64) -> Result<Uuid> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let c = clock(&tx, &self.key, &self.context, now_ms)?;
        write_clock(&tx, &self.key, &self.context, &c)?;
        tx.commit()?;
        Ok(c.device)
    }
    /// Compatibility for callers without a generation: never append a second
    /// ambiguous edit to pending work. They must refresh and send the CAS value.
    pub fn implicit_generation(&mut self, id: Uuid) -> Result<i64> {
        let tx = self.db.transaction()?;
        let pending: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_operations WHERE resource_id=?1 AND state NOT IN ('applied','resolved'))", [id.to_string()], |r| r.get(0))?;
        ensure!(!pending, "local_generation_required");
        Ok(read_record(&tx, &self.key, &self.context, id)?
            .context("Resource not cached")?
            .generation)
    }
    /// The caller obtained an online, resource-specific create capability. Both
    /// the draft and its visible local record commit together before any send.
    pub fn queue_create(
        &mut self,
        id: Uuid,
        vault_id: Uuid,
        content: ResourceContent,
        capability: Uuid,
        now_ms: i64,
    ) -> Result<Uuid> {
        self.queue_creation(id, vault_id, content, capability, None, now_ms)
    }
    pub(crate) fn cache_creation_scope(
        &mut self,
        vault: Uuid,
        parent: Option<Uuid>,
        capability: Uuid,
        now: i64,
    ) -> Result<()> {
        let name = format!(
            "create-scope:{vault}:{}",
            parent.map(|v| v.to_string()).unwrap_or_default()
        );
        let old = self.read(&name)?;
        ensure!(
            self.replace(
                &name,
                old.revision,
                &serde_json::to_vec(&(capability, now))?
            )?,
            "Creation scope cache changed"
        );
        Ok(())
    }
    pub(crate) fn forget_creation_scope(
        &mut self,
        vault: Uuid,
        parent: Option<Uuid>,
    ) -> Result<()> {
        self.db.execute(
            "DELETE FROM entries WHERE name=?1",
            [format!(
                "create-scope:{vault}:{}",
                parent.map(|v| v.to_string()).unwrap_or_default()
            )],
        )?;
        Ok(())
    }
    pub fn queue_offline_create(
        &mut self,
        id: Uuid,
        vault: Uuid,
        content: ResourceContent,
        now: i64,
    ) -> Result<Uuid> {
        // Write high-water before checking a per-scope lease, including rejection.
        self.journal_status(now)?;
        let entry = self.read(&format!(
            "create-scope:{vault}:{}",
            content.parent_id.map(|v| v.to_string()).unwrap_or_default()
        ))?;
        ensure!(entry.revision > 0, "offline_creation_scope_missing");
        let (cap, verified): (Uuid, i64) = serde_json::from_slice(&entry.bytes)?;
        ensure!(
            now >= verified && now.saturating_sub(verified) < EDIT_WINDOW_MS,
            "offline_edit_expired"
        );
        self.queue_creation(id, vault, content, cap, Some(cap), now)
    }
    fn queue_creation(
        &mut self,
        id: Uuid,
        vault_id: Uuid,
        content: ResourceContent,
        capability: Uuid,
        creation_scope: Option<Uuid>,
        now_ms: i64,
    ) -> Result<Uuid> {
        self.journal_status(now_ms)?;
        content.validate()?;
        ensure!(!content.deleted, "Cannot create a deleted resource");
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut c = clock(&tx, &self.key, &self.context, now_ms)?;
        ensure!(c.verified && c.remaining_ms > 0, "offline_edit_expired");
        ensure!(
            read_record(&tx, &self.key, &self.context, id)?.is_none(),
            "local_revision_conflict"
        );
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM journal_operations WHERE resource_id=?1)",
            [id.to_string()],
            |r| r.get(0),
        )?;
        ensure!(!exists, "local_revision_conflict");
        c.sequence = c
            .sequence
            .checked_add(1)
            .context("Device sequence exhausted")?;
        let draft = Draft {
            creation_scope,
            id: Uuid::new_v4(),
            device: c.device,
            sequence: c.sequence,
            resource_id: id,
            vault_id,
            expected_revision: 0,
            client_at_ms: now_ms,
            content: content.clone(),
            delivery: None,
            decision_delivery: None,
            write_capability: Some(capability),
            receipt: None,
        };
        let record = CachedResource {
            id,
            vault_id,
            server_revision: 0,
            generation: 1,
            navigation_only: false,
            permissions: Permissions {
                read: true,
                edit: true,
                connect: false,
            },
            write_capability: Some(capability),
            verified_at_ms: now_ms,
            content,
        };
        write_record(&tx, &self.key, &self.context, &record)?;
        write_draft(&tx, &self.key, &self.context, &draft, "pending")?;
        write_clock(&tx, &self.key, &self.context, &c)?;
        tx.commit()?;
        Ok(draft.id)
    }
    /// CAS generation is distinct from server revision. Never coalesce edits.
    pub fn queue_edit(
        &mut self,
        id: Uuid,
        generation: i64,
        content: ResourceContent,
        now_ms: i64,
    ) -> Result<Uuid> {
        // Persist the clock high-water even if a later validation rejects work.
        self.journal_status(now_ms)?;
        content.validate()?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut c = clock(&tx, &self.key, &self.context, now_ms)?;
        // Persist expiry even when refusing the write (otherwise a caller can
        // set the clock back then retry after reopening).
        if !c.verified || c.remaining_ms <= 0 {
            write_clock(&tx, &self.key, &self.context, &c)?;
            tx.commit()?;
            anyhow::bail!("offline_edit_expired");
        }
        let mut r = read_record(&tx, &self.key, &self.context, id)?
            .context("Resource not available offline")?;
        ensure!(
            r.permissions.edit && !r.content.deleted,
            "permission_denied"
        );
        ensure!(
            now_ms >= r.verified_at_ms && now_ms.saturating_sub(r.verified_at_ms) < EDIT_WINDOW_MS,
            "offline_edit_expired"
        );
        ensure!(
            r.write_capability.is_some(),
            "Download an online edit receipt first"
        );
        ensure!(r.generation == generation, "local_revision_conflict");
        ensure!(
            r.content.kind == content.kind && r.content.parent_id == content.parent_id,
            "move_requires_online_plan"
        );
        let blocked:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM journal_operations WHERE resource_id=?1 AND state IN ('conflict','quarantined'))",[id.to_string()],|r|r.get(0))?;
        ensure!(!blocked, "Resolve pending decision first");
        let pending:i64=tx.query_row("SELECT count(*) FROM journal_operations WHERE resource_id=?1 AND state IN ('pending','sending')",[id.to_string()],|r|r.get(0))?;
        c.sequence = c
            .sequence
            .checked_add(1)
            .context("Device sequence exhausted")?;
        let draft = Draft {
            creation_scope: if r.server_revision == 0 {
                let first:String=tx.query_row("SELECT id FROM journal_operations WHERE resource_id=?1 ORDER BY sequence LIMIT 1",[id.to_string()],|r|r.get(0))?;
                read_draft(&tx, &self.key, &self.context, Uuid::parse_str(&first)?)?
                    .0
                    .creation_scope
            } else {
                None
            },
            id: Uuid::new_v4(),
            device: c.device,
            sequence: c.sequence,
            resource_id: id,
            vault_id: r.vault_id,
            expected_revision: r
                .server_revision
                .checked_add(pending)
                .context("Revision exhausted")?,
            client_at_ms: now_ms,
            content: content.clone(),
            delivery: None,
            decision_delivery: None,
            write_capability: r.write_capability,
            receipt: None,
        };
        r.generation = r
            .generation
            .checked_add(1)
            .context("Local generation exhausted")?;
        if !super::connections::same_endpoint(&r.content, &content) {
            super::connections::invalidate_for(&tx, &self.key, &self.context, id)?;
        }
        r.content = content;
        write_record(&tx, &self.key, &self.context, &r)?;
        write_draft(&tx, &self.key, &self.context, &draft, "pending")?;
        write_clock(&tx, &self.key, &self.context, &c)?;
        tx.commit()?;
        Ok(draft.id)
    }
    /// FIFO per resource, while a conflict in one resource does not block others.
    /// A quarantined draft may only be sent to the restricted receipt endpoint.
    pub(crate) fn next_delivery(&mut self) -> Result<Option<(Draft, String)>> {
        let tx = self.db.transaction()?;
        let id:Option<String>=tx.query_row("SELECT o.id FROM journal_operations o WHERE o.state IN ('pending','sending','quarantined') AND NOT EXISTS(SELECT 1 FROM journal_operations earlier WHERE earlier.resource_id=o.resource_id AND earlier.sequence<o.sequence AND earlier.state NOT IN ('applied','resolved')) ORDER BY o.sequence LIMIT 1",[],|r|r.get(0)).optional()?;
        id.map(|id| read_draft(&tx, &self.key, &self.context, Uuid::parse_str(&id)?))
            .transpose()
    }
    pub(crate) fn bind_creation_receipt(&mut self, id: Uuid, capability: Uuid) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (mut d, state) = read_draft(&tx, &self.key, &self.context, id)?;
        ensure!(
            d.creation_scope.is_some() && d.delivery.is_none() && d.decision_delivery.is_none(),
            "Invalid creation claim state"
        );
        d.creation_scope = None;
        d.write_capability = Some(capability);
        if let Some(mut r) = read_record(&tx, &self.key, &self.context, d.resource_id)? {
            r.write_capability = Some(capability);
            write_record(&tx, &self.key, &self.context, &r)?;
        }
        write_draft(&tx, &self.key, &self.context, &d, &state)?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn prepare_delivery(&mut self, id: Uuid, body: serde_json::Value) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (mut d, state) = read_draft(&tx, &self.key, &self.context, id)?;
        ensure!(
            matches!(state.as_str(), "pending" | "sending"),
            "Invalid delivery state"
        );
        if let Some(old) = &d.delivery {
            ensure!(*old == body, "Cannot replace an uncertain delivery");
        }
        ensure!(
            body["operationId"].as_str() == Some(&id.to_string()),
            "Delivery operation mismatch"
        );
        ensure!(
            body["sequence"].as_i64() == Some(d.sequence),
            "Delivery sequence mismatch"
        );
        ensure!(
            body["resourceId"].as_str() == Some(&d.resource_id.to_string()),
            "Delivery resource mismatch"
        );
        d.delivery = Some(body);
        write_draft(&tx, &self.key, &self.context, &d, "sending")?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn acknowledge_delivery(
        &mut self,
        id: Uuid,
        receipt: serde_json::Value,
    ) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (mut d, state) = read_draft(&tx, &self.key, &self.context, id)?;
        if let Some(old) = &d.receipt {
            ensure!(*old == receipt, "Receipt changed");
            return Ok(());
        }
        ensure!(
            receipt["operationId"].as_str() == Some(&id.to_string()),
            "Receipt operation mismatch"
        );
        let next = receipt["status"]
            .as_str()
            .context("Missing receipt state")?
            .to_string();
        ensure!(
            next == "conflict"
                || (next == "applied"
                    && (state == "sending" || (state == "quarantined" && d.delivery.is_some()))),
            "Invalid receipt state"
        );
        if next == "applied" {
            let revision = receipt["revision"]
                .as_i64()
                .context("Missing applied revision")?;
            ensure!(
                revision == d.expected_revision + 1,
                "Unexpected applied revision"
            );
            if let Some(mut r) = read_record(&tx, &self.key, &self.context, d.resource_id)? {
                r.server_revision = revision;
                write_record(&tx, &self.key, &self.context, &r)?;
            }
        }
        d.receipt = Some(receipt);
        write_draft(&tx, &self.key, &self.context, &d, &next)?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn prepare_decision_delivery(
        &mut self,
        id: Uuid,
        body: serde_json::Value,
    ) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (mut d, state) = read_draft(&tx, &self.key, &self.context, id)?;
        ensure!(
            state == "quarantined",
            "Decision delivery requires quarantine"
        );
        if let Some(old) = &d.decision_delivery {
            ensure!(
                *old == body,
                "Cannot replace an uncertain decision delivery"
            );
        }
        ensure!(
            body["operationId"].as_str() == Some(&id.to_string())
                && body["sequence"].as_i64() == Some(d.sequence)
                && body["resourceId"].as_str() == Some(&d.resource_id.to_string()),
            "Decision delivery mismatch"
        );
        d.decision_delivery = Some(body);
        write_draft(&tx, &self.key, &self.context, &d, "quarantined")?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn conflict_operations(&self) -> Result<Vec<Uuid>> {
        let mut q = self.db.prepare(
            "SELECT id FROM journal_operations WHERE state='conflict' ORDER BY sequence LIMIT 5",
        )?;
        let ids = q
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.into_iter().map(|s| Ok(Uuid::parse_str(&s)?)).collect()
    }
    pub(crate) fn settle_decision(&mut self, id: Uuid) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (draft, state) = read_draft(&tx, &self.key, &self.context, id)?;
        ensure!(
            state == "conflict" || state == "resolved",
            "Not a decision operation"
        );
        // Descendants were written on a losing base. They must not silently
        // overwrite the decision just because a numeric revision now matches.
        let ids: Vec<String> = {
            let mut q=tx.prepare("SELECT id FROM journal_operations WHERE resource_id=?1 AND sequence>?2 AND state IN ('pending','sending')")?;
            let rows = q
                .query_map(
                    params![draft.resource_id.to_string(), draft.sequence],
                    |r| r.get(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        for next in ids {
            let (d, _) = read_draft(&tx, &self.key, &self.context, Uuid::parse_str(&next)?)?;
            write_draft(&tx, &self.key, &self.context, &d, "quarantined")?;
        }
        write_draft(&tx, &self.key, &self.context, &draft, "resolved")?;
        tx.commit()?;
        Ok(())
    }
    pub fn prune_journal_receipts(&mut self) -> Result<usize> {
        // Server history is authoritative. Keep the latest 1000 local receipts;
        // never prune pending work or unresolved decisions. Bounded per tick.
        Ok(self.db.execute("DELETE FROM journal_operations WHERE id IN (SELECT id FROM journal_operations WHERE state IN ('applied','resolved') ORDER BY sequence DESC LIMIT 100 OFFSET 1000)",[])?)
    }
    /// Explicit denial is not an outage. Accessible data is removed atomically;
    /// pending encrypted drafts survive solely for authorized decision delivery.
    pub fn revoke_cached_resource(&mut self, id: Uuid) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ids: Vec<String> = {
            let mut q=tx.prepare("SELECT id FROM journal_operations WHERE resource_id=?1 AND state IN ('pending','sending')")?;
            let rows = q
                .query_map([id.to_string()], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        for op in ids {
            let (d, _) = read_draft(&tx, &self.key, &self.context, Uuid::parse_str(&op)?)?;
            write_draft(&tx, &self.key, &self.context, &d, "quarantined")?;
        }
        tx.execute("DELETE FROM cached_resources WHERE id=?1", [id.to_string()])?;
        tx.execute(
            "DELETE FROM entries WHERE name LIKE 'create-scope:%:' || ?1",
            [id.to_string()],
        )?;
        super::connections::invalidate_for(&tx, &self.key, &self.context, id)?;
        tx.commit()?;
        Ok(())
    }
    pub fn reconcile_cached_vault(
        &mut self,
        vault: Uuid,
        visible: &std::collections::HashSet<Uuid>,
    ) -> Result<usize> {
        let ids: Vec<String> = {
            let mut q = self
                .db
                .prepare("SELECT id FROM cached_resources WHERE vault_id=?1")?;
            let rows = q
                .query_map([vault.to_string()], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        let mut removed = 0;
        for id in ids {
            let id = Uuid::parse_str(&id)?;
            if !visible.contains(&id) {
                // A new local identifier cannot appear in the server's list
                // before its create operation is delivered. Absence is not a
                // revocation; the sender checks current parent ACL separately.
                if self
                    .cached_resource(id)?
                    .is_some_and(|r| r.server_revision == 0)
                {
                    continue;
                }
                self.revoke_cached_resource(id)?;
                removed += 1;
            }
        }
        Ok(removed)
    }
    pub fn revoke_all_cached_resources(&mut self) -> Result<()> {
        self.clear_connection_cache()?;
        self.forget_cached_auth()?;
        self.db
            .execute("DELETE FROM entries WHERE name LIKE 'create-scope:%'", [])?;
        let ids: Vec<String> = {
            let mut q = self.db.prepare("SELECT id FROM cached_resources")?;
            let rows = q
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        for id in ids {
            self.revoke_cached_resource(Uuid::parse_str(&id)?)?;
        }
        // Caller must also close connections. A denial never starts a new lease.
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut c = clock(&tx, &self.key, &self.context, 0)?;
        c.verified = false;
        c.remaining_ms = 0;
        write_clock(&tx, &self.key, &self.context, &c)?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const NOW: i64 = 1_791_302_400_000;
    #[test]
    fn navigation_only_ancestors_survive_reopen_without_granting_edit_or_connect() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let identity = Identity::generate();
        let mut s = Store::open(dir.path(), "nav", &identity)?;
        let mut folder = resource();
        folder.navigation_only = true;
        folder.permissions = Permissions {
            read: false,
            connect: false,
            edit: false,
        };
        folder.write_capability = None;
        folder.content = ResourceContent {
            document: None, proxy: None,
            kind: "folder".into(),
            parent_id: None,
            name: "Path only".into(),
            address: None,
            protocol: None,
            port: None,
            username_policy: None,
            username: None,
            deleted: false,
        };
        s.refresh_cached_resource(folder.clone())?;
        s.authorize_offline_edits(NOW)?;
        assert!(s
            .queue_edit(folder.id, 0, folder.content.clone(), NOW + 1)
            .is_err());
        let mut invalid = folder.clone();
        invalid.permissions.connect = true;
        assert!(s.refresh_cached_resource(invalid).is_err());
        drop(s);
        let mut s = Store::open(dir.path(), "nav", &identity)?;
        let rows = s.cached_page(folder.vault_id, None)?;
        assert_eq!(rows.len(), 1);
        assert!(rows[0].navigation_only);
        assert!(
            !rows[0].permissions.read && !rows[0].permissions.connect && !rows[0].permissions.edit
        );
        Ok(())
    }
    #[test]
    fn creation_is_atomic_durable_and_cannot_repeat_without_generation() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let identity = Identity::generate();
        let mut s = Store::open(dir.path(), "create-scope", &identity)?;
        let r = resource();
        assert!(s
            .queue_create(r.id, r.vault_id, r.content.clone(), Uuid::new_v4(), NOW)
            .is_err());
        assert!(s.cached_resource(r.id)?.is_none());
        s.authorize_offline_edits(NOW)?;
        let operation =
            s.queue_create(r.id, r.vault_id, r.content.clone(), Uuid::new_v4(), NOW + 1)?;
        assert!(s.implicit_generation(r.id).is_err());
        assert!(s
            .queue_create(r.id, r.vault_id, r.content.clone(), Uuid::new_v4(), NOW + 2)
            .is_err());
        drop(s);
        let mut s = Store::open(dir.path(), "create-scope", &identity)?;
        assert_eq!(s.cached_resource(r.id)?.unwrap().server_revision, 0);
        let (draft, state) = s.next_delivery()?.unwrap();
        assert_eq!(draft.id, operation);
        assert_eq!(draft.expected_revision, 0);
        assert_eq!(draft.content, r.content);
        assert_eq!(state, "pending");
        s.prepare_delivery(operation, body(&draft))?;
        s.acknowledge_delivery(
            operation,
            serde_json::json!({"operationId":operation,"status":"applied","revision":1}),
        )?;
        let generation = s.implicit_generation(r.id)?;
        assert_eq!(generation, 1);
        let next = s.queue_edit(r.id, generation, r.content, NOW + 3)?;
        let (draft, _) = s.next_delivery()?.unwrap();
        assert_eq!(draft.id, next);
        assert_eq!(draft.expected_revision, 1);
        Ok(())
    }

    #[test]
    fn document_journal_survives_restart_and_identical_refresh_keeps_preview() -> Result<()> {
        use crate::resources::TeamDocument;
        let dir=tempfile::tempdir()?;let identity=Identity::generate();
        let mut store=Store::open(dir.path(),"documents",&identity)?;
        let mut r=resource();r.content.kind="snippet".into();r.content.address=None;r.content.port=None;r.content.protocol=None;r.content.username_policy=None;r.content.username=None;
        r.content.document=Some(TeamDocument::Snippet{command:"printf 'şifreli belge'".into()});
        store.refresh_cached_resource(r.clone())?;
        let generation=store.cached_resource(r.id)?.unwrap().generation;
        store.refresh_cached_resource(r.clone())?;
        assert_eq!(store.cached_resource(r.id)?.unwrap().generation,generation);
        store.authorize_offline_edits(NOW)?;
        let mut draft=r.content.clone();draft.document=Some(TeamDocument::Snippet{command:"printf 'sonraki belge'".into()});
        store.queue_edit(r.id,generation,draft.clone(),NOW+1)?;drop(store);
        let mut store=Store::open(dir.path(),"documents",&identity)?;
        assert_eq!(store.cached_resource(r.id)?.unwrap().content,draft);
        assert_eq!(store.next_delivery()?.unwrap().0.content,draft);
        let bytes:Vec<u8>=store.db.query_row("SELECT ciphertext FROM cached_resources",[],|row|row.get(0))?;
        assert!(!String::from_utf8_lossy(&bytes).contains("printf"));
        Ok(())
    }
    fn resource() -> CachedResource {
        CachedResource {
            navigation_only: false,
            id: Uuid::new_v4(),
            vault_id: Uuid::new_v4(),
            server_revision: 7,
            generation: 0,
            permissions: Permissions {
                read: true,
                connect: true,
                edit: true,
            },
            write_capability: Some(Uuid::new_v4()),
            verified_at_ms: NOW,
            content: ResourceContent {
                document: None, proxy: None,
            kind: "host".into(),
                parent_id: None,
                name: "şirket-secret-cache-marker".into(),
                address: Some("private-host.example".into()),
                protocol: Some("ssh".into()),
                port: Some(22),
                username_policy: Some("personal".into()),
                username: None,
                deleted: false,
            },
        }
    }
    #[test]
    fn online_authority_renewal_preserves_draft_and_base_without_renewing_other_records() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let identity = crate::keys::Identity::generate();
        let mut store = Store::open(dir.path(), "renewal", &identity)?;
        let first = resource();
        let second = resource();
        store.refresh_cached_resource(first.clone())?;
        store.refresh_cached_resource(second.clone())?;
        store.authorize_offline_edits(NOW)?;
        let mut draft = first.content.clone(); draft.name = "unsent local change".into();
        store.queue_edit(first.id, 0, draft.clone(), NOW+1)?;
        let expired = NOW + EDIT_WINDOW_MS + 5;
        assert!(store.edit_authority_needs_refresh(first.id, expired)?);
        assert!(store.queue_edit(first.id, 1, draft.clone(), expired).is_err());
        let receipt = Uuid::new_v4();
        store.renew_edit_authority(first.id, receipt, expired)?;
        let renewed = store.cached_resource(first.id)?.unwrap();
        assert_eq!(renewed.server_revision, first.server_revision);
        assert_eq!(renewed.generation, 1);
        assert_eq!(renewed.content, draft);
        assert_eq!(renewed.write_capability, Some(receipt));
        assert!(!store.edit_authority_needs_refresh(first.id, expired+1)?);
        assert!(store.edit_authority_needs_refresh(second.id, expired+1)?);
        assert!(store.queue_edit(second.id, 0, second.content.clone(), expired+1).is_err());
        // Clock rollback stays closed until another authenticated refresh.
        assert!(store.edit_authority_needs_refresh(first.id, NOW)?);
        assert!(store.queue_edit(first.id, 1, draft.clone(), NOW).is_err());
        store.renew_edit_authority(first.id, Uuid::new_v4(), NOW)?;
        store.queue_edit(first.id, 1, draft, NOW+1)?;
        assert_eq!(store.journal_status(NOW+2)?.pending, 2);
        Ok(())
    }
    fn body(d: &Draft) -> serde_json::Value {
        serde_json::json!({"operationId":d.id,"sequence":d.sequence,"resourceId":d.resource_id,"snapshot":"ciphertext"})
    }
    #[test]
    fn offline_creation_is_scoped_expires_and_preserves_followup_edits() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let identity = Identity::generate();
        let mut s = Store::open(dir.path(), "scope-create", &identity)?;
        let r = resource();
        let cap = Uuid::new_v4();
        s.authorize_offline_edits(NOW)?;
        assert!(s
            .queue_offline_create(r.id, r.vault_id, r.content.clone(), NOW)
            .is_err());
        s.cache_creation_scope(r.vault_id, None, cap, NOW)?;
        drop(s);
        let mut s = Store::open(dir.path(), "scope-create", &identity)?;
        let mut outside = r.content.clone();
        outside.parent_id = Some(Uuid::new_v4());
        assert!(s
            .queue_offline_create(Uuid::new_v4(), r.vault_id, outside, NOW)
            .is_err());
        assert!(s
            .queue_offline_create(Uuid::new_v4(), Uuid::new_v4(), r.content.clone(), NOW)
            .is_err());
        let op = s.queue_offline_create(r.id, r.vault_id, r.content.clone(), NOW + 1)?;
        assert_eq!(s.next_delivery()?.unwrap().0.creation_scope, Some(cap));
        s.queue_edit(r.id, 1, r.content.clone(), NOW + 2)?;
        let receipt = Uuid::new_v4();
        s.bind_creation_receipt(op, receipt)?;
        assert_eq!(s.next_delivery()?.unwrap().0.creation_scope, None);
        assert_eq!(
            s.cached_resource(r.id)?.unwrap().write_capability,
            Some(receipt)
        );
        assert_eq!(s.journal_status(NOW + 3)?.pending, 2);
        assert!(s
            .queue_offline_create(
                Uuid::new_v4(),
                r.vault_id,
                r.content.clone(),
                NOW + EDIT_WINDOW_MS
            )
            .is_err());
        s.revoke_all_cached_resources()?;
        s.authorize_offline_edits(NOW + EDIT_WINDOW_MS + 1)?;
        assert!(s
            .queue_offline_create(
                Uuid::new_v4(),
                r.vault_id,
                r.content.clone(),
                NOW + EDIT_WINDOW_MS + 1
            )
            .is_err());
        Ok(())
    }
    #[test]
    fn completed_permission_refresh_purges_only_missing_scope_and_keeps_drafts() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let identity = Identity::generate();
        let mut s = Store::open(dir.path(), "scope", &identity)?;
        let r = resource();
        let mut sibling = resource();
        sibling.vault_id = r.vault_id;
        let other = resource();
        for row in [&r, &sibling, &other] {
            s.refresh_cached_resource(row.clone())?;
        }
        s.authorize_offline_edits(NOW)?;
        s.queue_edit(r.id, 0, r.content.clone(), NOW + 1)?;
        s.replace(
            &format!("connection:{}", r.id),
            0,
            b"encrypted connection material",
        )?;
        assert_eq!(
            s.reconcile_cached_vault(r.vault_id, &std::collections::HashSet::from([sibling.id]))?,
            1
        );
        assert!(s.cached_resource(r.id)?.is_none());
        assert!(s.cached_resource(sibling.id)?.is_some());
        assert!(s.cached_resource(other.id)?.is_some());
        assert_eq!(s.read(&format!("connection:{}", r.id))?.revision, 0);
        assert_eq!(s.journal_status(NOW + 2)?.quarantined, 1);
        assert_eq!(s.next_delivery()?.unwrap().0.content, r.content);
        Ok(())
    }
    #[test]
    fn rejected_edits_cannot_roll_back_the_clock_high_water() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut s = Store::open(dir.path(), "scope", &Identity::generate())?;
        let r = resource();
        s.refresh_cached_resource(r.clone())?;
        s.authorize_offline_edits(NOW)?;
        assert!(s
            .queue_edit(r.id, 99, r.content.clone(), NOW + 10_000)
            .is_err());
        assert!(s.queue_edit(r.id, 0, r.content.clone(), NOW + 1).is_err());
        assert_eq!(s.journal_status(NOW + 10_001)?.edit_remaining_ms, 0);
        Ok(())
    }
    #[test]
    fn reopen_preserves_every_edit_sequence_and_ciphertext() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let identity = Identity::generate();
        let r = resource();
        let mut s = Store::open(dir.path(), "scope", &identity)?;
        s.refresh_cached_resource(r.clone())?;
        s.authorize_offline_edits(NOW)?;
        let device = s.journal_device(NOW)?;
        let first = s.queue_edit(r.id, 0, r.content.clone(), NOW + 1)?;
        let mut changed = r.content.clone();
        changed.name = "second change".into();
        let second = s.queue_edit(r.id, 1, changed.clone(), NOW + 2)?;
        assert_ne!(first, second);
        assert_eq!(s.journal_status(NOW + 3)?.pending, 2);
        drop(s);
        let mut s = Store::open(dir.path(), "scope", &identity)?;
        assert_eq!(s.journal_device(NOW + 4)?, device);
        assert_eq!(s.cached_resource(r.id)?.unwrap().content, changed);
        let (d, state) = s.next_delivery()?.unwrap();
        assert_eq!(state, "pending");
        assert_eq!(d.id, first);
        assert_eq!(d.expected_revision, 7);
        assert_eq!(d.sequence, 1);
        let payload = body(&d);
        s.prepare_delivery(first, payload.clone())?;
        drop(s);
        let mut s = Store::open(dir.path(), "scope", &identity)?;
        let (d, state) = s.next_delivery()?.unwrap();
        assert_eq!(state, "sending");
        assert_eq!(d.delivery, Some(payload.clone()));
        assert!(s
            .prepare_delivery(first, serde_json::json!({"operationId":first}))
            .is_err());
        let receipt = serde_json::json!({"operationId":first,"status":"applied","revision":8});
        s.acknowledge_delivery(first, receipt.clone())?;
        s.acknowledge_delivery(first, receipt)?;
        let (d, _) = s.next_delivery()?.unwrap();
        assert_eq!(d.id, second);
        assert_eq!(d.expected_revision, 8);
        assert_eq!(d.sequence, 2);
        for file in std::fs::read_dir(dir.path())? {
            let bytes = std::fs::read(file?.path())?;
            for marker in [
                r.content.name.as_bytes(),
                b"private-host.example",
                b"second change",
            ] {
                assert!(!bytes.windows(marker.len()).any(|w| w == marker));
            }
        }
        Ok(())
    }
    #[test]
    fn edit_and_journal_are_atomic_on_disk_failure() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut s = Store::open(dir.path(), "scope", &Identity::generate())?;
        let r = resource();
        s.refresh_cached_resource(r.clone())?;
        s.authorize_offline_edits(NOW)?;
        s.db.execute_batch("CREATE TEMP TRIGGER simulated_disk_full BEFORE INSERT ON journal_operations BEGIN SELECT RAISE(ABORT,'database or disk is full'); END;")?;
        let mut changed = r.content.clone();
        changed.name = "must roll back".into();
        assert!(s.queue_edit(r.id, 0, changed, NOW + 1).is_err());
        assert_eq!(s.cached_resource(r.id)?.unwrap().generation, 0);
        assert_eq!(s.cached_resource(r.id)?.unwrap().content, r.content);
        assert_eq!(s.journal_status(NOW + 2)?.pending, 0);
        s.db.execute_batch("DROP TRIGGER simulated_disk_full")?;
        s.queue_edit(r.id, 0, r.content, NOW + 3)?;
        assert_eq!(s.next_delivery()?.unwrap().0.sequence, 1);
        Ok(())
    }
    #[test]
    fn concurrent_edit_refresh_conflict_and_revocation_preserve_work() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let identity = Identity::generate();
        let r = resource();
        let mut a = Store::open(dir.path(), "scope", &identity)?;
        let mut b = Store::open(dir.path(), "scope", &identity)?;
        a.refresh_cached_resource(r.clone())?;
        a.authorize_offline_edits(NOW)?;
        let first = a.queue_edit(r.id, 0, r.content.clone(), NOW + 1)?;
        assert!(b.queue_edit(r.id, 0, r.content.clone(), NOW + 2).is_err());
        let second = a.queue_edit(r.id, 1, r.content.clone(), NOW + 3)?;
        let mut remote = r.clone();
        remote.server_revision = 20;
        remote.content.name = "remote winner".into();
        a.refresh_cached_resource(remote)?;
        assert_eq!(a.cached_resource(r.id)?.unwrap().content, r.content);
        let (draft, _) = a.next_delivery()?.unwrap();
        a.prepare_delivery(first, body(&draft))?;
        a.acknowledge_delivery(first,serde_json::json!({"operationId":first,"status":"conflict","conflictId":Uuid::new_v4()}))?;
        assert!(a.next_delivery()?.is_none()); // second edit cannot skip its parent
        assert!(a.queue_edit(r.id, 2, r.content.clone(), NOW + 4).is_err());
        let other = resource();
        a.refresh_cached_resource(other.clone())?;
        let independent = a.queue_edit(other.id, 0, other.content.clone(), NOW + 5)?;
        assert_eq!(a.next_delivery()?.unwrap().0.id, independent);
        a.revoke_cached_resource(r.id)?;
        assert!(b.cached_resource(r.id)?.is_none());
        let tx = b.db.transaction()?;
        let (_, state) = read_draft(&tx, &b.key, &b.context, second)?;
        assert_eq!(state, "quarantined");
        Ok(())
    }
    #[test]
    fn expiry_and_clock_rollback_survive_restart_without_disabling_connections() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let identity = Identity::generate();
        let r = resource();
        let mut s = Store::open(dir.path(), "scope", &identity)?;
        s.refresh_cached_resource(r.clone())?;
        assert!(s.queue_edit(r.id, 0, r.content.clone(), NOW).is_err());
        s.authorize_offline_edits(NOW)?;
        assert_eq!(
            s.journal_status(NOW + EDIT_WINDOW_MS - 1)?
                .edit_remaining_ms,
            1
        );
        assert!(s
            .queue_edit(r.id, 0, r.content.clone(), NOW + EDIT_WINDOW_MS)
            .is_err());
        assert!(s.cached_resource(r.id)?.unwrap().permissions.connect);
        s.authorize_offline_edits(NOW + EDIT_WINDOW_MS)?;
        assert!(s.queue_edit(r.id, 0, r.content.clone(), NOW).is_err());
        drop(s);
        let mut s = Store::open(dir.path(), "scope", &identity)?;
        assert_eq!(
            s.journal_status(NOW + EDIT_WINDOW_MS + 1)?
                .edit_remaining_ms,
            0
        );
        assert!(s
            .queue_edit(r.id, 0, r.content.clone(), NOW + EDIT_WINDOW_MS + 1)
            .is_err());
        s.authorize_offline_edits(NOW + EDIT_WINDOW_MS + 2)?;
        let mut fresh = r.clone();
        fresh.verified_at_ms = NOW + EDIT_WINDOW_MS + 2;
        s.refresh_cached_resource(fresh)?;
        let generation=s.cached_resource(r.id)?.unwrap().generation;
        s.queue_edit(r.id, generation, r.content, NOW + EDIT_WINDOW_MS + 3)?;
        Ok(())
    }
    #[test]
    fn tamper_permissions_scope_and_generation_are_rejected() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let identity = Identity::generate();
        let mut s = Store::open(dir.path(), "scope", &identity)?;
        let mut r = resource();
        r.permissions.edit = false;
        s.refresh_cached_resource(r.clone())?;
        s.authorize_offline_edits(NOW)?;
        assert!(s.queue_edit(r.id, 0, r.content.clone(), NOW + 1).is_err());
        r.permissions.edit = true;
        s.refresh_cached_resource(r.clone())?;
        let mut moved = r.content.clone();
        moved.parent_id = Some(Uuid::new_v4());
        assert!(s.queue_edit(r.id, 1, moved, NOW + 2).is_err());
        assert!(Store::open(dir.path(), "other", &identity)?
            .cached_resource(r.id)?
            .is_none());
        s.queue_edit(r.id, 1, r.content, NOW + 3)?;
        s.db.execute("UPDATE journal_operations SET state='applied'", [])?;
        let tx = s.db.transaction()?;
        let id: String = tx.query_row("SELECT id FROM journal_operations", [], |r| r.get(0))?;
        assert!(read_draft(&tx, &s.key, &s.context, Uuid::parse_str(&id)?).is_err());
        drop(tx);
        s.db.execute("UPDATE cached_resources SET generation=generation+1", [])?;
        assert!(s.cached_resource(r.id).is_err());
        Ok(())
    }
    #[test]
    fn migration_preserves_v1_known_hosts_and_rejects_newer_formats() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let identity = Identity::generate();
        let mut s = Store::open(dir.path(), "scope", &identity)?;
        s.replace("known-hosts", 0, b"existing trust")?;
        s.db.execute_batch("DROP TABLE journal_operations; DROP TABLE cached_resources; DROP TABLE journal_state; PRAGMA user_version=1;")?;
        drop(s);
        let s = Store::open(dir.path(), "scope", &identity)?;
        assert_eq!(&*s.read("known-hosts")?.bytes, b"existing trust");
        assert_eq!(
            s.db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
            8
        );
        s.db.execute_batch("PRAGMA user_version=9")?;
        drop(s);
        assert!(Store::open(dir.path(), "scope", &identity).is_err());
        Ok(())
    }
}
