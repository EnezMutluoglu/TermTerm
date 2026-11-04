//! Shared and personal SQL documents. These are reviewed text, never executable jobs.
use crate::model::Record;
use anyhow::{ensure, Result};

pub fn is_document(kind: &str) -> bool {
    matches!(kind, "savedQuery" | "databaseRunbook")
}

pub fn validate(record: &Record) -> Result<()> {
    if !is_document(&record.kind) { return Ok(()); }
    let data = &record.data;
    ensure!(uuid::Uuid::parse_str(data["connectionId"].as_str().unwrap_or("")).is_ok(), "SQL belgesi için geçerli bağlantı gerekli");
    let label = data["label"].as_str().unwrap_or("");
    ensure!(!label.trim().is_empty() && label.chars().count() <= 160, "SQL belge adı 1–160 karakter olmalı");
    ensure!(data["description"].as_str().unwrap_or("").len() <= 16_384, "SQL açıklaması çok uzun");
    ensure!(serde_json::to_vec(data)?.len() <= 512 * 1024, "SQL belgesi en fazla 512 KiB olabilir");
    // Parameters/results belong to an execution, not a shared document. Reject
    // rather than silently persisting values accidentally supplied through IPC.
    for field in ["parameters", "results", "password", "privateKey", "token"] {
        ensure!(data.get(field).is_none(), "SQL belgesinde çalışma değerleri / sırlar saklanamaz: {field}");
    }
    if record.kind == "savedQuery" {
        ensure!(data["sql"].as_str().is_some_and(|sql| !sql.trim().is_empty()), "Sorgu boş olamaz");
    } else {
        let steps = data["steps"].as_array().ok_or_else(|| anyhow::anyhow!("Runbook adımları gerekli"))?;
        ensure!(!steps.is_empty() && steps.len() <= 50, "Runbook 1–50 adım içermeli");
        for step in steps {
            let obj = step.as_object().ok_or_else(|| anyhow::anyhow!("Geçersiz runbook adımı"))?;
            ensure!(obj.keys().all(|k| ["title", "sql", "description"].contains(&k.as_str())), "Runbook adımı yalnız başlık, açıklama ve SQL içerebilir");
            ensure!(step["title"].as_str().is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= 160), "Adım başlığı gerekli (en fazla 160 karakter)");
            ensure!(step["sql"].as_str().is_some_and(|s| !s.trim().is_empty()), "Adım SQL'i boş olamaz");
            ensure!(step["description"].as_str().unwrap_or("").len() <= 16_384, "Adım açıklaması çok uzun");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn documents_are_bounded_text_without_execution_values() {
        let mut r = Record::new("databaseRunbook", json!({"label":"İzleme", "connectionId":uuid::Uuid::new_v4().to_string(), "steps":[{"title":"Oturumlar", "sql":"SELECT :name", "description":"Elle çalıştır"}]}));
        assert!(validate(&r).is_ok());
        r.data["steps"][0]["parameters"] = json!({"name":"secret"});
        assert!(validate(&r).is_err());
        r.data["steps"][0].as_object_mut().unwrap().remove("parameters");
        r.data["steps"] = json!([]); assert!(validate(&r).is_err());
        r.kind = "savedQuery".into(); r.data["sql"] = json!("SELECT 1");
        assert!(validate(&r).is_ok());
        r.data["parameters"] = json!({}); assert!(validate(&r).is_err());
    }
}
