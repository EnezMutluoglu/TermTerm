use anyhow::{bail, ensure, Context, Result};
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use url::Url;
use uuid::Uuid;
use zeroize::Zeroize;

/// This is the entire distributable profile. No DB or application credentials.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceProfile {
    pub service_url: String,
    pub team_id: Uuid,
    pub ca_pem: Option<String>,
}
impl ServiceProfile {
    fn base_url(&self) -> Result<Url> {
        let mut url = Url::parse(&self.service_url)?;
        ensure!(
            url.scheme() == "https" && url.host_str().is_some(),
            "Team service requires HTTPS"
        );
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "Service URL cannot contain credentials, query or fragment"
        );
        ensure!(
            url.path() == "/" || url.path().is_empty(),
            "Service URL must be an origin without a path"
        );
        url.set_path("/");
        Ok(url)
    }
    pub fn cache_scope(&self, account: Uuid) -> Result<String> {
        Ok(format!(
            "team-v2:{}:{}:{account}",
            self.base_url()?,
            self.team_id
        ))
    }
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Tokens {
    team_id: Uuid,
    account_id: Uuid,
    access_token: String,
    refresh_token: String,
    activation_required: bool,
    identity: Option<Value>,
}
impl Drop for Tokens {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub team_id: Uuid,
    pub account_id: Uuid,
    pub activation_required: bool,
}
pub struct OpenCredential {
    pub kind: String,
    pub revision: i64,
    pub data: zeroize::Zeroizing<Vec<u8>>,
    pub secret: zeroize::Zeroizing<Vec<u8>>,
}
pub struct TemporaryAccount {
    pub account_id: Uuid,
    pub password: zeroize::Zeroizing<String>,
}
impl TemporaryAccount {
    fn from_value(mut value: Value) -> Result<Self> {
        let id = serde_json::from_value(value["accountId"].take())?;
        let password = serde_json::from_value(value["temporaryPassword"].take())?;
        Ok(Self {
            account_id: id,
            password: zeroize::Zeroizing::new(password),
        })
    }
}
pub struct TeamClient {
    policy_store: Option<crate::compatibility::PolicyStore>,
    policy: std::sync::Mutex<Option<crate::compatibility::Policy>>,
    client: Client,
    profile: ServiceProfile,
    base: Url,
    tokens: Option<Tokens>,
    renewed_at: std::time::Instant,
    network_retry_after: std::sync::Mutex<Option<std::time::Instant>>,
    pub(crate) device: Option<Uuid>,
}
pub(crate) enum CandidateRoute {
    Prepare,
    Submit,
    Claim,
    Receipt(Uuid),
}
impl TeamClient {
    pub(crate) fn matches_profile(&self, profile: &ServiceProfile) -> Result<bool> {
        Ok(self.base == profile.base_url()? && self.profile.team_id == profile.team_id
            && self.profile.ca_pem == profile.ca_pem)
    }
    pub fn configure_policy(&mut self, root: &std::path::Path) -> Result<()> {
        self.policy_store = Some(crate::compatibility::PolicyStore::new(root, &format!("{}:{}",self.base,self.profile.team_id))?);
        Ok(())
    }
    pub(crate) fn ensure_compatible(&self) -> Result<()> {
        if let Some(store) = &self.policy_store {
            if let Some(policy) = store.read()? { policy.enforce()?; }
        }
        if let Some(policy) = self.policy.lock().map_err(|_| anyhow::anyhow!("Policy lock"))?.as_ref() { policy.enforce()?; }
        Ok(())
    }
    fn remember_policy(&self, value: Value) -> Result<crate::compatibility::Policy> {
        let mut policy: crate::compatibility::Policy = serde_json::from_value(value)?;
        policy.validate()?;
        // Set memory first: even a full disk must never turn a denial into outage.
        *self.policy.lock().map_err(|_| anyhow::anyhow!("Policy lock"))? = Some(policy.clone());
        if let Some(store) = &self.policy_store { policy=store.save(&policy)?; }
        *self.policy.lock().map_err(|_| anyhow::anyhow!("Policy lock"))? = Some(policy.clone());
        Ok(policy)
    }
    pub async fn compatibility(&self) -> Result<crate::compatibility::Policy> {
        self.retry_network_now()?;
        let response = self.send(Method::GET, &format!("client-policy?teamId={}",self.profile.team_id), None, None).await;
        match response {
            Ok(value) => self.remember_policy(value),
            Err(e) => { self.ensure_compatible()?; Err(e) }
        }
    }
    // These bytes are only written to the identity-encrypted native cache.
    pub(crate) fn cached_auth(&self, binding: &str) -> Result<zeroize::Zeroizing<Vec<u8>>> {
        #[derive(Serialize)]
        struct Auth<'a> {
            binding: &'a str,
            device: Uuid,
            tokens: &'a Tokens,
        }
        Ok(zeroize::Zeroizing::new(serde_json::to_vec(&Auth {
            binding,
            device: self.device.context("Device missing")?,
            tokens: self.tokens.as_ref().context("Sign in first")?,
        })?))
    }
    pub(crate) fn restore_cached_auth(
        &mut self,
        bytes: &[u8],
        binding: &str,
        account: Uuid,
        device: Uuid,
        password: &str,
        identity: &crate::keys::Identity,
    ) -> Result<SessionView> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Auth {
            binding: String,
            device: Uuid,
            tokens: Tokens,
        }
        let auth: Auth = serde_json::from_slice(bytes)?;
        ensure!(
            auth.binding == binding
                && auth.device == device
                && auth.tokens.team_id == self.profile.team_id
                && auth.tokens.account_id == account
                && !auth.tokens.activation_required,
            "Cached session scope mismatch"
        );
        self.tokens = Some(auth.tokens);
        self.device = Some(device);
        ensure!(
            self.unlock_identity(password)?.public_key() == identity.public_key(),
            "Cached identity mismatch"
        );
        // Access tokens are never assumed valid after restart. Every online
        // operation must first refresh; a proven outage only enables cache paths.
        self.renewed_at = std::time::Instant::now()
            .checked_sub(Duration::from_secs(3600))
            .context("Monotonic clock unavailable")?;
        Ok(SessionView {
            team_id: self.profile.team_id,
            account_id: account,
            activation_required: false,
        })
    }
    pub(crate) fn protected_identity(&self) -> Result<crate::keys::IdentityFile> {
        let identity = self
            .tokens
            .as_ref()
            .context("Sign in first")?
            .identity
            .as_ref()
            .context("Activate account first")?;
        Ok(serde_json::from_slice(&hex::decode(
            identity["encryptedPrivateKey"]
                .as_str()
                .context("Identity missing")?,
        )?)?)
    }
    pub(crate) fn local_scope(&self) -> Result<String> {
        self.profile.cache_scope(self.actor()?.1)
    }
    pub fn new(profile: ServiceProfile) -> Result<Self> {
        let base = profile.base_url()?;
        let mut builder = Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(25))
            .connect_timeout(Duration::from_secs(5));
        if let Some(ca) = &profile.ca_pem {
            builder = builder.add_root_certificate(reqwest::Certificate::from_pem(ca.as_bytes())?);
        }
        Ok(Self {
            policy_store: None,
            policy: std::sync::Mutex::new(None),
            client: builder.build()?,
            profile,
            base,
            tokens: None,
            renewed_at: std::time::Instant::now(),
            network_retry_after: std::sync::Mutex::new(None),
            device: None,
        })
    }
    pub(crate) async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        authenticated: bool,
    ) -> Result<Value> {
        let proof = if authenticated {
            Some(
                self.tokens
                    .as_ref()
                    .context("Sign in first")?
                    .access_token
                    .as_str(),
            )
        } else {
            None
        };
        self.send(method, path, body, proof).await
    }
    pub(crate) async fn candidate_request(
        &self,
        proof: &str,
        route: CandidateRoute,
        body: Option<&Value>,
    ) -> Result<Value> {
        let (method, path) = match route {
            CandidateRoute::Prepare => (Method::POST, "v1/offline/decisions/prepare".into()),
            CandidateRoute::Submit => (Method::POST, "v1/offline/decisions".into()),
            CandidateRoute::Claim => (Method::POST, "v1/offline/create-claim".into()),
            CandidateRoute::Receipt(id) => (Method::GET, format!("v1/operations/{id}/receipt")),
        };
        self.send(method, &path, body, Some(proof)).await
    }
    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        proof: Option<&str>,
    ) -> Result<Value> {
        // A single proven outage must not cause every cached screen read to
        // wait for another TCP timeout. No TLS/auth/ambiguous-write failures
        // enter this short, monotonic negative cache.
        if !path.starts_with("client-policy?") && path != "v1/auth/logout" { self.ensure_compatible()?; }
        ensure!(
            !self
                .network_retry_after
                .lock()
                .map_err(|_| anyhow::anyhow!("Network state lock"))?
                .is_some_and(|at| at > std::time::Instant::now()),
            "network_unavailable"
        );
        let mut request = self.client.request(method.clone(), self.base.join(path)?)
            .header("x-termterm-client-version", crate::compatibility::CLIENT_VERSION)
            .header("x-termterm-api-version", crate::compatibility::API_VERSION.to_string());
        if let Some(proof) = proof {
            request = request.bearer_auth(proof);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = match request.send().await {
            Ok(r) => r,
            Err(error) => {
                let kind = transport_failure(&error, method == Method::GET);
                if kind == "network_unavailable" {
                    *self
                        .network_retry_after
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Network state lock"))? =
                        Some(std::time::Instant::now() + Duration::from_secs(5));
                }
                bail!(kind)
            }
        };
        self.retry_network_now()?;
        let status = response.status();
        // Never include arbitrary server response text/HTML in an error or log.
        match status {
            StatusCode::UNPROCESSABLE_ENTITY => {
                let mut response = response;
                let mut data = Vec::new();
                while let Some(chunk) = response.chunk().await? {
                    ensure!(data.len() + chunk.len() <= 1024, "Response too large");
                    data.extend_from_slice(&chunk);
                }
                if serde_json::from_slice::<Value>(&data).ok().is_some_and(|v| v["error"] == "history_route_unavailable") {
                    bail!("Bu eski sürümün chain bilgisi kaydedilmemiş. Güncel sürüme dönün.");
                }
                bail!("service_error: 422")
            }
            StatusCode::UNAUTHORIZED => bail!("authentication_denied"),
            StatusCode::FORBIDDEN => bail!("permission_denied"),
            StatusCode::CONFLICT => bail!("revision_conflict"),
            StatusCode::TOO_MANY_REQUESTS => bail!("rate_limited"),
            StatusCode::UPGRADE_REQUIRED => (),
            _ if !status.is_success() => bail!("service_error: {}", status.as_u16()),
            _ => (),
        }
        ensure!(
            response.content_length().unwrap_or(0) <= 4 * 1024 * 1024,
            "Response too large"
        );
        let mut response = response;
        let mut data = zeroize::Zeroizing::new(Vec::new());
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                data.len() + chunk.len() <= 4 * 1024 * 1024,
                "Response too large"
            );
            data.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&data)?;
        if status == StatusCode::UPGRADE_REQUIRED {
            let policy = self.remember_policy(value["compatibility"].clone())?;
            policy.enforce()?;
            bail!("Team service returned an inconsistent compatibility contract");
        }
        Ok(value)
    }
    pub(crate) fn retry_network_now(&self) -> Result<()> {
        *self
            .network_retry_after
            .lock()
            .map_err(|_| anyhow::anyhow!("Network state lock"))? = None;
        Ok(())
    }
    pub async fn login(
        &mut self,
        username: &str,
        password: &str,
        device: Uuid,
        device_name: &str,
    ) -> Result<SessionView> {
        self.tokens = None;
        self.device = None;
        self.compatibility().await?.enforce()?;
        let mut body = json!({"teamId":self.profile.team_id,"username":username,"password":password,"deviceId":device,"deviceName":device_name});
        let result = self
            .request(Method::POST, "v1/auth/login", Some(&body), false)
            .await;
        if let Some(Value::String(password)) = body.get_mut("password") {
            password.zeroize();
        }
        let tokens: Tokens = serde_json::from_value(result?)?;
        ensure!(
            tokens.team_id == self.profile.team_id,
            "Login team mismatch"
        );
        let view = SessionView {
            team_id: tokens.team_id,
            account_id: tokens.account_id,
            activation_required: tokens.activation_required,
        };
        self.tokens = Some(tokens);
        self.device = Some(device);
        self.renewed_at = std::time::Instant::now();
        Ok(view)
    }
    pub async fn renew_if_needed(&mut self) -> Result<()> {
        self.ensure_compatible()?;
        if self.renewed_at.elapsed() < Duration::from_secs(12 * 60) {
            return Ok(());
        }
        self.renew().await
    }
    pub async fn renew(&mut self) -> Result<()> {
        let old = self.tokens.as_ref().context("Sign in first")?;
        ensure!(!old.activation_required, "Activate account first");
        let expected = (old.team_id, old.account_id);
        let identity = old.identity.clone();
        let mut body = json!({"refreshToken":old.refresh_token});
        let response = self
            .request(Method::POST, "v1/auth/refresh", Some(&body), false)
            .await;
        if let Some(Value::String(v)) = body.get_mut("refreshToken") {
            v.zeroize();
        }
        // Refresh rotates the token. An uncertain response must not replay the
        // old token and revoke the family; require a fresh login instead.
        // A refused connection is different: the refresh was never submitted,
        // so retain the real session and actor for authorised offline reads.
        if response
            .as_ref()
            .is_err_and(|e| e.to_string() == "network_unavailable")
        {
            return Err(response.err().expect("Checked error"));
        }
        self.tokens = None;
        let mut tokens: Tokens = serde_json::from_value(response?)?;
        ensure!(
            (tokens.team_id, tokens.account_id) == expected && !tokens.activation_required,
            "Session scope changed"
        );
        // Refresh checks the account auth epoch but only returns new tokens;
        // the encrypted identity supplied by login is retained for this scope.
        if tokens.identity.is_none() {
            tokens.identity = identity;
        }
        self.tokens = Some(tokens);
        self.renewed_at = std::time::Instant::now();
        Ok(())
    }
    pub fn encrypted_identity(&self) -> Result<Option<&Value>> {
        Ok(self
            .tokens
            .as_ref()
            .context("Sign in first")?
            .identity
            .as_ref())
    }
    pub async fn activate(
        &mut self,
        password: &str,
        identity: &crate::keys::Identity,
    ) -> Result<()> {
        let tokens = self.tokens.as_ref().context("Sign in first")?;
        ensure!(tokens.activation_required, "Account is already activated");
        let file = identity.protect(tokens.team_id, tokens.account_id, password)?;
        let mut body = json!({"password":password,"publicKey":identity.public_key(),"encryptedPrivateKey":hex::encode(serde_json::to_vec(&file)?)});
        let result = self
            .request(Method::POST, "v1/auth/activate", Some(&body), true)
            .await;
        if let Some(Value::String(p)) = body.get_mut("password") {
            p.zeroize();
        }
        result?;
        self.tokens = None;
        Ok(())
    }
    pub async fn overview(&self) -> Result<Value> {
        self.request(Method::GET, "v1/me", None, true).await
    }
    pub(crate) fn actor(&self) -> Result<(Uuid, Uuid, i64)> {
        let tokens = self.tokens.as_ref().context("Sign in first")?;
        let version = tokens.identity.as_ref().context("Activate account first")?["version"]
            .as_i64()
            .context("Missing identity version")?;
        Ok((tokens.team_id, tokens.account_id, version))
    }
    pub fn unlock_identity(&self, password: &str) -> Result<crate::keys::Identity> {
        let session = self.tokens.as_ref().context("Sign in first")?;
        let stored = session
            .identity
            .as_ref()
            .context("Activate account first")?;
        let encoded = stored["encryptedPrivateKey"]
            .as_str()
            .context("Missing identity envelope")?;
        let identity: crate::keys::IdentityFile = serde_json::from_slice(&hex::decode(encoded)?)?;
        identity.unlock(session.team_id, session.account_id, password)
    }
    pub async fn create_member(
        &self,
        username: &str,
        display_name: &str,
        role: &str,
    ) -> Result<TemporaryAccount> {
        TemporaryAccount::from_value(
            self.request(
                Method::POST,
                "v1/members",
                Some(&json!({"username":username,"displayName":display_name,"role":role})),
                true,
            )
            .await?,
        )
    }
    pub async fn reset_member(&self, account: Uuid) -> Result<TemporaryAccount> {
        TemporaryAccount::from_value(
            self.request(
                Method::POST,
                &format!("v1/members/{account}/reset"),
                None,
                true,
            )
            .await?,
        )
    }
    pub async fn enroll_recovery(&self, password: &str, public_key: &str) -> Result<Uuid> {
        let mut body = json!({"password":password,"publicKey":public_key});
        let result = self
            .request(Method::POST, "v1/recovery/key", Some(&body), true)
            .await;
        if let Some(Value::String(p)) = body.get_mut("password") {
            p.zeroize();
        }
        Ok(serde_json::from_value(result?["keyId"].clone())?)
    }
    pub(crate) async fn prepare_private_credential(
        &self,
        id: Uuid,
        expected_revision: i64,
        kind: &str,
        data: &[u8],
        secret: &[u8],
        identity: &crate::keys::Identity,
    ) -> Result<Value> {
        use crate::keys::{encrypt, Purpose};
        ensure!(
            (0..i64::MAX).contains(&expected_revision),
            "Invalid revision"
        );
        let session = self.tokens.as_ref().context("Sign in first")?;
        let context = self
            .request(Method::GET, "v1/crypto/context", None, true)
            .await?;
        ensure!(
            context["identity"]["publicKey"].as_str() == Some(identity.public_key().as_str()),
            "Account identity changed"
        );
        let recovery = context["recovery"]["publicKey"]
            .as_str()
            .context("Team leader must enroll a recovery key first")?;
        let data = encrypt(
            session.team_id,
            id,
            expected_revision + 1,
            Purpose::Data,
            data,
            &identity.public_key(),
            recovery,
        )?;
        let secret = encrypt(
            session.team_id,
            id,
            expected_revision + 1,
            Purpose::Secret,
            secret,
            &identity.public_key(),
            recovery,
        )?;
        let body = json!({"expectedRevision":expected_revision,"kind":kind,"encryptedData":data.ciphertext,"encryptedSecret":secret.ciphertext,
            "identityVersion":context["identity"]["version"],"recoveryKeyId":context["recovery"]["keyId"],
            "userData":data.user_envelope,"userSecret":secret.user_envelope,"recoveryData":data.recovery_envelope,"recoverySecret":secret.recovery_envelope});
        Ok(body)
    }
    pub async fn save_private_credential(&self,id:Uuid,expected_revision:i64,kind:&str,data:&[u8],secret:&[u8],identity:&crate::keys::Identity)->Result<i64> {
        let body=self.prepare_private_credential(id,expected_revision,kind,data,secret,identity).await?;
        self.request(Method::PUT,&format!("v1/credentials/private/{id}"),Some(&body),true).await?["revision"].as_i64().context("Invalid revision receipt")
    }
    pub async fn private_credentials(&self) -> Result<Value> {
        self.request(Method::GET, "v1/credentials/private", None, true)
            .await
    }
    pub async fn open_private_credential(
        &self,
        id: Uuid,
        identity: &crate::keys::Identity,
    ) -> Result<OpenCredential> {
        use crate::keys::{decrypt, Purpose};
        let session = self.tokens.as_ref().context("Sign in first")?;
        let value = self
            .request(
                Method::GET,
                &format!("v1/credentials/private/{id}"),
                None,
                true,
            )
            .await?;
        let revision = value["revision"]
            .as_i64()
            .context("Invalid credential revision")?;
        let identity_version = session.identity.as_ref().context("Missing identity")?["version"]
            .as_i64()
            .context("Missing identity version")?;
        let open =
            |purpose: Purpose, name: &str, field: &str| -> Result<zeroize::Zeroizing<Vec<u8>>> {
                let envelope = value["envelopes"]
                    .as_array()
                    .and_then(|a| {
                        a.iter().find(|e| {
                            e["purpose"] == name && e["identityVersion"] == identity_version
                        })
                    })
                    .and_then(|v| v["envelope"].as_str())
                    .context("Credential recovery required")?;
                decrypt(
                    session.team_id,
                    id,
                    revision,
                    purpose,
                    value[field].as_str().context("Missing ciphertext")?,
                    envelope,
                    identity,
                )
            };
        Ok(OpenCredential {
            kind: value["kind"]
                .as_str()
                .context("Missing credential kind")?
                .to_string(),
            revision,
            data: open(Purpose::Data, "data", "encryptedData")?,
            secret: open(Purpose::Secret, "secret", "encryptedSecret")?,
        })
    }
    pub async fn set_default_credential(&self, id: Uuid, expected_revision: i64) -> Result<Value> {
        self.request(Method::PUT,"v1/profiles",Some(&json!({"hostId":null,"expectedRevision":expected_revision,"method":"credential","credentialId":id,"encryptedUsername":null})),true).await
    }
    pub async fn profiles(&self) -> Result<Value> {
        self.request(Method::GET, "v1/profiles", None, true).await
    }
    pub async fn members(&self) -> Result<Value> {
        self.request(Method::GET, "v1/members", None, true).await
    }
    pub async fn disable_member(&self, id: Uuid) -> Result<Value> {
        self.request(
            Method::POST,
            &format!("v1/members/{id}/disable"),
            None,
            true,
        )
        .await
    }
    pub async fn logout(&mut self) -> Result<()> {
        let result = self
            .request(Method::POST, "v1/auth/logout", None, true)
            .await;
        self.tokens = None;
        result.map(|_| ())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn known_mandatory_update_is_not_an_offline_login_after_restart() -> Result<()> {
        let root = tempfile::tempdir()?;
        let profile = ServiceProfile {service_url:"https://127.0.0.1:1".into(),team_id:Uuid::new_v4(),ca_pem:None};
        let first=TeamClient::new(profile.clone())?;
        let policy=crate::compatibility::Policy {revision:1,api_version:1,minimum_client_version:"0.4.2".into(),service_version:"0.4.2".into()};
        crate::compatibility::PolicyStore::new(root.path(),&format!("{}:{}",first.base,profile.team_id))?.save(&policy)?;
        drop(first);
        let desktop=crate::desktop::DesktopState::default();
        desktop.configure_cache(root.path().to_owned())?;
        let error=desktop.login(profile,"fixture".into(),"Disposable-password-2026!".into()).await.unwrap_err();
        assert_eq!(error.to_string(),"team_update_required:0.4.2:1");
        assert!(desktop.call(crate::desktop::DesktopAction::Overview{}).await.is_err());
        Ok(())
    }
    #[test]
    fn password_cache_binds_origin_account_and_identity_without_plaintext_tokens() -> Result<()> {
        let root = tempfile::tempdir()?;
        let profile = ServiceProfile {
            service_url: "https://localhost:1".into(),
            team_id: Uuid::new_v4(),
            ca_pem: None,
        };
        let account = Uuid::new_v4();
        let identity = crate::keys::Identity::generate();
        let password = "Disposable-cache-password-2026!";
        let envelope = identity.protect(profile.team_id, account, password)?;
        let mut client = TeamClient::new(profile.clone())?;
        let mut store =
            crate::local::Store::open(root.path(), &profile.cache_scope(account)?, &identity)?;
        client.device = Some(store.journal_device(1000)?);
        client.tokens = Some(Tokens {
            team_id: profile.team_id,
            account_id: account,
            activation_required: false,
            access_token: "disposable-access-token-marker".into(),
            refresh_token: "disposable-refresh-token-marker".into(),
            identity: Some(
                json!({"version":1,"encryptedPrivateKey":hex::encode(serde_json::to_vec(&envelope)?)}),
            ),
        });
        let binding = crate::login_cache::binding(&profile, "testuser")?;
        crate::login_cache::save(root.path(), &binding, &client, &mut store)?;
        let (_, view, restored, _) =
            crate::login_cache::open(root.path(), &binding, &profile, "testuser", password, 1001)?;
        assert_eq!(view.account_id, account);
        assert_eq!(restored.public_key(), identity.public_key());
        assert!(crate::login_cache::open(
            root.path(),
            &binding,
            &profile,
            "testuser",
            "wrong-password",
            1002
        )
        .is_err());
        assert!(crate::login_cache::open(
            root.path(),
            &binding,
            &profile,
            "another-user",
            password,
            1002
        )
        .is_err());
        let mut other = profile.clone();
        other.service_url = "https://localhost:2".into();
        assert!(crate::login_cache::open(
            root.path(),
            &binding,
            &other,
            "testuser",
            password,
            1002
        )
        .is_err());
        for entry in std::fs::read_dir(root.path())? {
            let bytes = std::fs::read(entry?.path())?;
            for marker in [
                password,
                "disposable-access-token-marker",
                "disposable-refresh-token-marker",
            ] {
                assert!(!bytes.windows(marker.len()).any(|w| w == marker.as_bytes()));
            }
        }
        store.forget_cached_auth()?;
        assert!(crate::login_cache::open(
            root.path(),
            &binding,
            &profile,
            "testuser",
            password,
            1003
        )
        .is_err());
        Ok(())
    }
    #[test]
    fn profile_cannot_distribute_db_credentials_or_disable_tls() {
        assert!(serde_json::from_value::<ServiceProfile>(json!({"serviceUrl":"https://example.test","teamId":Uuid::new_v4(),"password":"db-secret"})).is_err());
        for url in [
            "http://example.test",
            "https://user:password@example.test",
            "https://example.test/?token=secret",
            "https://example.test/path",
        ] {
            assert!(TeamClient::new(ServiceProfile {
                service_url: url.into(),
                team_id: Uuid::new_v4(),
                ca_pem: None
            })
            .is_err());
        }
        let profile = ServiceProfile {
            service_url: "https://example.test".into(),
            team_id: Uuid::new_v4(),
            ca_pem: None,
        };
        assert_ne!(
            profile.cache_scope(Uuid::new_v4()).unwrap(),
            profile.cache_scope(Uuid::new_v4()).unwrap()
        );
    }
}

// Offline access is permitted only for a proven network interruption. An invalid
// certificate or other unclassified transport failure must never unlock a cache.
fn transport_failure(error: &reqwest::Error, read: bool) -> &'static str {
    use std::error::Error;
    let mut source = error.source();
    let mut network = error.is_timeout();
    while let Some(cause) = source {
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            network |= matches!(
                io.kind(),
                std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::NotConnected
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::NetworkUnreachable
                    | std::io::ErrorKind::HostUnreachable
            );
        }
        source = cause.source();
    }
    if network {
        if read || error.is_connect() {
            "network_unavailable"
        } else {
            "unknown_outcome: verify the server result before repeating the operation"
        }
    } else {
        "secure_transport_failed"
    }
}
