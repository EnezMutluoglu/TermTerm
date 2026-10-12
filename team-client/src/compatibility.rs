//! Public compatibility metadata only. Kept separately from encrypted user data
//! so a known mandatory update cannot be bypassed by restarting offline.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const API_VERSION: u64 = 1;
pub const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub fn satisfies_minimum(version: &str, minimum: &str) -> Result<bool> {
    Ok(semver::Version::parse(version)?
        .cmp_precedence(&semver::Version::parse(minimum)?)
        .is_ge())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    #[serde(default)]
    pub revision: u64,
    pub api_version: u64,
    pub minimum_client_version: String,
    pub service_version: String,
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.api_version > 0
                && self.minimum_client_version.len() <= 80
                && self.service_version.len() <= 80,
            "Invalid Team compatibility contract"
        );
        semver::Version::parse(&self.minimum_client_version)?;
        semver::Version::parse(&self.service_version)?;
        Ok(())
    }
    pub fn required(&self) -> Result<bool> {
        self.validate()?;
        Ok(self.api_version != API_VERSION
            || semver::Version::parse(CLIENT_VERSION)?
                .cmp_precedence(&semver::Version::parse(&self.minimum_client_version)?)
                .is_lt())
    }
    pub fn enforce(&self) -> Result<()> {
        ensure!(
            !self.required()?,
            "team_update_required:{}:{}",
            self.minimum_client_version,
            self.api_version
        );
        Ok(())
    }
}

#[derive(Clone)]
pub struct PolicyStore(PathBuf);
impl PolicyStore {
    pub fn new(root: &Path, origin: &str) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        Ok(Self(root.join(format!(
            "{}.ttpolicy",
            hex::encode(Sha256::digest(origin.as_bytes()))
        ))))
    }
    fn db(&self) -> Result<rusqlite::Connection> {
        let db = rusqlite::Connection::open(&self.0)?;
        db.busy_timeout(std::time::Duration::from_secs(3))?;
        // Atomic SQLite commits, no profile names, bearer tokens or user data.
        db.execute_batch("PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS policy (id INTEGER PRIMARY KEY CHECK(id=1), document TEXT NOT NULL);")?;
        Ok(db)
    }
    pub fn read(&self) -> Result<Option<Policy>> {
        use rusqlite::OptionalExtension;
        let data: Option<String> = self
            .db()?
            .query_row("SELECT document FROM policy WHERE id=1", [], |r| r.get(0))
            .optional()?;
        data.map(|d| {
            let policy: Policy = serde_json::from_str(&d).context("Invalid stored Team policy")?;
            policy.validate()?;
            Ok(policy)
        })
        .transpose()
    }
    pub fn save(&self, policy: &Policy) -> Result<Policy> {
        policy.validate()?;
        use rusqlite::OptionalExtension;
        let mut db = self.db()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let previous: Option<String> = tx
            .query_row("SELECT document FROM policy WHERE id=1", [], |r| r.get(0))
            .optional()?;
        let mut selected = policy.clone();
        if let Some(previous) = previous {
            let previous: Policy = serde_json::from_str(&previous)?;
            previous.validate()?;
            // An older in-flight response must not undo a newly learned denial.
            // An explicit administrator policy change has a higher revision.
            if previous.revision > policy.revision
                || (previous.revision == policy.revision
                    && (previous.api_version > policy.api_version
                        || semver::Version::parse(&previous.minimum_client_version)?
                            .cmp_precedence(&semver::Version::parse(
                                &policy.minimum_client_version,
                            )?)
                            .is_gt()))
            {
                selected = previous;
            }
        }
        tx.execute("INSERT INTO policy VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET document=excluded.document",[serde_json::to_string(&selected)?])?;
        tx.commit()?;
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_upgrade_survives_restart_without_touching_user_journal() -> Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(
            dir.path().join("pending.ttcache"),
            b"encrypted journal fixture",
        )?;
        let policy = Policy {
            revision: 2,
            api_version: 1,
            minimum_client_version: "0.4.2".into(),
            service_version: "0.4.2".into(),
        };
        PolicyStore::new(dir.path(), "https://one/")?.save(&policy)?;
        assert!(PolicyStore::new(dir.path(), "https://one/")?
            .read()?
            .unwrap()
            .enforce()
            .is_err());
        assert!(PolicyStore::new(dir.path(), "https://two/")?
            .read()?
            .is_none());
        assert_eq!(
            std::fs::read(dir.path().join("pending.ttcache"))?,
            b"encrypted journal fixture"
        );
        let stale = Policy {
            revision: 1,
            api_version: 1,
            minimum_client_version: CLIENT_VERSION.into(),
            service_version: CLIENT_VERSION.into(),
        };
        assert!(PolicyStore::new(dir.path(), "https://one/")?
            .save(&stale)?
            .required()?);
        let administrator_change = Policy {
            revision: 3,
            ..stale
        };
        assert!(!PolicyStore::new(dir.path(), "https://one/")?
            .save(&administrator_change)?
            .required()?);
        Ok(())
    }
    #[test]
    fn semantic_and_protocol_gate() -> Result<()> {
        for (version, api, required) in [
            ("0.4.0", 1, false),
            (CLIENT_VERSION, 1, false),
            ("0.4.1-dev.10", 1, false),
            ("0.4.1", 1, false),
            ("0.4.2", 1, true),
            ("0.4.0", 2, true),
        ] {
            assert_eq!(
                Policy {
                    revision: 0,
                    api_version: api,
                    minimum_client_version: version.into(),
                    service_version: CLIENT_VERSION.into()
                }
                .required()?,
                required
            );
        }
        Ok(())
    }
}
