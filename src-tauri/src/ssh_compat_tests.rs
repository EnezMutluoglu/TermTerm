use super::*;

#[tokio::test]
async fn certificate_signer_uses_requested_rsa_sha2_and_rejects_another_key() {
    use russh::keys::ssh_key::{certificate, Signature};
    use russh::Signer;
    use ssh_encoding::Decode;
    let key = Arc::new(rsa4096().clone());
    let ca = random_key(Algorithm::Ed25519);
    let mut builder =
        certificate::Builder::new_with_random_nonce(&mut safe_rng(), key.public_key(), 0, u64::MAX)
            .unwrap();
    builder.valid_principal("testuser").unwrap();
    let identity = builder.sign(&ca).unwrap().into();
    let mut signer = CertificateSigner(key.clone());
    for hash in [HashAlg::Sha256, HashAlg::Sha512] {
        let message = b"disposable SSH auth packet";
        let packet = signer
            .auth_sign(&identity, Some(hash), message.to_vec())
            .await
            .unwrap();
        assert_eq!(&packet[..message.len()], message);
        let mut framed = &packet[message.len()..];
        let encoded = Vec::<u8>::decode(&mut framed).unwrap();
        assert!(framed.is_empty());
        let mut bytes = encoded.as_slice();
        let signature = Signature::decode(&mut bytes).unwrap();
        assert!(bytes.is_empty());
        assert_eq!(signature.algorithm(), Algorithm::Rsa { hash: Some(hash) });
        signature::Verifier::verify(key.public_key(), message, &signature).unwrap();
        assert!(signature::Verifier::verify(key.public_key(), b"modified", &signature).is_err());
    }
    let other = random_key(Algorithm::Ed25519).public_key().clone().into();
    assert!(signer
        .auth_sign(&other, Some(HashAlg::Sha256), vec![1])
        .await
        .is_err());
}
use russh::keys::{
    key::safe_rng,
    ssh_key::{
        private::{KeypairData, RsaKeypair},
        EcdsaCurve, LineEnding,
    },
    PublicKeyOrCertificate,
};
use russh::{client, server};
use serde_json::json;
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

fn ecdsa() -> Algorithm {
    Algorithm::Ecdsa {
        curve: EcdsaCurve::NistP256,
    }
}
fn random_key(algorithm: Algorithm) -> PrivateKey {
    PrivateKey::random(&mut safe_rng(), algorithm).unwrap()
}
fn rsa4096() -> &'static PrivateKey {
    static KEY: OnceLock<PrivateKey> = OnceLock::new();
    KEY.get_or_init(|| {
        let pair = RsaKeypair::random(&mut safe_rng(), 4096).unwrap();
        assert_eq!(pair.key_size(), 4096);
        PrivateKey::new(KeypairData::Rsa(pair), "disposable RSA 4096 test key").unwrap()
    })
}
fn known(address: &str, key: &PrivateKey) -> Record {
    Record::new(
        "knownHost",
        json!({"address":address,"publicKey":key.public_key().to_openssh().unwrap()}),
    )
}

#[test]
fn imported_ecdsa_is_preferred_without_trusting_ed25519() {
    let ecdsa_key = random_key(ecdsa());
    let ed = random_key(Algorithm::Ed25519);
    let saved = vec![known("jump.example", &ecdsa_key)];
    assert_eq!(preferred_host_keys(&saved)[0], ecdsa());
    assert!(verify_host_key(&saved, "jump.example", ecdsa_key.public_key()).unwrap());
    assert!(verify_host_key(&saved, "jump.example", ed.public_key())
        .unwrap_err()
        .to_string()
        .contains("UNTRUSTED HOST KEY TYPE"));
    let changed = random_key(ecdsa());
    assert!(
        verify_host_key(&saved, "jump.example", changed.public_key())
            .unwrap_err()
            .to_string()
            .contains("HOST KEY CHANGED")
    );
}

#[test]
fn key_matching_handles_comments_ports_and_invalid_records() {
    let key = random_key(Algorithm::Ed25519);
    let mut record = known("[example.test]:2222", &key);
    record.data["publicKey"] = format!(
        "{} imported comment",
        key.public_key().to_openssh().unwrap()
    )
    .into();
    assert!(known_hosts(&[record.clone()], "example.test", 22).is_empty());
    let records = known_hosts(&[record], "example.test", 2222);
    assert_eq!(records.len(), 1);
    assert!(verify_host_key(&records, "[example.test]:2222", key.public_key()).unwrap());
    assert!(!verify_host_key(&[], "new.example", key.public_key()).unwrap());
    let bad = Record::new(
        "knownHost",
        json!({"address":"bad.example","publicKey":"broken"}),
    );
    assert!(verify_host_key(&[bad], "bad.example", key.public_key()).is_err());
}

