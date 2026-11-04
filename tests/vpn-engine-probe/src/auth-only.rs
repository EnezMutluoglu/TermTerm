//! Disposable loopback fixture driver; identical engine/job/SSO path, no packet endpoint.
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::{io::{BufReader, Write}, path::PathBuf, sync::mpsc, time::Duration};
use termterm_vpn_windows_broker::{protocol::{self, Challenge, EngineEvent, SsoResult}, windows::process};
use uuid::Uuid;

fn main() -> Result<()> {
    ensure!(cfg!(debug_assertions), "debug_fixture_only");
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(args.len() == 1, "engine_path_required");
    let request = protocol::read_sync(&mut std::io::stdin().lock())?;
    let server = request["server"].as_str().ok_or_else(||anyhow::anyhow!("fixture_server_required"))?;
    let origin = termterm_vpn_core_origin(server)?;
    let gateway = request["gateway"].as_str().map(termterm_vpn_core_origin).transpose()?;
    let failure = request["failure"].as_str().unwrap_or_default();
    ensure!(matches!(failure,"" | "stale" | "offOrigin" | "cancel"),"invalid_fixture_failure");
    let mut child = process::auth_engine(&PathBuf::from(&args[0]).canonicalize()?)?;
    let id = Uuid::new_v4();
    let mut write = |mut value: Value| -> Result<()> {
        value["sessionId"] = json!(id); value["generation"] = json!(1);
        let mut bytes = serde_json::to_vec(&value)?; bytes.push(b'\n');
        child.input.write_all(&bytes)?; child.input.flush()?; Ok(())
    };
    write(json!({"protocol":"gp","server":server,"realm":request["realm"].as_str().unwrap_or_default(),"caFile":request["caFile"],"authOnly":true}))?;
    let output = child.output.try_clone()?;
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(output);
        loop {
            let value = protocol::read_sync(&mut reader);
            let stop = value.is_err(); if send.send(value).is_err() || stop { break; }
        }
    });
    let mut challenges = 0;
    let mut last_challenge = 0;
    loop {
        let event: EngineEvent = serde_json::from_value(receive.recv_timeout(Duration::from_secs(30))??)?;
        ensure!(event.session_id == id && event.generation == 1, "wrong_session_generation");
        match event.kind.as_str() {
            "connecting" => {},
            "challenge" => {
                challenges += 1; ensure!(challenges <= if gateway.is_some() {3} else {1}, "unexpected_repeat_auth");
                let challenge: Challenge = serde_json::from_value(event.detail)?; challenge.validate()?;
                ensure!(challenge.id > last_challenge,"reused_challenge_id"); last_challenge = challenge.id;
                if gateway.is_some() && challenges == 2 {
                    let selected = gateway.as_ref().unwrap().strip_prefix("https://").unwrap();
                    let values = std::collections::BTreeMap::from([("gateway".into(),selected.into())]);
                    challenge.answer(&values)?;
                    ensure!(challenge.gateway_origin(&values)?.as_ref() == gateway.as_ref(),"wrong_gateway_choice");
                    if !request["realm"].as_str().unwrap_or_default().is_empty() {ensure!(challenge.fields[0].value == selected,"saved_gateway_default_lost");}
                    write(json!({"action":"answer","id":challenge.id,"values":values}))?;
                    continue;
                }
                let target = if challenges == 3 {gateway.as_ref().unwrap()} else {&origin};
                let private = challenge.private.as_ref().ok_or_else(||anyhow::anyhow!("sso_challenge_required"))?;
                ensure!(private.expected_origin == *target && private.url == format!("{target}/idp"), "wrong_fixture_challenge");
                let stage = if challenges == 3 || gateway.is_none() {"gateway"} else {"portal"};
                ensure!(challenge.stage.as_deref() == Some(stage),"wrong_auth_stage");
                if challenges == 3 && failure == "cancel" {
                    drop(write); child.terminate()?;
                    println!("{}",json!({"authenticated":false,"challenges":challenges,"networkEvents":0,"tunnelCreated":false,"failure":failure}));
                    break;
                }
                let result_origin = if challenges == 3 && failure == "offOrigin" {&origin} else {target};
                let token_name = if gateway.is_some() {if challenges == 1 {"portal-userauthcookie"} else {"prelogin-cookie"}} else {request["tokenName"].as_str().unwrap_or_default()};
                let result: SsoResult = serde_json::from_value(json!({"uri":format!("{result_origin}/SAML20/SP/ACS"),"username":"fixture-user","token":"fixture-token","tokenName":token_name,"status":200,"authStatus":"1"}))?;
                // Negative fixtures deliberately bypass the broker's result guard to exercise the engine guard too.
                if failure.is_empty() || challenges != 3 {private.answer(&result)?;}
                write(json!({"action":"ssoAnswer","id":if challenges == 3 && failure == "stale" {1} else {challenge.id},"result":result}))?;
            }
            "authenticated" => {
                ensure!(failure.is_empty() && challenges == if gateway.is_some() {3} else {1}, "sso_not_exercised");
                drop(write);
                child.terminate()?;
                println!("{}", json!({"authenticated":true,"challenges":challenges,"networkEvents":0,"tunnelCreated":false}));
                break;
            }
            "failed" if !failure.is_empty() => {
                ensure!(challenges == 3,"failure_before_gateway_sso");
                drop(write); child.terminate()?;
                println!("{}",json!({"authenticated":false,"challenges":challenges,"networkEvents":0,"tunnelCreated":false,"failure":failure}));
                break;
            }
            _ => anyhow::bail!("auth_only_unexpected_engine_event"),
        }
    } Ok(())
}

fn termterm_vpn_core_origin(server: &str) -> Result<String> {
    // The fixture listener alone can receive these synthetic credentials.
    let value = server.strip_prefix("https://").ok_or_else(||anyhow::anyhow!("loopback_fixture_required"))?;
    let (authority, mode) = value.split_once('/').ok_or_else(||anyhow::anyhow!("fixture_selector_required"))?;
    let (host, port) = authority.split_once(':').ok_or_else(||anyhow::anyhow!("fixture_port_required"))?;
    ensure!(matches!(host,"127.0.0.1" | "localhost") && port.parse::<u16>().is_ok_and(|p|p > 0) && matches!(mode,"gateway" | "portal"), "invalid_fixture_server");
    use std::net::ToSocketAddrs;
    ensure!((host,port.parse::<u16>()?).to_socket_addrs()?.all(|address|address.ip().is_loopback()),"fixture_dns_must_remain_loopback");
    Ok(format!("https://{authority}"))
}
