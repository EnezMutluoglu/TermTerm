//! Private encrypted cache representation; never used as a desktop IPC type.
use super::*;
use crate::{
    chain::ConnectionPlan,
    connection::{Authentication, KeySecret, Target},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedPlan {
    format: u32,
    target: Uuid,
    steps: Vec<SavedStep>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedStep {
    id: Uuid,
    #[serde(default = "ssh_protocol")]
    protocol: String,
    label: String,
    address: String,
    port: u16,
    username: Option<String>,
    method: String,
    password: String,
    key: Option<KeySecret>,
    #[serde(default)]
    agent: Option<crate::connection::AgentIdentity>,
    #[serde(default)]
    proxy: Option<crate::resources::ProxyConfig>,
    #[serde(default)]
    proxy_auth: Option<crate::proxy::ProxySecret>,
}
fn ssh_protocol() -> String {
    "ssh".into()
}
impl Drop for SavedStep {
    fn drop(&mut self) {
        self.password.zeroize();
        self.username.zeroize();
    }
}
/// Runs inside the same transaction as a resource change or explicit denial.
/// A dependency can occur in several roots; deleting only connection:{id}
/// would leave a usable stale secret in the other cached routes.
pub(super) fn invalidate_for(
    tx: &rusqlite::Transaction<'_>,
    key: &[u8; 32],
    context: &str,
    id: Uuid,
) -> Result<()> {
    let rows = {
        let mut q = tx.prepare(
            "SELECT name,revision,ciphertext FROM entries WHERE name LIKE 'connection:%'",
        )?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    for (name, revision, ciphertext) in rows {
        let decoded = (|| -> Result<SavedPlan> {
            let bytes = crypto::unseal(
                key,
                &ciphertext,
                format!("{context}:{name}:{revision}").as_bytes(),
            )?;
            Ok(serde_json::from_slice(&bytes)?)
        })();
        // Corrupt cache entries cannot prevent revocation; discard them.
        if decoded.is_err()
            || decoded.is_ok_and(|s| {
                s.format != 1 || s.target == id || s.steps.iter().any(|s| s.id == id)
            })
        {
            tx.execute("DELETE FROM entries WHERE name=?1", [name])?;
        }
    }
    Ok(())
}

pub(super) fn same_endpoint(
    a: &crate::resources::ResourceContent,
    b: &crate::resources::ResourceContent,
) -> bool {
    a.kind == b.kind
        && a.parent_id == b.parent_id
        && a.address == b.address
        && a.protocol == b.protocol
        && a.port == b.port
        && a.username_policy == b.username_policy
        && a.username == b.username
        && a.proxy == b.proxy
        && a.deleted == b.deleted
}
impl Store {
    /// Called only after online authorization AND per-hop identity resolution.
    pub(crate) fn save_connection(&mut self, plan: &ConnectionPlan) -> Result<()> {
        let mut saved = SavedPlan {
            format: 1,
            target: plan.target_id,
            steps: Vec::new(),
        };
        for step in &plan.steps {
            let (method, password, key) = match &step.authentication {
                Authentication::Agent(_) => ("agent", String::new(), None),
                Authentication::Prompt => ("prompt", String::new(), None),
                Authentication::KeyboardInteractive => {
                    ("keyboard_interactive", String::new(), None)
                }
                Authentication::Password(p) => ("password", p.to_string(), None),
                Authentication::Key(k) => (
                    "key",
                    String::new(),
                    Some(KeySecret {
                        format: k.format,
                        private_key: k.private_key.clone(),
                        passphrase: k.passphrase.clone(),
                        certificate: k.certificate.clone(),
                    }),
                ),
            };
            saved.steps.push(SavedStep {
                proxy: step.proxy.clone(),
                proxy_auth: step.proxy_auth.clone(),
                id: step.id,
                protocol: step.protocol.clone(),
                label: step.label.clone(),
                address: step.address.clone(),
                port: step.port,
                username: step.username.clone(),
                method: method.into(),
                password,
                key,
                agent: match &step.authentication {
                    Authentication::Agent(a) => Some(a.clone()),
                    _ => None,
                },
            });
        }
        let bytes = Zeroizing::new(serde_json::to_vec(&saved)?);
        let name = format!("connection:{}", plan.target_id);
        let previous = self.read(&name)?;
        ensure!(
            self.replace(&name, previous.revision, &bytes)?,
            "Connection cache changed; retry"
        );
        Ok(())
    }
    pub(crate) fn cached_connection(&self, id: Uuid) -> Result<ConnectionPlan> {
        let entry = self.read(&format!("connection:{id}"))?;
        ensure!(
            entry.revision > 0,
            "Connect online once before using this host offline"
        );
        let saved: SavedPlan = serde_json::from_slice(&entry.bytes)?;
        ensure!(
            saved.format == 1
                && saved.target == id
                && !saved.steps.is_empty()
                && saved.steps.len() <= 16
                && saved.steps.last().is_some_and(|s| s.id == id),
            "Invalid cached connection plan"
        );
        let mut seen = std::collections::HashSet::new();
        let mut steps = Vec::new();
        for mut step in saved.steps {
            ensure!(
                seen.insert(step.id) && step.port > 0 && !step.address.is_empty(),
                "Invalid cached hop"
            );
            ensure!(
                step.protocol == "ssh" || (step.id == id && step.protocol == "mosh"),
                "Invalid cached protocol"
            );
            let authentication = match step.method.as_str() {
                "agent" => {
                    let agent = step.agent.take().context("Cached agent missing")?;
                    agent.validate()?;
                    Authentication::Agent(agent)
                }
                "prompt" => Authentication::Prompt,
                "keyboard_interactive" => Authentication::KeyboardInteractive,
                "password" => {
                    Authentication::Password(Zeroizing::new(std::mem::take(&mut step.password)))
                }
                "key" => Authentication::Key(step.key.take().context("Cached key missing")?),
                _ => anyhow::bail!("Unsupported cached authentication method"),
            };
            if let Some(proxy) = &step.proxy {
                proxy.validate()?;
            }
            if let Some(secret) = &step.proxy_auth {
                secret.validate(step.proxy.as_ref().context("Cached proxy missing")?)?;
            }
            steps.push(Target {
                proxy_auth: step.proxy_auth.take(),
                proxy: step.proxy.take(),
                id: step.id,
                protocol: std::mem::take(&mut step.protocol),
                label: std::mem::take(&mut step.label),
                address: std::mem::take(&mut step.address),
                port: step.port,
                username: step.username.take(),
                authentication,
            });
        }
        Ok(ConnectionPlan {
            target_id: id,
            steps,
        })
    }
    pub(crate) fn clear_connection_cache(&mut self) -> Result<()> {
        self.db
            .execute("DELETE FROM entries WHERE name LIKE 'connection:%'", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_edits_invalidate_routes_atomically_but_labels_do_not() -> Result<()> {
        use crate::local::journal::{CachedResource, Permissions};
        use crate::resources::ResourceContent;
        let dir = tempfile::tempdir()?;
        let mut s = Store::open(dir.path(), "edited-route", &Identity::generate())?;
        let root = Uuid::new_v4();
        let hop = Uuid::new_v4();
        let now = 1_800_000_000_000;
        let row = CachedResource {
            navigation_only: false,
            id: hop,
            vault_id: Uuid::new_v4(),
            server_revision: 1,
            generation: 0,
            permissions: Permissions {
                read: true,
                edit: true,
                connect: true,
            },
            write_capability: Some(Uuid::new_v4()),
            verified_at_ms: now,
            content: ResourceContent {
                document: None,
                proxy: None,
                kind: "host".into(),
                parent_id: None,
                name: "jump".into(),
                address: Some("localhost".into()),
                protocol: Some("ssh".into()),
                port: Some(22),
                username_policy: Some("personal".into()),
                username: None,
                deleted: false,
            },
        };
        s.refresh_cached_resource(row.clone())?;
        s.authorize_offline_edits(now)?;
        let plan = ConnectionPlan {
            target_id: root,
            steps: vec![hop, root]
                .into_iter()
                .map(|id| Target {
                    proxy: None,
                    proxy_auth: None,
                    id,
                    protocol: "ssh".into(),
                    label: "host".into(),
                    address: "localhost".into(),
                    port: 22,
                    username: None,
                    authentication: Authentication::Prompt,
                })
                .collect(),
        };
        s.save_connection(&plan)?;
        let mut renamed = row.content.clone();
        renamed.name = "new label".into();
        s.queue_edit(hop, 0, renamed.clone(), now + 1)?;
        assert!(s.cached_connection(root).is_ok());
        let mut changed = renamed.clone();
        changed.address = Some("other.example".into());
        s.db.execute_batch("CREATE TRIGGER simulated_failure BEFORE INSERT ON journal_operations BEGIN SELECT RAISE(ABORT,'disk unavailable'); END;")?;
        assert!(s.queue_edit(hop, 1, changed.clone(), now + 2).is_err());
        assert!(s.cached_connection(root).is_ok());
        assert_eq!(s.cached_resource(hop)?.unwrap().content, renamed);
        s.db.execute_batch("DROP TRIGGER simulated_failure;")?;
        s.queue_edit(hop, 1, changed, now + 3)?;
        assert!(s.cached_connection(root).is_err());
        Ok(())
    }
    #[test]
    fn cached_chain_is_encrypted_root_bound_and_revocable() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let identity = Identity::generate();
        let mut s = Store::open(dir.path(), "connection-scope", &identity)?;
        let root = Uuid::new_v4();
        let hop = Uuid::new_v4();
        let plan = ConnectionPlan {
            target_id: root,
            steps: vec![
                Target {
                    proxy_auth: Some(crate::proxy::ProxySecret {
                        endpoint: crate::resources::ProxyConfig {
                            kind: "http".into(),
                            host: "proxy.example".into(),
                            port: 8080,
                            authenticate: true,
                        },
                        username: "proxy-user".into(),
                        password: "proxy-secret-marker".into(),
                    }),
                    proxy: Some(crate::resources::ProxyConfig {
                        kind: "http".into(),
                        host: "proxy.example".into(),
                        port: 8080,
                        authenticate: true,
                    }),
                    id: hop,
                    protocol: "ssh".into(),
                    label: "jump".into(),
                    address: "127.0.0.1".into(),
                    port: 22,
                    username: Some("user".into()),
                    authentication: Authentication::Key(KeySecret {
                        format: 1,
                        private_key: "private-test-marker".into(),
                        passphrase: "passphrase-test-marker".into(),
                        certificate: "certificate".into(),
                    }),
                },
                Target {
                    proxy: None,
                    proxy_auth: None,
                    id: root,
                    protocol: "mosh".into(),
                    label: "target".into(),
                    address: "127.0.0.2".into(),
                    port: 22,
                    username: None,
                    authentication: Authentication::Password(Zeroizing::new(
                        "password-test-marker".into(),
                    )),
                },
            ],
        };
        s.save_connection(&plan)?;
        assert_eq!(
            s.cached_connection(root)?.steps[0].proxy,
            plan.steps[0].proxy
        );
        drop(s);
        let mut s = Store::open(dir.path(), "connection-scope", &identity)?;
        let loaded = s.cached_connection(root)?;
        assert_eq!(loaded.steps.len(), 2);
        assert_eq!(
            loaded.steps[0].proxy_auth.as_ref().unwrap().password,
            "proxy-secret-marker"
        );
        assert_eq!(loaded.steps[1].protocol, "mosh");
        assert!(
            matches!(&loaded.steps[1].authentication,Authentication::Password(p) if p.as_str()=="password-test-marker")
        );
        assert!(s.cached_connection(hop).is_err());
        // Removing permission to a hop removes every cached root containing it,
        // even when that hop was never independently visible in the browser.
        s.revoke_cached_resource(hop)?;
        assert!(s.cached_connection(root).is_err());
        s.save_connection(&plan)?;
        for entry in std::fs::read_dir(dir.path())? {
            let bytes = std::fs::read(entry?.path())?;
            for secret in [
                "private-test-marker",
                "passphrase-test-marker",
                "password-test-marker",
                "proxy-secret-marker",
            ] {
                assert!(!bytes.windows(secret.len()).any(|w| w == secret.as_bytes()));
            }
        }
        s.clear_connection_cache()?;
        assert!(s.cached_connection(root).is_err());
        s.save_connection(&plan)?;
        // Plans can exist without a browsed resource row; account revocation
        // must still remove them all.
        s.revoke_all_cached_resources()?;
        assert!(s.cached_connection(root).is_err());
        Ok(())
    }
}
