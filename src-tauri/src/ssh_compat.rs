//! Host identity and RSA signature selection shared by SSH, SFTP and jump hops.
use crate::{keys::known_address_matches, model::Record};
use anyhow::{bail, ensure, Result};
use russh::keys::{Algorithm, HashAlg, PrivateKey, PublicKey};

pub fn endpoint(address: &str, port: u16) -> String {
    if port == 22 {
        address.to_owned()
    } else {
        format!("[{address}]:{port}")
    }
}

pub fn known_hosts(records: &[Record], address: &str, port: u16) -> Vec<Record> {
    let address = endpoint(address, port);
    records
        .iter()
        .filter(|r| {
            r.kind == "knownHost"
                && r.data["address"]
                    .as_str()
                    .is_some_and(|saved| known_address_matches(saved, &address))
        })
        .cloned()
        .collect()
}

fn stored_key(record: &Record) -> Option<PublicKey> {
    PublicKey::from_openssh(record.data["publicKey"].as_str()?).ok()
}

fn same_algorithm(a: &Algorithm, b: &Algorithm) -> bool {
    matches!((a, b), (Algorithm::Rsa { .. }, Algorithm::Rsa { .. })) || a == b
}

pub fn preferred_host_keys(known: &[Record]) -> Vec<Algorithm> {
    let trusted: Vec<_> = known.iter().filter_map(stored_key).collect();
    let mut algorithms = russh::Preferred::default().key.into_owned();
    // Reorder only the library's existing algorithms; do not enable extra ciphers
    // or trust a new key merely because another key belongs to this address.
    algorithms.sort_by_key(|algorithm| {
        !trusted
            .iter()
            .any(|key| same_algorithm(&key.algorithm(), algorithm))
    });
    algorithms
}

/// False means a first-use prompt is required; mismatches always fail closed.
pub fn verify_host_key(known: &[Record], address: &str, public: &PublicKey) -> Result<bool> {
    if known.is_empty() {
        return Ok(false);
    }
    let stored: Vec<_> = known.iter().filter_map(stored_key).collect();
    if stored.iter().any(|key| key.key_data() == public.key_data()) {
        return Ok(true);
    }
    let algorithm = public.algorithm();
    let fingerprint = public.fingerprint(HashAlg::Sha256);
    if let Some(expected) = stored
        .iter()
        .find(|key| same_algorithm(&key.algorithm(), &algorithm))
    {
        bail!("HOST KEY CHANGED for {address} ({algorithm}). Expected {}, received {fingerprint}. Verify the change independently before updating the matching Known Host entry.", expected.fingerprint(HashAlg::Sha256));
    }
    if stored.is_empty() {
        bail!("INVALID KNOWN HOST KEY for {address}. Repair the saved public key before reconnecting; no trust entry was replaced.");
    }
    let mut saved_algorithms: Vec<_> = stored
        .iter()
        .map(|key| key.algorithm().to_string())
        .collect();
    saved_algorithms.sort();
    saved_algorithms.dedup();
    bail!("UNTRUSTED HOST KEY TYPE for {address}: received {algorithm} {fingerprint}; saved key types: {}. No saved key was replaced. Verify the new key independently before adding it to Known Hosts.", saved_algorithms.join(", "))
}

pub fn decode_private_key(text: &str, passphrase: &str) -> Result<PrivateKey> {
    ensure!(text.len() <= 1024 * 1024, "SSH private key exceeds 1 MiB");
    let text = text.trim_start_matches(|c: char| c == '\u{feff}' || c.is_whitespace());
    if text
        .lines()
        .any(|line| line.trim() == "Proc-Type: 4,ENCRYPTED")
    {
        return legacy_pem::decode(text, passphrase);
    }
    let pass = if passphrase.is_empty() {
        None
    } else {
        Some(passphrase)
    };
    Ok(if text.starts_with("PuTTY-User-Key-File-") {
        PrivateKey::from_ppk(text, pass.map(str::to_owned))?
    } else {
        // A stale stored passphrase must not make an unencrypted PKCS#8/SEC1
        // key look encrypted to the underlying decoder.
        let pass = if text.starts_with("-----BEGIN PRIVATE KEY-----")
            || text.starts_with("-----BEGIN EC PRIVATE KEY-----")
        {
            None
        } else {
            pass
        };
        russh::keys::decode_secret_key(text, pass)?
    })
}

#[path = "ssh_legacy_pem.rs"]
mod legacy_pem;

pub fn rsa_hash_candidates(advertised: Option<Option<HashAlg>>) -> Vec<Option<HashAlg>> {
    match advertised {
        Some(hash) => vec![hash],
        // Missing server-sig-algs is not evidence that SHA-2 is unsupported.
        // Keep the existing legacy fallback, but only after both SHA-2 probes.
        None => vec![Some(HashAlg::Sha512), Some(HashAlg::Sha256), None],
    }
}

pub async fn authenticate_key<H: russh::client::Handler + Send + 'static>(
    handle: &mut russh::client::Handle<H>,
    username: &str,
    key: std::sync::Arc<PrivateKey>,
) -> Result<bool> {
    let candidates = if key.algorithm().is_rsa() {
        rsa_hash_candidates(handle.best_supported_rsa_hash().await?)
    } else {
        vec![None]
    };
    for hash in candidates {
        if handle
            .authenticate_publickey(
                username,
                russh::keys::PrivateKeyWithHashAlg::new(key.clone(), hash),
            )
            .await?
            .success()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
#[path = "ssh_compat_tests.rs"]
mod tests;
