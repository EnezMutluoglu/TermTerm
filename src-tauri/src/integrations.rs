use crate::{imports::ImportPreview, model::Record, state::Shared};
use anyhow::{anyhow, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub fn aws_records(value: &Value) -> Vec<Record> {
    let mut out = vec![];
    if let Some(reservations) = value["Reservations"].as_array() {
        for reservation in reservations {
            if let Some(instances) = reservation["Instances"].as_array() {
                for vm in instances {
                    let label = vm["Tags"]
                        .as_array()
                        .and_then(|a| a.iter().find(|t| t["Key"] == "Name"))
                        .and_then(|t| t["Value"].as_str())
                        .or(vm["InstanceId"].as_str())
                        .unwrap_or("EC2");
                    let address = vm["PublicIpAddress"]
                        .as_str()
                        .or(vm["PrivateIpAddress"].as_str());
                    if let Some(address) = address {
                        out.push(Record::new("host",json!({"label":label,"address":address,"protocol":"ssh","port":22,"tags":["aws",vm["InstanceId"]],"cloudId":vm["InstanceId"]})));
                    }
                }
            }
        }
    }
    out
}
pub async fn discover(
    provider: &str,
    profile: &str,
    region: &str,
    token: &str,
) -> Result<ImportPreview> {
    let records = match provider {
        "aws" => {
            ensure!(
                profile
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_- .".contains(c))
                    && region
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "Invalid AWS profile or region"
            );
            let mut cmd =
                tokio::process::Command::new(if cfg!(windows) { "aws.exe" } else { "aws" });
            cmd.args([
                "ec2",
                "describe-instances",
                "--output",
                "json",
                "--no-cli-pager",
            ]);
            if !profile.is_empty() {
                cmd.args(["--profile", profile]);
            }
            if !region.is_empty() {
                cmd.args(["--region", region]);
            }
            cmd.env("AWS_PAGER", "").kill_on_drop(true);
            #[cfg(windows)]
            cmd.creation_flags(0x08000000);
            let result = tokio::time::timeout(std::time::Duration::from_secs(60), cmd.output())
                .await?
                .context("Install and configure AWS CLI v2 before using AWS discovery")?;
            ensure!(
                result.status.success(),
                "AWS discovery failed: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            ensure!(
                result.stdout.len() < 32 * 1024 * 1024,
                "Cloud response too large"
            );
            aws_records(&serde_json::from_slice(&result.stdout)?)
        }
        "digitalocean" => {
            ensure!(
                !token.is_empty(),
                "DigitalOcean read-only API token is required"
            );
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()?;
            let mut out = vec![];
            for page in 1..=100 {
                let mut response = client
                    .get(format!(
                        "https://api.digitalocean.com/v2/droplets?per_page=200&page={page}"
                    ))
                    .bearer_auth(token)
                    .send()
                    .await?
                    .error_for_status()?;
                let mut bytes = vec![];
                while let Some(chunk) = response.chunk().await? {
                    ensure!(
                        bytes.len() + chunk.len() < 32 * 1024 * 1024,
                        "Cloud response too large"
                    );
                    bytes.extend(chunk);
                }
                let value: Value = serde_json::from_slice(&bytes)?;
                let droplets = value["droplets"]
                    .as_array()
                    .ok_or_else(|| anyhow!("Invalid DigitalOcean response"))?;
                for vm in droplets {
                    if let Some(ip) = vm["networks"]["v4"]
                        .as_array()
                        .and_then(|a| a.iter().find(|i| i["type"] == "public"))
                        .and_then(|i| i["ip_address"].as_str())
                    {
                        out.push(Record::new("host",json!({"label":vm["name"],"address":ip,"protocol":"ssh","port":22,"username":"root","tags":["digitalocean"],"cloudId":vm["id"]})));
                    }
                }
                if droplets.len() < 200 {
                    break;
                }
            }
            out
        }
        _ => anyhow::bail!("Choose AWS or DigitalOcean"),
    };
    Ok(ImportPreview{format:provider.into(),records,items:vec![],warnings:vec!["Review addresses and assign an SSH identity before connecting. Cloud resources are only listed; no infrastructure is changed.".into()]})
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BridgeHost {
    #[serde(default)]
    id: Option<String>,
    label: String,
    address: String,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    username: String,
    #[serde(default)]
    tags: Vec<String>,
}
fn constant_time_equal(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |diff, (a, b)| diff | (a ^ b)) == 0
}
async fn request(
    app: &AppHandle,
    state: &Shared,
    stream: &mut tokio::net::TcpStream,
    port: u16,
    token: &str,
) -> Result<Value> {
    let mut header = vec![];
    while !header.ends_with(b"\r\n\r\n") {
        ensure!(header.len() < 16384, "Header too large");
        header.push(stream.read_u8().await?);
    }
    let header = String::from_utf8(header)?;
    let mut lines = header.lines();
    let first = lines.next().unwrap_or_default();
    let fields: Vec<_> = first.split_whitespace().collect();
    ensure!(
        fields.len() == 3 && fields[2] == "HTTP/1.1",
        "Invalid HTTP request"
    );
    let mut auth = "";
    let mut length = 0usize;
    let mut host = "";
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim();
            match k.to_lowercase().as_str() {
                "origin" => anyhow::bail!("Browser origins are not accepted"),
                "authorization" => auth = v,
                "host" => host = v,
                "content-length" => length = v.parse()?,
                "transfer-encoding" => anyhow::bail!("Chunked requests are not accepted"),
                _ => {}
            }
        }
    }
    ensure!(
        host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}"),
        "Invalid Host header"
    );
    ensure!(
        constant_time_equal(auth.as_bytes(), format!("Bearer {token}").as_bytes()),
        "Unauthorized"
    );
    ensure!(length <= 512 * 1024, "Body too large");
    match (fields[0], fields[1]) {
        ("GET", "/v1/hosts") => {
            let g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
            let records = g
                .as_ref()
                .ok_or_else(|| anyhow!("Vault locked"))?
                .records()?;
            Ok(
                json!({"hosts":records.into_iter().filter(|r|r.kind=="host").map(|r|json!({"id":r.id,"label":r.data["label"],"address":r.data["address"],"port":r.data["port"],"username":r.data["username"],"tags":r.data["tags"]})).collect::<Vec<_>>()}),
            )
        }
        ("POST", "/v1/hosts") => {
            let mut bytes = vec![0; length];
            stream.read_exact(&mut bytes).await?;
            let host: BridgeHost = serde_json::from_slice(&bytes)?;
            ensure!(
                !host.label.trim().is_empty()
                    && !host.address.trim().is_empty()
                    && host.label.len() < 512
                    && host.address.len() < 512
                    && host.tags.len() < 64,
                "Invalid host"
            );
            let mut g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
            let v = g.as_mut().ok_or_else(|| anyhow!("Vault locked"))?;
            let mut record = if let Some(id) = host.id {
                ensure!(uuid::Uuid::parse_str(&id).is_ok(), "Invalid ID");
                v.records()?
                    .into_iter()
                    .find(|r| r.id == id && r.kind == "host")
                    .ok_or_else(|| anyhow!("Host not found"))?
            } else {
                Record::new("host", json!({"protocol":"ssh"}))
            };
            for (k, value) in [
                ("label", json!(host.label)),
                ("address", json!(host.address)),
                ("port", json!(host.port.unwrap_or(22))),
                ("username", json!(host.username)),
                ("tags", json!(host.tags)),
            ] {
                record.data[k] = value;
            }
            v.put(&[record.clone()])?;
            let _ = app.emit("vault-changed", json!({"id":v.id}));
            Ok(json!({"id":record.id}))
        }
        _ => anyhow::bail!("Supported endpoints: GET /v1/hosts, POST /v1/hosts"),
    }
}
pub async fn bridge(app: AppHandle, state: Shared, enabled: bool) -> Result<Value> {
    if let Some(task) = state
        .bridge
        .lock()
        .map_err(|_| anyhow!("Bridge lock"))?
        .take()
    {
        task.abort();
    }
    if !enabled {
        return Ok(json!({"enabled":false}));
    }
    ensure!(
        state
            .vault
            .lock()
            .map_err(|_| anyhow!("Vault lock"))?
            .is_some(),
        "Unlock a vault first"
    );
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let port = listener.local_addr()?.port();
    let token = hex::encode(&*crate::crypto::random_key());
    let result = json!({"enabled":true,"url":format!("http://127.0.0.1:{port}"),"token":token});
    let worker = state.clone();
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                request(&app, &worker, &mut stream, port, &token),
            )
            .await;
            let (status, body) = match result {
                Ok(Ok(v)) => ("200 OK", v),
                Ok(Err(e)) => ("400 Bad Request", json!({"error":e.to_string()})),
                Err(_) => ("408 Request Timeout", json!({"error":"Request timed out"})),
            };
            let body = body.to_string();
            let response=format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",body.len(),body);
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                stream.write_all(response.as_bytes()),
            )
            .await;
        }
    });
    *state.bridge.lock().map_err(|_| anyhow!("Bridge lock"))? = Some(task);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cloud_mapping_and_bridge_whitelist() {
        let v = json!({"Reservations":[{"Instances":[{"InstanceId":"i-example","PrivateIpAddress":"10.0.0.9","Tags":[{"Key":"Name","Value":"Türkçe"}]}]}]});
        let r = aws_records(&v);
        assert_eq!(r[0].data["label"], "Türkçe");
        assert_eq!(r[0].data["address"], "10.0.0.9");
        assert!(serde_json::from_value::<BridgeHost>(
            json!({"label":"x","address":"localhost","startup":"command"})
        )
        .is_err());
        assert!(!constant_time_equal(b"abc", b"abd"));
    }
}
