use crate::vault_crypto as crypto;
use anyhow::{anyhow, ensure, Result};
use crypto_box::{PublicKey, SecretKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

pub struct Identity {
    secret: Zeroizing<[u8; 32]>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityFile {
    format: u32,
    team_id: Uuid,
    account_id: Uuid,
    public_key: String,
    encrypted_key: crypto::Envelope,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryFile {
    format: u32,
    team_id: Uuid,
    public_key: String,
    encrypted_key: crypto::Envelope,
}
fn check_password(p: &str) -> Result<()> {
    ensure!(
        p.chars().count() >= 12 && p.len() <= 72,
        "Password must contain at least 12 characters and at most 72 UTF-8 bytes"
    );
    Ok(())
}
impl Identity {
    pub fn generate() -> Self {
        Self {
            secret: Zeroizing::new(SecretKey::generate(&mut OsRng).to_bytes()),
        }
    }
    pub fn public_key(&self) -> String {
        hex::encode(SecretKey::from(*self.secret).public_key().as_bytes())
    }
    pub fn protect(&self, team: Uuid, account: Uuid, password: &str) -> Result<IdentityFile> {
        check_password(password)?;
        let context = format!(
            "termterm-team-v2:identity:{team}:{account}:{}",
            self.public_key()
        );
        Ok(IdentityFile {
            format: 1,
            team_id: team,
            account_id: account,
            public_key: self.public_key(),
            encrypted_key: crypto::Envelope::create(password, &self.secret, context.as_bytes())?,
        })
    }
    pub fn recovery_file(&self, team: Uuid, password: &str) -> Result<RecoveryFile> {
        check_password(password)?;
        let context = format!("termterm-team-v2:recovery:{team}:{}", self.public_key());
        Ok(RecoveryFile {
            format: 1,
            team_id: team,
            public_key: self.public_key(),
            encrypted_key: crypto::Envelope::create(password, &self.secret, context.as_bytes())?,
        })
    }
}
impl IdentityFile {
    pub fn unlock(&self, team: Uuid, account: Uuid, password: &str) -> Result<Identity> {
        ensure!(
            self.format == 1 && self.team_id == team && self.account_id == account,
            "Identity scope/version mismatch"
        );
        let context = format!(
            "termterm-team-v2:identity:{team}:{account}:{}",
            self.public_key
        );
        let identity = Identity {
            secret: self.encrypted_key.unlock(password, context.as_bytes())?,
        };
        ensure!(
            identity.public_key() == self.public_key,
            "Identity public key mismatch"
        );
        Ok(identity)
    }
}
impl RecoveryFile {
    pub fn unlock(&self, team: Uuid, password: &str) -> Result<Identity> {
        ensure!(
            self.format == 1 && self.team_id == team,
            "Recovery scope/version mismatch"
        );
        let context = format!("termterm-team-v2:recovery:{team}:{}", self.public_key);
        let identity = Identity {
            secret: self.encrypted_key.unlock(password, context.as_bytes())?,
        };
        ensure!(
            identity.public_key() == self.public_key,
            "Recovery public key mismatch"
        );
        Ok(identity)
    }
}
#[derive(Clone, Copy)]
pub enum Purpose {
    Data,
    Secret,
}
fn context(team: Uuid, credential: Uuid, revision: i64, purpose: Purpose) -> Result<String> {
    ensure!(revision > 0, "Invalid revision");
    Ok(format!(
        "termterm-team-v2:{team}:{credential}:{revision}:{}",
        match purpose {
            Purpose::Data => "data",
            Purpose::Secret => "secret",
        }
    ))
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SealedValue {
    pub ciphertext: String,
    pub user_envelope: String,
    pub recovery_envelope: String,
}
pub(crate) fn wrap(key: &[u8; 32], public: &str, aad: &str) -> Result<String> {
    let public: [u8; 32] = hex::decode(public)?
        .try_into()
        .map_err(|_| anyhow!("Invalid public key"))?;
    let payload = Zeroizing::new([key.as_slice(), aad.as_bytes()].concat());
    Ok(hex::encode(
        PublicKey::from(public)
            .seal(&mut OsRng, &payload)
            .map_err(|_| anyhow!("Key wrapping failed"))?,
    ))
}
pub(crate) fn unwrap(
    identity: &Identity,
    envelope: &str,
    aad: &str,
) -> Result<Zeroizing<[u8; 32]>> {
    let payload = Zeroizing::new(
        SecretKey::from(*identity.secret)
            .unseal(&hex::decode(envelope)?)
            .map_err(|_| anyhow!("Not an authorized key recipient"))?,
    );
    ensure!(
        payload.len() == 32 + aad.len() && &payload[32..] == aad.as_bytes(),
        "Key envelope scope mismatch"
    );
    let mut key = Zeroizing::new([0; 32]);
    key.copy_from_slice(&payload[..32]);
    Ok(key)
}
pub fn encrypt(
    team: Uuid,
    credential: Uuid,
    revision: i64,
    purpose: Purpose,
    plaintext: &[u8],
    user_public: &str,
    recovery_public: &str,
) -> Result<SealedValue> {
    let aad = context(team, credential, revision, purpose)?;
    let key = crypto::random_key();
    Ok(SealedValue {
        ciphertext: hex::encode(crypto::seal(&key, plaintext, aad.as_bytes())?),
        user_envelope: wrap(&key, user_public, &aad)?,
        recovery_envelope: wrap(&key, recovery_public, &aad)?,
    })
}
pub fn decrypt(
    team: Uuid,
    credential: Uuid,
    revision: i64,
    purpose: Purpose,
    ciphertext: &str,
    envelope: &str,
    identity: &Identity,
) -> Result<Zeroizing<Vec<u8>>> {
    let aad = context(team, credential, revision, purpose)?;
    crypto::unseal(
        &*unwrap(identity, envelope, &aad)?,
        &hex::decode(ciphertext)?,
        aad.as_bytes(),
    )
}
pub fn rewrap(
    team: Uuid,
    credential: Uuid,
    revision: i64,
    purpose: Purpose,
    envelope: &str,
    old_identity: &Identity,
    new_public: &str,
) -> Result<String> {
    let aad = context(team, credential, revision, purpose)?;
    wrap(&*unwrap(old_identity, envelope, &aad)?, new_public, &aad)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_recovery_scope_and_reset_roundtrip() {
        let team = Uuid::new_v4();
        let account = Uuid::new_v4();
        let id = Uuid::new_v4();
        let user = Identity::generate();
        let recovery = Identity::generate();
        let other = Identity::generate();
        let encrypted = encrypt(
            team,
            id,
            1,
            Purpose::Secret,
            b"private test credential",
            &user.public_key(),
            &recovery.public_key(),
        )
        .unwrap();
        assert_eq!(
            &**decrypt(
                team,
                id,
                1,
                Purpose::Secret,
                &encrypted.ciphertext,
                &encrypted.user_envelope,
                &user
            )
            .unwrap(),
            b"private test credential"
        );
        assert!(decrypt(
            team,
            id,
            1,
            Purpose::Secret,
            &encrypted.ciphertext,
            &encrypted.user_envelope,
            &other
        )
        .is_err());
        assert!(decrypt(
            Uuid::new_v4(),
            id,
            1,
            Purpose::Secret,
            &encrypted.ciphertext,
            &encrypted.recovery_envelope,
            &recovery
        )
        .is_err());
        assert!(decrypt(
            team,
            id,
            2,
            Purpose::Secret,
            &encrypted.ciphertext,
            &encrypted.recovery_envelope,
            &recovery
        )
        .is_err());
        assert!(decrypt(
            team,
            id,
            1,
            Purpose::Data,
            &encrypted.ciphertext,
            &encrypted.recovery_envelope,
            &recovery
        )
        .is_err());
        let new_user = Identity::generate();
        let envelope = rewrap(
            team,
            id,
            1,
            Purpose::Secret,
            &encrypted.recovery_envelope,
            &recovery,
            &new_user.public_key(),
        )
        .unwrap();
        assert_eq!(
            &**decrypt(
                team,
                id,
                1,
                Purpose::Secret,
                &encrypted.ciphertext,
                &envelope,
                &new_user
            )
            .unwrap(),
            b"private test credential"
        );
        let file = user
            .protect(team, account, "Strong-test-password-123")
            .unwrap();
        assert!(file.unlock(team, account, "Wrong-password-123").is_err());
        assert!(file
            .unlock(team, Uuid::new_v4(), "Strong-test-password-123")
            .is_err());
        assert_eq!(
            file.unlock(team, account, "Strong-test-password-123")
                .unwrap()
                .public_key(),
            user.public_key()
        );
        let recovery_file = recovery
            .recovery_file(team, "Recovery-password-123")
            .unwrap();
        assert_eq!(
            recovery_file
                .unlock(team, "Recovery-password-123")
                .unwrap()
                .public_key(),
            recovery.public_key()
        );
    }
}
