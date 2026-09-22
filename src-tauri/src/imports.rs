use crate::model::Record;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, path::Path};
mod ansible;
mod openssh;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    pub record_id: Option<String>,
    pub label: String,
    pub status: String,
    pub notes: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub format: String,
    pub records: Vec<Record>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub items: Vec<ImportItem>,
}
fn host(label: &str, address: &str) -> Record {
    Record::new(
        "host",
        json!({"label":label,"address":address,"protocol":"ssh","port":22}),
    )
}
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}
fn group(records: &mut Vec<Record>, path: &str) -> String {
    let mut parent = String::new();
    for label in path.split(['/', '\\']).filter(|s| !s.is_empty()) {
        if let Some(g) = records
            .iter()
            .find(|r| r.kind == "group" && r.data["label"] == label && r.data["groupId"] == parent)
        {
            parent = g.id.clone();
        } else {
            let g = Record::new("group", json!({"label":label,"groupId":parent}));
            parent = g.id.clone();
            records.push(g);
        }
    }
    parent
}
pub fn parse(path: &Path, format: &str) -> Result<ImportPreview> {
    parse_encoded(path, format, None)
}
pub fn parse_encoded(path: &Path, format: &str, encoding: Option<&str>) -> Result<ImportPreview> {
    let text = read_text_encoded(path, encoding)?;
    let format = if format == "auto" {
        detect(path, &text)
    } else {
        format
    };
    let mut p = if format == "openssh" {
        let mut p = ImportPreview {
            format: format.into(),
            records: vec![],
            warnings: vec![],
            items: vec![],
        };
        openssh::parse(&text, Some(path), &mut p)?;
        p
    } else {
        parse_text(&text, format)?
    };
    for r in &mut p.records {
        for field in ["keyPath", "certificatePath"] {
            if let Some(value) = r.data[field]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string)
            {
                let raw = if let Some(v) = value.strip_prefix("~/") {
                    crate::platform::home().join(v)
                } else {
                    std::path::PathBuf::from(&value)
                };
                let resolved = if raw.is_absolute() {
                    raw
                } else {
                    path.parent().unwrap_or(Path::new(".")).join(raw)
                };
                r.data[field] = resolved.to_string_lossy().into_owned().into();
                if !resolved.is_file() {
                    p.warnings.push(format!(
                        "{}: missing {field}: {}",
                        r.data["label"].as_str().unwrap_or("Record"),
                        resolved.display()
                    ));
                }
            }
        }
    }
    annotate(&mut p);
    Ok(p)
}
pub fn read_text(path: &Path) -> Result<String> {
    read_text_encoded(path, None)
}
pub fn read_text_encoded(path: &Path, encoding: Option<&str>) -> Result<String> {
    ensure!(
        std::fs::metadata(path)?.len() < 32 * 1024 * 1024,
        "Import exceeds 32 MB limit"
    );
    let bytes = std::fs::read(path)?;
    let text = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        ensure!(bytes.len() % 2 == 0, "Truncated UTF-16 import file");
        let little = bytes.starts_with(&[0xff, 0xfe]);
        String::from_utf16(
            &bytes[2..]
                .chunks_exact(2)
                .map(|c| {
                    if little {
                        u16::from_le_bytes([c[0], c[1]])
                    } else {
                        u16::from_be_bytes([c[0], c[1]])
                    }
                })
                .collect::<Vec<_>>(),
        )?
    } else if let Some(label) = encoding.filter(|s| *s != "auto" && *s != "utf-8") {
        ensure!(
            ["windows-1254", "windows-1252", "windows-1251"].contains(&label),
            "Unsupported text encoding"
        );
        let codec = encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
        let (text, _, invalid) = codec.decode(&bytes);
        ensure!(!invalid, "Invalid bytes for selected text encoding");
        text.into_owned()
    } else {
        String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("File is not UTF-8. Select its original Windows text encoding and preview again."))?
            .trim_start_matches('\u{feff}')
            .into()
    };
    ensure!(!text.trim().is_empty(), "Import file is empty");
    Ok(text)
}
pub fn detect<'a>(path: &Path, text: &str) -> &'a str {
    let lower = text.trim_start().to_lowercase();
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_lowercase();
    if lower.contains("[bookmarks")
        || lower.contains("[mobaxterm")
        || ["mobaconf", "mxtsession", "mxtsessions"].contains(&ext.as_str())
    {
        return "mobaxterm";
    }
    if lower.starts_with("windows registry editor") || lower.contains("\\putty\\sessions\\") {
        return "putty";
    }
    if lower.starts_with("<?xml") || lower.starts_with("<key") || ext == "xml" {
        return "securecrt";
    }
    if path.file_name().is_some_and(|s| s == "known_hosts")
        || text.lines().any(|l| {
            l.split_whitespace()
                .nth(1)
                .is_some_and(|w| w.starts_with("ssh-") || w.starts_with("ecdsa-"))
        })
    {
        return "known_hosts";
    }
    if text.lines().any(|l| l.trim_start().starts_with('['))
        || ["ini", "yaml", "yml"].contains(&ext.as_str())
        || lower.contains("\n  hosts:")
        || lower.contains("\n  children:")
    {
        return "ansible";
    }
    if ext == "csv"
        || text.lines().next().is_some_and(|l| {
            l.contains(',')
                && ["address", "hostname", "label"]
                    .iter()
                    .any(|k| l.to_lowercase().contains(k))
        })
    {
        return "csv";
    }
    "openssh"
}
pub fn annotate(p: &mut ImportPreview) {
    p.warnings.sort();
    p.warnings.dedup();
    p.items = p
        .records
        .iter()
        .map(|r| {
            let label = r.data["label"]
                .as_str()
                .or(r.data["address"].as_str())
                .unwrap_or(&r.kind)
                .to_string();
            let notes = p
                .warnings
                .iter()
                .filter(|w| w.starts_with(&format!("{label}:")))
                .cloned()
                .collect::<Vec<_>>();
            let status = if notes.iter().any(|w| {
                let w = w.to_lowercase();
                w.contains("missing")
                    || w.contains("password")
                    || w.contains("credential")
                    || w.contains("encrypted")
            }) {
                "missing-identity"
            } else if notes.is_empty() {
                "ready"
            } else {
                "review"
            };
            ImportItem {
                record_id: Some(r.id.clone()),
                label,
                status: status.into(),
                notes,
            }
        })
        .collect();
    for warning in &p.warnings {
        if !p.items.iter().any(|i| i.notes.contains(warning)) {
            p.items.push(ImportItem {
                record_id: None,
                label: "Source".into(),
                status: if warning.contains("skipped") {
                    "skipped"
                } else {
                    "review"
                }
                .into(),
                notes: vec![warning.clone()],
            });
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeItem {
    pub source_id: String,
    pub target_id: String,
    pub action: String,
}
pub struct MergeResult {
    pub records: Vec<Record>,
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
    pub items: Vec<MergeItem>,
}
pub fn merge(existing: &[Record], incoming: &[Record], policy: &str) -> Result<Vec<Record>> {
    Ok(merge_report(existing, incoming, policy)?.records)
}
pub fn merge_report(existing: &[Record], incoming: &[Record], policy: &str) -> Result<MergeResult> {
    use std::collections::{BTreeMap, HashSet};
    ensure!(
        ["copy", "skip", "update"].contains(&policy),
        "Invalid duplicate policy"
    );
    fn group_path(records: &HashMap<&str, &Record>, id: &str) -> Result<Vec<String>> {
        let mut id = id.to_string();
        let mut visited = HashSet::new();
        let mut path = vec![];
        while !id.is_empty() {
            ensure!(
                visited.insert(id.clone()),
                "Cyclic imported group relationship"
            );
            ensure!(visited.len() <= 128, "Group nesting exceeds 128 levels");
            let Some(g) = records.get(id.as_str()) else {
                break;
            };
            path.push(g.data["label"].as_str().unwrap_or("").to_string());
            id = g.data["groupId"].as_str().unwrap_or("").to_string();
        }
        path.reverse();
        Ok(path)
    }
    fn identity(r: &Record, paths: &HashMap<String, Vec<String>>) -> Result<String> {
        Ok(json!([
            r.kind,
            r.data["label"],
            paths.get(r.data["groupId"].as_str().unwrap_or("")),
            r.data["address"],
            if r.kind == "host" {
                r.data["port"].as_u64().unwrap_or(22)
            } else {
                0
            },
            r.data["username"].as_str().unwrap_or(""),
            if r.kind == "knownHost" {
                r.data["publicKey"].clone()
            } else {
                Value::Null
            }
        ])
        .to_string())
    }
    fn paths(records: &[Record]) -> Result<HashMap<String, Vec<String>>> {
        let by_id: HashMap<_, _> = records.iter().map(|r| (r.id.as_str(), r)).collect();
        let mut paths = HashMap::new();
        paths.insert(String::new(), vec![]);
        for r in records {
            let id = r.data["groupId"].as_str().unwrap_or("");
            if !paths.contains_key(id) {
                paths.insert(id.into(), group_path(&by_id, id)?);
            }
        }
        Ok(paths)
    }
    let existing_paths = paths(existing)?;
    let incoming_paths = paths(incoming)?;
    let mut existing_index = BTreeMap::new();
    for r in existing {
        existing_index
            .entry(identity(r, &existing_paths)?)
            .or_insert(r.id.clone());
    }
    let mut mapping = HashMap::new();
    let mut skipped = HashSet::new();
    let mut seen = HashSet::new();
    let mut items = vec![];
    for r in incoming {
        ensure!(seen.insert(&r.id), "Repeated record id in import");
        let identity = identity(r, &incoming_paths)?;
        let found = if policy == "copy" {
            None
        } else {
            existing_index.get(&identity).cloned()
        };
        if policy == "skip" && found.is_some() {
            skipped.insert(&r.id);
        }
        let target = found
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        items.push(MergeItem {
            source_id: r.id.clone(),
            target_id: target.clone(),
            action: if found.is_some() {
                if policy == "skip" {
                    "skipped"
                } else {
                    "updated"
                }
            } else {
                "added"
            }
            .into(),
        });
        mapping.insert(r.id.clone(), target.clone());
        if policy != "copy" {
            existing_index.entry(identity).or_insert(target);
        }
    }
    let mut out = vec![];
    for r in incoming {
        if skipped.contains(&r.id) {
            continue;
        }
        let mut r = r.clone();
        r.id = mapping[&r.id].clone();
        for key in ["groupId", "credentialId", "hostId"] {
            if let Some(id) = r.data[key].as_str().and_then(|id| mapping.get(id)) {
                r.data[key] = id.clone().into();
            }
        }
        for key in ["chain", "hostIds", "inventoryGroups"] {
            if let Some(ids) = r.data.get_mut(key).and_then(Value::as_array_mut) {
                for id in ids {
                    if let Some(mapped) = id.as_str().and_then(|id| mapping.get(id)) {
                        *id = mapped.clone().into();
                    }
                }
            }
        }
        out.push(r);
    }
    let unique: BTreeMap<_, _> = out.into_iter().map(|r| (r.id.clone(), r)).collect();
    let records: Vec<_> = unique.into_values().collect();
    let existing_ids: HashSet<_> = existing.iter().map(|r| r.id.as_str()).collect();
    let updated = records
        .iter()
        .filter(|r| existing_ids.contains(r.id.as_str()))
        .count();
    Ok(MergeResult {
        added: records.len() - updated,
        updated,
        skipped: skipped.len(),
        records,
        items,
    })
}
pub fn validate(existing: &[Record], incoming: &[Record]) -> Result<()> {
    let by_id: HashMap<_, _> = existing
        .iter()
        .chain(incoming)
        .map(|r| (r.id.as_str(), r))
        .collect();
    for r in incoming {
        ensure!(r.data.is_object(), "Record data must be an object");
        let label = r.data["label"].as_str().unwrap_or(&r.kind);
        if r.kind == "host" {
            if let Some(port) = r.data.get("port") {
                ensure!(
                    port.as_u64().is_some_and(|p| p > 0 && p <= 65535),
                    "{label}: invalid port"
                );
            }
        }
        for (field, kind) in [
            ("groupId", "group"),
            ("credentialId", "credential"),
            ("hostId", "host"),
        ] {
            if let Some(id) = r.data[field].as_str().filter(|s| !s.is_empty()) {
                ensure!(
                    by_id.get(id).is_some_and(|target| target.kind == kind),
                    "{label}: missing {field} relationship"
                );
            }
        }
        for (field, kind) in [
            ("chain", "host"),
            ("hostIds", "host"),
            ("inventoryGroups", "group"),
        ] {
            if let Some(ids) = r.data[field].as_array() {
                for id in ids {
                    ensure!(
                        id.as_str()
                            .and_then(|id| by_id.get(id))
                            .is_some_and(|target| target.kind == kind),
                        "{label}: missing {field} relationship"
                    );
                }
            }
        }
    }
    Ok(())
}
pub fn parse_text(text: &str, format: &str) -> Result<ImportPreview> {
    let mut p = ImportPreview {
        format: format.into(),
        records: vec![],
        warnings: vec![],
        items: vec![],
    };
    match format {
        "csv" => parse_csv(text, &mut p)?,
        "openssh" => openssh::parse(text, None, &mut p)?,
        "known_hosts" => parse_known_hosts(text, &mut p),
        "putty" => parse_putty(text, &mut p),
        "mobaxterm" => parse_moba(text, &mut p),
        "securecrt" => parse_securecrt(text, &mut p)?,
        "ansible" => ansible::parse(text, &mut p)?,
        _ => anyhow::bail!("Unsupported import format"),
    }
    let mut invalid = std::collections::HashSet::new();
    p.records.retain(|r| {
        if r.kind == "host" && !r.data["port"].as_u64().is_some_and(|p| p > 0 && p <= 65535) {
            p.warnings.push(format!(
                "{}: invalid port; skipped",
                r.data["label"].as_str().unwrap_or("Host")
            ));
            invalid.insert(r.id.clone());
            false
        } else {
            true
        }
    });
    p.records.retain(|r| {
        !r.data["hostId"]
            .as_str()
            .is_some_and(|id| invalid.contains(id))
    });
    if p.records.is_empty() {
        p.warnings
            .push("No supported records were found. Nothing will be changed.".into());
    }
    annotate(&mut p);
    Ok(p)
}
fn parse_csv(text: &str, p: &mut ImportPreview) -> Result<()> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(false)
        .from_reader(text.as_bytes());
    let headers = reader.headers()?.clone();
    let mapping: Vec<String> = headers.iter().map(normalize).collect();
    for (i, row) in reader.records().enumerate() {
        let row = row?;
        let mut h = host("", "");
        let mut group_path = String::new();
        let mut invalid = false;
        for (key, value) in mapping.iter().zip(row.iter()) {
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                "label" | "name" | "hostname" => h.data["label"] = value.into(),
                "address" | "ip" | "host" | "hostaddress" => h.data["address"] = value.into(),
                "username" | "user" | "sshusername" => h.data["username"] = value.into(),
                "password" | "sshpassword" => h.data["password"] = value.into(),
                "port" | "sshport" => {
                    if let Some(port) = value.parse::<u16>().ok().filter(|p| *p > 0) {
                        h.data["port"] = port.into()
                    } else {
                        invalid = true;
                        p.warnings
                            .push(format!("Row {}: invalid port; skipped", i + 2))
                    }
                }
                "protocol" => h.data["protocol"] = value.to_lowercase().into(),
                "group" | "groups" => group_path = value.into(),
                "tags" => {
                    h.data["tags"] =
                        json!(value.split([';', ',']).map(str::trim).collect::<Vec<_>>())
                }
                _ => p
                    .warnings
                    .push(format!("Row {}: unsupported column {}", i + 2, key)),
            }
        }
        if !["ssh", "telnet", "mosh", "serial", "local"]
            .contains(&h.data["protocol"].as_str().unwrap_or(""))
        {
            invalid = true;
            p.warnings
                .push(format!("Row {}: unsupported protocol; skipped", i + 2));
        }
        if invalid {
            continue;
        }
        if h.data["address"].as_str().unwrap_or_default().is_empty() {
            p.warnings
                .push(format!("Row {}: missing address; skipped", i + 2));
            continue;
        }
        if h.data["label"] == "" {
            h.data["label"] = h.data["address"].clone();
        }
        if !group_path.is_empty() {
            h.data["groupId"] = group(&mut p.records, &group_path).into();
        }
        p.records.push(h);
    }
    Ok(())
}
// Tokenize configuration without evaluating shell substitutions, escapes or commands.
fn words(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut current = String::new();
    let mut quote = None;
    for c in line.chars() {
        if Some(c) == quote {
            quote = None;
        } else if quote.is_none() && (c == '"' || c == '\'') {
            quote = Some(c);
        } else if c == '#' && quote.is_none() {
            break;
        } else if c.is_whitespace() && quote.is_none() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}
fn parse_known_hosts(text: &str, p: &mut ImportPreview) {
    for (line_no, line) in text.lines().enumerate() {
        let w = words(line);
        if w.is_empty() {
            continue;
        }
        if w.len() < 3 || w[0].starts_with('@') {
            p.warnings.push(format!(
                "Line {}: invalid or certificate authority known-host entry",
                line_no + 1
            ));
            continue;
        }
        for address in w[0].split(',') {
            p.records.push(Record::new("knownHost",json!({"address":address,"algorithm":w[1],"publicKey":format!("{} {}",w[1],w[2]),"imported":true})));
        }
    }
}
fn parse_putty(text: &str, p: &mut ImportPreview) {
    fn finish(mut h: Record, p: &mut ImportPreview) {
        if h.data["address"] == "" {
            return;
        }
        let method = h.data["importProxyMethod"].as_u64().unwrap_or(0);
        if method == 2 || method == 3 {
            h.data["proxy"]["kind"] = if method == 2 { "socks5" } else { "http" }.into();
        } else {
            if method != 0 {
                p.warnings.push(format!(
                    "{}: PuTTY proxy method {method} is unsupported",
                    h.data["label"].as_str().unwrap_or("Session")
                ));
            }
            h.data.as_object_mut().unwrap().remove("proxy");
        }
        if let Some(forwards) = h.data["importForwards"].as_str() {
            for entry in forwards.split(',').filter(|s| !s.is_empty()) {
                let (source, target) = entry.split_once('=').unwrap_or((entry, ""));
                let source = source.trim_start_matches(['A', '4', '6']);
                let (kind, value) = match source.chars().next() {
                    Some('L') => ("localforward", format!("{} {target}", &source[1..])),
                    Some('R') => ("remoteforward", format!("{} {target}", &source[1..])),
                    Some('D') => ("dynamicforward", source[1..].to_string()),
                    _ => {
                        p.warnings.push(format!(
                            "{}: unsupported PuTTY forwarding {entry}",
                            h.data["label"]
                        ));
                        continue;
                    }
                };
                match openssh::tunnel(&h, kind, &value) {
                    Ok(t) => p.records.push(t),
                    Err(e) => p
                        .warnings
                        .push(format!("{}: forwarding omitted: {e}", h.data["label"])),
                }
            }
        }
        h.data.as_object_mut().unwrap().remove("importProxyMethod");
        h.data.as_object_mut().unwrap().remove("importForwards");
        p.records.push(h);
    }
    let mut current: Option<Record> = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            if let Some(h) = current.take() {
                finish(h, p);
            }
            if let Some((_, name)) = line.split_once("\\Sessions\\") {
                let name = name.trim_end_matches(']');
                let decoded = url::form_urlencoded::parse(
                    format!("v={}", name.replace('+', "%2B")).as_bytes(),
                )
                .next()
                .map(|(_, s)| s.into_owned())
                .unwrap_or_else(|| name.into());
                current = Some(host(&decoded, ""));
            }
            continue;
        }
        let Some(h) = current.as_mut() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim_matches('"');
        let value = value
            .trim_matches('"')
            .replace("\\\\", "\\")
            .replace("\\\"", "\"");
        match key {
            "HostName" => h.data["address"] = value.into(),
            "UserName" => h.data["username"] = value.into(),
            "PortNumber" => {
                if let Some(hex) = value.strip_prefix("dword:") {
                    h.data["port"] = u16::from_str_radix(hex, 16)
                        .ok()
                        .filter(|p| *p > 0)
                        .map(Value::from)
                        .unwrap_or(Value::Null);
                } else {
                    h.data["port"] = Value::Null;
                }
            }
            "Protocol" => h.data["protocol"] = value.into(),
            "PublicKeyFile" => {
                h.data["keyPath"] = value.into();
            }
            "ProxyMethod" => {
                h.data["importProxyMethod"] =
                    u64::from_str_radix(value.trim_start_matches("dword:"), 16)
                        .unwrap_or(0)
                        .into()
            }
            "ProxyHost" => h.data["proxy"]["host"] = value.into(),
            "ProxyPort" => {
                h.data["proxy"]["port"] =
                    u16::from_str_radix(value.trim_start_matches("dword:"), 16)
                        .unwrap_or(8080)
                        .into()
            }
            "ProxyUsername" => h.data["proxy"]["username"] = value.into(),
            "ProxyPassword" => h.data["proxy"]["password"] = value.into(),
            "PortForwardings" => h.data["importForwards"] = value.into(),
            _ => {
                if !value.is_empty() {
                    p.warnings.push(format!(
                        "{}: PuTTY field {} is not converted",
                        h.data["label"].as_str().unwrap_or("Session"),
                        key
                    ));
                }
            }
        }
    }
    if let Some(h) = current {
        finish(h, p);
    }
}
fn parse_moba(text: &str, p: &mut ImportPreview) {
    let mut section = String::new();
    let mut folder = String::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line.trim_matches(['[', ']']).into();
            folder.clear();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key == "SubRep" {
            folder = value.into();
            if section.starts_with("Bookmarks") {
                group(&mut p.records, &folder);
            }
            continue;
        }
        if section.starts_with("Bookmarks") && value.contains('#') {
            let chunks: Vec<&str> = value.split('#').collect();
            let Some(conn) = chunks.get(2) else {
                p.warnings
                    .push(format!("{}: unrecognized MobaXterm session", key));
                continue;
            };
            let fields: Vec<&str> = conn.split('%').collect();
            if fields.first() != Some(&"0") || fields.len() < 4 {
                p.warnings
                    .push(format!("{}: only SSH session encoding is supported", key));
                continue;
            }
            let mut h = host(key, fields[1]);
            h.data["port"] = fields[2]
                .parse::<u16>()
                .ok()
                .filter(|p| *p > 0)
                .map(Value::from)
                .unwrap_or(Value::Null);
            h.data["username"] = fields[3].into();
            h.data["groupId"] = group(&mut p.records, &folder).into();
            // Recognise key paths without guessing undocumented positional flags.
            if let Some(key_ref) = fields.iter().skip(4).find(|s| {
                s.ends_with(".ppk")
                    || s.ends_with(".pem")
                    || s.ends_with("id_rsa")
                    || s.ends_with("id_ed25519")
            }) {
                h.data["keyPath"] = (**key_ref).into();
            }
            p.records.push(h);
            p.warnings.push(format!("{}: imported connection fields; advanced settings and encrypted passwords require manual entry",key));
        } else if section.to_lowercase().contains("password") || section == "Credentials" {
            p.warnings.push("Encrypted MobaXterm credentials are not decoded by this adapter. Enter them in Keychain.".into());
        }
    }
    p.warnings.sort();
    p.warnings.dedup();
}
fn parse_securecrt(text: &str, p: &mut ImportPreview) -> Result<()> {
    use quick_xml::{events::Event, Reader};
    ensure!(
        !text.contains("<!DOCTYPE") && !text.contains("<!ENTITY"),
        "XML DTD/entities are not accepted"
    );
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<(String, HashMap<String, String>)> = vec![];
    let mut property = String::new();
    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let name = e
                    .attributes()
                    .filter_map(|a| a.ok())
                    .find(|a| a.key.as_ref() == b"name")
                    .map(|a| a.unescape_value().map(|s| s.into_owned()))
                    .transpose()?
                    .unwrap_or_default();
                if tag == "key" {
                    stack.push((name, HashMap::new()));
                } else {
                    property = name;
                }
            }
            Event::Text(e) => {
                if let Some((_, props)) = stack.last_mut() {
                    if !property.is_empty() {
                        let decoded = e.decode()?;
                        let value = quick_xml::escape::unescape(&decoded)?.into_owned();
                        props.entry(property.clone()).or_default().push_str(&value);
                    }
                }
            }
            Event::End(e) => {
                if e.name().as_ref() == b"key" {
                    if let Some((name, props)) = stack.pop() {
                        if let Some(address) = props.get("Hostname") {
                            let mut h = host(&name, address);
                            h.data["username"] =
                                props.get("Username").cloned().unwrap_or_default().into();
                            h.data["port"] = props
                                .get("[SSH2] Port")
                                .or(props.get("Port"))
                                .map(|v| {
                                    v.parse::<u16>()
                                        .ok()
                                        .filter(|p| *p > 0)
                                        .map(Value::from)
                                        .unwrap_or(Value::Null)
                                })
                                .unwrap_or(json!(22));
                            h.data["groupId"] = group(
                                &mut p.records,
                                &stack
                                    .iter()
                                    .map(|(s, _)| s.as_str())
                                    .filter(|s| *s != "Sessions" && *s != "VanDyke")
                                    .collect::<Vec<_>>()
                                    .join("/"),
                            )
                            .into();
                            for key in [
                                "Identity Filename",
                                "Identity File",
                                "[SSH2] Identity Filename",
                            ] {
                                if let Some(value) = props.get(key).filter(|s| !s.is_empty()) {
                                    h.data["keyPath"] = value.clone().into();
                                    break;
                                }
                            }
                            if let Some(value) = props.get("Certificate Filename") {
                                h.data["certificatePath"] = value.clone().into();
                            }
                            if let Some(value) = props.get("Protocol Name") {
                                h.data["protocol"] = if value.to_lowercase().contains("telnet") {
                                    "telnet"
                                } else {
                                    "ssh"
                                }
                                .into();
                            }
                            if let Some(value) = props
                                .get("Firewall Name")
                                .or(props.get("Gateway Session"))
                                .filter(|s| !s.is_empty() && s.as_str() != "None")
                            {
                                h.data["importGateway"] = value.clone().into();
                            }
                            for key in ["LocalForward", "RemoteForward", "DynamicForward"] {
                                if let Some(value) = props.get(key) {
                                    for line in value.lines() {
                                        match openssh::tunnel(&h, &key.to_lowercase(), line) {
                                            Ok(t) => p.records.push(t),
                                            Err(e) => p
                                                .warnings
                                                .push(format!("{name}: {key} omitted: {e}")),
                                        }
                                    }
                                }
                            }
                            for k in props.keys() {
                                if ![
                                    "Hostname",
                                    "Username",
                                    "[SSH2] Port",
                                    "Port",
                                    "Identity Filename",
                                    "Identity File",
                                    "[SSH2] Identity Filename",
                                    "Certificate Filename",
                                    "Protocol Name",
                                    "Firewall Name",
                                    "Gateway Session",
                                    "LocalForward",
                                    "RemoteForward",
                                    "DynamicForward",
                                ]
                                .contains(&k.as_str())
                                {
                                    p.warnings.push(format!(
                                        "{}: SecureCRT field {} is not converted",
                                        name, k
                                    ));
                                }
                            }
                            p.records.push(h);
                        }
                    }
                }
                property.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let gateways = p
        .records
        .iter()
        .filter_map(|r| {
            r.data["importGateway"]
                .as_str()
                .map(|s| (r.id.clone(), s.to_string()))
        })
        .collect::<Vec<_>>();
    for (id, gateway) in gateways {
        let matches = p
            .records
            .iter()
            .filter(|r| {
                r.kind == "host"
                    && r.data["label"].as_str()
                        == Some(gateway.rsplit(['/', '\\']).next().unwrap_or(&gateway))
            })
            .map(|r| r.id.clone())
            .collect::<Vec<_>>();
        if let Some(h) = p.records.iter_mut().find(|r| r.id == id) {
            if matches.len() == 1 && matches[0] != id {
                h.data["chain"] = json!(matches);
            } else {
                p.warnings.push(format!(
                    "{}: unresolved gateway {gateway}; choose it before connecting",
                    h.data["label"].as_str().unwrap_or("Session")
                ));
            }
            h.data.as_object_mut().unwrap().remove("importGateway");
        }
    }
    Ok(())
}
pub fn export(records: &[Record], format: &str, secrets: bool) -> Result<(String, Vec<String>)> {
    let mut warnings = vec![];
    let hosts: Vec<&Record> = records.iter().filter(|r| r.kind == "host").collect();
    for h in &hosts {
        for key in ["proxy", "chain", "startup", "environment", "credentialId"] {
            if h.data.get(key).is_some_and(|v| {
                !v.is_null() && v != &json!("") && v != &json!([]) && v != &json!({})
            }) {
                warnings.push(format!(
                    "{}: {} requires .ttbackup for full fidelity",
                    h.data["label"].as_str().unwrap_or("Host"),
                    key
                ));
            }
        }
    }
    match format {
        "csv" => {
            let mut w = csv::Writer::from_writer(vec![]);
            let mut headers = vec!["label", "address", "port", "username", "protocol", "tags"];
            if secrets {
                headers.push("password");
            }
            w.write_record(&headers)?;
            for h in hosts {
                let mut row: Vec<String> = headers
                    .iter()
                    .map(|k| {
                        if *k == "tags" {
                            h.data[*k]
                                .as_array()
                                .map(|a| {
                                    a.iter()
                                        .filter_map(Value::as_str)
                                        .collect::<Vec<_>>()
                                        .join(";")
                                })
                                .unwrap_or_default()
                        } else {
                            h.data[*k].as_str().map(str::to_string).unwrap_or_else(|| {
                                h.data[*k]
                                    .as_u64()
                                    .map(|x| x.to_string())
                                    .unwrap_or_default()
                            })
                        }
                    })
                    .collect();
                for v in &mut row {
                    if v.starts_with(['=', '+', '-', '@', '\t', '\r']) {
                        *v = format!("'{}", v);
                    }
                }
                w.write_record(row)?;
            }
            Ok((String::from_utf8(w.into_inner()?)?, warnings))
        }
        "openssh" => {
            let mut out = String::from("# Exported by TermTerm. No passwords are included.\n");
            for h in hosts {
                let scalar = |k: &str| {
                    h.data[k]
                        .as_str()
                        .unwrap_or_default()
                        .replace(['\r', '\n'], "")
                };
                let alias = scalar("label")
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() || "-_.".contains(c) {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect::<String>();
                out += &format!(
                    "\nHost {}\n  HostName {}\n  Port {}\n",
                    alias,
                    scalar("address"),
                    h.data["port"].as_u64().unwrap_or(22)
                );
                if !scalar("username").is_empty() {
                    out += &format!("  User {}\n", scalar("username"));
                }
            }
            Ok((out, warnings))
        }
        "known_hosts" => Ok((
            records
                .iter()
                .filter(|r| r.kind == "knownHost")
                .filter_map(|r| {
                    Some(format!(
                        "{} {}\n",
                        r.data["address"].as_str()?,
                        r.data["publicKey"].as_str()?
                    ))
                })
                .collect(),
            warnings,
        )),
        _ => anyhow::bail!("Unsupported export format"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_detection_and_vendor_relationships() {
        assert_eq!(
            detect(Path::new("connections.ini"), "[Bookmarks]\nSubRep=Prod"),
            "mobaxterm"
        );
        assert_eq!(
            detect(
                Path::new("connections.ini"),
                "[prod:vars]\nansible_user=deploy"
            ),
            "ansible"
        );
        let p=parse_text("Windows Registry Editor Version 5.00\n[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\prod]\n\"HostName\"=\"example.test\"\n\"ProxyMethod\"=dword:00000002\n\"ProxyHost\"=\"proxy.test\"\n\"ProxyPort\"=dword:00000438\n\"PortForwardings\"=\"L8080=localhost:80,D1080=\"\n\"PublicKeyFile\"=\"identity.ppk\"","putty").unwrap();
        let h = p.records.iter().find(|r| r.kind == "host").unwrap();
        assert_eq!(h.data["proxy"]["kind"], "socks5");
        assert_eq!(h.data["proxy"]["port"], 1080);
        assert_eq!(
            p.records
                .iter()
                .filter(|r| r.kind == "tunnel" && r.data["hostId"] == h.id)
                .count(),
            2
        );
        let p=parse_text("<key name=\"Sessions\"><key name=\"gateway\"><string name=\"Hostname\">jump.test</string></key><key name=\"prod\"><string name=\"Hostname\">target.test</string><string name=\"Gateway Session\">gateway</string><string name=\"Identity Filename\">id_ed25519</string><string name=\"Password V2\">unverified-ciphertext</string></key></key>","securecrt").unwrap();
        let h = p
            .records
            .iter()
            .find(|r| r.data["label"] == "prod")
            .unwrap();
        assert_eq!(h.data["chain"].as_array().unwrap().len(), 1);
        assert!(h.data["password"].is_null());
        assert!(p.items.iter().any(|i| !i.notes.is_empty()));
    }
    #[test]
    fn merge_remaps_every_relationship_and_distinguishes_groups() {
        let a = Record::new("group", json!({"label":"A"}));
        let b = Record::new("group", json!({"label":"B"}));
        let host_a = Record::new(
            "host",
            json!({"label":"web","address":"example.test","groupId":a.id}),
        );
        let host_b = Record::new(
            "host",
            json!({"label":"web","address":"example.test","groupId":b.id}),
        );
        let ws = Record::new(
            "workspace",
            json!({"label":"Work","hostIds":[host_b.id],"statsEnabled":[false]}),
        );
        for policy in ["copy", "skip", "update"] {
            let merged = merge(
                &[a.clone(), host_a.clone()],
                &[b.clone(), host_b.clone(), ws.clone()],
                policy,
            )
            .unwrap();
            assert_eq!(merged.len(), 3);
            let h = merged.iter().find(|r| r.kind == "host").unwrap();
            assert_ne!(h.id, host_a.id);
            assert_eq!(
                merged.iter().find(|r| r.kind == "workspace").unwrap().data["hostIds"][0],
                h.id
            );
            assert_eq!(
                merged.iter().find(|r| r.kind == "group").unwrap().id,
                h.data["groupId"]
            );
        }
        let existing = [b.clone(), host_b.clone()];
        let merged = merge(&existing, &[b, host_b, ws], "skip").unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].data["hostIds"][0], existing[1].id);
    }
    #[test]
    fn ssh_chain_and_commands() {
        let p=parse_text("Host jump\n HostName 1.2.3.4\nHost prod\n HostName test.internal\n ProxyJump jump\n ProxyCommand calc.exe\nMatch exec touch /tmp/evil\n User root", "openssh").unwrap();
        assert_eq!(p.records.len(), 2);
        assert_eq!(p.records[1].data["chain"][0], p.records[0].id);
        assert!(p.warnings.len() >= 2);
    }
    #[test]
    fn csv_quotes_unicode() {
        let p = parse_text(
            "label,address,username,tags\n\"İstanbul, web\",example.org,alice,prod;web\n",
            "csv",
        )
        .unwrap();
        assert_eq!(p.records[0].data["label"], "İstanbul, web");
        assert_eq!(p.records[0].data["tags"].as_array().unwrap().len(), 2);
    }
    #[test]
    fn registry_is_data() {
        let p=parse_text("Windows Registry Editor Version 5.00\n[HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\web]\n\"HostName\"=\"server.example\"\n\"PortNumber\"=dword:00000016","putty").unwrap();
        assert_eq!(p.records[0].data["port"], 22);
    }
}
