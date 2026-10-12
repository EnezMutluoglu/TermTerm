//! Recovery private key and plaintext stay in native memory. One credential per
//! step gives the UI a cancellation boundary; only the recipient confirms success.
use crate::{
    keys::{self, Identity, Purpose},
    transport::TeamClient,
};
use anyhow::{ensure, Context, Result};
use reqwest::Method;
use serde_json::{json, Value};
use uuid::Uuid;

pub struct Job {
    operation: Uuid,
    manifest: Value,
    key: Option<Identity>,
    index: usize,
}
impl Job {
    pub fn preview(&self) -> Value {
        json!({"operationId":self.operation,"displayName":self.manifest["displayName"],
        "publicKey":self.manifest["publicKey"],"total":self.manifest["credentials"].as_array().map_or(0,Vec::len)+self.manifest["profiles"].as_array().map_or(0,Vec::len),"verification":self.key.is_none()})
    }
    pub async fn cancel(self, client: &TeamClient) -> Result<()> {
        if self.key.is_some() {
            client
                .request(
                    Method::POST,
                    &format!("v1/recovery/operations/{}/cancel", self.operation),
                    None,
                    true,
                )
                .await?;
        }
        Ok(())
    }
    pub async fn step(&mut self, client: &TeamClient, identity: &Identity) -> Result<Value> {
        let credentials = self.manifest["credentials"]
            .as_array()
            .context("Recovery manifest unavailable")?;
        let empty=Vec::new();
        let profiles=self.manifest["profiles"].as_array().unwrap_or(&empty);
        let total = credentials.len()+profiles.len();
        let team = client.actor()?.0;
        if let Some(id) = credentials.get(self.index) {
            let credential: Uuid = serde_json::from_value(id.clone())?;
            let key = self.key.as_ref().unwrap_or(identity);
            let endpoint = if self.key.is_some() {
                "credentials"
            } else {
                "verify"
            };
            let row = client
                .request(
                    Method::GET,
                    &format!(
                        "v1/recovery/operations/{}/{endpoint}/{credential}",
                        self.operation
                    ),
                    None,
                    true,
                )
                .await?;
            let versions = row["versions"]
                .as_array()
                .context("Recovery versions unavailable")?;
            ensure!(!versions.is_empty(), "Recovery versions missing");
            for version in versions {
                let rev = version["revision"]
                    .as_i64()
                    .context("Invalid recovery revision")?;
                let mut wrapped = Vec::new();
                for (purpose, name, field) in [
                    (Purpose::Data, "data", "encryptedData"),
                    (Purpose::Secret, "secret", "encryptedSecret"),
                ] {
                    let envelope = version["envelopes"]
                        .as_array()
                        .and_then(|es| es.iter().find(|e| e["purpose"] == name))
                        .and_then(|e| e["envelope"].as_str())
                        .context("Recovery envelope missing")?;
                    // AEAD authentication must succeed, not merely sealed-key unwrap.
                    let _plaintext = keys::decrypt(
                        team,
                        credential,
                        rev,
                        purpose,
                        version[field]
                            .as_str()
                            .context("Recovery ciphertext missing")?,
                        envelope,
                        key,
                    )?;
                    if self.key.is_some() {
                        wrapped.push(keys::rewrap(
                            team,
                            credential,
                            rev,
                            purpose,
                            envelope,
                            key,
                            self.manifest["publicKey"]
                                .as_str()
                                .context("Recipient identity missing")?,
                        )?);
                    }
                }
                if self.key.is_some() {
                    client.request(Method::POST,&format!("v1/recovery/operations/{}/rewrap",self.operation),Some(&json!({
                    "credentialId":credential,"revision":rev,"identityVersion":self.manifest["identityVersion"],"dataKey":wrapped[0],"secretKey":wrapped[1]})),true).await?;
                }
            }
            self.index += 1;
        } else if let Some(profile)=profiles.get(self.index.saturating_sub(credentials.len())) {
            let host_key=profile["hostKey"].as_str().context("Profile scope missing")?;
            let host=if host_key=="default" {None}else{Some(host_key.parse()?)};
            let rev=profile["revision"].as_i64().context("Profile revision missing")?;
            let account=serde_json::from_value(self.manifest["subjectId"].clone())?;
            let aad=crate::profiles::context(team,account,host,rev);
            let row=client.request(Method::POST,&format!("v1/recovery/operations/{}/profiles/read",self.operation),Some(&json!({"hostKey":host_key,"revision":rev,"verification":self.key.is_none()})),true).await?;
            let encoded=row["encryptedUsername"].as_str().context("Profile ciphertext missing")?;
            let envelope=row["envelope"].as_str().context("Profile recovery envelope missing")?;
            let key=self.key.as_ref().unwrap_or(identity);
            let _verified=zeroize::Zeroizing::new(crate::profiles::open_username_envelope(key,&aad,encoded,Some(envelope))?);
            if self.key.is_some() {
                let data_key=keys::unwrap(key,envelope,&aad)?;
                let wrapped=keys::wrap(&data_key,self.manifest["publicKey"].as_str().context("Recipient identity missing")?,&aad)?;
                client.request(Method::POST,&format!("v1/recovery/operations/{}/profiles/rewrap",self.operation),Some(&json!({"hostKey":host_key,"revision":rev,"identityVersion":self.manifest["identityVersion"],"envelope":wrapped})),true).await?;
            }
            self.index+=1;
        }
        let done = self.index == total;
        if done && self.key.is_none() {
            let current = client.recovery_manifest(self.operation).await?;
            ensure!(
                current["credentials"] == self.manifest["credentials"]
                    && current["profiles"] == self.manifest["profiles"]
                    && current["identityVersion"] == self.manifest["identityVersion"],
                "Recovery changed; verify again"
            );
            client
                .request(
                    Method::POST,
                    &format!("v1/recovery/operations/{}/complete", self.operation),
                    None,
                    true,
                )
                .await?;
        }
        Ok(
            json!({"completed":done,"processed":self.index,"total":total,"verificationRequired":self.key.is_some()}),
        )
    }
}
impl TeamClient {
    async fn recovery_manifest(&self, operation: Uuid) -> Result<Value> {
        self.request(
            Method::GET,
            &format!("v1/recovery/operations/{operation}/manifest"),
            None,
            true,
        )
        .await
    }
    pub async fn begin_recovery_job(
        &self,
        subject: Uuid,
        password: &str,
        reason: &str,
        key: Identity,
    ) -> Result<Job> {
        let context = self
            .request(Method::GET, "v1/crypto/context", None, true)
            .await?;
        ensure!(
            context["recovery"]["publicKey"] == key.public_key(),
            "Recovery file does not match the active company key"
        );
        let op=self.request(Method::POST,"v1/recovery/operations",Some(&json!({"password":password,"subjectId":subject,"reason":reason,"recoveryKeyId":context["recovery"]["keyId"]})),true).await?;
        let operation: Uuid = serde_json::from_value(op["operationId"].clone())?;
        let manifest = self.recovery_manifest(operation).await?;
        ensure!(
            manifest["publicKey"].is_string(),
            "Recipient activation required"
        );
        Ok(Job {
            operation,
            manifest,
            key: Some(key),
            index: 0,
        })
    }
    pub async fn verify_recovery_job(&self, operation: Uuid, identity: &Identity) -> Result<Job> {
        let manifest = self.recovery_manifest(operation).await?;
        ensure!(
            manifest["subjectId"] == self.actor()?.1.to_string()
                && manifest["publicKey"] == identity.public_key(),
            "Only the recovered recipient can verify"
        );
        Ok(Job {
            operation,
            manifest,
            key: None,
            index: 0,
        })
    }
}
