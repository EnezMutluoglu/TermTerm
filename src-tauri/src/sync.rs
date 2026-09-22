use crate::{
    crypto::{self, Envelope},
    model::Record,
    state::Shared,
    vault::Vault,
};
use anyhow::{anyhow, ensure, Context, Result};
use postgres_native_tls::MakeTlsConnector;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use tokio_postgres::{config::SslMode, Client};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncProfile {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub schema: String,
    pub ca_path: String,
    pub tls: String,
}
impl Default for SyncProfile {
    fn default() -> Self {
        Self {
            label: "PostgreSQL".into(),
            host: "localhost".into(),
            port: 55432,
            database: "termterm_dev".into(),
            username: "termterm_app".into(),
            password: String::new(),
            schema: "termterm".into(),
            ca_path: String::new(),
            tls: "verify-full".into(),
        }
    }
}
pub fn schema(p: &SyncProfile) -> Result<String> {
    ensure!(
        !p.schema.is_empty()
            && p.schema.len() < 64
            && p.schema
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !p.schema.starts_with(|c: char| c.is_ascii_digit()),
        "Schema must be an SQL identifier"
    );
    Ok(format!("\"{}\"", p.schema))
}
pub async fn connect(p: &SyncProfile) -> Result<Client> {
    let _ = schema(p)?;
    ensure!(
        p.tls == "verify-full",
        "TLS with hostname verification is required"
    );
    let mut config = tokio_postgres::Config::new();
    config
        .host(&p.host)
        .port(p.port)
        .dbname(&p.database)
        .user(&p.username)
        .password(&p.password)
        .ssl_mode(SslMode::Require)
        .connect_timeout(std::time::Duration::from_secs(8));
    let mut builder = native_tls::TlsConnector::builder();
    builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
    if !p.ca_path.is_empty() {
        builder.add_root_certificate(native_tls::Certificate::from_pem(
            &std::fs::read(&p.ca_path).context("Cannot read CA certificate")?,
        )?);
    }
    let (client, connection) = config
        .connect(MakeTlsConnector::new(builder.build()?))
        .await
        .context("PostgreSQL connection failed; local vault remains available")?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    Ok(client)
}
pub async fn test(p: &SyncProfile) -> Result<Value> {
    let client = connect(p).await?;
    let row = client
        .query_one(
            "SELECT current_database(), session_user::text, version()",
            &[],
        )
        .await?;
    let s = schema(p)?;
    let exists: bool = client
        .query_one(
            "SELECT to_regclass($1) IS NOT NULL",
            &[&format!("{s}.schema_version")],
        )
        .await?
        .get(0);
    let schema_version: Option<i32> = if exists {
        client
            .query_one(&format!("SELECT max(version) FROM {s}.schema_version"), &[])
            .await?
            .get(0)
    } else {
        None
    };
    let required = [
        "schema_version",
        "vaults",
        "members",
        "records",
        "operations",
        "terminal_sessions",
        "terminal_frames",
    ]
    .map(|name| format!("{s}.{name}"))
    .to_vec();
    let missing: Vec<String> = client
        .query(
            "SELECT name FROM unnest($1::text[]) AS name WHERE to_regclass(name) IS NULL",
            &[&required],
        )
        .await?
        .into_iter()
        .map(|r| r.get(0))
        .collect();
    Ok(
        json!({"database":row.get::<_,String>(0),"username":row.get::<_,String>(1),"version":row.get::<_,String>(2),"tls":"verify-full","schemaReady":missing.is_empty() && schema_version == Some(2),"schemaVersion":schema_version,"missingTables":missing}),
    )
}
pub async fn prepare(p: &SyncProfile) -> Result<()> {
    let client = connect(p).await?;
    let name = schema(p)?;
    let exists: bool = client
        .query_one(
            "SELECT to_regclass($1) IS NOT NULL",
            &[&format!("{name}.schema_version")],
        )
        .await?
        .get(0);
    if exists {
        let version: Option<i32> = client
            .query_one(
                &format!("SELECT max(version) FROM {name}.schema_version"),
                &[],
            )
            .await?
            .get(0);
        ensure!(
            version.unwrap_or(0) <= 2,
            "Database schema is newer than this application; no changes made"
        );
    }
    let migrations = format!(
        "{}\n{}\n{}",
        include_str!("../../migrations/001_sync.sql"),
        include_str!("../../migrations/002_shared_terminal.sql"),
        include_str!("../../migrations/003_schema_version.sql")
    );
    let sql = migrations
        .replace("termterm.", &format!("{}.", name))
        .replace("EXISTS termterm;", &format!("EXISTS {};", name))
        .replace("pg_catalog,termterm", &format!("pg_catalog,{}", name))
        .replace("SCHEMA termterm", &format!("SCHEMA {}", name));
    client.batch_execute(&sql).await?;
    Ok(())
}
fn target(p: &SyncProfile) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(
        json!([p.host.to_lowercase(), p.port, p.database, p.schema]).to_string(),
    ))
}
fn binding(v: &Vault) -> Result<Option<String>> {
    Ok(v.conn
        .query_row("SELECT value FROM state WHERE key='sync_target'", [], |r| {
            r.get(0)
        })
        .optional()?)
}
fn check_target(v: &Vault, p: &SyncProfile, allow_unbound: bool) -> Result<()> {
    match binding(v)? {
        Some(current) => ensure!(
            current == target(p),
            "Database target changed. Preview and connect this target before syncing."
        ),
        None => ensure!(
            allow_unbound,
            "Preview and connect this database target before syncing."
        ),
    }
    Ok(())
}
fn save_binding(v: &Vault, p: &SyncProfile) -> Result<()> {
    v.conn.execute("INSERT INTO state(key,value) VALUES('sync_target',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[target(p)])?;
    Ok(())
}
pub async fn preview(state: &Shared, p: &SyncProfile) -> Result<Value> {
    use sha2::{Digest, Sha256};
    let (id, key, local, pending, current) = {
        let g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        let v = g.as_ref().ok_or_else(|| anyhow!("Vault locked"))?;
        (
            v.id.clone(),
            v.key.clone(),
            v.records()?,
            v.pending()?,
            binding(v)?,
        )
    };
    let mut client = connect(p).await?;
    let c = client
        .build_transaction()
        .isolation_level(tokio_postgres::IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()
        .await?;
    let s = schema(p)?;
    let uuid = Uuid::parse_str(&id)?;
    let header = c
        .query_opt(
            &format!("SELECT revision FROM {s}.vaults WHERE id=$1"),
            &[&uuid],
        )
        .await?;
    let revision: Option<i64> = header.as_ref().map(|r| r.get(0));
    let mut changes = Vec::new();
    let changing_target = current.as_deref() != Some(target(p).as_str());
    let mut remote_ids = std::collections::HashSet::new();
    if header.is_some() {
        let local_by_id: std::collections::HashMap<_, _> =
            local.iter().map(|r| (r.id.as_str(), r)).collect();
        for row in c
            .query(
                &format!(
                    "SELECT id,payload,deleted FROM {s}.records WHERE vault_id=$1 ORDER BY id"
                ),
                &[&uuid],
            )
            .await?
        {
            let rid: Uuid = row.get(0);
            let rid = rid.to_string();
            remote_ids.insert(rid.clone());
            let bytes: Vec<u8> = row.get(1);
            let deleted: bool = row.get(2);
            let clear = crypto::unseal(&key, &bytes, rid.as_bytes())
                .context("Remote records do not match this vault key")?;
            let remote: Option<Record> = if deleted {
                None
            } else {
                Some(serde_json::from_slice(&clear)?)
            };
            let here = local_by_id.get(rid.as_str()).copied();
            if here != remote.as_ref() {
                changes.push(json!({"id":rid,"label":here.or(remote.as_ref()).and_then(|r|r.data["label"].as_str()).unwrap_or("Deleted record"),"action":if pending.iter().any(|o|o.record_id==rid) || (changing_target && here.is_some()) {"conflict"}else if deleted{"delete-local"}else{"download"}}));
            }
        }
    }
    for record in &local {
        if record.kind != "syncProfile" && !remote_ids.contains(&record.id) {
            changes.push(json!({"id":record.id,"label":record.data["label"],"action":"upload"}));
        }
    }
    let token = hex::encode(Sha256::digest(serde_json::to_vec(&json!([
        id,
        target(p),
        revision,
        local,
        pending
    ]))?));
    Ok(
        json!({"vaultId":id,"target":target(p),"token":token,"registered":header.is_some(),"revision":revision,"targetChanged":current.as_ref().is_some_and(|v|v!=&target(p)),"localRecords":local.len(),"pending":pending.len(),"changes":changes}),
    )
}
pub async fn bind(state: &Shared, p: &SyncProfile, token: &str) -> Result<()> {
    let _gate = state.sync_gate.lock().await;
    let checked = preview(state, p).await?;
    ensure!(
        checked["token"] == token,
        "Vault or database changed; refresh the preview"
    );
    let mut g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
    let v = g.as_mut().ok_or_else(|| anyhow!("Vault locked"))?;
    ensure!(
        Some(v.id.as_str()) == checked["vaultId"].as_str(),
        "Vault changed"
    );
    use sha2::{Digest, Sha256};
    let current = hex::encode(Sha256::digest(serde_json::to_vec(&json!([
        v.id,
        target(p),
        checked["revision"],
        v.records()?,
        v.pending()?
    ]))?));
    ensure!(
        current == token,
        "Local changes arrived during preview; refresh it before connecting"
    );
    if binding(v)?.as_deref() != Some(target(p).as_str()) {
        let records = v.records()?;
        let tx = v.conn.transaction()?;
        // Preserve local-only and divergent records on first binding / target changes.
        // Their zero base revision forces a reviewable conflict for existing remote IDs.
        for record in records.iter().filter(|r| r.kind != "syncProfile") {
            if checked["changes"].as_array().is_some_and(|changes| {
                changes.iter().any(|c| {
                    c["id"] == record.id && (c["action"] == "upload" || c["action"] == "conflict")
                })
            }) {
                let payload =
                    crypto::seal(&v.key, &serde_json::to_vec(record)?, record.id.as_bytes())?;
                tx.execute("INSERT INTO outbox SELECT ?1,?2,0,0,?3 WHERE NOT EXISTS(SELECT 1 FROM outbox WHERE record_id=?2)",params![Uuid::new_v4().to_string(),record.id,payload])?;
            }
        }
        tx.execute("UPDATE records SET server_revision=0", [])?;
        tx.execute("UPDATE outbox SET base_revision=0", [])?;
        tx.execute("INSERT INTO state(key,value) VALUES('sync_target',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[target(p)])?;
        tx.commit()?;
    }
    Ok(())
}
pub async fn list_remote(p: &SyncProfile) -> Result<Vec<Value>> {
    let c = connect(p).await?;
    let s = schema(p)?;
    let rows=c.query(&format!("SELECT id,revision,created_at::text,{}.member_role(id) FROM {}.vaults ORDER BY created_at",s,s),&[]).await?;
    Ok(rows.iter().map(|r|json!({"id":r.get::<_,Uuid>(0).to_string(),"revision":r.get::<_,i64>(1),"createdAt":r.get::<_,String>(2),"role":r.get::<_,String>(3)})).collect())
}
pub async fn upload_new(state: &Shared, p: &SyncProfile) -> Result<()> {
    let (id, envelope, name) = {
        let g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        let v = g.as_ref().ok_or_else(|| anyhow!("Vault locked"))?;
        check_target(v, p, true)?;
        let name: Vec<u8> = v
            .conn
            .query_row("SELECT name FROM header WHERE id=1", [], |r| r.get(0))?;
        (Uuid::parse_str(&v.id)?, v.envelope()?, name)
    };
    let c = connect(p).await?;
    let s = schema(p)?;
    c.execute(
        &format!("SELECT {}.create_vault($1,$2,$3)", s),
        &[&id, &envelope, &name],
    )
    .await?;
    // Portable copies have an empty outbox, so enqueue all records explicitly for a new DB.
    let mut guard = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
    let v = guard.as_mut().ok_or_else(|| anyhow!("Vault locked"))?;
    ensure!(v.id == id.to_string(), "Vault changed");
    let records = v
        .records()?
        .into_iter()
        .filter(|r| r.kind != "syncProfile")
        .collect::<Vec<_>>();
    v.conn.execute("UPDATE records SET server_revision=0", [])?;
    v.put(&records)?;
    save_binding(v, p)?;
    Ok(())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub uploaded: usize,
    pub downloaded: usize,
    pub conflicts: Vec<Value>,
    pub revision: i64,
}
pub async fn synchronize(state: &Shared, p: &SyncProfile) -> Result<SyncReport> {
    let _gate = state.sync_gate.lock().await;
    let (id, key, pending) = {
        let g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        let v = g.as_ref().ok_or_else(|| anyhow!("Vault locked"))?;
        check_target(v, p, false)?;
        (Uuid::parse_str(&v.id)?, v.key.clone(), v.pending()?)
    };
    let c = connect(p).await?;
    let s = schema(p)?;
    let header=c.query_opt(&format!("SELECT revision,envelope FROM {}.vaults WHERE id=$1",s),&[&id]).await?.ok_or_else(||anyhow!("This vault is not registered on the server. Use Upload local vault or Open remote vault."))?;
    let mut report = SyncReport {
        uploaded: 0,
        downloaded: 0,
        conflicts: vec![],
        revision: header.get(0),
    };
    for op in pending {
        if !op.deleted {
            let record: Record = serde_json::from_slice(&crypto::unseal(
                &key,
                &op.payload,
                op.record_id.as_bytes(),
            )?)?;
            if record.kind == "syncProfile" {
                continue;
            }
        }
        let rid = Uuid::parse_str(&op.record_id)?;
        let opid = Uuid::parse_str(&op.op_id)?;
        match c
            .query_one(
                &format!("SELECT {}.apply_operation($1,$2,$3,$4,$5,$6)", s),
                &[
                    &id,
                    &opid,
                    &rid,
                    &op.base_revision,
                    &op.payload,
                    &op.deleted,
                ],
            )
            .await
        {
            Ok(row) => {
                let revision: i64 = row.get(0);
                let mut g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
                let v = g
                    .as_mut()
                    .filter(|v| v.id == id.to_string())
                    .ok_or_else(|| anyhow!("Vault changed"))?;
                let tx = v.conn.transaction()?;
                tx.execute("DELETE FROM outbox WHERE op_id=?1", [&op.op_id])?;
                tx.execute(
                    "UPDATE records SET server_revision=MAX(server_revision,?2) WHERE id=?1",
                    params![op.record_id, revision],
                )?;
                tx.execute(
                    "UPDATE outbox SET base_revision=MAX(base_revision,?2) WHERE record_id=?1",
                    params![op.record_id, revision],
                )?;
                tx.commit()?;
                report.uploaded += 1;
            }
            Err(e)
                if e.as_db_error()
                    .is_some_and(|e| e.message().contains("TERMTTERM_CONFLICT")) => {}
            Err(e) => return Err(e.into()),
        }
    }
    let rows=c.query(&format!("SELECT id,revision,payload,deleted FROM {}.records WHERE vault_id=$1 ORDER BY revision",s),&[&id]).await?;
    let mut g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
    let v = g
        .as_mut()
        .filter(|v| v.id == id.to_string())
        .ok_or_else(|| anyhow!("Vault changed"))?;
    let tx = v.conn.transaction()?;
    for row in rows {
        let rid: Uuid = row.get(0);
        let rid = rid.to_string();
        let revision: i64 = row.get(1);
        let payload: Vec<u8> = row.get(2);
        let deleted: bool = row.get(3);
        report.revision = report.revision.max(revision);
        let clear = crypto::unseal(&key, &payload, rid.as_bytes())?;
        let remote = if deleted {
            Value::Null
        } else {
            serde_json::from_slice::<Value>(&clear)?
        };
        let local_revision: Option<i64> = tx
            .query_row(
                "SELECT server_revision FROM records WHERE id=?1",
                [&rid],
                |r| r.get(0),
            )
            .optional()?;
        if local_revision.is_some_and(|r| r >= revision) {
            continue;
        }
        let dirty: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM outbox WHERE record_id=?1)",
            [&rid],
            |r| r.get(0),
        )?;
        if dirty {
            let local: Option<Vec<u8>> = tx
                .query_row("SELECT payload FROM records WHERE id=?1", [&rid], |r| {
                    r.get(0)
                })
                .optional()?;
            let local = local
                .map(|b| {
                    crypto::unseal(&key, &b, rid.as_bytes())
                        .and_then(|b| Ok(serde_json::from_slice::<Value>(&b)?))
                })
                .transpose()?
                .unwrap_or(Value::Null);
            report.conflicts.push(json!({"id":rid,"revision":revision,"local":local,"remote":remote,"deleted":deleted}));
            continue;
        }
        tx.execute("INSERT INTO records(id,payload,server_revision,deleted) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload,server_revision=excluded.server_revision,deleted=excluded.deleted",params![rid,payload,revision,deleted])?;
        report.downloaded += 1;
    }
    tx.commit()?;
    Ok(report)
}
pub async fn resolve(
    state: &Shared,
    p: &SyncProfile,
    rid: &str,
    choice: &str,
    expected_revision: i64,
) -> Result<()> {
    ensure!(
        ["local", "remote", "both"].contains(&choice),
        "Invalid conflict choice"
    );
    let id = {
        let g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        Uuid::parse_str(&g.as_ref().ok_or_else(|| anyhow!("Vault locked"))?.id)?
    };
    let rid_uuid = Uuid::parse_str(rid)?;
    let c = connect(p).await?;
    let s = schema(p)?;
    let row = c
        .query_one(
            &format!(
                "SELECT revision,payload,deleted FROM {}.records WHERE vault_id=$1 AND id=$2",
                s
            ),
            &[&id, &rid_uuid],
        )
        .await?;
    let revision: i64 = row.get(0);
    ensure!(
        revision == expected_revision,
        "Remote changed again; refresh the conflict preview"
    );
    let payload: Vec<u8> = row.get(1);
    let deleted: bool = row.get(2);
    let mut g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
    let v = g
        .as_mut()
        .filter(|v| v.id == id.to_string())
        .ok_or_else(|| anyhow!("Vault changed"))?;
    crypto::unseal(&v.key, &payload, rid.as_bytes())?;
    if choice == "local" {
        v.conn.execute(
            "UPDATE outbox SET base_revision=?2,op_id=?3 WHERE record_id=?1",
            params![rid, revision, Uuid::new_v4().to_string()],
        )?;
    } else {
        if choice == "both" {
            if let Some(mut local) = v.records()?.into_iter().find(|r| r.id == rid) {
                local.id = Uuid::new_v4().to_string();
                local.data["label"] = format!(
                    "{} (local copy)",
                    local.data["label"].as_str().unwrap_or("Record")
                )
                .into();
                v.put(&[local])?;
            }
        }
        let tx = v.conn.transaction()?;
        tx.execute("DELETE FROM outbox WHERE record_id=?1", [rid])?;
        tx.execute("INSERT INTO records(id,payload,server_revision,deleted) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload,server_revision=excluded.server_revision,deleted=excluded.deleted",params![rid,payload,revision,deleted])?;
        tx.commit()?;
    }
    Ok(())
}
pub async fn download(p: &SyncProfile, id: &str, password: &str, path: &Path) -> Result<Vault> {
    ensure!(!path.exists(), "Destination exists");
    let c = connect(p).await?;
    let s = schema(p)?;
    let uuid = Uuid::parse_str(id)?;
    let row=c.query_one(&format!("SELECT COALESCE(m.key_envelope,v.envelope),v.encrypted_name FROM {}.vaults v JOIN {}.members m ON m.vault_id=v.id AND m.principal=session_user WHERE v.id=$1",s,s),&[&uuid]).await?;
    let envelope: Vec<u8> = row.get(0);
    let encrypted_name: Vec<u8> = row.get(1);
    let parsed: Envelope = serde_json::from_slice(&envelope)?;
    let key = parsed.unlock(password, id.as_bytes())?;
    let name = String::from_utf8(crypto::unseal(&key, &encrypted_name, b"vault-name")?.to_vec())?;
    let rows = c
        .query(
            &format!(
                "SELECT id,revision,payload,deleted FROM {}.records WHERE vault_id=$1",
                s
            ),
            &[&uuid],
        )
        .await?;
    for row in &rows {
        let rid: Uuid = row.get(0);
        let payload: Vec<u8> = row.get(2);
        crypto::unseal(&key, &payload, rid.to_string().as_bytes())?;
    }
    let mut v = Vault::create(path, &name, password)?;
    v.id = id.into();
    v.key = key;
    let tx = v.conn.transaction()?;
    tx.execute(
        "UPDATE header SET vault_id=?1,envelope=?2,name=?3",
        params![id, envelope, encrypted_name],
    )?;
    for row in rows {
        let rid: Uuid = row.get(0);
        let revision: i64 = row.get(1);
        let payload: Vec<u8> = row.get(2);
        let deleted: bool = row.get(3);
        tx.execute(
            "INSERT INTO records(id,payload,server_revision,deleted) VALUES(?1,?2,?3,?4)",
            params![rid.to_string(), payload, revision, deleted],
        )?;
    }
    tx.commit()?;
    save_binding(&v, p)?;
    Ok(v)
}
pub async fn members(state: &Shared, p: &SyncProfile) -> Result<Vec<Value>> {
    let id = {
        let g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        Uuid::parse_str(&g.as_ref().ok_or_else(|| anyhow!("Vault locked"))?.id)?
    };
    let c = connect(p).await?;
    let s = schema(p)?;
    Ok(c.query(
        &format!(
            "SELECT principal::text,role FROM {}.members WHERE vault_id=$1 ORDER BY principal",
            s
        ),
        &[&id],
    )
    .await?
    .iter()
    .map(|r| json!({"username":r.get::<_,String>(0),"role":r.get::<_,String>(1)}))
    .collect())
}
pub async fn set_member(
    state: &Shared,
    p: &SyncProfile,
    username: &str,
    role: &str,
    password: &str,
) -> Result<()> {
    let (id, key) = {
        let g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        let v = g.as_ref().ok_or_else(|| anyhow!("Vault locked"))?;
        (Uuid::parse_str(&v.id)?, v.key.clone())
    };
    let envelope = if role == "remove" {
        None
    } else {
        Some(serde_json::to_vec(&Envelope::create(
            password,
            &key,
            id.to_string().as_bytes(),
        )?)?)
    };
    let c = connect(p).await?;
    let s = schema(p)?;
    c.execute(
        &format!("SELECT {}.set_member($1,$2::text::name,$3,$4)", s),
        &[&id, &username, &role, &envelope],
    )
    .await?;
    Ok(())
}

pub async fn watch(
    app: tauri::AppHandle,
    state: Shared,
    profile: SyncProfile,
    enabled: bool,
) -> Result<()> {
    use tauri::Emitter;
    if let Some(task) = state
        .sync_task
        .lock()
        .map_err(|_| anyhow!("Sync lock"))?
        .take()
    {
        task.abort();
    }
    if !enabled {
        return Ok(());
    }
    let vault_id = {
        let g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        g.as_ref()
            .ok_or_else(|| anyhow!("Vault locked"))?
            .id
            .clone()
    };
    let worker = state.clone();
    let task = tokio::spawn(async move {
        let state = worker;
        // LISTEN is an optimization. A lost listener never disables periodic reconciliation.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
        let p = profile.clone();
        let changed_id = vault_id.clone();
        let listener = tokio::spawn(async move {
            let result: Result<()> = async {
                let mut config = tokio_postgres::Config::new();
                config
                    .host(&p.host)
                    .port(p.port)
                    .dbname(&p.database)
                    .user(&p.username)
                    .password(&p.password)
                    .ssl_mode(SslMode::Require)
                    .connect_timeout(std::time::Duration::from_secs(8));
                let mut builder = native_tls::TlsConnector::builder();
                builder.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
                if !p.ca_path.is_empty() {
                    builder.add_root_certificate(native_tls::Certificate::from_pem(
                        &std::fs::read(&p.ca_path)?,
                    )?);
                }
                let (client, mut connection) = config
                    .connect(MakeTlsConnector::new(builder.build()?))
                    .await?;
                let drive = async move {
                    while let Some(message) =
                        futures_util::future::poll_fn(|cx| connection.poll_message(cx)).await
                    {
                        match message {
                            Ok(tokio_postgres::AsyncMessage::Notification(n))
                                if n.payload() == changed_id =>
                            {
                                let _ = tx.try_send(());
                            }
                            Err(_) => break,
                            _ => {}
                        }
                    }
                };
                let listen = async move {
                    client.batch_execute("LISTEN termterm_changes").await?;
                    std::future::pending::<()>().await;
                    Ok::<(), anyhow::Error>(())
                };
                tokio::select! {_=drive=>{},r=listen=>r?};
                Ok(())
            }
            .await;
            let _ = result;
        });
        struct Abort(tokio::task::JoinHandle<()>);
        impl Drop for Abort {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _listener = Abort(listener);
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(15));
        let mut notify_open = true;
        loop {
            tokio::select! {_=ticker.tick()=>{},message=rx.recv(),if notify_open=>{if message.is_none(){notify_open=false;continue;}}}
            let matches = state
                .vault
                .lock()
                .ok()
                .and_then(|g| g.as_ref().map(|v| v.id == vault_id))
                .unwrap_or(false);
            if !matches {
                break;
            }
            match synchronize(&state, &profile).await {
                Ok(report) => {
                    let _ = app.emit("sync-event", json!({"report":report}));
                }
                Err(error) => {
                    let _ = app.emit("sync-event", json!({"error":format!("{:#}",error)}));
                }
            }
        }
    });
    *state.sync_task.lock().map_err(|_| anyhow!("Sync lock"))? = Some(task);
    Ok(())
}
