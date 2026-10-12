//! Resolved connection material never implements Serialize or Debug. Only the
//! native transport consumes it; no credential secret crosses a return IPC.
use crate::{
    keys::Identity,
    profiles::{self, Method, Profile, UsernamePolicy},
    resources,
    transport::TeamClient,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub enum Authentication {
    Prompt,
    KeyboardInteractive,
    Password(Zeroizing<String>),
    Key(KeySecret),
    Agent(AgentIdentity),
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_credentials_never_degrade_to_prompt() {
        assert!(Authentication::Password(Zeroizing::new(String::new())).validate().is_err());
        assert!(Authentication::Key(KeySecret { format:1, private_key:" \n".into(), passphrase:String::new(), certificate:String::new() }).validate().is_err());
        assert!(Authentication::Prompt.validate().is_ok());
        assert!(Authentication::KeyboardInteractive.validate().is_ok());
        assert!(Authentication::Password(Zeroizing::new("test password".into())).validate().is_ok());
    }
    #[test]
    fn agent_selection_requires_an_exact_key_or_certificate_fingerprint() {
        for prefix in ["SHA256:","CERT-SHA256:"] {
            let mut identity=AgentIdentity {format:1,provider:"unix".into(),fingerprint:format!("{prefix}{}","a".repeat(43))};
            assert!(identity.validate().is_ok());
            identity.fingerprint.push('=');assert!(identity.validate().is_err());
        }
        assert!(AgentIdentity {format:1,provider:"unix".into(),fingerprint:"any".into()}.validate().is_err());
    }
}
impl Authentication {
    /// Validate both online material and decrypted offline plans before the
    /// transport can interpret empty fields as an interactive prompt.
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Password(p) => ensure!(!p.is_empty(), "Selected password is empty; no fallback was attempted"),
            Self::Key(k) => ensure!(k.format == 1 && !k.private_key.trim().is_empty(), "Selected SSH key is empty or unsupported; no fallback was attempted"),
            Self::Agent(a) => a.validate()?,
            Self::Prompt | Self::KeyboardInteractive => {}
        }
        Ok(())
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentIdentity {
    pub format: u32,
    pub provider: String,
    pub fingerprint: String,
}
impl AgentIdentity {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.format == 1 && matches!(self.provider.as_str(), "openssh" | "pageant" | "unix"), "Unsupported signing agent");
        let hash = self.fingerprint.strip_prefix("SHA256:").or_else(||self.fingerprint.strip_prefix("CERT-SHA256:")).context("Select a specific agent key")?;
        ensure!(hash.len() == 43 && hash.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'+' || c == b'/'), "Invalid agent fingerprint");
        Ok(())
    }
}
#[derive(Deserialize, Serialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeySecret {
    pub format: u32,
    pub private_key: String,
    pub passphrase: String,
    #[serde(default)]
    pub certificate: String,
}
pub struct Target {
    pub id: Uuid,
    pub protocol: String,
    pub label: String,
    pub address: String,
    pub port: u16,
    pub username: Option<String>,
    pub authentication: Authentication,
    pub proxy: Option<resources::ProxyConfig>,
    pub proxy_auth: Option<crate::proxy::ProxySecret>,
}
fn to_profile(p: &profiles::OpenProfile) -> Result<Option<Profile>> {
    ensure!(
        !p.unavailable,
        "Personal profile recovery required; no fallback was attempted"
    );
    let method = match p.method.as_str() {
        "inherit" => return Ok(None),
        "prompt" => Method::Prompt,
        "keyboard_interactive" => Method::KeyboardInteractive,
        "credential" => Method::Credential {
            id: p.credential_id.context("Missing selected identity")?,
        },
        _ => anyhow::bail!("Unsupported profile method"),
    };
    Ok(Some(Profile {
        method,
        username: p.username.clone(),
    }))
}
impl TeamClient {
    pub async fn authorize_target(&self, id: Uuid) -> Result<serde_json::Value> {
        self.request(
            reqwest::Method::GET,
            &format!("v1/connections/{id}/target"),
            None,
            true,
        )
        .await
    }
    pub async fn resolve_target(&self, id: Uuid, identity: &Identity) -> Result<Target> {
        let row = self.authorize_target(id).await?;
        self.resolve_step(id, id, row, identity).await
    }
    pub(crate) async fn resolve_step(&self, root: Uuid, id: Uuid, row: serde_json::Value, identity: &Identity) -> Result<Target> {
        let (team, _, _) = self.actor()?;
        let resource = resources::open(team, &row, identity)?;
        ensure!(
            resource.id == id && resource.content.kind == "host" && !resource.content.deleted,
            "Invalid connection target"
        );
        let mut host = resource.content;
        let historical_revision = row["temporaryRevision"].as_i64();
        if let Some(revision) = historical_revision {
            let mut previous = self
                .history(id, identity)
                .await?
                .into_iter()
                .find(|v| v.revision == revision)
                .context("Pinned history unavailable")?
                .content
                .context("Pinned history cannot be decrypted")?;
            ensure!(
                previous.kind == "host" && !previous.deleted,
                "Invalid historical connection"
            );
            // Today's mandatory company username remains a security policy.
            previous.username_policy = host.username_policy;
            previous.username = host.username;
            host = previous;
        }
        ensure!(
            host.protocol.as_deref() == Some("ssh")
                || (root == id && host.protocol.as_deref() == Some("mosh")),
            "Unsupported Team transport; intermediate hops must use SSH"
        );
        let profiles = self.open_profiles(identity).await?;
        let personal = profiles
            .hosts
            .iter()
            .find(|p| p.host_id == Some(id))
            .map(to_profile)
            .transpose()?
            .flatten();
        let default = profiles
            .default
            .as_ref()
            .map(to_profile)
            .transpose()?
            .flatten();
        let policy = match host.username_policy.as_deref() {
            Some("shared_required") => {
                UsernamePolicy::SharedRequired(host.username.context("Shared username missing")?)
            }
            Some("personal") => UsernamePolicy::Personal,
            _ => anyhow::bail!("Invalid username policy"),
        };
        let selected = profiles::resolve(
            &profiles::Host { id, policy },
            personal.as_ref(),
            default.as_ref(),
        )?;
        let authentication = match selected.method {
            Method::Prompt => Authentication::Prompt,
            Method::KeyboardInteractive => Authentication::KeyboardInteractive,
            Method::Credential { id: credential_id } => {
                let credential = self
                    .open_plan_credential(root, id, credential_id, identity)
                    .await?;
                match credential.kind.as_str() {
                    "agent" => {
                        let agent: AgentIdentity = serde_json::from_slice(&credential.secret)?;
                        agent.validate()?;
                        Authentication::Agent(agent)
                    }
                    "password" => Authentication::Password(Zeroizing::new(
                        String::from_utf8(credential.secret.to_vec())
                            .context("Invalid password encoding")?,
                    )),
                    "ssh_key" => {
                        let key = if credential.secret.first() == Some(&b'{') {
                            serde_json::from_slice::<KeySecret>(&credential.secret)
                                .context("Invalid stored SSH key")?
                        } else {
                            KeySecret {
                                format: 1,
                                private_key: String::from_utf8(credential.secret.to_vec())
                                    .context("Invalid SSH key encoding")?,
                                passphrase: String::new(),
                                certificate: String::new(),
                            }
                        };
                        ensure!(key.format == 1, "Unsupported SSH key format");
                        Authentication::Key(key)
                    }
                    _ => anyhow::bail!("Unsupported selected identity; no fallback was attempted"),
                }
            }
        };
        authentication.validate()?;
        let proxy_auth=if let Some(endpoint)=host.proxy.as_ref().filter(|p|p.authenticate) { self.resolve_proxy(root,id,endpoint,identity).await? } else {None};
        Ok(Target {
            proxy_auth,
            id,
            protocol: host.protocol.context("Host protocol missing")?,
            label: if let Some(rev) = historical_revision {
                format!("{} · Geçmiş sürüm {rev}", host.name)
            } else {
                host.name
            },
            address: host.address.context("Host address missing")?,
            port: host.port.context("Host port missing")?,
            username: selected.username,
            authentication,
            proxy: host.proxy,
        })
    }
}
