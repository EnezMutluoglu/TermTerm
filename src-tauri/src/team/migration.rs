//! Read-only v0.4.0 extraction and password-protected migration preflight.
//! No destination writes, legacy cutover, credential material or tokens in IPC replies.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::Path,
};
use tokio_postgres::{GenericClient, IsolationLevel};
const CONTEXT: &[u8] = b"termterm-team-migration-v1";
const MAX_FILE: u64 = 64 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Snapshot {
    version: u32,
    team: Value,
    captured_at: String,
    members: Value,
    pub(super) vaults: Vec<SourceVault>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SourceVault {
    id: String,
    name: String,
    revision: i64,
    pub(super) records: Vec<SourceRecord>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SourceRecord {
    id: String,
    kind: String,
    revision: i64,
    deleted: bool,
    pub(super) record: Option<Record>,
    issue: Option<String>,
}
impl Drop for Snapshot {
    fn drop(&mut self) {
        fn wipe(v: &mut Value) {
            match v {
                Value::String(s) => s.zeroize(),
                Value::Array(a) => a.iter_mut().for_each(wipe),
                Value::Object(m) => m.values_mut().for_each(wipe),
                _ => {}
            }
        }
        for v in &mut self.vaults {
            for r in &mut v.records {
                if let Some(record) = &mut r.record {
                    wipe(&mut record.data)
                }
            }
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Archive {
    magic: String,
    version: u32,
    envelope: crate::crypto::Envelope,
    ciphertext: Vec<u8>,
}
async fn read_rpc<C: GenericClient + Sync>(
    db: &C,
    token: &str,
    action: &str,
    body: Value,
) -> Result<Value> {
    ensure!(
        matches!(action, "overview" | "vaults" | "members" | "records"),
        "Read-only migration operation required"
    );
    let row = db
        .query_one(
            "SELECT termterm_team.rpc($1,$2,$3)",
            &[&token, &action, &body],
        )
        .await?;
    Ok(row.get(0))
}
pub(super) async fn capture(auth: &Auth, team_id: &str) -> Result<Snapshot> {
    Uuid::parse_str(team_id)?;
    let mut db = connect_checked(&auth.profile).await?;
    let tx = db
        .build_transaction()
        .isolation_level(IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()
        .await?;
    tx.batch_execute(
        "SET LOCAL statement_timeout='20s'; SET LOCAL idle_in_transaction_session_timeout='30s'",
    )
    .await?;
    let overview = read_rpc(&tx, &auth.access, "overview", json!({})).await?;
    let team = overview["teams"]
        .as_array()
        .context("Takım listesi alınamadı")?
        .iter()
        .find(|t| t["id"] == team_id)
        .context("Kaynak takıma erişim yok")?
        .clone();
    ensure!(
        team["ownerId"] == auth.user_id && team["role"] == "owner",
        "Geçiş arşivini yalnız eski takımın sahibi hazırlayabilir"
    );
    let members = read_rpc(&tx, &auth.access, "members", json!({"teamId":team_id})).await?;
    let list = read_rpc(&tx, &auth.access, "vaults", json!({"teamId":team_id})).await?;
    let mut vaults = vec![];
    let key = identity(auth)?;
    let mut total = 0;
    for vault in list.as_array().context("Kasa listesi alınamadı")? {
        let id = vault["id"].as_str().context("Kasa kimliği eksik")?;
        let pull = read_rpc(
            &tx,
            &auth.access,
            "records",
            json!({"teamId":team_id,"vaultId":id}),
        )
        .await?;
        let mut records = vec![];
        for row in pull["records"]
            .as_array()
            .context("Kayıt listesi alınamadı")?
        {
            total += 1;
            ensure!(
                total <= 20000,
                "Geçiş önizlemesi en fazla 20.000 kayıt destekliyor"
            );
            let has = |name: &str| {
                row["permissions"]
                    .as_array()
                    .is_some_and(|v| v.iter().any(|p| p == name))
            };
            let opened = if !has("read") || !has("export") || !has("reveal") {
                Err(anyhow!("permission"))
            } else if !row["secrets"].is_string()
                || !row["envelopes"]
                    .as_array()
                    .is_some_and(|v| v.iter().any(|e| e["purpose"] == "secret"))
            {
                Err(anyhow!("key"))
            } else {
                crypto::decrypt(&key, id, row)
            };
            let (record,issue)=match opened { Ok(r)=>(Some(r),None),Err(_)=>(None,Some("Kayıt/sır anahtarı açılamadı veya dışa aktarma yetkisi eksik; içerik atlanmış sayılmaz".into())) };
            records.push(SourceRecord {
                id: row["id"].as_str().context("Kayıt kimliği eksik")?.into(),
                kind: row["kind"].as_str().context("Tür eksik")?.into(),
                revision: row["revision"].as_i64().context("Revizyon eksik")?,
                deleted: row["deleted"] == true,
                record,
                issue,
            });
        }
        records.sort_by(|a, b| a.id.cmp(&b.id));
        vaults.push(SourceVault {
            id: id.into(),
            name: vault["name"].as_str().context("Kasa adı eksik")?.into(),
            revision: vault["revision"].as_i64().context("Kasa revizyonu eksik")?,
            records,
        });
    }
    tx.commit().await?;
    vaults.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Snapshot {
        version: 1,
        team,
        captured_at: chrono::Utc::now().to_rfc3339(),
        members,
        vaults,
    })
}
fn write_archive(path: &Path, password: &str, snapshot: &Snapshot) -> Result<()> {
    ensure!(
        password.chars().count() >= 12,
        "Arşiv parolası en az 12 karakter olmalı"
    );
    let plain = Zeroizing::new(serde_json::to_vec(snapshot)?);
    ensure!(
        plain.len() <= 16 * 1024 * 1024,
        "Geçiş içeriği 16 MiB sınırını aşıyor"
    );
    let key = crate::crypto::random_key();
    let archive = Archive {
        magic: "TERMTTERM_TEAM_MIGRATION".into(),
        version: 1,
        envelope: crate::crypto::Envelope::create(password, &key, CONTEXT)?,
        ciphertext: crate::crypto::seal(&key, &plain, CONTEXT)?,
    };
    let bytes = serde_json::to_vec(&archive)?;
    ensure!(bytes.len() as u64 <= MAX_FILE, "Geçiş dosyası çok büyük");
    let parent = path.parent().context("Çıktı klasörü gerekli")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&bytes)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path)
        .map_err(|_| anyhow!("Dosya zaten var veya yazılamıyor; mevcut dosya değiştirilmedi"))?;
    Ok(())
}
fn read_archive(path: &Path, password: &str) -> Result<(Snapshot, String)> {
    let file = std::fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= MAX_FILE,
        "Geçiş dosyası 64 MiB sınırını aşıyor"
    );
    let mut bytes = vec![];
    file.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_FILE, "Geçiş dosyası çok büyük");
    let hash = hex::encode(Sha256::digest(&bytes));
    let a: Archive =
        serde_json::from_slice(&bytes).context("Geçersiz veya kesilmiş geçiş arşivi")?;
    ensure!(
        a.magic == "TERMTTERM_TEAM_MIGRATION" && a.version == 1,
        "Desteklenmeyen geçiş dosyası; dosya değiştirilmedi"
    );
    let key = a.envelope.unlock(password, CONTEXT)?;
    let plain = crate::crypto::unseal(&key, &a.ciphertext, CONTEXT)?;
    let s: Snapshot = serde_json::from_slice(&plain)?;
    ensure!(s.version == 1, "Desteklenmeyen içerik sürümü");
    Ok((s, hash))
}
fn mapped_id(target: &str, source_team: &str, source: &str) -> String {
    let h = Sha256::digest(format!(
        "termterm-migration-map-v1:{target}:{source_team}:{source}"
    ));
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&h[..16]);
    bytes[6] = (bytes[6] & 15) | 0x80;
    bytes[8] = (bytes[8] & 63) | 0x80;
    Uuid::from_bytes(bytes).to_string()
}
/// Only labels, IDs, field names, counts and fixed error messages may leave this module.
fn preview(s: &Snapshot, hash: &str, target: &str) -> Result<Value> {
    Uuid::parse_str(target)?;
    let team = s.team["id"]
        .as_str()
        .context("Kaynak takım kimliği eksik")?;
    Uuid::parse_str(team)?;
    ensure!(
        s.vaults.iter().map(|v| v.records.len()).sum::<usize>() <= 20000,
        "Çok fazla kayıt"
    );
    let members = s.members["members"].as_array().context("Üyeler eksik")?;
    let mut member_ids = BTreeSet::new();
    let mut mapped_members = vec![];
    for m in members {
        let id = m["userId"].as_str().context("Üye kimliği eksik")?;
        Uuid::parse_str(id)?;
        ensure!(member_ids.insert(id), "Yinelenen üye kimliği");
        ensure!(
            m["username"].is_string() && m["active"].is_boolean(),
            "Geçersiz üye"
        );
        mapped_members.push(json!({"sourceId":id,"username":m["username"],"role":m["role"],"active":m["active"],"targetAccountId":Value::Null}));
    }
    ensure!(
        s.team["ownerId"]
            .as_str()
            .is_some_and(|u| member_ids.contains(u)),
        "Kaynak sahibi üye listesinde yok"
    );
    let mut all = BTreeSet::new();
    let mut rows = vec![];
    let mut issues = 0;
    let mut vault_rows = vec![];
    ensure!(s.vaults.len() <= 1000, "Çok fazla kasa");
    for v in &s.vaults {
        Uuid::parse_str(&v.id)?;
        ensure!(all.insert(v.id.as_str()), "Yinelenen kasa kimliği");
        let records: BTreeMap<_, _> = v.records.iter().map(|r| (r.id.as_str(), r)).collect();
        ensure!(records.len() == v.records.len(), "Yinelenen kayıt kimliği");
        for r in &v.records {
            Uuid::parse_str(&r.id)?;
            ensure!(all.insert(&r.id), "Yinelenen kaynak kimliği");
            let mut problems = vec![];
            if let Some(issue) = &r.issue {
                let _ = issue;
                problems.push("Kaynak içerik/anahtar açılamadı".to_string());
            }
            let mut label = String::new();
            let mut parent = None;
            let mut references = vec![];
            let mut fields = vec![];
            if let Some(record) = &r.record {
                ensure!(
                    record.id == r.id && record.kind == r.kind,
                    "Şifreli içerikte kimlik/tür uyuşmazlığı"
                );
                let d = record.data.as_object().context("Kayıt nesne olmalı")?;
                label = d
                    .get("label")
                    .or_else(|| d.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .take(160)
                    .collect();
                parent = d
                    .get("groupId")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty());
                let mut seen = BTreeSet::new();
                let mut cursor = parent;
                while let Some(id) = cursor {
                    if seen.len() >= 128 {
                        problems.push("Klasör derinliği 128 sınırını aşıyor".into());
                        break;
                    }
                    if id == r.id || !seen.insert(id) {
                        problems.push("Klasör döngüsü".into());
                        break;
                    }
                    match records.get(id) {
                        Some(p) if p.kind == "group" && !p.deleted => {
                            cursor = p
                                .record
                                .as_ref()
                                .and_then(|x| x.data["groupId"].as_str())
                                .filter(|s| !s.is_empty());
                        }
                        _ => {
                            problems.push("Eksik/silinmiş üst klasör".into());
                            break;
                        }
                    }
                }
                for dep in cache::record_dependencies(record) {
                    if !records.contains_key(dep.as_str())
                        || records.get(dep.as_str()).is_some_and(|r| r.deleted)
                    {
                        problems.push("Eksik/silinmiş kayıt bağımlılığı".into());
                    }
                    references.push(json!({"sourceId":dep,"targetId":mapped_id(target,team,&dep)}));
                }
                for key in d.keys() {
                    if key.starts_with("_team") {
                        continue;
                    }
                    let represented = match r.kind.as_str() {
                        "group" => matches!(key.as_str(), "label" | "name" | "groupId"),
                        "host" => matches!(
                            key.as_str(),
                            "label"
                                | "name"
                                | "groupId"
                                | "address"
                                | "port"
                                | "protocol"
                                | "username"
                                | "credentialId"
                                | "chain"
                                | "password"
                                | "privateKey"
                                | "passphrase"
                        ),
                        "credential" => matches!(
                            key.as_str(),
                            "label"
                                | "name"
                                | "username"
                                | "password"
                                | "privateKey"
                                | "passphrase"
                        ),
                        "snippet" => {
                            matches!(key.as_str(), "label" | "name" | "groupId" | "command")
                        }
                        "workspace" => matches!(
                            key.as_str(),
                            "label" | "name" | "groupId" | "hostIds" | "layout" | "panes"
                        ),
                        _ => false,
                    };
                    if !represented
                        && !d[key].is_null()
                        && d[key] != ""
                        && d[key] != json!([])
                        && d[key] != json!({})
                        && d[key] != false
                    {
                        fields.push(key.clone());
                    }
                }
                if r.kind == "host"
                    && d.get("protocol")
                        .and_then(Value::as_str)
                        .is_some_and(|p| !matches!(p, "ssh" | "mosh"))
                {
                    problems.push("Bu protokolün Team bağlantı yolu doğrulanmadı".into());
                }
                if !fields.is_empty() {
                    problems.push("Ek alanlar için açık dönüşüm kararı gerekli".into());
                }
            } else if problems.is_empty() {
                problems.push("Kaynak içerik eksik".into());
            }
            if !matches!(
                r.kind.as_str(),
                "host" | "group" | "credential" | "snippet" | "workspace"
            ) {
                problems.push("Hedef kayıt türü henüz desteklenmiyor".into());
            }
            problems.sort();
            problems.dedup();
            if !problems.is_empty() {
                issues += 1
            }
            rows.push(json!({"sourceId":r.id,"targetId":mapped_id(target,team,&r.id),"vaultId":v.id,"kind":r.kind,"name":label,"revision":r.revision,"deleted":r.deleted,"parentTargetId":parent.map(|p|mapped_id(target,team,p)),"references":references,"unsupportedFields":fields,"issues":problems}));
        }
        vault_rows.push(json!({"sourceId":v.id,"targetId":mapped_id(target,team,&v.id),"name":v.name,"revision":v.revision,"recordCount":v.records.len()}));
    }
    ensure!(rows.len() <= 20000, "Çok fazla kayıt");
    // Keep explicit denies/delegations visible. Do not infer grants from a role label.
    let mut rules = vec![];
    for (kind, list) in [
        ("acl", &s.members["acl"]),
        ("delegation", &s.members["delegations"]),
    ] {
        for a in list.as_array().context("İzin listesi eksik")? {
            ensure!(
                a["team_id"].as_str().is_none_or(|id| id == team),
                "İzin başka takıma bağlı"
            );
            if kind == "acl" {
                ensure!(
                    matches!(
                        a["permission"].as_str(),
                        Some("read" | "connect" | "edit" | "reveal" | "export" | "manage")
                    ) && matches!(a["effect"].as_str(), Some("allow" | "deny")),
                    "Geçersiz izin/engel"
                );
            } else {
                ensure!(
                    a["permissions"]
                        .as_array()
                        .is_some_and(|p| p.iter().all(|x| matches!(
                            x.as_str(),
                            Some("read" | "connect" | "edit" | "reveal" | "export" | "manage")
                        ))),
                    "Geçersiz yetki devri"
                );
            }
            let user = a["user_id"].as_str().context("İzin üyesi eksik")?;
            ensure!(member_ids.contains(user), "İzin tanımsız üyeye bağlı");
            let vault = a["vault_id"].as_str().context("İzin kasası eksik")?;
            ensure!(
                s.vaults.iter().any(|v| v.id == vault),
                "İzin tanımsız kasaya bağlı"
            );
            let resource = a["resource_id"].as_str();
            ensure!(
                resource.is_none_or(|id| s
                    .vaults
                    .iter()
                    .any(|v| v.id == vault && v.records.iter().any(|r| r.id == id))),
                "İzin tanımsız kaynağa bağlı"
            );
            rules.push(json!({"kind":kind,"sourceAccountId":user,"targetVaultId":mapped_id(target,team,vault),"targetResourceId":resource.map(|r|mapped_id(target,team,r)),"permission":a["permission"],"permissions":a["permissions"],"effect":a["effect"]}));
        }
    }
    Ok(
        json!({"sourceTeamId":team,"sourceTeamName":s.team["name"],"targetTeamId":target,"sourceSha256":hash,"capturedAt":s.captured_at,"aclRevision":s.team["aclRevision"],"members":mapped_members,"vaults":vault_rows,"records":rows,"rules":rules,"issueRecords":issues,"canApply":false,"limitations":["Önizleme; hedefe henüz yazılmaz ve eski giriş kapatılmaz","Hesap eşlemesi, alıcı anahtarlarını yeniden sarma ve atomik aktarım henüz tamamlanmadı","Arşiv güncel kayıtları ve izinleri içerir; tam DB yedeği veya geçmiş/audit aktarımı değildir"]}),
    )
}
mod accounts;
use accounts::AccountMapping;
#[tauri::command]
pub async fn team_migration_export(
    state: State<'_, Shared>,
    team_id: String,
    path: String,
    password: String,
) -> std::result::Result<Value, String> {
    let _gate = state.team_gate.lock().await;
    let result:Result<Value>=async {
        let _rpc_gate=state.team_rpc_gate.lock().await;
        let auth=with(&state,|a|{ensure!(a.online,"Arşiv hazırlamak için çevrimiçi giriş gerekli");Ok(a.auth.clone())})?;
        let snapshot=tokio::time::timeout(Duration::from_secs(60),capture(&auth,&team_id)).await.context("Kaynak okuma zaman aşımına uğradı; arşiv yazılmadı")??;
        let password=Zeroizing::new(password);
        tauri::async_runtime::spawn_blocking(move||{
            write_archive(Path::new(&path),&password,&snapshot)?;
            Ok(json!({"vaults":snapshot.vaults.len(),"records":snapshot.vaults.iter().map(|v|v.records.len()).sum::<usize>(),"unreadable":snapshot.vaults.iter().flat_map(|v|&v.records).filter(|r|r.record.is_none()).count(),"currentSnapshotOnly":true}))
        }).await?
    }.await;
    result.map_err(err)
}
#[tauri::command]
pub async fn team_migration_preview(
    state: State<'_, termterm_team_client::desktop::DesktopState>,
    path: String,
    password: String,
    mappings: Option<Vec<AccountMapping>>,
    expected_source_sha256: Option<String>,
    expected_target_fingerprint: Option<String>,
) -> std::result::Result<Value, String> {
    use termterm_team_client::desktop::DesktopAction;
    let result: Result<Value> = async {
        let epoch = state.current_epoch();
        let owner = state.call(DesktopAction::Overview {}).await?;
        ensure!(
            owner["role"] == "owner" && owner["offline"] != true,
            "Hedef takım sahibinin çevrimiçi girişi gerekli"
        );
        let target = owner["teamId"]
            .as_str()
            .context("Hedef takım kimliği eksik")?
            .to_string();
        let members = state.call(DesktopAction::Members {}).await?;
        let fingerprint = accounts::fingerprint(&owner, &members)?;
        if let Some(expected) = &expected_target_fingerprint {
            ensure!(expected == &fingerprint, "Hedef üyeler/yetkiler değişti; arşivi yeniden önizleyin");
        }
        ensure!(mappings.as_ref().is_none_or(|m| m.is_empty()) ||
            (expected_source_sha256.is_some() && expected_target_fingerprint.is_some()),
            "Hesap eşlemesi için kaynak ve hedef önizleme kimliği gerekli");
        let pw = Zeroizing::new(password);
        let result = tauri::async_runtime::spawn_blocking(move || {
            let (s, h) = read_archive(Path::new(&path), &pw)?;
            if let Some(expected) = expected_source_sha256 {
                ensure!(h == expected, "Kaynak arşiv değişti; yeniden önizleyin");
            }
            let mut result = preview(&s, &h, &target)?;
            accounts::map(&mut result, &members, &fingerprint, &mappings.unwrap_or_default())?;
            Ok::<_, anyhow::Error>(result)
        })
        .await??;
        let current = state.call(DesktopAction::Overview {}).await?;
        let members = state.call(DesktopAction::Members {}).await?;
        ensure!(
            state.current_epoch() == epoch && current["role"] == "owner"
                && current["offline"] != true
                && current["teamId"] == result["targetTeamId"]
                && accounts::fingerprint(&current, &members)? == result["targetFingerprint"],
            "Hesap değişti; önizlemeyi yeniden açın"
        );
        Ok(result)
    }
    .await;
    result.map_err(err)
}
#[cfg(test)]
mod tests;
