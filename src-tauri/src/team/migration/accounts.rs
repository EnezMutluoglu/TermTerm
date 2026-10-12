//! Explicit legacy-account mapping. This prepares a reviewed plan, never grants
//! access or creates encryption envelopes. Apply must revalidate live identities.
use super::*;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountMapping {
    pub source_account_id: String,
    pub target_account_id: String,
}

fn target_rows(members: &Value) -> Result<Vec<Value>> {
    let members = members.as_array().context("Hedef üyeler alınamadı")?;
    ensure!(members.len() <= 10000, "En fazla 10.000 hesap eşlenebilir");
    let mut ids = BTreeSet::new();
    let mut rows = vec![];
    for m in members {
        let id = m["accountId"].as_str().context("Hedef hesap kimliği eksik")?;
        Uuid::parse_str(id)?;
        ensure!(ids.insert(id), "Yinelenen hedef hesap");
        ensure!(matches!(m["status"].as_str(), Some("active" | "pending" | "disabled")), "Geçersiz hedef hesap durumu");
        ensure!(matches!(m["role"].as_str(), Some("owner" | "manager" | "editor" | "operator" | "observer")), "Geçersiz hedef rol");
        ensure!(m["username"].is_string() && m["displayName"].is_string(), "Geçersiz hedef hesap adı");
        // Explicit allowlist: no auth/crypto fields may leak if API evolves.
        rows.push(json!({"accountId":id,"username":m["username"],"displayName":m["displayName"],"role":m["role"],"status":m["status"]}));
    }
    rows.sort_by(|a,b| a["accountId"].as_str().cmp(&b["accountId"].as_str()));
    Ok(rows)
}

pub(super) fn fingerprint(owner: &Value, members: &Value) -> Result<String> {
    let team = owner["teamId"].as_str().context("Hedef takım eksik")?;
    let account = owner["accountId"].as_str().context("Hedef sahip eksik")?;
    Uuid::parse_str(team)?;
    Uuid::parse_str(account)?;
    ensure!(owner["role"] == "owner" && owner["offline"] != true, "Çevrimiçi takım sahibi gerekli");
    ensure!(owner["aclRevision"].as_i64().is_some_and(|n| n > 0), "Yetki revizyonu eksik");
    let members = target_rows(members)?;
    ensure!(members.iter().any(|m|m["accountId"]==account && m["role"]=="owner" && m["status"]=="active"), "Hedef sahip etkin değil");
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&json!({
        "team":team,"account":account,"aclRevision":owner["aclRevision"],"members":members
    }))?)))
}

pub(super) fn map(result: &mut Value, members: &Value, fingerprint: &str, mappings: &[AccountMapping]) -> Result<()> {
    let targets = target_rows(members)?;
    let sources = result["members"].as_array().context("Kaynak hesaplar eksik")?;
    ensure!(sources.len() <= 10000 && mappings.len() <= sources.len(), "Geçersiz hesap eşleme sayısı");
    let mut by_source = BTreeMap::new();
    let mut used = BTreeSet::new();
    for m in mappings {
        Uuid::parse_str(&m.source_account_id)?;
        Uuid::parse_str(&m.target_account_id)?;
        ensure!(sources.iter().any(|s|s["sourceId"] == m.source_account_id), "Kaynakta bulunmayan hesap eşlenemez");
        ensure!(targets.iter().any(|t|t["accountId"] == m.target_account_id), "Hedef takımda bulunmayan hesap eşlenemez");
        ensure!(by_source.insert(&m.source_account_id,&m.target_account_id).is_none(), "Kaynak hesap birden fazla eşlenemez");
        ensure!(used.insert(&m.target_account_id), "İki kaynak hesap aynı hedefe birleştirilemez");
    }
    let mut rows = vec![];
    let mut unresolved = 0;
    for source in sources {
        let id = source["sourceId"].as_str().context("Kaynak hesap eksik")?;
        let target = by_source.get(&id.to_string()).and_then(|id| targets.iter().find(|t|t["accountId"] == **id));
        let mut row = source.clone();
        let mut issues = vec![];
        if let Some(t) = target {
            row["targetAccountId"] = t["accountId"].clone();
            if t["role"] != source["role"] { issues.push("Roller eşleşmiyor; geçiş sırasında yetki yükseltilmez"); }
            if source["active"] == true {
                match t["status"].as_str() {
                    Some("pending") => issues.push("Kullanıcının ilk giriş ve parola değiştirme işlemi bekleniyor"),
                    Some("disabled") => issues.push("Etkin kaynak hesap kapalı hedefe aktarılamaz"),
                    _ => {}
                }
            } else if t["status"] != "disabled" {
                issues.push("Kapalı kaynak hesabı yeniden etkinleştiren eşleme yapılamaz");
            }
        } else {
            row["targetAccountId"] = Value::Null;
            issues.push("Hedef hesabı açıkça seçin; kullanıcı adıyla otomatik eşleme yapılmaz");
        }
        if !issues.is_empty() { unresolved += 1; }
        row["mappingIssues"] = json!(issues);
        rows.push(row);
    }
    for rule in result["rules"].as_array_mut().context("İzinler eksik")? {
        rule["targetAccountId"] = rows.iter().find(|m|m["sourceId"]==rule["sourceAccountId"])
            .map(|m|m["targetAccountId"].clone()).unwrap_or(Value::Null);
    }
    result["members"] = json!(rows);
    result["targetAccounts"] = json!(targets);
    result["targetFingerprint"] = json!(fingerprint);
    result["unresolvedAccounts"] = json!(unresolved);
    result["accountsReady"] = json!(unresolved == 0);
    result["canApply"] = json!(false);
    result["limitations"] = json!([
        "Hesap eşlemesi salt okunur; hesap oluşturmaz, rol veya izin değiştirmez",
        "Yeni kullanıcıyı Üyeler alanında oluşturun; geçici parolasını alıp ilk girişini tamamlamasını bekleyin",
        "Hedefe atomik aktarım ve alıcı anahtarlarını yeniden sarma tamamlanmadan uygulama açılmaz",
        "Arşiv güncel kayıtları ve izinleri içerir; tam DB yedeği veya geçmiş/audit aktarımı değildir"
    ]);
    Ok(())
}
