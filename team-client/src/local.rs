//! Identity-bound local state. Values are authenticated ciphertext; SQLite keeps
//! a completed transaction on interruption. This is NOT a portable vault format.
use crate::{
    keys::{self, Identity},
    vault_crypto as crypto,
};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};
use zeroize::Zeroizing;

mod connections;
pub mod journal;

pub struct Store {
    db: Connection,
    key: Zeroizing<[u8; 32]>,
    context: String,
}
pub struct Entry {
    pub revision: i64,
    pub bytes: Zeroizing<Vec<u8>>,
}
impl Store {
    pub(crate) fn forget_cached_auth(&mut self) -> Result<()> {
        self.db
            .execute("DELETE FROM entries WHERE name LIKE 'login-auth:%'", [])?;
        Ok(())
    }
    pub fn open(root: &Path, scope: &str, identity: &Identity) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        // Key rotation starts an independent cache. The old encrypted file is
        // preserved for recovery, never silently opened under a different key.
        let context = format!("termterm-local-v2:{scope}:{}", identity.public_key());
        let name = hex::encode(Sha256::digest(context.as_bytes()));
        let db = Connection::open(root.join(format!("{name}.ttcache")))?;
        db.busy_timeout(Duration::from_secs(2))?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        ensure!(
            (0..=8).contains(&version),
            "Unsupported Team cache version; file not changed"
        );
        db.execute_batch("PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")?;
        let mut result = Self {
            db,
            key: crypto::random_key(),
            context,
        };
        let tx = result
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Recheck inside the write lock so two processes can initialize safely.
        let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        ensure!((0..=8).contains(&version), "Unsupported Team cache version");
        if version == 0 {
            let tables: i64 = tx.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )?;
            ensure!(tables == 0, "Unrecognized Team cache; file not changed");
            tx.execute_batch("CREATE TABLE header (id INTEGER PRIMARY KEY CHECK(id=1), envelope TEXT NOT NULL);
                CREATE TABLE entries (name TEXT PRIMARY KEY, revision INTEGER NOT NULL CHECK(revision>0), ciphertext BLOB NOT NULL);
                PRAGMA user_version=1;")?;
            let envelope = keys::wrap(&result.key, &identity.public_key(), &result.context)?;
            tx.execute("INSERT INTO header VALUES(1,?1)", [envelope])?;
        } else {
            let envelope: String =
                tx.query_row("SELECT envelope FROM header WHERE id=1", [], |r| r.get(0))?;
            result.key = keys::unwrap(identity, &envelope, &result.context)?;
        }
        if version < 2 {
            journal::migrate(&tx)?;
        }
        if version < 3 {
            journal::migrate_settled(&tx)?;
        }
        if version < 5 {
            // v5 includes scoped offline creation receipts in addition to
            // offline login/connection entries. Older apps must not write them.
            tx.execute_batch("PRAGMA user_version=5;")?;
        }
        if version < 6 { tx.execute_batch("PRAGMA user_version=6;")?; }
        if version < 8 { tx.execute_batch("PRAGMA user_version=8;")?; }
        tx.commit()?;
        Ok(result)
    }
    pub fn read(&self, name: &str) -> Result<Entry> {
        let row: Option<(i64, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT revision,ciphertext FROM entries WHERE name=?1",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            None => Ok(Entry {
                revision: 0,
                bytes: Zeroizing::new(Vec::new()),
            }),
            Some((revision, ciphertext)) => Ok(Entry {
                revision,
                bytes: crypto::unseal(
                    &self.key,
                    &ciphertext,
                    format!("{}:{name}:{revision}", self.context).as_bytes(),
                )?,
            }),
        }
    }
    /// CAS is shared by separate processes, not only the in-process mutex.
    pub fn replace(&mut self, name: &str, expected: i64, bytes: &[u8]) -> Result<bool> {
        ensure!(
            name.len() <= 100 && bytes.len() <= 8 * 1024 * 1024,
            "Cache entry too large"
        );
        let revision = expected
            .checked_add(1)
            .context("Cache revision exhausted")?;
        ensure!(expected >= 0, "Invalid cache revision");
        let ciphertext = crypto::seal(
            &self.key,
            bytes,
            format!("{}:{name}:{revision}", self.context).as_bytes(),
        )?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: i64 = tx
            .query_row("SELECT revision FROM entries WHERE name=?1", [name], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or(0);
        if current != expected {
            return Ok(false);
        }
        tx.execute("INSERT INTO entries VALUES(?1,?2,?3) ON CONFLICT(name) DO UPDATE SET revision=excluded.revision,ciphertext=excluded.ciphertext",
            params![name, revision, ciphertext])?;
        tx.commit()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_reopen_isolation_cas_and_tamper() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let identity = Identity::generate();
        let mut first = Store::open(dir.path(), "https://one/team/account", &identity)?;
        let mut second = Store::open(dir.path(), "https://one/team/account", &identity)?;
        let content = b"private-host.example ssh-ed25519 known-host-secret-marker";
        assert!(first.replace("known-hosts", 0, content)?);
        assert!(!second.replace("known-hosts", 0, b"lost update")?);
        assert_eq!(&*second.read("known-hosts")?.bytes, content);
        drop(first);
        drop(second);
        for file in std::fs::read_dir(dir.path())? {
            assert!(!std::fs::read(file?.path())?
                .windows(content.len())
                .any(|w| w == content));
        }
        let again = Store::open(dir.path(), "https://one/team/account", &identity)?;
        assert_eq!(again.read("known-hosts")?.revision, 1);
        assert_eq!(
            Store::open(dir.path(), "https://other/team/account", &identity)?
                .read("known-hosts")?
                .revision,
            0
        );
        assert_eq!(
            Store::open(
                dir.path(),
                "https://one/team/account",
                &Identity::generate()
            )?
            .read("known-hosts")?
            .revision,
            0
        );
        again
            .db
            .execute("UPDATE entries SET ciphertext=zeroblob(64)", [])?;
        assert!(again.read("known-hosts").is_err());
        again.db.execute_batch("PRAGMA user_version=999")?;
        assert!(Store::open(dir.path(), "https://one/team/account", &identity).is_err());
        assert_eq!(
            again
                .db
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?,
            999
        );
        Ok(())
    }
    #[test]
    fn copying_another_scope_cache_is_rejected() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let identity = Identity::generate();
        let a = Store::open(dir.path(), "scope-a", &identity)?;
        let b = Store::open(dir.path(), "scope-b", &identity)?;
        let wrong: String =
            a.db.query_row("SELECT envelope FROM header", [], |r| r.get(0))?;
        b.db.execute("UPDATE header SET envelope=?1", [wrong])?;
        drop(b);
        assert!(Store::open(dir.path(), "scope-b", &identity).is_err());
        Ok(())
    }
}
