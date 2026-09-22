use crate::{
    imports::{self, ImportPreview},
    model::Record,
    vault::{self, Backup, BackupVault, Vault},
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
pub struct Source {
    pub path: String,
    pub password: String,
}
#[derive(Serialize)]
pub struct Report {
    pub warnings: Vec<String>,
    pub records: usize,
    pub vaults: usize,
}
pub fn referenced_path(path: &str) -> PathBuf {
    if path.starts_with("~/") || path.starts_with("~\\") {
        crate::platform::home().join(&path[2..])
    } else {
        PathBuf::from(path)
    }
}
pub fn portable_records(
    records: Vec<Record>,
    ids: &[String],
    profiles: bool,
    files: bool,
) -> Result<(Vec<Record>, Vec<String>)> {
    let mut selected: HashSet<String> = ids.iter().cloned().collect();
    if !ids.is_empty() {
        loop {
            let count = selected.len();
            for record in &records {
                if !selected.contains(&record.id) {
                    continue;
                }
                for key in ["groupId", "credentialId", "hostId"] {
                    if let Some(id) = record.data[key].as_str() {
                        selected.insert(id.into());
                    }
                }
                for key in ["chain", "hostIds", "inventoryGroups"] {
                    if let Some(ids) = record.data[key].as_array() {
                        for id in ids {
                            if let Some(id) = id.as_str() {
                                selected.insert(id.into());
                            }
                        }
                    }
                }
            }
            if count == selected.len() {
                break;
            }
        }
    }
    let mut result: Vec<_> = records
        .into_iter()
        .filter(|r| {
            (ids.is_empty() || selected.contains(&r.id))
                && (profiles || !["syncProfile", "integration"].contains(&r.kind.as_str()))
        })
        .collect();
    let mut warnings = vec![];
    for r in &mut result {
        if r.data["agent"].as_str().is_some_and(|s| !s.is_empty())
            || r.data["hardware"].as_bool() == Some(true)
        {
            warnings.push(format!("{}: signing device / agent must be reattached on the new computer; no hardware private key exported",r.data["label"].as_str().unwrap_or("Identity")));
        }
        for (reference, contents) in [
            ("keyPath", "privateKey"),
            ("certificatePath", "certificate"),
        ] {
            if let Some(path) = r.data[reference]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
            {
                if !files {
                    warnings.push(format!(
                        "{}: file reference {} is not embedded",
                        r.data["label"].as_str().unwrap_or("Record"),
                        path
                    ));
                    continue;
                }
                let file = referenced_path(&path);
                match std::fs::metadata(&file).and_then(|m| {
                    if m.len() > 1024 * 1024 {
                        return Err(std::io::Error::other("Key exceeds 1 MiB"));
                    }
                    std::fs::read_to_string(&file)
                }) {
                    Ok(text) => {
                        r.data[contents] = text.into();
                        r.data.as_object_mut().unwrap().remove(reference);
                    }
                    Err(e) => warnings.push(format!(
                        "{}: cannot include {} ({})",
                        r.data["label"].as_str().unwrap_or("Record"),
                        path,
                        e
                    )),
                }
            }
        }
    }
    Ok((result, warnings))
}
pub fn bundle(
    active: &Vault,
    path: &Path,
    password: &str,
    sources: Vec<Source>,
    ids: &[String],
    profiles: bool,
    files: bool,
) -> Result<Report> {
    let (records, mut warnings) = portable_records(active.records()?, ids, profiles, files)?;
    let mut backup = Backup {
        version: 1,
        vaults: vec![BackupVault {
            name: active.name()?,
            records,
        }],
    };
    for source in sources {
        let v = Vault::inspect(Path::new(&source.path), &source.password)?;
        let (records, notes) = portable_records(v.records, &[], profiles, files)?;
        warnings.extend(notes);
        backup.vaults.push(BackupVault {
            name: v.name,
            records,
        });
    }
    let report = Report {
        warnings,
        records: backup.vaults.iter().map(|v| v.records.len()).sum(),
        vaults: backup.vaults.len(),
    };
    vault::write_backup(path, password, &backup)?;
    Ok(report)
}
pub fn preview(
    path: &Path,
    format: &str,
    password: &str,
    mapping: Option<HashMap<String, String>>,
) -> Result<ImportPreview> {
    preview_encoded(path, format, password, mapping, None)
}
pub fn preview_encoded(
    path: &Path,
    format: &str,
    password: &str,
    mapping: Option<HashMap<String, String>>,
    encoding: Option<&str>,
) -> Result<ImportPreview> {
    if format == "ttbackup" || path.extension().is_some_and(|e| e == "ttbackup") {
        let backup = vault::read_backup(path, password)?;
        let mut records = vec![];
        for v in backup.vaults {
            records.extend(vault::restored_records(&v.records)?);
        }
        return Ok(ImportPreview {
            items: vec![],
            format: "ttbackup".into(),
            records,
            warnings: vec!["Restored records receive new IDs. Sync profiles are disabled.".into()],
        });
    }
    if format == "ttvault" || path.extension().is_some_and(|e| e == "ttvault") {
        let v = Vault::inspect(path, password)?;
        return Ok(ImportPreview {
            items: vec![],
            format: "ttvault".into(),
            records: vault::restored_records(&v.records)?,
            warnings: vec!["Vault imported as new records. Source file is unchanged.".into()],
        });
    }
    if format == "csv" {
        if let Some(mapping) = mapping {
            ensure!(
                std::fs::metadata(path)?.len() < 32 * 1024 * 1024,
                "Import exceeds 32 MiB"
            );
            let text = imports::read_text_encoded(path, encoding)?;
            let mut reader = csv::Reader::from_reader(text.as_bytes());
            let headers = reader.headers()?.clone();
            let mut writer = csv::Writer::from_writer(vec![]);
            let mut columns = vec![];
            let mut output = vec![];
            for (index, name) in headers.iter().enumerate() {
                let field = mapping.get(name).map(String::as_str).unwrap_or(name);
                if !field.is_empty() {
                    columns.push(index);
                    output.push(field);
                }
            }
            writer.write_record(output)?;
            for row in reader.records() {
                let row = row?;
                writer.write_record(columns.iter().map(|&i| row.get(i).unwrap_or_default()))?;
            }
            return imports::parse_text(&String::from_utf8(writer.into_inner()?)?, "csv");
        }
    }
    imports::parse_encoded(path, format, encoding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn partial_backup_includes_relationships() {
        let group = Record::new("group", json!({"label":"Team"}));
        let credential = Record::new("credential", json!({"label":"Key"}));
        let jump = Record::new("host", json!({"label":"Jump","credentialId":credential.id}));
        let host = Record::new(
            "host",
            json!({"label":"Target","chain":[jump.id],"groupId":group.id}),
        );
        let (included, _) = portable_records(
            vec![host.clone(), group, credential, jump],
            &[host.id],
            false,
            false,
        )
        .unwrap();
        assert_eq!(included.len(), 4);
    }
}
