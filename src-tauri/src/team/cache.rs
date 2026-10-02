//! Format 2 Team cache: encrypted metadata, records and append-only journal.
//! Personal .ttvault/.ttbackup readers intentionally reject this format.
use super::crypto as team_crypto;
use crate::{
    crypto,
    model::{Record, VaultInfo},
};
use anyhow::{anyhow, ensure, Result};
use fs2::FileExt;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedRecord {
    pub record: Record,
    pub revision: i64,
    pub permissions: Vec<String>,
    pub deleted: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub operation_id: String,
    pub sequence: i64,
    pub vault_id: String,
    pub record: Record,
    pub expected_revision: i64,
    pub deleted: bool,
    pub client_at: String,
    #[serde(default)]
    pub restore_from: Option<i64>,
}
pub struct Cache {
    pub conn: Connection,
    key: Zeroizing<[u8; 32]>,
    pub path: PathBuf,
    _lock: File,
}
impl Cache {
    pub fn open(path: &Path, password: &str, create: bool) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("ttteam.lock"))?;
        lock.try_lock_exclusive()
            .map_err(|_| anyhow!("Team önbelleği başka bir uygulamada açık"))?;
        let exists = path.exists();
        ensure!(
            exists || create,
            "Bu hesap için yerel Team önbelleği yok. İlk giriş PostgreSQL gerektirir."
        );
        if !exists {
            let key = crypto::random_key();
            let envelope = crypto::Envelope::create(password, &key, b"termterm-team-cache-v2")?;
            let reservation = OpenOptions::new().create_new(true).write(true).open(path)?;
            drop(reservation);
            let conn = Connection::open(path)?;
            conn.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA secure_delete=ON; CREATE TABLE header(version INTEGER NOT NULL,envelope BLOB NOT NULL); CREATE TABLE state(id TEXT PRIMARY KEY,payload BLOB NOT NULL); CREATE TABLE records(vault TEXT NOT NULL,id TEXT NOT NULL,payload BLOB NOT NULL,PRIMARY KEY(vault,id)); CREATE TABLE journal(sequence INTEGER PRIMARY KEY AUTOINCREMENT,op_id TEXT UNIQUE NOT NULL,vault TEXT NOT NULL,record_id TEXT NOT NULL,payload BLOB NOT NULL,status TEXT NOT NULL DEFAULT 'queued',receipt BLOB);")?;
            conn.execute(
                "INSERT INTO header VALUES(2,?1)",
                [serde_json::to_vec(&envelope)?],
            )?;
            return Ok(Self {
                conn,
                key,
                path: path.into(),
                _lock: lock,
            });
        }
        // Recover a ciphertext copy first. An invalid password/newer format must
        // leave both the original database and its hot rollback journal untouched.
        let journal = PathBuf::from(format!("{}-journal", path.display()));
        let recovery = if journal.is_file() {
            Some(tempfile::tempdir()?)
        } else {
            None
        };
        let read = if let Some(dir) = &recovery {
            let copy = dir.path().join("recovery.ttteam");
            std::fs::copy(path, &copy)?;
            std::fs::copy(&journal, dir.path().join("recovery.ttteam-journal"))?;
            Connection::open(copy)?
        } else {
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?
        };
        let (version, bytes): (i64, Vec<u8>) =
            read.query_row("SELECT version,envelope FROM header", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        ensure!(
            version == 2,
            "Desteklenmeyen Team önbelleği sürümü; dosya değiştirilmedi"
        );
        let envelope: crypto::Envelope = serde_json::from_slice(&bytes)?;
        let key = envelope.unlock(password, b"termterm-team-cache-v2")?;
        drop(read);
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA synchronous=FULL; PRAGMA secure_delete=ON;")?;
        Ok(Self {
            conn,
            key,
            path: path.into(),
            _lock: lock,
        })
    }
    pub fn set<T: Serialize>(&self, id: &str, value: &T) -> Result<()> {
        let payload = crypto::seal(&self.key, &serde_json::to_vec(value)?, id.as_bytes())?;
        self.conn.execute("INSERT INTO state VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",params![id,payload])?;
        Ok(())
    }
    pub fn get<T: serde::de::DeserializeOwned>(&self, id: &str) -> Result<Option<T>> {
        let bytes: Option<Vec<u8>> = self
            .conn
            .query_row("SELECT payload FROM state WHERE id=?1", [id], |r| r.get(0))
            .optional()?;
        bytes
            .map(|b| {
                Ok(serde_json::from_slice(&crypto::unseal(
                    &self.key,
                    &b,
                    id.as_bytes(),
                )?)?)
            })
            .transpose()
    }
    pub fn records(&self, vault: &str) -> Result<Vec<CachedRecord>> {
        let mut s = self
            .conn
            .prepare("SELECT id,payload FROM records WHERE vault=?1")?;
        let rows = s.query_map([vault], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        rows.map(|row| {
            let (id, b) = row?;
            Ok(serde_json::from_slice(&crypto::unseal(
                &self.key,
                &b,
                format!("{vault}:{id}").as_bytes(),
            )?)?)
        })
        .collect()
    }
    pub fn permitted(&self, vault: &str, id: &str, permission: &str) -> Result<CachedRecord> {
        let r = self
            .record(vault, id)?
            .filter(|r| !r.deleted)
            .ok_or_else(|| anyhow!("Kayıt erişilebilir değil"))?;
        ensure!(
            r.permissions.iter().any(|p| p == permission),
            "Team yetkisi gerekli: {permission}"
        );
        Ok(r)
    }
    pub fn record(&self, vault: &str, id: &str) -> Result<Option<CachedRecord>> {
        let bytes: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT payload FROM records WHERE vault=?1 AND id=?2",
                params![vault, id],
                |r| r.get(0),
            )
            .optional()?;
        bytes
            .map(|b| {
                Ok(serde_json::from_slice(&crypto::unseal(
                    &self.key,
                    &b,
                    format!("{vault}:{id}").as_bytes(),
                )?)?)
            })
            .transpose()
    }
    pub fn snapshot(
        &mut self,
        vault: &str,
        records: &[CachedRecord],
        verified: i64,
    ) -> Result<bool> {
        let old = self.records(vault)?;
        let revoked = old.iter().any(|r| {
            r.permissions.iter().any(|p| p == "connect")
                && !records.iter().any(|n| {
                    n.record.id == r.record.id
                        && n.permissions.iter().any(|p| p == "connect")
                        && !n.deleted
                })
        });
        let tx = self.conn.transaction()?;
        // Preserve drafts in the journal; unauthorized material is removed from the normal cache.
        tx.execute("DELETE FROM records WHERE vault=?1", [vault])?;
        for r in records {
            let payload = crypto::seal(
                &self.key,
                &serde_json::to_vec(r)?,
                format!("{vault}:{}", r.record.id).as_bytes(),
            )?;
            tx.execute(
                "INSERT INTO records VALUES(?1,?2,?3)",
                params![vault, r.record.id, payload],
            )?;
        }
        tx.commit()?;
        self.set(&format!("verified:{vault}"), &verified)?;
        Ok(revoked)
    }
    pub fn apply_permissions(&mut self, vault: &str, rows: &[Value]) -> Result<bool> {
        let old = self.records(vault)?;
        let mut revoked = false;
        let tx = self.conn.transaction()?;
        for mut cached in old {
            let current = rows.iter().find(|r| r["id"] == cached.record.id);
            let permissions: Vec<String> = current
                .map(|r| serde_json::from_value(r["permissions"].clone()))
                .transpose()?
                .unwrap_or_default();
            let connect = permissions.iter().any(|p| p == "connect")
                && current.is_some_and(|r| r["deleted"] != true);
            if cached.permissions.iter().any(|p| p == "connect") && !connect {
                revoked = true;
            }
            if current.is_none() {
                tx.execute(
                    "DELETE FROM records WHERE vault=?1 AND id=?2",
                    params![vault, cached.record.id],
                )?;
                continue;
            }
            cached.permissions = permissions;
            cached.deleted = current.is_some_and(|r| r["deleted"] == true);
            if !cached
                .permissions
                .iter()
                .any(|p| ["connect", "reveal", "edit"].contains(&p.as_str()))
            {
                cached.record = team_crypto::split(&cached.record).0;
            }
            let payload = crypto::seal(
                &self.key,
                &serde_json::to_vec(&cached)?,
                format!("{vault}:{}", cached.record.id).as_bytes(),
            )?;
            tx.execute(
                "UPDATE records SET payload=?3 WHERE vault=?1 AND id=?2",
                params![vault, cached.record.id, payload],
            )?;
        }
        tx.commit()?;
        if revoked {
            self.purge_connection_material(vault)?;
            self.set(&format!("dependencies:{vault}"), &Vec::<Record>::new())?;
        }
        Ok(revoked)
    }
    pub fn enqueue(
        &mut self,
        vault: &str,
        record: Record,
        deleted: bool,
        now: i64,
        restore_from: Option<i64>,
        owner: bool,
    ) -> Result<Draft> {
        let verified = self
            .get::<i64>(&format!("verified:{vault}"))?
            .ok_or_else(|| anyhow!("Önce Team kasasını çevrimiçi açın"))?;
        ensure!(editable_at(verified,now),"Son yetki doğrulaması 24 saati geçti. Bağlantı kullanılabilir; düzenleme için PostgreSQL'e bağlanın.");
        ensure!(
            !self.get::<bool>("authRejected")?.unwrap_or(false)
                && !self
                    .get::<bool>(&format!("revoked:{vault}"))?
                    .unwrap_or(false),
            "Team erişimi reddedildi; yeniden giriş / yetki doğrulaması gerekli"
        );
        ensure!(
            Uuid::parse_str(&record.id).is_ok(),
            "Geçersiz kayıt kimliği"
        );
        let existing = self.record(vault, &record.id)?;
        let old = existing.as_ref();
        let parent = record.data["groupId"].as_str().filter(|s| !s.is_empty());
        let permissions = if let Some(old) = old {
            ensure!(
                old.permissions.iter().any(|p| p == "edit"),
                "Düzenleme yetkisi yok"
            );
            ensure!(old.record.kind == record.kind, "Kayıt türü değiştirilemez");
            ensure!(
                old.record.data["groupId"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    == parent,
                "Klasör taşıma çevrimiçi erişim önizlemesi gerektirir"
            );
            old.permissions.clone()
        } else if owner {
            vec!["read", "connect", "edit", "reveal", "export", "manage"]
                .into_iter()
                .map(str::to_string)
                .collect()
        } else {
            let folder = self.permitted(
                vault,
                parent.ok_or_else(|| anyhow!("Bu kökte oluşturma yetkisi yok"))?,
                "edit",
            )?;
            folder.permissions
        };
        ensure!(
            [
                "host",
                "group",
                "credential",
                "snippet",
                "workspace",
                "tunnel",
                "knownHost"
            ]
            .contains(&record.kind.as_str()),
            "Bu kayıt türü Team kasasında desteklenmiyor"
        );
        if deleted {
            ensure!(
                !self.records(vault)?.iter().any(|r| !r.deleted
                    && r.record.id != record.id
                    && record_dependencies(&r.record).contains(&record.id)),
                "Kayıt halen başka kayıtlar tarafından kullanılıyor"
            );
        } else {
            let old_dependencies = old
                .map(|r| record_dependencies(&r.record))
                .unwrap_or_default();
            for id in record_dependencies(&record) {
                ensure!(id != record.id, "Kayıt kendisine bağlanamaz");
                if old_dependencies.contains(&id) {
                    continue;
                }
                let dependency = self.permitted(vault, &id, "read")?;
                if parent != Some(id.as_str()) && ["host", "group"].contains(&record.kind.as_str())
                {
                    ensure!(dependency.permissions.iter().any(|p|p=="manage"),"Yeni kimlik / jump host bağımlılığı paylaşmak için kaynakta erişim yönetimi yetkisi gerekli");
                }
            }
        }
        // Redacted UI updates preserve secrets unless the user is allowed to reveal/edit them.
        let record = if let Some(old) = old.filter(|r| !r.permissions.iter().any(|p| p == "reveal"))
        {
            team_crypto::merge(
                team_crypto::split(&record).0,
                &team_crypto::split(&old.record).1,
            )
        } else {
            record
        };
        let tx = self.conn.savepoint()?;
        let sequence: i64 =
            tx.query_row("SELECT coalesce(max(sequence),0)+1 FROM journal", [], |r| {
                r.get(0)
            })?;
        let expected = old.map(|r| r.revision).unwrap_or(0);
        let draft = Draft {
            operation_id: Uuid::new_v4().to_string(),
            sequence,
            vault_id: vault.into(),
            record: record.clone(),
            expected_revision: expected,
            deleted,
            client_at: chrono::DateTime::from_timestamp_millis(now)
                .ok_or_else(|| anyhow!("Invalid clock"))?
                .to_rfc3339(),
            restore_from,
        };
        let payload = crypto::seal(
            &self.key,
            &serde_json::to_vec(&draft)?,
            draft.operation_id.as_bytes(),
        )?;
        tx.execute(
            "INSERT INTO journal(sequence,op_id,vault,record_id,payload) VALUES(?1,?2,?3,?4,?5)",
            params![sequence, draft.operation_id, vault, record.id, payload],
        )?;
        let cached = CachedRecord {
            record: record.clone(),
            revision: expected,
            permissions,
            deleted,
        };
        let payload = crypto::seal(
            &self.key,
            &serde_json::to_vec(&cached)?,
            format!("{vault}:{}", record.id).as_bytes(),
        )?;
        tx.execute("INSERT INTO records VALUES(?1,?2,?3) ON CONFLICT(vault,id) DO UPDATE SET payload=excluded.payload",params![vault,record.id,payload])?;
        tx.commit()?;
        Ok(draft)
    }
    pub fn enqueue_batch(
        &mut self,
        vault: &str,
        records: Vec<Record>,
        deleted: bool,
        now: i64,
        owner: bool,
    ) -> Result<()> {
        let records = dependency_order(records, deleted)?;
        self.conn.execute_batch("SAVEPOINT team_batch")?;
        let result = (|| {
            for record in records {
                self.enqueue(vault, record, deleted, now, None, owner)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.conn.execute_batch("RELEASE team_batch")?;
                Ok(())
            }
            Err(e) => {
                self.conn
                    .execute_batch("ROLLBACK TO team_batch; RELEASE team_batch")?;
                Err(e)
            }
        }
    }
    pub fn pending(&self, vault: &str) -> Result<Vec<Draft>> {
        self.journal(vault, "queued")
    }
    pub fn journal(&self, vault: &str, status: &str) -> Result<Vec<Draft>> {
        let mut stmt = self.conn.prepare(
            "SELECT op_id,payload FROM journal WHERE vault=?1 AND status=?2 ORDER BY sequence",
        )?;
        let rows = stmt.query_map(params![vault, status], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        rows.map(|row| {
            let (id, b) = row?;
            Ok(serde_json::from_slice(&crypto::unseal(
                &self.key,
                &b,
                id.as_bytes(),
            )?)?)
        })
        .collect()
    }
    pub fn last_applied(
        &self,
        vault: &str,
        record: &str,
        before: i64,
        base: i64,
    ) -> Result<Option<i64>> {
        let row:Option<(String,Vec<u8>,Vec<u8>)>=self.conn.query_row("SELECT op_id,receipt,payload FROM journal WHERE vault=?1 AND record_id=?2 AND sequence<?3 AND status='applied' ORDER BY sequence DESC LIMIT 1",params![vault,record,before],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        row.map(|(id, b, p)| {
            let previous: Draft =
                serde_json::from_slice(&crypto::unseal(&self.key, &p, id.as_bytes())?)?;
            if previous.expected_revision != base {
                return Ok(None);
            }
            let receipt: Value =
                serde_json::from_slice(&crypto::unseal(&self.key, &b, id.as_bytes())?)?;
            Ok(receipt["revision"].as_i64())
        })
        .transpose()
        .map(Option::flatten)
    }
    pub fn purge_connection_material(&self, vault: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM state WHERE id LIKE ?1",
            [format!("connection:{vault}:%")],
        )?;
        self.conn.execute(
            "DELETE FROM state WHERE id LIKE ?1",
            [format!("checkout:{vault}:%")],
        )?;
        self.conn.execute(
            "DELETE FROM state WHERE id LIKE ?1",
            [format!("checkout-revision:{vault}:%")],
        )?;
        Ok(())
    }
    pub fn revoke(&mut self, vault: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM records WHERE vault=?1", [vault])?;
        self.purge_connection_material(vault)?;
        self.set(&format!("dependencies:{vault}"), &Vec::<Record>::new())?;
        self.set(&format!("known:{vault}"), &Vec::<Record>::new())?;
        self.set(&format!("revoked:{vault}"), &true)?;
        Ok(())
    }
    pub fn receipt(&self, id: &str, result: &Value) -> Result<()> {
        let bytes = crypto::seal(&self.key, &serde_json::to_vec(result)?, id.as_bytes())?;
        let payload: Vec<u8> =
            self.conn
                .query_row("SELECT payload FROM journal WHERE op_id=?1", [id], |r| {
                    r.get(0)
                })?;
        let mut draft: Draft =
            serde_json::from_slice(&crypto::unseal(&self.key, &payload, id.as_bytes())?)?;
        // Keep the ordered receipt and original base revision, release acknowledged SSH material.
        draft.record.data = json!({});
        let payload = crypto::seal(&self.key, &serde_json::to_vec(&draft)?, id.as_bytes())?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE journal SET status=?2,receipt=?3,payload=?4 WHERE op_id=?1",
            params![
                id,
                result["status"].as_str().unwrap_or("failed"),
                bytes,
                payload
            ],
        )?;
        tx.execute(
            "DELETE FROM state WHERE id=?1 OR id=?2",
            params![format!("prepared:{id}"), format!("decision:{id}")],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn record_online_receipt(&self, draft: &Draft, result: &Value) -> Result<()> {
        let payload = crypto::seal(
            &self.key,
            &serde_json::to_vec(draft)?,
            draft.operation_id.as_bytes(),
        )?;
        let receipt = crypto::seal(
            &self.key,
            &serde_json::to_vec(result)?,
            draft.operation_id.as_bytes(),
        )?;
        self.conn.execute("INSERT INTO journal(sequence,op_id,vault,record_id,payload,status,receipt) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(op_id) DO UPDATE SET status=excluded.status,receipt=excluded.receipt",params![draft.sequence,draft.operation_id,draft.vault_id,draft.record.id,payload,result["status"].as_str().unwrap_or("online"),receipt])?;
        if result["status"] != "online" {
            self.receipt(&draft.operation_id, result)?;
        }
        Ok(())
    }
    pub fn info(&self, vault: &str, name: &str, device: &str) -> Result<VaultInfo> {
        let mut rows = self.records(vault)?;
        let pending = self.pending(vault)?;
        let latest: HashMap<_, _> = pending.iter().map(|d| (d.record.id.as_str(), d)).collect();
        for row in &mut rows {
            if let Some(d) = latest
                .get(row.record.id.as_str())
                .filter(|_| row.permissions.iter().any(|p| p == "edit"))
            {
                row.record = d.record.clone();
                row.deleted = d.deleted;
            }
        }
        // A fresh snapshot does not contain offline-created records. Overlay only
        // genuinely new drafts whose current parent/root scope still allows edits.
        if !self.get::<bool>("authRejected")?.unwrap_or(false)
            && !self
                .get::<bool>(&format!("revoked:{vault}"))?
                .unwrap_or(false)
        {
            for draft in &pending {
                if draft.expected_revision != 0
                    || rows.iter().any(|r| r.record.id == draft.record.id)
                {
                    continue;
                }
                let permissions = if let Some(parent) = draft.record.data["groupId"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                {
                    rows.iter()
                        .find(|r| {
                            r.record.id == parent
                                && !r.deleted
                                && r.permissions.iter().any(|p| p == "edit")
                        })
                        .map(|r| r.permissions.clone())
                } else if self
                    .get::<bool>(&format!("rootEditable:{vault}"))?
                    .unwrap_or(false)
                {
                    Some(
                        ["read", "connect", "edit", "reveal", "export", "manage"]
                            .iter()
                            .map(|s| s.to_string())
                            .collect(),
                    )
                } else {
                    None
                };
                if let Some(permissions) = permissions {
                    let last = latest[draft.record.id.as_str()];
                    rows.push(CachedRecord {
                        record: last.record.clone(),
                        revision: 0,
                        permissions,
                        deleted: last.deleted,
                    });
                }
            }
        }
        let mut records: Vec<Record> = rows
            .into_iter()
            .filter(|r| !r.deleted)
            .map(|r| {
                let path_only = r.record.data["_teamPathOnly"] == true;
                let mut record = if r.permissions.iter().any(|p| p == "reveal") {
                    r.record
                } else {
                    team_crypto::split(&r.record).0
                };
                if path_only {
                    record.data["_teamPathOnly"] = json!(true);
                }
                record.data["_teamPermissions"] = json!(r.permissions);
                record.data["_teamRevision"] = json!(r.revision);
                record.data["_teamPending"] = json!(latest.contains_key(record.id.as_str()));
                record
            })
            .collect();
        for record in &mut records {
            if let Some(Some(revision)) =
                self.get::<Option<i64>>(&format!("checkout-revision:{vault}:{}", record.id))?
            {
                record.data["_teamTemporaryRevision"] = json!(revision);
            }
        }
        records.extend(self.get::<Vec<Record>>("preferences")?.unwrap_or_default());
        records.extend(
            self.get::<Vec<Record>>(&format!("known:{vault}"))?
                .unwrap_or_default(),
        );
        Ok(VaultInfo {
            id: vault.into(),
            name: name.into(),
            path: self.path.to_string_lossy().into(),
            device_id: device.into(),
            records,
        })
    }
}
// Stable dependency order also makes a multi-record local import atomic. A bad
// later record rolls back all earlier writes, including their journal entries.
fn dependency_order(records: Vec<Record>, deleted: bool) -> Result<Vec<Record>> {
    let ids: std::collections::HashSet<_> = records.iter().map(|r| r.id.clone()).collect();
    ensure!(
        ids.len() == records.len(),
        "Aynı işlemde yinelenen kayıt kimliği"
    );
    let mut remaining = records;
    let mut ordered = Vec::new();
    let mut ready = std::collections::HashSet::new();
    while !remaining.is_empty() {
        let index = remaining
            .iter()
            .position(|r| {
                record_dependencies(r)
                    .iter()
                    .all(|id| !ids.contains(id) || ready.contains(id))
            })
            .ok_or_else(|| anyhow!("Klasör / kimlik / host chain ilişkilerinde döngü var"))?;
        let record = remaining.remove(index);
        ready.insert(record.id.clone());
        ordered.push(record);
    }
    if deleted {
        ordered.reverse();
    }
    Ok(ordered)
}
pub fn record_dependencies(record: &Record) -> Vec<String> {
    let mut ids = Vec::new();
    for field in ["groupId", "credentialId", "hostId"] {
        if let Some(id) = record.data[field].as_str().filter(|id| !id.is_empty()) {
            ids.push(id.to_string());
        }
    }
    for id in record.data["chain"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        ids.push(id.to_string());
    }
    for id in record.data["hostIds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        ids.push(id.to_string());
    }
    for pane in record.data["panes"].as_array().into_iter().flatten() {
        if let Some(id) = pane["hostId"].as_str() {
            ids.push(id.to_string());
        }
    }
    ids
}
pub fn editable_at(verified: i64, now: i64) -> bool {
    now >= verified && now.saturating_sub(verified) < 24 * 60 * 60 * 1000
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_deadline_and_clock_rollback_fail_closed() {
        let now = 1_000_000;
        assert!(editable_at(now, now + 86_399_999));
        assert!(!editable_at(now, now + 86_400_000));
        assert!(!editable_at(now, now - 1));
    }
    #[test]
    fn offline_drafts_permissions_deadline_and_batch_rollback() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut c = Cache::open(
            &dir.path().join("drafts.ttteam"),
            "Disposable-test-1234",
            true,
        )?;
        c.snapshot("v", &[], 1_000_000)?;
        c.set("rootEditable:v", &true)?;
        let group = Record::new("group", json!({"label":"Grup"}));
        let host = Record::new(
            "host",
            json!({"label":"Yeni","groupId":group.id,"password":"secret"}),
        );
        c.enqueue_batch(
            "v",
            vec![host.clone(), group.clone()],
            false,
            1_000_001,
            true,
        )?;
        assert_eq!(c.pending("v")?[0].record.id, group.id);
        c.snapshot("v", &[], 1_000_002)?;
        assert_eq!(c.info("v", "Lab", "d")?.records.len(), 2);
        let mut bad = Record::new("host", json!({"label":"Bad"}));
        bad.id = "invalid".into();
        assert!(c
            .enqueue_batch(
                "v",
                vec![Record::new("group", json!({"label":"Rolled back"})), bad],
                false,
                1_000_003,
                true
            )
            .is_err());
        assert_eq!(c.pending("v")?.len(), 2);
        c.snapshot(
            "v",
            &[CachedRecord {
                record: host.clone(),
                revision: 1,
                permissions: vec!["read".into(), "connect".into(), "edit".into()],
                deleted: false,
            }],
            1_000_002,
        )?;
        assert!(c
            .enqueue(
                "v",
                host.clone(),
                false,
                1_000_002 + 86_400_000,
                None,
                false
            )
            .is_err());
        assert!(c.permitted("v", &host.id, "connect").is_ok());
        c.revoke("v")?;
        assert!(c.info("v", "Lab", "d")?.records.is_empty());
        assert!(c.permitted("v", &host.id, "connect").is_err());
        assert_eq!(
            c.pending("v")?.len(),
            2,
            "revocation retains encrypted delivery queue"
        );
        let d = c.pending("v")?.pop().unwrap();
        c.receipt(&d.operation_id, &json!({"status":"conflict"}))?;
        assert_eq!(c.journal("v", "conflict")?[0].record.data, json!({}));
        Ok(())
    }
    #[test]
    fn interrupted_team_write_preserves_queue_and_wrong_password_preserves_files() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("crash.ttteam");
        let mut c = Cache::open(&path, "Disposable-test-1234", true)?;
        c.snapshot("v", &[], 1_000_000)?;
        c.enqueue(
            "v",
            Record::new("host", json!({"label":"Committed"})),
            false,
            1_000_001,
            None,
            true,
        )?;
        drop(c);
        let status = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "team::cache::tests::crash_child", "--ignored"])
            .env("TERMTERM_TEAM_CRASH_TEST", &path)
            .status()?;
        assert_eq!(status.code(), Some(73));
        let journal = PathBuf::from(format!("{}-journal", path.display()));
        let bytes = std::fs::read(&path)?;
        let hot = std::fs::read(&journal)?;
        assert!(Cache::open(&path, "wrong", false).is_err());
        assert_eq!(std::fs::read(&path)?, bytes);
        assert_eq!(std::fs::read(&journal)?, hot);
        let c = Cache::open(&path, "Disposable-test-1234", false)?;
        assert_eq!(c.pending("v")?.len(), 1);
        assert_eq!(c.records("v")?[0].record.data["label"], "Committed");
        Ok(())
    }
    #[test]
    #[ignore = "only invoked by interrupted-write test"]
    fn crash_child() {
        if let Ok(path) = std::env::var("TERMTERM_TEAM_CRASH_TEST") {
            let c = Cache::open(Path::new(&path), "Disposable-test-1234", false).unwrap();
            c.conn.execute_batch("PRAGMA cache_size=1; BEGIN IMMEDIATE; UPDATE records SET payload=zeroblob(1000000); UPDATE journal SET status='lost'").unwrap();
            std::process::exit(73);
        }
    }
    #[test]
    fn journal_is_atomic_append_only_and_encrypted() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.ttteam");
        let mut cache = Cache::open(&path, "Disposable-test-1234", true)?;
        cache.snapshot("v", &[], 1_000_000)?;
        let mut r = Record::new(
            "host",
            json!({"label":"never plaintext","password":"not on disk"}),
        );
        cache.enqueue("v", r.clone(), false, 1_000_001, None, true)?;
        r.data["label"] = json!("second edit");
        cache.enqueue("v", r, false, 1_000_002, None, true)?;
        assert_eq!(cache.pending("v")?.len(), 2);
        drop(cache);
        assert!(!String::from_utf8_lossy(&std::fs::read(&path)?).contains("never plaintext"));
        assert!(Cache::open(&path, "wrong password", false).is_err());
        let cache = Cache::open(&path, "Disposable-test-1234", false)?;
        assert_eq!(cache.pending("v")?.len(), 2);
        Ok(())
    }
}
