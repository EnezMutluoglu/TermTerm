use anyhow::{anyhow, ensure, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub const FORMAT_VERSION: u32 = 1;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub version: u32,
    pub salt: Vec<u8>,
    pub wrapped_key: Vec<u8>,
}
pub fn random_key() -> Zeroizing<[u8; 32]> {
    let mut k = Zeroizing::new([0; 32]);
    OsRng.fill_bytes(&mut *k);
    k
}
pub fn derive(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    ensure!(salt.len() == 16, "Invalid salt");
    let mut key = Zeroizing::new([0; 32]);
    let params = Params::new(65536, 3, 1, Some(32)).map_err(|e| anyhow!(e.to_string()))?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut *key)
        .map_err(|e| anyhow!(e.to_string()))?;
    Ok(key)
}
pub fn seal(key: &[u8; 32], data: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let mut nonce = [0; 24];
    OsRng.fill_bytes(&mut nonce);
    let encrypted = XChaCha20Poly1305::new(key.into())
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: data, aad })
        .map_err(|_| anyhow!("Encryption failed"))?;
    Ok([nonce.to_vec(), encrypted].concat())
}
pub fn unseal(key: &[u8; 32], data: &[u8], aad: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(data.len() >= 40, "Encrypted data is truncated");
    let bytes = XChaCha20Poly1305::new(key.into())
        .decrypt(
            XNonce::from_slice(&data[..24]),
            Payload {
                msg: &data[24..],
                aad,
            },
        )
        .map_err(|_| anyhow!("Wrong password or damaged encrypted data"))?;
    Ok(Zeroizing::new(bytes))
}
impl Envelope {
    pub fn create(password: &str, key: &[u8; 32], context: &[u8]) -> Result<Self> {
        ensure!(
            password.chars().count() >= 8,
            "Use a vault password of at least 8 characters"
        );
        let mut salt = vec![0; 16];
        OsRng.fill_bytes(&mut salt);
        let wrapping = derive(password, &salt)?;
        Ok(Self {
            version: FORMAT_VERSION,
            salt,
            wrapped_key: seal(&wrapping, key, context)?,
        })
    }
    pub fn unlock(&self, password: &str, context: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
        ensure!(
            self.version == FORMAT_VERSION,
            "Unsupported file version; the file has not been changed"
        );
        let wrapping = derive(password, &self.salt)?;
        let bytes = unseal(&wrapping, &self.wrapped_key, context)?;
        ensure!(bytes.len() == 32, "Invalid key envelope");
        let mut key = Zeroizing::new([0; 32]);
        key.copy_from_slice(&bytes);
        Ok(key)
    }
}