#[test]
fn rsa_signature_priority_and_host_key_variants() {
    assert_eq!(
        rsa_hash_candidates(None),
        vec![Some(HashAlg::Sha512), Some(HashAlg::Sha256), None]
    );
    assert_eq!(
        rsa_hash_candidates(Some(Some(HashAlg::Sha256))),
        vec![Some(HashAlg::Sha256)]
    );
    assert_eq!(rsa_hash_candidates(Some(None)), vec![None]);
    let algorithms = preferred_host_keys(&[known("rsa.example", rsa4096())]);
    assert_eq!(
        &algorithms[..3],
        &[
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha512)
            },
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha256)
            },
            Algorithm::Rsa { hash: None }
        ]
    );
    assert_eq!(algorithms.len(), russh::Preferred::default().key.len());
}

#[test]
fn rsa4096_plain_encrypted_openssh_and_wrong_password() {
    let key = rsa4096();
    let text = key.to_openssh(LineEnding::LF).unwrap();
    let decoded = decode_private_key(&format!("\u{feff}\n{}", &*text), "").unwrap();
    assert_eq!(decoded.public_key().key_data(), key.public_key().key_data());
    let encrypted = key
        .encrypt(&mut safe_rng(), "disposable-test-passphrase")
        .unwrap()
        .to_openssh(LineEnding::CRLF)
        .unwrap();
    assert_eq!(
        decode_private_key(&encrypted, "disposable-test-passphrase")
            .unwrap()
            .public_key()
            .key_data(),
        key.public_key().key_data()
    );
    assert!(decode_private_key(&encrypted, "incorrect").is_err());
}

struct TrustedClient {
    known: Vec<Record>,
}
impl client::Handler for TrustedClient {
    type Error = anyhow::Error;
    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool> {
        verify_host_key(&self.known, "loopback SSH fixture", &key.public_key())
    }
}
struct KeyAuthServer {
    authorized: PublicKey,
}
impl server::Handler for KeyAuthServer {
    type Error = russh::Error;
    async fn auth_publickey(
        &mut self,
        user: &str,
        key: &PublicKey,
    ) -> std::result::Result<server::Auth, Self::Error> {
        Ok(
            if user == "fixture" && key.key_data() == self.authorized.key_data() {
                server::Auth::Accept
            } else {
                server::Auth::reject()
            },
        )
    }
    async fn auth_openssh_certificate(&mut self,user:&str,cert:&russh::keys::Certificate)->Result<server::Auth,Self::Error> {
        Ok(if user=="fixture" && cert.public_key()==self.authorized.key_data() {server::Auth::Accept} else {server::Auth::reject()})
    }
}
struct Fixture {
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn connect_fixture(
    user_key: &PrivateKey,
    advertised_hash: Option<HashAlg>,
    extensions: bool,
    changed_host_key: bool,
) -> (Fixture, Result<client::Handle<TrustedClient>>) {
    let host_key = random_key(ecdsa());
    let trust = vec![known("fixture", &host_key)];
    let host_key = if changed_host_key {
        random_key(ecdsa())
    } else {
        host_key
    };
    let mut config = server::Config::default();
    config.keys = vec![random_key(Algorithm::Ed25519), host_key];
    config.auth_rejection_time = Duration::from_millis(1);
    config.auth_rejection_time_initial = Some(Duration::from_millis(1));
    config.preferred.key = vec![
        Algorithm::Ed25519,
        ecdsa(),
        Algorithm::Rsa {
            hash: advertised_hash,
        },
    ]
    .into();
    let config = Arc::new(config);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let authorized = user_key.public_key().clone();
    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        if let Ok(running) = server::run_stream(config, socket, KeyAuthServer { authorized }).await
        {
            let _ = running.await;
        }
    });
    let mut config = client::Config::default();
    config.preferred.key = preferred_host_keys(&trust).into();
    if !extensions {
        config.preferred.kex.to_mut().retain(|k| {
            *k != russh::kex::EXTENSION_SUPPORT_AS_CLIENT
                && *k != russh::kex::EXTENSION_SUPPORT_AS_SERVER
        });
    }
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        client::connect(Arc::new(config), address, TrustedClient { known: trust }),
    )
    .await
    .unwrap();
    (Fixture { task }, result)
}

