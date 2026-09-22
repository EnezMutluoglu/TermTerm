use crate::{
    crypto::{self, Envelope},
    model::{Record, VaultInfo},
};
use anyhow::{anyhow, ensure, Context, Result};
use fs2::FileExt;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};
use uuid::Uuid;
use zeroize::Zeroizing;

pub struct Vault {
    pub conn: Connection,
    pub key: Zeroizing<[u8; 32]>,
    pub id: String,
    pub path: PathBuf,
    pub device_id: String,
    _lock: File,
}
#[derive(Serialize, Deserialize)]
pub struct Backup {
    pub version: u32,
    pub vaults: Vec<BackupVault>,
}
#[derive(Serialize, Deserialize)]
pub struct BackupVault {
    pub name: String,
    pub records: Vec<Record>,
}
#[derive(Serialize, Deserialize)]
struct BackupFile {
    magic: String,
    envelope: Envelope,
    payload: Vec<u8>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub op_id: String,
    pub record_id: String,
    pub base_revision: i64,
    pub deleted: bool,
    pub payload: Vec<u8>,
}

impl Vault {
    /// Recover/decrypt a disposable ciphertext copy; imports never write to the source.
    pub fn inspect(path: &Path, password: &str) -> Result<BackupVault> {
        ensure!(path.is_file(), "Vault file does not exist");
        let _source_lock = Self::lock(path)?;
        let directory = tempfile::tempdir()?;
        let copy = directory.path().join("snapshot.ttvault");
        std::fs::copy(path, &copy)?;
        let mut journal = path.as_os_str().to_os_string();
        journal.push("-journal");
        if Path::new(&journal).is_file() {
            std::fs::copy(journal, directory.path().join("snapshot.ttvault-journal"))?;
        }
        let v = Self::open(&copy, password)?;
        Ok(BackupVault {
            name: v.name()?,
            records: v.records()?,
        })
    }
    fn lock(path: &Path) -> Result<File> {
        let mut lockpath = path.as_os_str().to_os_string();
        lockpath.push(".lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lockpath)?;
        file.try_lock_exclusive()
            .context("This vault is already open in another application")?;
        Ok(file)
    }
    pub fn create(path: &Path, name: &str, password: &str) -> Result<Self> {
        ensure!(!name.trim().is_empty(), "Vault name is required");
        let key = crypto::random_key();
        let id = Uuid::new_v4().to_string();
        let envelope = Envelope::create(password, &key, id.as_bytes())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let lock = Self::lock(path)?;
        let reservation = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .context("The destination already exists or cannot be created")?;
        drop(reservation);
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA secure_delete=ON; CREATE TABLE header (id INTEGER PRIMARY KEY CHECK(id=1), version INTEGER NOT NULL, vault_id TEXT NOT NULL, envelope BLOB NOT NULL, name BLOB NOT NULL); CREATE TABLE records (id TEXT PRIMARY KEY, payload BLOB NOT NULL, server_revision INTEGER NOT NULL DEFAULT 0, deleted INTEGER NOT NULL DEFAULT 0); CREATE TABLE outbox (op_id TEXT PRIMARY KEY, record_id TEXT NOT NULL, base_revision INTEGER NOT NULL, deleted INTEGER NOT NULL, payload BLOB NOT NULL); CREATE TABLE state (key TEXT PRIMARY KEY, value TEXT NOT NULL);")?;
        conn.execute(
            "INSERT INTO header VALUES(1,1,?1,?2,?3)",
            params![
                id,
                serde_json::to_vec(&envelope)?,
                crypto::seal(&key, name.as_bytes(), b"vault-name")?
            ],
        )?;
        let device_id = Uuid::new_v4().to_string();
        conn.execute("INSERT INTO state VALUES('device_id',?1)", [&device_id])?;
        Ok(Self {
            conn,
            key,
            id,
            path: path.into(),
            device_id,
            _lock: lock,
        })
    }
    pub fn open(path: &Path, password: &str) -> Result<Self> {
        ensure!(path.is_file(), "Vault file does not exist");
        let lock = Self::lock(path)?;
        // Open read-only first: unknown versions and invalid passwords never modify the file.
        // A hot rollback journal cannot be read in SQLite read-only mode. Recover a
        // disposable ciphertext copy first; wrong passwords/newer formats still leave
        // both original files untouched. Only a verified password permits real recovery.
        let mut journal = path.as_os_str().to_os_string();
        journal.push("-journal");
        let journal = PathBuf::from(journal);
        let recovery = if journal.is_file() {
            Some(tempfile::tempdir()?)
        } else {
            None
        };
        let read = if let Some(dir) = &recovery {
            let copy = dir.path().join("recovery.ttvault");
            std::fs::copy(path, &copy)?;
            std::fs::copy(&journal, dir.path().join("recovery.ttvault-journal"))?;
            Connection::open(&copy)?
        } else {
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?
        };
        let (version, id, envelope): (u32, String, Vec<u8>) = read
            .query_row(
                "SELECT version,vault_id,envelope FROM header WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .context("This is not a TermTerm vault")?;
        ensure!(
            version == crypto::FORMAT_VERSION,
            "Unsupported vault version; file has not been changed"
        );
        let envelope: Envelope = serde_json::from_slice(&envelope)?;
        let key = envelope.unlock(password, id.as_bytes())?;
        let device_id: String =
            read.query_row("SELECT value FROM state WHERE key='device_id'", [], |r| {
                r.get(0)
            })?;
        drop(read);
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA synchronous=FULL; PRAGMA secure_delete=ON;")?;
        let vault = Self {
            conn,
            key,
            id,
            path: path.into(),
            device_id,
            _lock: lock,
        };
        vault.records()?;
        Ok(vault)
    }
    pub fn name(&self) -> Result<String> {
        let bytes: Vec<u8> =
            self.conn
                .query_row("SELECT name FROM header WHERE id=1", [], |r| r.get(0))?;
        Ok(String::from_utf8(
            crypto::unseal(&self.key, &bytes, b"vault-name")?.to_vec(),
        )?)
    }
    pub fn records(&self) -> Result<Vec<Record>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id,payload FROM records WHERE deleted=0 ORDER BY rowid")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        let mut all = Vec::new();
        for row in rows {
            let (id, bytes) = row?;
            all.push(serde_json::from_slice(&crypto::unseal(
                &self.key,
                &bytes,
                id.as_bytes(),
            )?)?);
        }
        Ok(all)
    }
    pub fn info(&self) -> Result<VaultInfo> {
        Ok(VaultInfo {
            id: self.id.clone(),
            name: self.name()?,
            path: self.path.to_string_lossy().into(),
            device_id: self.device_id.clone(),
            records: self.records()?,
        })
    }
    pub fn put(&mut self, records: &[Record]) -> Result<()> {
        self.put_checked(records, || Ok(()), || Ok(()))
    }
    pub fn put_checked(
        &mut self,
        records: &[Record],
        check: impl Fn() -> Result<()>,
        commit: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        for r in records {
            check()?;
            ensure!(Uuid::parse_str(&r.id).is_ok(), "Invalid record id");
            ensure!(
                [
                    "host",
                    "group",
                    "credential",
                    "snippet",
                    "workspace",
                    "tunnel",
                    "knownHost",
                    "log",
                    "settings",
                    "syncProfile",
                    "integration"
                ]
                .contains(&r.kind.as_str()),
                "Unknown record kind"
            );
            let payload = crypto::seal(&self.key, &serde_json::to_vec(r)?, r.id.as_bytes())?;
            let rev = tx
                .query_row(
                    "SELECT server_revision FROM records WHERE id=?1",
                    [&r.id],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
                .unwrap_or(0);
            tx.execute("INSERT INTO records(id,payload) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload,deleted=0",params![r.id,payload])?;
            tx.execute("DELETE FROM outbox WHERE record_id=?1", [&r.id])?;
            tx.execute(
                "INSERT INTO outbox VALUES(?1,?2,?3,0,?4)",
                params![Uuid::new_v4().to_string(), r.id, rev, payload],
            )?;
        }
        commit()?;
        tx.commit()?;
        Ok(())
    }
    pub fn delete(&mut self, ids: &[String]) -> Result<()> {
        let tx = self.conn.transaction()?;
        for id in ids {
            let rev = tx
                .query_row(
                    "SELECT server_revision FROM records WHERE id=?1",
                    [id],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
                .unwrap_or(0);
            let payload = crypto::seal(&self.key, b"null", id.as_bytes())?;
            tx.execute(
                "UPDATE records SET deleted=1,payload=?2 WHERE id=?1",
                params![id, payload],
            )?;
            tx.execute("DELETE FROM outbox WHERE record_id=?1", [id])?;
            tx.execute(
                "INSERT INTO outbox VALUES(?1,?2,?3,1,?4)",
                params![Uuid::new_v4().to_string(), id, rev, payload],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn portable_copy(&self, path: &Path) -> Result<()> {
        ensure!(!path.exists(), "Destination exists; choose a new file");
        let _copylock = Self::lock(path)?;
        let reservation = OpenOptions::new().create_new(true).write(true).open(path)?;
        drop(reservation);
        let mut dest = Connection::open(path)?;
        rusqlite::backup::Backup::new(&self.conn, &mut dest)?.run_to_completion(
            128,
            std::time::Duration::from_millis(1),
            None,
        )?;
        dest.execute(
            "UPDATE state SET value=?1 WHERE key='device_id'",
            [Uuid::new_v4().to_string()],
        )?;
        dest.execute("DELETE FROM outbox", [])?;
        Ok(())
    }
    pub fn backup(
        &self,
        path: &Path,
        password: &str,
        include_profiles: bool,
        ids: &[String],
    ) -> Result<()> {
        let (records, _) =
            crate::archive::portable_records(self.records()?, ids, include_profiles, false)?;
        write_backup(
            path,
            password,
            &Backup {
                version: 1,
                vaults: vec![BackupVault {
                    name: self.name()?,
                    records,
                }],
            },
        )
    }
    pub fn pending(&self) -> Result<Vec<Pending>> {
        let mut q = self.conn.prepare(
            "SELECT op_id,record_id,base_revision,deleted,payload FROM outbox ORDER BY rowid",
        )?;
        let rows = q.query_map([], |r| {
            Ok(Pending {
                op_id: r.get(0)?,
                record_id: r.get(1)?,
                base_revision: r.get(2)?,
                deleted: r.get(3)?,
                payload: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
    pub fn envelope(&self) -> Result<Vec<u8>> {
        Ok(self
            .conn
            .query_row("SELECT envelope FROM header WHERE id=1", [], |r| r.get(0))?)
    }
}
pub fn write_backup(path: &Path, password: &str, backup: &Backup) -> Result<()> {
    let key = crypto::random_key();
    let envelope = Envelope::create(password, &key, b"termterm-backup-v1")?;
    let content = serde_json::to_vec(&BackupFile {
        magic: "TERMTTERM_BACKUP".into(),
        envelope,
        payload: crypto::seal(&key, &serde_json::to_vec(backup)?, b"termterm-backup-v1")?,
    })?;
    use std::io::Write;
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .context("Destination exists or cannot be created")?;
    f.write_all(&content)?;
    f.sync_all()?;
    Ok(())
}
pub fn read_backup(path: &Path, password: &str) -> Result<Backup> {
    ensure!(
        std::fs::metadata(path)?.len() < 512 * 1024 * 1024,
        "Backup exceeds 512 MB import limit"
    );
    let file: BackupFile =
        serde_json::from_slice(&std::fs::read(path)?).context("Invalid or truncated backup")?;
    ensure!(file.magic == "TERMTTERM_BACKUP", "Not a TermTerm backup");
    let key = file.envelope.unlock(password, b"termterm-backup-v1")?;
    let backup: Backup =
        serde_json::from_slice(&crypto::unseal(&key, &file.payload, b"termterm-backup-v1")?)?;
    ensure!(backup.version == 1, "Unsupported backup version");
    Ok(backup)
}
pub fn restored_records(records: &[Record]) -> Result<Vec<Record>> {
    let mapping: std::collections::HashMap<String, String> = records
        .iter()
        .map(|r| (r.id.clone(), Uuid::new_v4().to_string()))
        .collect();
    fn replace(v: &mut serde_json::Value, map: &std::collections::HashMap<String, String>) {
        match v {
            serde_json::Value::String(s) => {
                if let Some(new) = map.get(s) {
                    *s = new.clone();
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(|v| replace(v, map)),
            serde_json::Value::Object(o) => o.values_mut().for_each(|v| replace(v, map)),
            _ => {}
        }
    }
    records
        .iter()
        .map(|r| {
            let mut r = r.clone();
            r.id = mapping
                .get(&r.id)
                .ok_or_else(|| anyhow!("Invalid reference"))?
                .clone();
            for field in [
                "groupId",
                "credentialId",
                "hostId",
                "chain",
                "hostIds",
                "inventoryGroups",
            ] {
                if let Some(value) = r.data.get_mut(field) {
                    replace(value, &mapping);
                }
            }
            if r.kind == "syncProfile" {
                r.data["enabled"] = false.into();
            }
            Ok(r)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn interrupted_write_and_full_disk_preserve_last_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("durable.ttvault");
        let mut v = Vault::create(&path, "Durability", "test-password").unwrap();
        let original = Record::new("host", json!({"label":"Committed"}));
        v.put(&[original.clone()]).unwrap();
        let pages: i64 = v
            .conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        v.conn
            .execute_batch(&format!("PRAGMA max_page_count={pages}"))
            .unwrap();
        let huge = Record::new(
            "snippet",
            json!({"label":"Cannot fit","command":"x".repeat(1024*1024)}),
        );
        assert!(v.put(&[huge]).is_err());
        assert_eq!(v.records().unwrap(), vec![original.clone()]);
        drop(v);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "vault::tests::crash_child", "--ignored"])
            .env("TERMTERM_TEST_CRASH_VAULT", &path)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(73));
        let before = std::fs::read(&path).unwrap();
        let mut journal = path.as_os_str().to_os_string();
        journal.push("-journal");
        let journal = PathBuf::from(journal);
        let before_journal = std::fs::read(&journal).unwrap();
        assert!(Vault::open(&path, "incorrect").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read(&journal).unwrap(), before_journal);
        let reopened = Vault::open(&path, "test-password").unwrap();
        assert_eq!(reopened.records().unwrap(), vec![original]);
    }
    #[test]
    #[ignore = "Helper invoked by durability test only"]
    fn crash_child() {
        if let Ok(path) = std::env::var("TERMTERM_TEST_CRASH_VAULT") {
            let v = Vault::open(Path::new(&path), "test-password").unwrap();
            v.conn.execute_batch("PRAGMA cache_size=1; BEGIN IMMEDIATE; UPDATE records SET payload=zeroblob(1000000)").unwrap();
            std::process::exit(73);
        }
    }
    #[test]
    fn truncated_backup_and_newer_backup_version_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("backup.ttbackup");
        write_backup(
            &p,
            "test-password",
            &Backup {
                version: 999,
                vaults: vec![],
            },
        )
        .unwrap();
        let data = std::fs::read(&p).unwrap();
        assert!(read_backup(&p, "test-password").is_err());
        assert_eq!(std::fs::read(&p).unwrap(), data);
        for len in [0, 1, 20, data.len() / 2, data.len() - 1] {
            let cut = dir.path().join(format!("cut-{len}"));
            std::fs::write(&cut, &data[..len]).unwrap();
            assert!(read_backup(&cut, "test-password").is_err());
        }
    }
    #[test]
    fn encryption_roundtrip_backup_and_lock() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.ttvault");
        let mut v = Vault::create(&p, "Private Vault", "sufficient-password").unwrap();
        let r = Record::new(
            "host",
            json!({"label":"SecretProduction","password":"ultrasecret","address":"10.50.60.70"}),
        );
        v.put(&[r.clone()]).unwrap();
        assert!(Vault::open(&p, "sufficient-password").is_err());
        let bytes = std::fs::read(&p).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("ultrasecret"));
        assert!(!String::from_utf8_lossy(&bytes).contains("SecretProduction"));
        let backup = dir.path().join("a.ttbackup");
        v.backup(&backup, "backup-password", true, &[]).unwrap();
        assert_eq!(
            read_backup(&backup, "backup-password").unwrap().vaults[0].records[0],
            r
        );
        assert!(read_backup(&backup, "incorrect").is_err());
        let copy = dir.path().join("portable.ttvault");
        v.portable_copy(&copy).unwrap();
        let c = Vault::open(&copy, "sufficient-password").unwrap();
        assert_eq!(c.records().unwrap(), vec![r.clone()]);
        assert_ne!(c.device_id, v.device_id);
        drop(c);
        drop(v);
        assert!(Vault::open(&p, "incorrect").is_err());
        let reopened = Vault::open(&p, "sufficient-password").unwrap();
        assert_eq!(reopened.records().unwrap(), vec![r]);
    }
    #[test]
    fn tamper_and_future_version_leave_source_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.ttvault");
        let v = Vault::create(&p, "A", "password123").unwrap();
        v.conn.execute("UPDATE header SET version=999", []).unwrap();
        drop(v);
        let before = std::fs::read(&p).unwrap();
        assert!(Vault::open(&p, "password123").is_err());
        assert_eq!(before, std::fs::read(&p).unwrap());
        let key = crypto::random_key();
        let mut encrypted = crypto::seal(&key, b"hello", b"id").unwrap();
        encrypted[28] ^= 1;
        assert!(crypto::unseal(&key, &encrypted, b"id").is_err());
    }
}
