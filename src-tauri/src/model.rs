use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: String,
    pub kind: String,
    pub data: Value,
    #[serde(default)]
    pub updated_at: i64,
}
impl Record {
    pub fn new(kind: &str, data: Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            kind: kind.into(),
            data,
            updated_at: chrono::Utc::now().timestamp_millis(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub device_id: String,
    pub records: Vec<Record>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Host {
    pub label: String,
    pub address: String,
    pub port: u16,
    pub protocol: String,
    pub username: String,
    pub password: String,
    pub credential_id: String,
    pub group_id: String,
    pub tags: Vec<String>,
    pub chain: Vec<String>,
    pub proxy: Option<Proxy>,
    pub startup: String,
    pub environment: std::collections::BTreeMap<String, String>,
    pub private_key: String,
    pub key_path: String,
    pub certificate_path: String,
    pub passphrase: String,
    pub certificate: String,
    pub agent: String,
    pub agent_key: String,
    pub mosh_address: String,
    pub serial_baud: u32,
    pub serial_data_bits: u8,
    pub serial_stop_bits: u8,
    pub serial_parity: String,
    pub serial_flow_control: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Proxy {
    pub kind: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

pub fn resolve_host(records: &[Record], id: &str) -> anyhow::Result<Host> {
    let record = records
        .iter()
        .find(|r| r.id == id && r.kind == "host")
        .ok_or_else(|| anyhow::anyhow!("Host not found"))?;
    let mut merged = serde_json::Map::new();
    let mut ancestors = Vec::new();
    let mut group = record.data["groupId"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let mut seen = std::collections::HashSet::new();
    while !group.is_empty() {
        anyhow::ensure!(seen.insert(group.clone()), "Group inheritance cycle");
        let g = records
            .iter()
            .find(|r| r.id == group && r.kind == "group")
            .ok_or_else(|| anyhow::anyhow!("Parent group not found"))?;
        ancestors.push(g.data.clone());
        group = g.data["groupId"].as_str().unwrap_or_default().to_string();
    }
    for item in ancestors
        .into_iter()
        .rev()
        .chain(std::iter::once(record.data.clone()))
    {
        if let Some(obj) = item.as_object() {
            for (k, v) in obj {
                let inherit_empty = [
                    "username",
                    "password",
                    "credentialId",
                    "keyPath",
                    "privateKey",
                    "certificatePath",
                    "certificate",
                    "passphrase",
                    "startup",
                ]
                .contains(&k.as_str())
                    && v.as_str() == Some("");
                if !v.is_null() && !inherit_empty {
                    if k == "environment" {
                        if let (Some(existing), Some(child)) = (
                            merged.get_mut(k).and_then(Value::as_object_mut),
                            v.as_object(),
                        ) {
                            existing.extend(child.clone());
                            continue;
                        }
                    }
                    merged.insert(k.clone(), v.clone());
                }
            }
        }
    }
    let mut host: Host = serde_json::from_value(Value::Object(merged))?;
    if !host.credential_id.is_empty() {
        let c = records
            .iter()
            .find(|r| r.id == host.credential_id && r.kind == "credential")
            .ok_or_else(|| anyhow::anyhow!("Credential not found"))?;
        for (field, target) in [
            ("username", &mut host.username),
            ("password", &mut host.password),
            ("privateKey", &mut host.private_key),
            ("keyPath", &mut host.key_path),
            ("certificatePath", &mut host.certificate_path),
            ("passphrase", &mut host.passphrase),
            ("certificate", &mut host.certificate),
            ("agent", &mut host.agent),
            ("agentKey", &mut host.agent_key),
        ] {
            if target.is_empty() {
                *target = c.data[field].as_str().unwrap_or_default().into();
            }
        }
    }
    for (path, contents) in [
        (&host.key_path, &mut host.private_key),
        (&host.certificate_path, &mut host.certificate),
    ] {
        if contents.is_empty() && !path.is_empty() {
            let path = crate::archive::referenced_path(path);
            anyhow::ensure!(
                std::fs::metadata(&path)?.len() < 1024 * 1024,
                "Key file exceeds 1 MiB"
            );
            *contents = std::fs::read_to_string(path)?;
        }
    }
    if host.protocol.is_empty() {
        host.protocol = "ssh".into();
    }
    if host.port == 0 {
        host.port = if host.protocol == "telnet" { 23 } else { 22 };
    }
    Ok(host)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn nested_groups_inherit_auth_and_environment() {
        let parent = Record::new(
            "group",
            json!({"label":"Parent","username":"alice","port":2222,"environment":{"LANG":"C.UTF-8","COLOR":"1"}}),
        );
        let child = Record::new(
            "group",
            json!({"label":"Child","groupId":parent.id,"environment":{"COLOR":"2"}}),
        );
        let host = Record::new(
            "host",
            json!({"label":"Target","address":"example.test","groupId":child.id,"username":"","port":null}),
        );
        let resolved = resolve_host(&[parent, child, host.clone()], &host.id).unwrap();
        assert_eq!(resolved.username, "alice");
        assert_eq!(resolved.port, 2222);
        assert_eq!(resolved.environment["LANG"], "C.UTF-8");
        assert_eq!(resolved.environment["COLOR"], "2");
    }
}