#[tokio::test]
async fn real_handshake_uses_saved_ecdsa_and_rsa4096_authenticates() {
    for (extensions, hash) in [
        (true, Some(HashAlg::Sha512)),
        (true, Some(HashAlg::Sha256)),
        (true, None),
        (false, Some(HashAlg::Sha512)),
    ] {
        let key = rsa4096().clone();
        let (_fixture, result) = connect_fixture(&key, hash, extensions, false).await;
        let mut client = result.unwrap();
        assert_eq!(
            client.best_supported_rsa_hash().await.unwrap(),
            if extensions { Some(hash) } else { None }
        );
        assert!(tokio::time::timeout(
            Duration::from_secs(10),
            authenticate_key(&mut client, "fixture", Arc::new(key))
        )
        .await
        .unwrap()
        .unwrap().success());
    }
}

#[tokio::test]
async fn real_handshake_still_rejects_a_changed_ecdsa_key() {
    let key = random_key(Algorithm::Ed25519);
    let (_fixture, result) = connect_fixture(&key, Some(HashAlg::Sha512), true, true).await;
    match result {
        Err(error) => assert!(format!("{error:#}").contains("HOST KEY CHANGED")),
        Ok(_) => panic!("Changed host key was accepted"),
    }
}

#[tokio::test]
async fn ed25519_and_ecdsa_user_keys_still_authenticate() {
    for algorithm in [
        Algorithm::Ed25519,
        ecdsa(),
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP384,
        },
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP521,
        },
    ] {
        let key = random_key(algorithm);
        let (_fixture, result) = connect_fixture(&key, Some(HashAlg::Sha512), true, false).await;
        assert!(
            authenticate_key(&mut result.unwrap(), "fixture", Arc::new(key))
                .await
                .unwrap().success()
        );
    }
}

fn fixture_text(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/ssh-keys")
            .join(name),
    )
    .unwrap()
}

/// Independent OpenSSL and PuTTY fixtures, not just roundtrips of our encoder.
fn format_cases() -> Vec<(String, String, &'static str)> {
    let mut cases = vec![];
    for bits in [2048, 3072, 4096] {
        let base = format!("rsa{bits}.pkcs8");
        cases.push((base.clone(), base.clone(), ""));
        cases.push((format!("rsa{bits}.pkcs1"), base, ""));
    }
    for suffix in ["aes128.pem", "aes192.pem", "aes256.pem", "encrypted.pkcs8"] {
        cases.push((
            format!("rsa4096.{suffix}"),
            "rsa4096.pkcs8".into(),
            "termterm-disposable-test",
        ));
    }
    for bits in [256, 384, 521] {
        let base = format!("ecdsa{bits}.pkcs8");
        cases.push((base.clone(), base.clone(), ""));
        cases.push((format!("ecdsa{bits}.sec1"), base.clone(), ""));
        cases.push((
            format!("ecdsa{bits}.aes256.pem"),
            base,
            "termterm-disposable-test",
        ));
    }
    cases.push(("ed25519.pkcs8".into(), "ed25519.pkcs8".into(), ""));
    cases.push((
        "ed25519.encrypted.pkcs8".into(),
        "ed25519.pkcs8".into(),
        "termterm-disposable-test",
    ));
    for stem in ["id_ecdsa_p256", "id_ed25519", "id_rsa_3072"] {
        for encrypted in [false, true] {
            cases.push((
                format!("{stem}{}.ppk", if encrypted { "_enc" } else { "" }),
                format!("{stem}.ppk"),
                if encrypted { "123" } else { "" },
            ));
        }
    }
    for encrypted in [false, true] {
        cases.push((
            format!("id_rsa_3072{}.ppk2", if encrypted { "_enc" } else { "" }),
            "id_rsa_3072.ppk".into(),
            if encrypted { "123" } else { "" },
        ));
    }
    cases
}

#[test]
fn independent_key_files_decode_and_reject_wrong_passwords() {
    for (name, base, password) in format_cases() {
        let text = fixture_text(&name);
        let key = decode_private_key(&text, password).unwrap_or_else(|e| panic!("{name}: {e:#}"));
        let expected = decode_private_key(&fixture_text(&base), "").unwrap();
        assert_eq!(
            key.public_key().key_data(),
            expected.public_key().key_data(),
            "{name}"
        );
        let windows_text = format!(
            " \n\u{feff}{}",
            text.replace("\r\n", "\n").replace('\n', "\r\n")
        );
        assert_eq!(
            decode_private_key(&windows_text, password)
                .unwrap()
                .public_key()
                .key_data(),
            key.public_key().key_data(),
            "BOM/CRLF: {name}"
        );
        if !password.is_empty() {
            assert!(
                decode_private_key(&text, "wrong-password").is_err(),
                "wrong password accepted: {name}"
            );
            assert!(
                decode_private_key(&text, "").is_err(),
                "missing password accepted: {name}"
            );
        } else {
            assert!(
                decode_private_key(&text, "stale-passphrase").is_ok(),
                "unused password: {name}"
            );
        }
    }
}

