use crate::{keys::Identity, resources::ProxyConfig, transport::TeamClient};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxySecret {
    #[zeroize(skip)]
    pub endpoint: ProxyConfig,
    pub username: String,
    pub password: String,
}
impl ProxySecret {
    pub fn validate(&self, endpoint: &ProxyConfig) -> Result<()> {
        self.endpoint.validate()?;
        ensure!(
            &self.endpoint == endpoint && endpoint.authenticate,
            "Proxy endpoint changed; update your personal proxy profile"
        );
        ensure!(
            !self.username.is_empty()
                && !self.password.is_empty()
                && self.username.len() <= 255
                && self.password.len() <= 255
                && !self.username.contains(':')
                && !self.username.chars().any(char::is_control)
                && !self.password.chars().any(char::is_control),
            "Invalid proxy credentials"
        );
        Ok(())
    }
}
impl TeamClient {
    async fn proxy_mapping(&self, root: Uuid, host: Uuid) -> Result<Option<Uuid>> {
        let value = self
            .request(
                reqwest::Method::POST,
                "v1/proxy/profile",
                Some(&json!({"root":root,"hostId":host,"credentialId":null,"body":null})),
                true,
            )
            .await?;
        Ok(serde_json::from_value(value["credentialId"].clone())?)
    }
    pub async fn proxy_status(&self, root: Uuid, host: Uuid, identity: &Identity) -> Result<Value> {
        if let Some(id) = self.proxy_mapping(root, host).await? {
            let credential = self.open_private_credential(id, identity).await?;
            ensure!(credential.kind == "proxy", "Invalid proxy identity");
            let secret: ProxySecret = serde_json::from_slice(&credential.secret)?;
            return Ok(
                json!({"credentialId":id,"revision":credential.revision,"username":secret.username,"endpoint":secret.endpoint}),
            );
        }
        Ok(json!({"credentialId":null,"revision":0}))
    }
    pub async fn resolve_proxy(
        &self,
        root: Uuid,
        host: Uuid,
        endpoint: &ProxyConfig,
        identity: &Identity,
    ) -> Result<Option<ProxySecret>> {
        let Some(id) = self.proxy_mapping(root, host).await? else {
            return Ok(None);
        };
        let credential = self.open_private_credential(id, identity).await?;
        ensure!(credential.kind == "proxy", "Invalid proxy identity");
        let secret: ProxySecret = serde_json::from_slice(&credential.secret)?;
        secret.validate(endpoint)?;
        Ok(Some(secret))
    }
    pub async fn save_proxy(
        &self,
        root: Uuid,
        host: Uuid,
        id: Option<Uuid>,
        revision: i64,
        secret: Option<ProxySecret>,
        identity: &Identity,
    ) -> Result<Value> {
        let id = id.unwrap_or_else(Uuid::new_v4);
        let body = if let Some(secret) = secret {
            secret.validate(&secret.endpoint)?;
            let bytes = Zeroizing::new(serde_json::to_vec(&secret)?);
            self.prepare_private_credential(
                id,
                revision,
                "proxy",
                b"Personal proxy",
                &bytes,
                identity,
            )
            .await?
        } else {
            json!({"action":"forget","expectedRevision":revision})
        };
        self.request(
            reqwest::Method::POST,
            "v1/proxy/profile",
            Some(&json!({"root":root,"hostId":host,"credentialId":id,"body":body})),
            true,
        )
        .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_are_bound_to_the_exact_proxy_endpoint() {
        let mut config = ProxyConfig {
            kind: "http".into(),
            host: "proxy.example".into(),
            port: 8080,
            authenticate: true,
        };
        let secret = ProxySecret {
            endpoint: config.clone(),
            username: "user".into(),
            password: "test-only".into(),
        };
        assert!(secret.validate(&config).is_ok());
        config.port = 8081;
        assert!(secret.validate(&config).is_err());
        config = secret.endpoint.clone();
        config.host = "other.example".into();
        assert!(secret.validate(&config).is_err());
        config = secret.endpoint.clone();
        config.kind = "socks5".into();
        assert!(secret.validate(&config).is_err());
    }
}
