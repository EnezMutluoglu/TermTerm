//! Password-protected login index. Bearer tokens exist only inside the encrypted
//! .ttcache, never in this index, frontend responses, command lines or logs.
use crate::{
    keys::{Identity, IdentityFile},
    local::Store,
    transport::{ServiceProfile, SessionView, TeamClient},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    format: u32,
    account: Uuid,
    identity: IdentityFile,
}
pub(crate) fn binding(profile: &ServiceProfile, username: &str) -> Result<String> {
    let origin = profile.cache_scope(Uuid::nil())?;
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        origin,
        username.trim().to_lowercase(),
        &profile.ca_pem,
    ))?)))
}
fn path(root: &Path, binding: &str) -> PathBuf {
    root.join(format!("login-{binding}.ttlogin"))
}
pub(crate) fn save(
    root: &Path,
    binding: &str,
    client: &TeamClient,
    store: &mut Store,
) -> Result<()> {
    let account = client.actor()?.1;
    persist(binding, client, store)?;
    let bytes = serde_json::to_vec(&Index {
        format: 1,
        account,
        identity: client.protected_identity()?,
    })?;
    std::fs::create_dir_all(root)?;
    let temp = root.join(format!("login-{}.tmp", Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options.open(&temp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&temp, path(root, binding))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
pub(crate) fn persist(binding: &str, client: &TeamClient, store: &mut Store) -> Result<()> {
    let name = format!("login-auth:{binding}");
    let old = store.read(&name)?;
    let bytes = client.cached_auth(binding)?;
    if *old.bytes != *bytes {
        ensure!(
            store.replace(&name, old.revision, &bytes)?,
            "Cached session changed; sign in again"
        );
    }
    Ok(())
}
pub(crate) fn open(
    root: &Path,
    binding: &str,
    profile: &ServiceProfile,
    username: &str,
    password: &str,
    now: i64,
) -> Result<(TeamClient, SessionView, Identity, Store)> {
    let (account, identity, mut store) = open_delivery(root, binding, profile, username, password)?;
    let auth = store.read(&format!("login-auth:{binding}"))?;
    ensure!(
        auth.revision > 0,
        "Offline login unavailable; sign in online again"
    );
    let mut client = TeamClient::new(profile.clone())?;
    let device = store.journal_device(now)?;
    let view =
        client.restore_cached_auth(&auth.bytes, binding, account, device, password, &identity)?;
    Ok((client, view, identity, store))
}
pub(crate) fn open_delivery(
    root: &Path,
    binding: &str,
    profile: &ServiceProfile,
    username: &str,
    password: &str,
) -> Result<(Uuid, Identity, Store)> {
    ensure!(
        self::binding(profile, username)? == binding,
        "Login scope mismatch"
    );
    let file = path(root, binding);
    ensure!(
        std::fs::metadata(&file)
            .context("İlk Team girişi çevrimiçi yapılmalı")?
            .len()
            <= 2 * 1024 * 1024,
        "Login index too large"
    );
    let index: Index = serde_json::from_slice(&std::fs::read(file)?)?;
    ensure!(
        index.format == 1,
        "Unsupported Team login cache; file not changed"
    );
    let identity = index
        .identity
        .unlock(profile.team_id, index.account, password)?;
    let store = Store::open(root, &profile.cache_scope(index.account)?, &identity)?;
    Ok((index.account, identity, store))
}
