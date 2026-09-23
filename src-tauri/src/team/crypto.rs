//! Separate data and connection-secret keys for every Team record revision.
use crate::{crypto, model::Record};
use anyhow::{anyhow, ensure, Result};
use crypto_box::{PublicKey, SecretKey};
use rand::rngs::OsRng;
use serde_json::{json, Value};
use zeroize::Zeroizing;

pub const IDENTITY_CONTEXT: &[u8] = b"termterm-team-identity-v1";
const SECRET_FIELDS: &[&str] = &[
    "password",
    "privateKey",
    "passphrase",
    "token",
    "secret",
    "keyPath",
    "certificatePath",
    "agent",
    "agentKey",
];
pub fn identity(password: &str) -> Result<(String, String)> {
    let secret = SecretKey::generate(&mut OsRng);
    let envelope = crypto::Envelope::create(password, &secret.to_bytes(), IDENTITY_CONTEXT)?;
    Ok((
        hex::encode(secret.public_key().as_bytes()),
        hex::encode(serde_json::to_vec(&envelope)?),
    ))
}
pub fn unlock_identity(password: &str, encoded: &str) -> Result<Zeroizing<[u8; 32]>> {
    let envelope: crypto::Envelope = serde_json::from_slice(&hex::decode(encoded)?)?;
    envelope.unlock(password, IDENTITY_CONTEXT)
}
pub fn split(record: &Record) -> (Record, Value) {
    let mut data = record.clone();
    if let Some(fields) = data.data.as_object_mut() {
        fields.retain(|key, _| !key.starts_with("_team"));
    }
    let mut secrets = json!({});
    for field in SECRET_FIELDS {
        if let Some(value) = data.data.as_object_mut().and_then(|o| o.remove(*field)) {
            secrets[*field] = value;
        }
    }
    if let Some(proxy) = data.data.get_mut("proxy").and_then(Value::as_object_mut) {
        if let Some(password) = proxy.remove("password") {
            secrets["proxyPassword"] = password;
        }
    }
    (data, secrets)
}
pub fn merge(mut record: Record, secrets: &Value) -> Record {
    for field in SECRET_FIELDS {
        if let Some(value) = secrets.get(*field) {
            record.data[*field] = value.clone();
        }
    }
    if let Some(value) = secrets.get("proxyPassword") {
        record.data["proxy"]["password"] = value.clone();
    }
    record
}
fn context(vault: &str, record: &str, revision: i64, purpose: &str) -> String {
    format!("termterm-team-v1:{vault}:{record}:{revision}:{purpose}")
}
pub(super) fn open_key(
    identity: &[u8; 32],
    envelope: &str,
    context: &str,
) -> Result<Zeroizing<[u8; 32]>> {
    let bytes = Zeroizing::new(
        SecretKey::from(*identity)
            .unseal(&hex::decode(envelope)?)
            .map_err(|_| anyhow!("Cannot open Team key envelope"))?,
    );
    ensure!(
        bytes.len() == 32 + context.len() && &bytes[32..] == context.as_bytes(),
        "Team key envelope context mismatch"
    );
    let mut key = Zeroizing::new([0; 32]);
    key.copy_from_slice(&bytes[..32]);
    Ok(key)
}
pub fn wrap_key(key: &[u8; 32], public: &str, context: &str) -> Result<String> {
    let bytes: [u8; 32] = hex::decode(public)?
        .try_into()
        .map_err(|_| anyhow!("Invalid member public key"))?;
    let payload = Zeroizing::new([key.as_slice(), context.as_bytes()].concat());
    Ok(hex::encode(
        PublicKey::from(bytes)
            .seal(&mut OsRng, &payload)
            .map_err(|_| anyhow!("Team key wrapping failed"))?,
    ))
}
pub fn encrypt(vault: &str, record: &Record, revision: i64, recipients: &Value) -> Result<Value> {
    ensure!(
        recipients
            .as_array()
            .is_some_and(|users| users.iter().any(|u| u["data"] == true)),
        "Bekleyen değişiklik için yetkili veri anahtarı alıcısı yok; yerel kayıt korunuyor"
    );
    let (data, secrets) = split(record);
    let mut result = json!({"envelopes":[]});
    for (purpose, value, field) in [
        ("data", serde_json::to_value(data)?, "payload"),
        ("secret", secrets, "secrets"),
    ] {
        let key = crypto::random_key();
        let aad = context(vault, &record.id, revision, purpose);
        result[field] = json!(hex::encode(crypto::seal(
            &key,
            &serde_json::to_vec(&value)?,
            aad.as_bytes()
        )?));
        for user in recipients
            .as_array()
            .ok_or_else(|| anyhow!("Invalid recipient list"))?
        {
            if user[purpose] == true {
                let wrapped = wrap_key(
                    &key,
                    user["publicKey"]
                        .as_str()
                        .ok_or_else(|| anyhow!("Missing public key"))?,
                    &aad,
                )?;
                result["envelopes"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"userId":user["userId"],"purpose":purpose,"envelope":wrapped}));
            }
        }
    }
    Ok(result)
}
pub fn decrypt(identity: &[u8; 32], vault: &str, row: &Value) -> Result<Record> {
    let id = row["id"]
        .as_str()
        .or_else(|| row["recordId"].as_str())
        .ok_or_else(|| anyhow!("Missing record identity"))?;
    let revision = row["revision"]
        .as_i64()
        .ok_or_else(|| anyhow!("Missing revision"))?;
    let mut values = json!({});
    for (purpose, field) in [("data", "payload"), ("secret", "secrets")] {
        let envelopes: Vec<_> = row["envelopes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["purpose"] == purpose)
            .collect();
        if !envelopes.is_empty() && row[field].is_string() {
            let aad = context(vault, id, revision, purpose);
            // Pending candidates contain multiple recipients, unlike filtered
            // normal reads. Open only the envelope addressed to this identity.
            let key = envelopes
                .iter()
                .find_map(|e| {
                    open_key(identity, e["envelope"].as_str().unwrap_or_default(), &aad).ok()
                })
                .ok_or_else(|| anyhow!("Bu sürümün anahtarı hesabınıza dağıtılmamış veya bozuk"))?;
            values[purpose] = serde_json::from_slice(&crypto::unseal(
                &key,
                &hex::decode(row[field].as_str().unwrap())?,
                aad.as_bytes(),
            )?)?;
        }
    }
    ensure!(
        values["data"].is_object(),
        "Missing authorized data key; owner must distribute envelopes"
    );
    let record: Record = serde_json::from_value(values["data"].take())?;
    ensure!(record.id == id, "Encrypted record identity mismatch");
    if let Some(kind) = row["kind"].as_str() {
        ensure!(record.kind == kind, "Encrypted record kind mismatch");
    }
    if row.get("parentId").is_some() {
        ensure!(
            record.data["groupId"].as_str().filter(|s| !s.is_empty())
                == row["parentId"].as_str().filter(|s| !s.is_empty()),
            "Encrypted folder relationship mismatch"
        );
    }
    Ok(merge(record, &values["secret"]))
}
pub fn rewrap(
    identity: &[u8; 32],
    vault: &str,
    row: &Value,
    recipients: &Value,
) -> Result<Vec<Value>> {
    let id = row["id"]
        .as_str()
        .ok_or_else(|| anyhow!("Record id missing"))?;
    let rev = row["revision"]
        .as_i64()
        .ok_or_else(|| anyhow!("Revision missing"))?;
    let mut result = vec![];
    for purpose in ["data", "secret"] {
        let aad = context(vault, id, rev, purpose);
        let envelope = row["envelopes"]
            .as_array()
            .and_then(|e| e.iter().find(|e| e["purpose"] == purpose));
        if let Some(e) = envelope {
            let key = open_key(identity, e["envelope"].as_str().unwrap_or_default(), &aad)?;
            for user in recipients
                .as_array()
                .ok_or_else(|| anyhow!("Recipients missing"))?
            {
                if user[purpose] == true {
                    result.push(json!({"recordId":id,"revision":rev,"userId":user["userId"],"purpose":purpose,"envelope":wrap_key(&key,user["publicKey"].as_str().unwrap_or_default(),&aad)?}));
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn envelopes_bind_record_revision_and_separate_secrets() -> Result<()> {
        let key = SecretKey::generate(&mut OsRng);
        let record = Record::new(
            "host",
            json!({"label":"sunucu","password":"not metadata","address":"localhost"}),
        );
        let users = json!([{"userId":"operator","publicKey":hex::encode(key.public_key().as_bytes()),"data":true,"secret":true}]);
        let mut row = encrypt("vault", &record, 3, &users)?;
        row["id"] = json!(record.id);
        row["revision"] = json!(3);
        assert_eq!(decrypt(&key.to_bytes(), "vault", &row)?, record);
        row["revision"] = json!(4);
        assert!(decrypt(&key.to_bytes(), "vault", &row).is_err());
        let (data, secret) = split(&record);
        assert!(data.data.get("password").is_none());
        assert_eq!(secret["password"], "not metadata");
        Ok(())
    }
}
