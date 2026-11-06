//! Unelevated real-engine traffic test. No adapter, address, route, or DNS API.
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::{BufReader, Write}, net::Ipv4Addr, path::{Path, PathBuf}, time::Duration};
use termterm_vpn_windows_broker::{policy::{Plan, Registry}, protocol, windows::{identity::Process, pipe, process}};
use tokio::{net::windows::named_pipe::NamedPipeServer, sync::mpsc};
use uuid::Uuid;

struct Session {
    id: Uuid, child: process::Child, pipe: NamedPipeServer, target: Ipv4Addr,
    plan: Option<Plan>, connected: bool,
}
impl Session {
    fn send(&mut self, mut value: Value) -> Result<()> {
        value["sessionId"] = json!(self.id); value["generation"] = json!(1);
        let mut bytes = serde_json::to_vec(&value)?; bytes.push(b'\n');
        self.child.input.write_all(&bytes)?; self.child.input.flush()?; Ok(())
    }
}
fn checksum(bytes: &[u8]) -> u16 {
    let mut sum = 0_u32;
    for pair in bytes.chunks(2) { sum += u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]) as u32; }
    while sum > 65535 { sum = (sum & 65535) + (sum >> 16); } !(sum as u16)
}
fn echo(source: Ipv4Addr, target: Ipv4Addr, marker: u16, sequence: u16) -> Vec<u8> {
    let mut bytes = vec![0_u8; 20 + 8 + 1024];
    bytes[0] = 0x45; let length = bytes.len() as u16; bytes[2..4].copy_from_slice(&length.to_be_bytes());
    bytes[4..6].copy_from_slice(&sequence.to_be_bytes()); bytes[8] = 64; bytes[9] = 1;
    bytes[12..16].copy_from_slice(&source.octets()); bytes[16..20].copy_from_slice(&target.octets());
    bytes[20] = 8; bytes[24..26].copy_from_slice(&marker.to_be_bytes()); bytes[26..28].copy_from_slice(&sequence.to_be_bytes());
    for (i, byte) in bytes[28..].iter_mut().enumerate() { *byte = (i.wrapping_mul(31) ^ sequence as usize ^ marker as usize) as u8; }
    let sum = checksum(&bytes[20..]); bytes[22..24].copy_from_slice(&sum.to_be_bytes());
    let sum = checksum(&bytes[..20]); bytes[10..12].copy_from_slice(&sum.to_be_bytes()); bytes
}
async fn traffic(session: &Session, index: usize, count: u16) -> Result<Value> {
    let plan = session.plan.as_ref().context("missing_plan")?;
    let source = plan.addresses.iter().find_map(|a| match a.cidr.address { std::net::IpAddr::V4(ip) => Some(ip), _ => None }).context("ipv4_required")?;
    let mut hash = Sha256::new(); let mut total = 0;
    for sequence in 0..count {
        let packet = echo(source, session.target, 0x7400 + index as u16, sequence);
        pipe::write_packet(&session.pipe, &packet, plan.mtu as usize).await?;
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let reply = pipe::read_packet(&session.pipe, plan.mtu as usize).await?;
                let header = (reply[0] as usize & 15) * 4;
                if reply[0] >> 4 != 4 || reply.len() < header + 8 || reply[9] != 1 || reply[header] != 0 { continue; }
                if reply[12..16] != session.target.octets() || reply[16..20] != source.octets() || reply[header + 4..header + 8] != packet[24..28] { continue; }
                ensure!(checksum(&reply[..header]) == 0 && checksum(&reply[header..]) == 0, "reply_checksum_mismatch");
                ensure!(reply[header + 8..] == packet[28..], "reply_payload_mismatch");
                hash.update(&reply[header + 8..]); total += 1024; return Ok::<_, anyhow::Error>(());
            }
        }).await.with_context(|| format!("tunnel_{index}_packet_{sequence}_timeout"))??;
    }
    Ok(json!({"session":index,"packets":count,"payloadBytes":total,"payloadSha256":format!("{:x}",hash.finalize())}))
}
fn hash_file(path: &Path) -> Result<String> { Ok(format!("{:x}", Sha256::digest(fs::read(path)?))) }

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(args.len() == 3, "usage: probe PRIVATE_FIXTURE_JSON ENGINE_DIRECTORY REPORT_JSON");
    let fixture: Value = serde_json::from_slice(&fs::read(&args[0])?)?;
    let engine_dir = PathBuf::from(&args[1]).canonicalize()?;
    let owner = Process::open(std::process::id())?; ensure!(!owner.identity.elevated, "unelevated_test_required");
    let key = if std::env::var_os("TERMTERM_VPN_PROBE_OPENVPN_ONLY").is_some() { "profiles" } else { "probeProfiles" };
    let profiles = fixture[key].as_array().context("probe_profiles_required")?;
    ensure!(profiles.len() == 3, "three_profiles_required");
    let protocols: Vec<_> = profiles.iter().map(|p| if p["profile"]["family"] == "openVpn" { "OpenVPN" } else { "AnyConnect/ocserv" }).collect();
    let (tx, mut events) = mpsc::unbounded_channel();
    let mut sessions = vec![]; let mut registry = Registry::default();
    let mut hashes = BTreeMap::new();
    for name in ["libopenconnect-5.dll", "libcrypto-3-x64.dll", "libssl-3-x64.dll", "jsoncpp.dll"] {
        hashes.insert(name.to_string(), hash_file(&engine_dir.join(name))?);
    }
    for (index, item) in profiles.iter().enumerate() {
        let parsed = serde_json::from_value(item["profile"].clone())?;
        protocol::validate_fixture_profile(&parsed)?;
        let name = format!(r"\\.\pipe\TermTerm.VpnDev.packet.{}", Uuid::new_v4());
        let packet = pipe::server(&name, &owner.identity, true)?;
        let endpoint = pipe::packet_endpoint(&name)?; packet.connect().await?;
        let profile = &item["profile"];
        let binary = engine_dir.join(if profile["family"] == "openVpn" { "termterm-openvpn.exe" } else { "termterm-openconnect.exe" });
        hashes.insert(binary.file_name().unwrap().to_string_lossy().to_string(), hash_file(&binary)?);
        let child = process::engine(&owner, &binary, endpoint.raw())?; drop(endpoint);
        let output = child.output.try_clone()?; let tx = tx.clone();
        std::thread::spawn(move || { let mut reader = BufReader::new(output); loop { let value = protocol::read_sync(&mut reader); let stop = value.is_err(); if tx.send((index, value)).is_err() || stop { break; } } });
        let mut session = Session { id:Uuid::new_v4(),child,pipe:packet,target:item["target"].as_str().context("target_required")?.parse()?,plan:None,connected:false };
        let request = if profile["family"] == "openVpn" { json!({"config":profile["config"]}) }
            else { json!({"protocol":"anyconnect","server":profile["server"],"realm":"","caFile":item["caFile"]}) };
        session.send(request)?; sessions.push(session);
    }
    drop(tx);
    tokio::time::timeout(Duration::from_secs(60), async {
        while sessions.iter().any(|s| !s.connected) {
            let (index, message) = events.recv().await.context("all_engines_closed")?;
            let message = message.with_context(|| format!("engine_{index}_channel_closed"))?;
            let event: protocol::EngineEvent = serde_json::from_value(message)?;
            let session = &mut sessions[index]; ensure!(event.session_id == session.id && event.generation == 1,"wrong_session_generation");
            println!("engine {index}: {}",event.kind);
            match event.kind.as_str() {
                "challenge" => {
                    let challenge: protocol::Challenge = serde_json::from_value(event.detail)?; challenge.validate()?;
                    let mut values = BTreeMap::new();
                    for field in &challenge.fields {
                        let value = if !field.choices.is_empty() {field.choices[0].value.clone()}
                            else if field.secret {fixture["password"].as_str().context("fixture_password_required")?.to_string()}
                            else {fixture["username"].as_str().context("fixture_username_required")?.to_string()};
                        values.insert(field.name.clone(), value);
                    }
                    challenge.answer(&values)?; session.send(json!({"action":"answer","id":challenge.id,"values":values}))?;
                },
                "network-plan" => { let plan = Plan::from_engine(event.detail).with_context(||format!("engine_{index}_network_plan"))?; registry.reserve(session.id,1,&plan)?; session.plan = Some(plan); session.send(json!({"action":"accept-network"}))?; },
                "tunnel-ready" => { ensure!(Some(Plan::from_engine(event.detail)?) == session.plan,"plan_changed"); session.send(json!({"action":"network-configured","packetHandle":session.child.packet_handle}))?; },
                "connected" => session.connected = true,
                "connecting" => {},
                "progress" => protocol::validate_progress(event.detail)?,
                "failed" => bail!("engine_{index}_failed: {}",event.detail),
                _ => bail!("unexpected_engine_event:{}",event.kind),
            }
        }
        Ok::<_,anyhow::Error>(())
    }).await.context("engine_connection_timeout")??;
    let (a,b,c) = tokio::join!(traffic(&sessions[0],0,128),traffic(&sessions[1],1,128),traffic(&sessions[2],2,128));
    let first = vec![a?,b?,c?];
    sessions[1].send(json!({"action":"disconnect"}))?;
    tokio::time::sleep(Duration::from_millis(300)).await; sessions[1].child.terminate()?;
    let (a,c) = tokio::join!(traffic(&sessions[0],0,32),traffic(&sessions[2],2,32)); let survivors = vec![a?,c?];
    for session in &mut sessions { session.send(json!({"action":"disconnect"})).ok(); session.child.terminate()?; }
    if let Some(parent) = Path::new(&args[2]).parent() {fs::create_dir_all(parent)?;}
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize()?;
    let mut source_hashes = BTreeMap::new();
    for name in ["tests/vpn-engine-probe/src/main.rs", "vpn-engine/openvpn.cpp", "vpn-engine/openconnect.cpp", "vpn-engine/channel.hpp", "vpn-engine/windows/core-packet.hpp", "vpn-engine/windows/openconnect-api.hpp", "vpn-engine/windows/packet.hpp", "vpn-engine/patches/openconnect-9.21-packet-handle.patch", "vpn-windows-broker/src/windows/process.rs", "vpn-windows-broker/src/windows/pipe.rs", "vpn-windows-broker/src/policy.rs"] {
        source_hashes.insert(name, hash_file(&source.join(name))?);
    }
    fs::write(&args[2],serde_json::to_vec_pretty(&json!({"completed":true,"test":"actual Windows engines over inherited packet pipes to disposable WSL VPN servers","simultaneousConnections":3,"protocols":protocols,"traffic":first,"middleDisconnectedOtherTraffic":survivors,"engineAndDllSha256":hashes,"sourceSha256":source_hashes,"networkApisUsed":false,"hostNetworkSnapshotCompared":false,"wintunLoaded":false,"productionInstallationTouched":false,"fortinetGlobalProtectServerValidated":false}))?)?;
    println!("PASS three real engines, packet integrity, and middle-session isolation"); Ok(())
}
