//! Online-only SQL authorization. Tokens and decrypted profiles never reach IPC.
use super::*;

pub struct SqlAuthorization {
    pub vault: String,
    pub record: Record,
    pub revision: i64,
    pub acl_revision: i64,
}

pub fn selected_vault(s: &Shared) -> Result<String> {
    with(s, |a| {
        ensure!(
            !a.cache.get::<bool>("authRejected")?.unwrap_or(false),
            "Team erişimi reddedildi"
        );
        Ok(a.vault
            .as_ref()
            .ok_or_else(|| anyhow!("Ortak kasa seçin"))?
            .0
            .clone())
    })
}

pub async fn load_document(s: &Shared, id: &str) -> Result<Record> {
    let vault = selected_vault(s)?;
    let key = with(s, |a| identity(&a.auth))?;
    let row = tokio::time::timeout(std::time::Duration::from_secs(8),
        rpc(s, "sql_document", &json!({"vaultId":vault,"recordId":id})))
        .await.map_err(|_| anyhow!("SQL belgesi için çevrimiçi Team doğrulaması gerekli"))??;
    ensure!(selected_vault(s)? == vault, "Team kasası değişti");
    let mut record = crypto::decrypt(&key, &vault, &row)?;
    ensure!(crate::database_library::is_document(&record.kind), "SQL belgesi gerekli");
    crate::database_library::validate(&record)?;
    // The server cannot inspect encrypted data; bind its clear relationship to
    // the authenticated decrypted document before it is offered in the editor.
    ensure!(record.data["connectionId"] == row["connectionId"], "SQL belgesi bağlantı ilişkisi uyuşmuyor");
    record.data["_teamRevision"] = row["revision"].clone();
    Ok(record)
}

pub async fn authorize(
    s: &Shared,
    record: &str,
    permission: &str,
    audit: bool,
) -> Result<SqlAuthorization> {
    ensure!(
        [
            "sql_access",
            "sql_run",
            "sql_monitor",
            "sql_manage",
            "sql_export"
        ]
        .contains(&permission),
        "Geçersiz SQL yetkisi"
    );
    let vault = selected_vault(s)?;
    let key = with(s, |a| identity(&a.auth))?;
    let mut body = json!({"vaultId":vault,"recordId":record,"permission":permission});
    if audit {
        body["auditId"] = json!(Uuid::new_v4().to_string());
        body["clientAt"] = json!(chrono::Utc::now().to_rfc3339());
    }
    let row = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        rpc(s, "sql_authorize", &body),
    )
    .await
    .map_err(|_| anyhow!("Team SQL yetkisi doğrulanamadı; çevrimiçi Team bağlantısı gerekli"))??;
    ensure!(selected_vault(s)? == vault, "Team kasası değişti");
    let record = crypto::decrypt(&key, &vault, &row)?;
    ensure!(
        record.kind == "databaseConnection",
        "SQL bağlantısı gerekli"
    );
    Ok(SqlAuthorization {
        vault,
        record,
        revision: row["revision"]
            .as_i64()
            .ok_or_else(|| anyhow!("SQL revizyonu yok"))?,
        acl_revision: row["aclRevision"]
            .as_i64()
            .ok_or_else(|| anyhow!("SQL yetki revizyonu yok"))?,
    })
}
