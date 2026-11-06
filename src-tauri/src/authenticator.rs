//! Personal TOTP state is encrypted, outside generic record listings and sync.
use crate::{crypto, state::Shared, vault::Vault};
use anyhow::{ensure, Context, Result};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{Manager, State};
use termterm_team_client::authenticator::{self, Action, Snapshot, Token};
use zeroize::Zeroizing;

const STORAGE_KEY: &str = "authenticator-v1";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub revision: i64,
    pub token: Token,
}
pub fn load(vault: &Vault) -> Result<Vec<Entry>> {
    let payload: Option<String> = vault
        .conn
        .query_row("SELECT value FROM state WHERE key=?1", [STORAGE_KEY], |r| {
            r.get(0)
        })
        .optional()?;
    let entries: Vec<Entry> = match payload {
        Some(value) => serde_json::from_slice(&crypto::unseal(
            &vault.key,
            &hex::decode(value)?,
            STORAGE_KEY.as_bytes(),
        )?)?,
        None => vec![],
    };
    validate(&entries)?;
    Ok(entries)
}
pub(crate) fn validate(entries: &[Entry]) -> Result<()> {
    ensure!(entries.len() <= 500, "Authenticator limit is 500 tokens");
    let mut ids = std::collections::HashSet::new();
    for entry in entries {
        ensure!(
            uuid::Uuid::parse_str(&entry.id).is_ok() && ids.insert(&entry.id) && entry.revision > 0,
            "Invalid Authenticator entries"
        );
        entry.token.validate()?;
    }
    Ok(())
}
pub fn restore(vault: &mut Vault, entries: &[Entry]) -> Result<()> {
    validate(entries)?;
    ensure!(
        load(vault)?.is_empty(),
        "Authenticator destination is not empty"
    );
    if entries.is_empty() {
        return Ok(());
    }
    save(vault, entries)
}
fn save(vault: &mut Vault, entries: &[Entry]) -> Result<()> {
    validate(entries)?;
    let plain = Zeroizing::new(serde_json::to_vec(entries)?);
    let sealed = hex::encode(crypto::seal(&vault.key, &plain, STORAGE_KEY.as_bytes())?);
    let tx = vault.conn.transaction()?;
    tx.execute("INSERT INTO state(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",rusqlite::params![STORAGE_KEY,sealed])?;
    if !entries.is_empty() {
        tx.execute("UPDATE header SET version=MAX(version,3) WHERE id=1", [])?;
    }
    tx.commit()?;
    Ok(())
}
pub fn personal(vault: &mut Vault, context: Option<&str>, action: Action) -> Result<Value> {
    let scope = format!("personal:{}:{}", vault.id, vault.authenticator_context);
    ensure!(
        context == Some(scope.as_str()) || (context.is_none() && matches!(action, Action::List)),
        "Vault changed; reopen Authenticator"
    );
    let mut entries = load(vault)?;
    match action {
        Action::List => {}
        Action::Add { input } => {
            let token = Token::parse(&input)?;
            ensure!(entries.len() < 500, "Authenticator limit is 500 tokens");
            entries.push(Entry {
                id: uuid::Uuid::new_v4().to_string(),
                revision: 1,
                token,
            });
            save(vault, &entries)?;
        }
        Action::Delete { id, revision } => {
            let index = entries
                .iter()
                .position(|e| e.id == id && e.revision == revision)
                .context("Token changed or was removed")?;
            entries.remove(index);
            save(vault, &entries)?;
        }
        Action::Reveal { id } => {
            let entry = entries
                .iter()
                .find(|e| e.id == id)
                .context("Token not found")?;
            return Ok(json!({"uri":entry.token.uri()?.as_str()}));
        }
    }
    let now = authenticator::now()?;
    Ok(serde_json::to_value(Snapshot {
        context: scope,
        entries: entries
            .iter()
            .map(|e| e.token.view(e.id.clone(), e.revision, now))
            .collect::<Result<_>>()?,
        server_time: now,
    })?)
}
#[tauri::command]
pub async fn authenticator(
    app: tauri::AppHandle,
    state: State<'_, Shared>,
    context: Option<String>,
    action: Action,
) -> std::result::Result<Value, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Value> {
        ensure!(
            !app.state::<termterm_team_client::desktop::DesktopState>()
                .active(),
            "Use the Team service Authenticator for Team accounts"
        );
        ensure!(
            !crate::team::active(&state),
            "Use the Team service Authenticator for Team accounts"
        );
        let mut guard = state
            .vault
            .lock()
            .map_err(|_| anyhow::anyhow!("Vault lock"))?;
        let result = personal(
            guard.as_mut().context("Unlock a vault first")?,
            context.as_deref(),
            action,
        )?;
        ensure!(
            !app.state::<termterm_team_client::desktop::DesktopState>()
                .active(),
            "Team session changed; reopen Authenticator"
        );
        Ok(result)
    })
    .await
    .map_err(|_| "Authenticator operation failed".to_string())?
    .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_reopen_context_redaction_and_revision_guard() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ttvault");
        let mut v = Vault::create(&path, "Disposable", "Disposable-password").unwrap();
        let snapshot = personal(&mut v, None, Action::List).unwrap();
        let context = snapshot["context"].as_str().unwrap();
        let action = || {
            serde_json::from_value::<Action>(json!({"kind":"add","input":{"provisioning":"GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ","label":"RFC fixture","issuer":"Tests","algorithm":"SHA1","digits":6,"period":30}})).unwrap()
        };
        assert!(personal(&mut v, Some("wrong"), action()).is_err());
        v.conn.execute_batch("CREATE TRIGGER reject_authenticator_upgrade BEFORE UPDATE OF version ON header BEGIN SELECT RAISE(ABORT,'fixture upgrade failure'); END;").unwrap();
        assert!(personal(&mut v, Some(context), action()).is_err());
        assert!(load(&v).unwrap().is_empty());
        assert_eq!(
            v.conn
                .query_row("SELECT version FROM header", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            1
        );
        v.conn
            .execute_batch("DROP TRIGGER reject_authenticator_upgrade")
            .unwrap();
        let result = personal(&mut v, Some(context), action()).unwrap();
        let version = v
            .conn
            .query_row("SELECT version FROM header", [], |r| r.get::<_, u32>(0))
            .unwrap();
        assert_eq!(version, 3);
        assert!(
            ![1, 2].contains(&version),
            "v1/v2 clients must reject before opening read/write"
        );
        let id = result["entries"][0]["id"].as_str().unwrap().to_string();
        let response = serde_json::to_string(&result).unwrap();
        assert!(!response.contains("GEZDG"));
        assert!(!response.contains("secret"));
        assert!(!response.contains("otpauth"));
        assert!(v.records().unwrap().is_empty());
        assert!(v.pending().unwrap().is_empty());
        assert!(personal(
            &mut v,
            Some(context),
            Action::Delete {
                id: id.clone(),
                revision: 2
            }
        )
        .is_err());
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes
            .windows(32)
            .any(|w| w == b"GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"));
        drop(v);
        let mut v = Vault::open(&path, "Disposable-password").unwrap();
        assert!(personal(&mut v, Some(context), Action::Reveal { id: id.clone() }).is_err());
        let reopened = personal(&mut v, None, Action::List).unwrap();
        assert_eq!(reopened["entries"][0]["id"], id);
        let context = reopened["context"].as_str().unwrap();
        assert!(
            personal(&mut v, Some(context), Action::Reveal { id: id.clone() }).unwrap()["uri"]
                .as_str()
                .unwrap()
                .contains("otpauth://totp")
        );
        assert!(
            personal(&mut v, Some(context), Action::Delete { id, revision: 1 }).unwrap()["entries"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn encrypted_backup_copy_restore_and_legacy_formats() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.ttvault");
        let mut v = Vault::create(&path, "Disposable", "Disposable-password").unwrap();
        let action=serde_json::from_value::<Action>(json!({"kind":"add","input":{"provisioning":"GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ","label":"RFC fixture","issuer":"Tests","algorithm":"SHA1","digits":6,"period":30}})).unwrap();
        let snapshot = personal(&mut v, None, Action::List).unwrap();
        let context = snapshot["context"].as_str().unwrap();
        personal(&mut v, Some(context), action).unwrap();
        let profile = crate::model::Record::new(
            "vpn",
            json!({"version":1,"label":"Disposable","family":"fortinet","source":"manual","server":"https://vpn.example.test","groupId":null,"username":"","password":"","realm":"","config":"","remember":false}),
        );
        crate::vpn::restore_profiles(&mut v, &[profile]).unwrap();
        assert_eq!(
            v.conn
                .query_row("SELECT version FROM header", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            3,
            "VPN must not downgrade an Authenticator vault"
        );
        let before = std::fs::read(&path).unwrap();
        let backup = dir.path().join("full.ttbackup");
        v.backup(&backup, "Backup-password", true, &[]).unwrap();
        let payload = crate::vault::read_backup(&backup, "Backup-password").unwrap();
        assert_eq!(payload.version, 3);
        assert_eq!(payload.vaults[0].authenticator.len(), 1);
        let bytes = std::fs::read(&backup).unwrap();
        assert!(!bytes
            .windows(32)
            .any(|w| w == b"GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"));
        assert!(crate::vault::read_backup(&backup, "wrong").is_err());
        let generic =
            crate::archive::preview(&backup, "ttbackup", "Backup-password", None).unwrap();
        assert!(generic.records.is_empty());
        assert!(generic.warnings.iter().any(|s| s.contains("Authenticator")));
        assert_eq!(before, std::fs::read(&path).unwrap());
        let mut restored = Vault::create(
            &dir.path().join("restored.ttvault"),
            "Restored",
            "New-password",
        )
        .unwrap();
        restore(&mut restored, &[]).unwrap();
        assert_eq!(
            restored
                .conn
                .query_row("SELECT version FROM header", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            1
        );
        restore(&mut restored, &payload.vaults[0].authenticator).unwrap();
        assert!(restore(&mut restored, &payload.vaults[0].authenticator).is_err());
        assert_eq!(
            load(&restored).unwrap()[0].token.code(59).unwrap(),
            "287082"
        );
        let copy = dir.path().join("copy.ttvault");
        v.portable_copy(&copy).unwrap();
        let copy = Vault::open(&copy, "Disposable-password").unwrap();
        assert_eq!(load(&copy).unwrap()[0].token.code(59).unwrap(), "287082");
        for version in [1, 2] {
            let legacy = dir.path().join(format!("v{version}.ttbackup"));
            crate::vault::write_backup(
                &legacy,
                "Backup-password",
                &crate::vault::Backup {
                    version,
                    vaults: vec![crate::vault::BackupVault {
                        name: "Legacy".into(),
                        records: vec![],
                        vpn_profiles: vec![],
                        authenticator: vec![],
                    }],
                },
            )
            .unwrap();
            let parsed = crate::vault::read_backup(&legacy, "Backup-password").unwrap();
            assert_eq!(parsed.version, version);
            assert!(parsed.vaults[0].authenticator.is_empty());
        }
    }
}
