use crate::{crypto, vault::Vault};
use anyhow::{anyhow, ensure, Result};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::Path;

const MAX_BYTES: i64 = 48 * 1024 * 1024;
const MAX_COUNT: i64 = 40;
fn open(root: &Path, vault: &Vault) -> Result<Connection> {
    ensure!(
        !vault.id.is_empty()
            && vault.id.len() <= 128
            && vault
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "Geçersiz kasa kimliği"
    );
    std::fs::create_dir_all(root)?;
    let c = Connection::open(root.join(format!("{}.schema.sqlite", vault.id)))?;
    c.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA auto_vacuum=FULL; PRAGMA secure_delete=ON; CREATE TABLE IF NOT EXISTS snapshots(id TEXT PRIMARY KEY, payload BLOB NOT NULL);")?;
    Ok(c)
}
fn aad(vault: &Vault, id: &str) -> String {
    format!("termterm-schema-v1:{}:{id}", vault.id)
}
fn decode(c: &Connection, vault: &Vault, id: &str) -> Result<Value> {
    let bytes: Vec<u8> = c.query_row("SELECT payload FROM snapshots WHERE id=?1", [id], |r| {
        r.get(0)
    })?;
    let value: Value = serde_json::from_slice(&crypto::unseal(
        &vault.key,
        &bytes,
        aad(vault, id).as_bytes(),
    )?)?;
    ensure!(value["format"] == 1, "Şema görüntüsü sürümü desteklenmiyor");
    Ok(value)
}
pub fn save(root: &Path, vault: &Vault, record_id: &str, mut snapshot: Value) -> Result<Value> {
    ensure!(
        snapshot["format"] == 1 && snapshot["objects"].is_array() && snapshot["source"].is_string(),
        "Geçersiz şema görüntüsü"
    );
    let id = uuid::Uuid::new_v4().to_string();
    snapshot["id"] = json!(id);
    snapshot["recordId"] = json!(record_id);
    let bytes = serde_json::to_vec(&snapshot)?;
    ensure!(bytes.len() <= 4_100_000, "Şema görüntüsü çok büyük");
    let payload = crypto::seal(&vault.key, &bytes, aad(vault, &id).as_bytes())?;
    let mut c = open(root, vault)?;
    let tx = c.transaction()?;
    let (count, size): (i64, i64) = tx.query_row(
        "SELECT count(*),coalesce(sum(length(payload)+128),0) FROM snapshots",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    ensure!(count<MAX_COUNT && size+payload.len() as i64+128<=MAX_BYTES,"Şema görüntüsü sınırı (40 görüntü / 48 MiB). Önce gereksiz görüntüleri silin; mevcut görüntüler korundu.");
    tx.execute(
        "INSERT INTO snapshots(id,payload) VALUES(?1,?2)",
        params![id, payload],
    )?;
    tx.commit()?;
    Ok(snapshot)
}
pub fn request(root: &Path, vault: &Vault, record_id: &str, request: &Value) -> Result<Value> {
    let c = open(root, vault)?;
    if request["op"] == "snapshotList" {
        let mut st = c.prepare("SELECT id FROM snapshots")?;
        let mut items = vec![];
        for row in st.query_map([], |r| r.get::<_, String>(0))? {
            let mut v = decode(&c, vault, &row?)?;
            if v["recordId"] == record_id {
                v["objectCount"] = json!(v["objects"].as_array().map_or(0, Vec::len));
                v.as_object_mut().unwrap().remove("objects");
                items.push(v);
            }
        }
        items.sort_by_key(|v| std::cmp::Reverse(v["capturedAt"].as_i64().unwrap_or(0)));
        return Ok(json!({"items":items,"maxCount":MAX_COUNT,"maxBytes":MAX_BYTES}));
    }
    let id = request["id"]
        .as_str()
        .ok_or_else(|| anyhow!("Görüntü seçin"))?;
    let v = decode(&c, vault, id)?;
    ensure!(
        v["recordId"] == record_id,
        "Görüntü bu bağlantıya ait değil"
    );
    if request["op"] == "snapshotDelete" {
        c.execute("DELETE FROM snapshots WHERE id=?1", [id])?;
        return Ok(json!({"deleted":true}));
    }
    ensure!(request["op"] == "snapshotGet", "Geçersiz görüntü işlemi");
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_scoped_atomic_snapshots() {
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::create(
            &root.path().join("vault.ttvault"),
            "test",
            "long-test-password",
        )
        .unwrap();
        let example = json!({"format":1,"source":"private-host-scope","schema":"private_schema","objects":[],"capturedAt":1});
        let first = save(root.path(), &vault, "connection", example.clone()).unwrap();
        let id = first["id"].as_str().unwrap();
        let file = std::fs::read(root.path().join(format!("{}.schema.sqlite", vault.id))).unwrap();
        for secret in ["connection", "private_schema", "private-host-scope"] {
            assert!(!file.windows(secret.len()).any(|w| w == secret.as_bytes()));
        }
        let get = json!({"op":"snapshotGet","id":id});
        assert_eq!(
            request(root.path(), &vault, "connection", &get).unwrap(),
            first
        );
        assert!(request(root.path(), &vault, "other", &get).is_err());
        assert!(request(
            root.path(),
            &vault,
            "other",
            &json!({"op":"snapshotDelete","id":id})
        )
        .is_err());
        assert_eq!(
            request(root.path(), &vault, "other", &json!({"op":"snapshotList"})).unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        for _ in 1..MAX_COUNT {
            save(root.path(), &vault, "connection", example.clone()).unwrap();
        }
        assert!(save(root.path(), &vault, "connection", example.clone()).is_err());
        assert_eq!(
            request(root.path(), &vault, "connection", &get).unwrap(),
            first
        );
        request(
            root.path(),
            &vault,
            "connection",
            &json!({"op":"snapshotDelete","id":id}),
        )
        .unwrap();
        assert!(request(root.path(), &vault, "connection", &get).is_err());
        save(root.path(), &vault, "connection", example).unwrap();
        let c = open(root.path(), &vault).unwrap();
        c.execute("UPDATE snapshots SET payload=zeroblob(100) WHERE id=(SELECT id FROM snapshots LIMIT 1)",[]).unwrap();
        assert!(request(
            root.path(),
            &vault,
            "connection",
            &json!({"op":"snapshotList"})
        )
        .is_err());
    }
}