#[tokio::test]
async fn independent_key_formats_authenticate_over_real_ssh() {
    for (name, _, password) in format_cases() {
        let key = decode_private_key(&fixture_text(&name), password).unwrap();
        let (_server, connected) = connect_fixture(&key, Some(HashAlg::Sha512), true, false).await;
        let success = tokio::time::timeout(
            Duration::from_secs(10),
            authenticate_key(&mut connected.unwrap(), "fixture", Arc::new(key)),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(success.success(), "SSH authentication failed: {name}");
    }
}

#[test]
fn damaged_pem_and_unsupported_ciphers_fail_without_panicking() {
    let text = fixture_text("rsa4096.aes256.pem");
    for damaged in [
        text.replace("AES-256-CBC", "UNKNOWN"),
        text.replace("-----END RSA PRIVATE KEY-----", ""),
        text.replace("DEK-Info:", "missing:"),
        text.replace("Proc-Type: 4,ENCRYPTED", "Proc-Type: broken"),
    ] {
        assert!(decode_private_key(&damaged, "termterm-disposable-test").is_err());
    }
    assert!(decode_private_key("not a key", "").is_err());
}

#[test]
fn second_factor_requires_explicit_partial_success() {
    let methods=russh::MethodSet::from(&[russh::MethodKind::KeyboardInteractive,russh::MethodKind::Password][..]);
    let rejected=client::AuthResult::Failure{remaining_methods:methods.clone(),partial_success:false};
    assert!(!interactive_factor(&rejected));assert!(!password_factor(&rejected));
    let partial=client::AuthResult::Failure{remaining_methods:methods,partial_success:true};
    assert!(partial_auth(&partial));assert!(interactive_factor(&partial));assert!(password_factor(&partial));
    assert!(!interactive_factor(&client::AuthResult::Success));
}
#[test]
fn agent_certificate_selection_does_not_alias_its_plain_key() {
    use russh::keys::ssh_key::certificate;
    let key=rsa4096();let ca=random_key(Algorithm::Ed25519);
    let mut builder=certificate::Builder::new_with_random_nonce(&mut safe_rng(),key.public_key(),0,u64::MAX).unwrap();
    builder.valid_principal("fixture").unwrap();
    let cert=builder.sign(&ca).unwrap();
    let plain=agent_identity_id(&key.public_key().clone().into()).unwrap();
    let signed=agent_identity_id(&cert.into()).unwrap();
    assert!(plain.starts_with("SHA256:"));assert!(signed.starts_with("CERT-SHA256:"));assert_ne!(plain,signed);
}

#[cfg(unix)]
#[tokio::test]
async fn real_openssh_agent_signs_without_exporting_private_key() {
    use std::process::{Command,Stdio};
    struct Agent(std::process::Child);
    impl Drop for Agent { fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();} }
    let dir=tempfile::tempdir().unwrap();let socket=dir.path().join("agent.sock");
    let _agent=Agent(Command::new("ssh-agent").args(["-D","-a"]).arg(&socket).stdout(Stdio::null()).stderr(Stdio::null()).spawn().expect("Install OpenSSH ssh-agent for this test"));
    for _ in 0..50 {if socket.exists(){break;}tokio::time::sleep(Duration::from_millis(20)).await;}
    let key=rsa4096();let file=dir.path().join("key");
    std::fs::write(&file,key.to_openssh(LineEnding::LF).unwrap().as_bytes()).unwrap();
    let ca=random_key(Algorithm::Ed25519);
    let mut builder=russh::keys::ssh_key::certificate::Builder::new_with_random_nonce(&mut safe_rng(),key.public_key(),0,u64::MAX).unwrap();
    builder.valid_principal("fixture").unwrap();
    std::fs::write(dir.path().join("key-cert.pub"),builder.sign(&ca).unwrap().to_openssh().unwrap()).unwrap();
    use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&file,std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(Command::new("ssh-add").arg(&file).env("SSH_AUTH_SOCK",&socket).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap().success());
    let stream=tokio::net::UnixStream::connect(&socket).await.unwrap();
    let mut agent=russh::keys::agent::client::AgentClient::connect(stream);
    let keys=agent.request_identities().await.unwrap();assert_eq!(keys.len(),2);
    assert!(keys.iter().any(|k|matches!(k,russh::keys::agent::AgentIdentity::Certificate{..})));
    for identity in keys {
        let (_server,connected)=connect_fixture(key,Some(HashAlg::Sha512),true,false).await;
        assert!(authenticate_agent_identity(&mut connected.unwrap(),"fixture",&identity,&mut agent).await.unwrap().success());
    }
    assert!(Command::new("ssh-add").arg("-d").arg(&file).env("SSH_AUTH_SOCK",&socket).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap().success());
    assert!(agent.request_identities().await.unwrap().is_empty());
}
