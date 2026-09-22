//! Read-only compatibility with OpenSSL's traditional encrypted PEM files.
//! New keys/vaults never use this legacy MD5 key derivation.
use aes::cipher::{block_padding::Pkcs7, BlockModeDecrypt, KeyIvInit};
use anyhow::{anyhow, bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use russh::keys::PrivateKey;
use zeroize::Zeroizing;

pub(super) fn decode(text: &str, password: &str) -> Result<PrivateKey> {
    ensure!(
        !password.is_empty(),
        "This SSH private key requires a passphrase"
    );
    let label = match text.lines().next().unwrap_or("").trim() {
        "-----BEGIN RSA PRIVATE KEY-----" => "RSA PRIVATE KEY",
        "-----BEGIN EC PRIVATE KEY-----" => "EC PRIVATE KEY",
        _ => bail!("Unsupported traditional encrypted PEM key type"),
    };
    let dek = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("DEK-Info:"))
        .ok_or_else(|| anyhow!("Encrypted PEM is missing DEK-Info"))?;
    let (cipher, iv) = dek
        .trim()
        .split_once(',')
        .ok_or_else(|| anyhow!("Invalid PEM encryption header"))?;
    let key_len = match cipher {
        "AES-128-CBC" => 16,
        "AES-192-CBC" => 24,
        "AES-256-CBC" => 32,
        _ => bail!(
            "Unsupported PEM encryption: {cipher}. Convert this key to OpenSSH or PKCS#8 format."
        ),
    };
    let iv = hex::decode(iv.trim()).context("Invalid PEM IV")?;
    ensure!(iv.len() == 16, "Invalid PEM IV length");
    let mut body = Zeroizing::new(String::new());
    let mut ended = false;
    let footer = format!("-----END {label}-----");
    for line in text.lines().skip(1).map(str::trim) {
        if line == footer {
            ended = true;
            break;
        }
        if line.is_empty() || line.starts_with("Proc-Type:") || line.starts_with("DEK-Info:") {
            continue;
        }
        body.push_str(line);
    }
    ensure!(ended, "Truncated encrypted PEM key");
    let mut data = Zeroizing::new(
        STANDARD
            .decode(body.as_bytes())
            .context("Invalid PEM data")?,
    );

    // OpenSSL PEM EVP_BytesToKey: MD5, one iteration, first 8 IV bytes as salt.
    // See https://docs.openssl.org/3.4/man3/PEM_read_bio_PrivateKey/
    let mut derived = Zeroizing::new(Vec::with_capacity(32));
    let mut previous = Zeroizing::new(Vec::new());
    while derived.len() < key_len {
        let mut hash = md5::Context::new();
        hash.consume(&*previous);
        hash.consume(password.as_bytes());
        hash.consume(&iv[..8]);
        *previous = hash.finalize().0.to_vec();
        derived.extend_from_slice(&previous);
    }
    let key = &derived[..key_len];
    let decrypted = match key_len {
        16 => cbc::Decryptor::<aes::Aes128>::new_from_slices(key, &iv)
            .unwrap()
            .decrypt_padded::<Pkcs7>(&mut data),
        24 => cbc::Decryptor::<aes::Aes192>::new_from_slices(key, &iv)
            .unwrap()
            .decrypt_padded::<Pkcs7>(&mut data),
        32 => cbc::Decryptor::<aes::Aes256>::new_from_slices(key, &iv)
            .unwrap()
            .decrypt_padded::<Pkcs7>(&mut data),
        _ => unreachable!(),
    }
    .map_err(|_| anyhow!("Incorrect PEM passphrase or damaged key"))?;
    let encoded = Zeroizing::new(STANDARD.encode(decrypted));
    let plain = Zeroizing::new(format!(
        "-----BEGIN {label}-----\n{}\n{footer}\n",
        &*encoded
    ));
    russh::keys::decode_secret_key(&plain, None)
        .map_err(|_| anyhow!("Incorrect PEM passphrase or damaged/unsupported key"))
}
