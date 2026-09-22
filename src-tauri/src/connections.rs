use crate::{
    model::{resolve_host, Host, Proxy, Record},
    state::{SessionInput, Shared},
};
use anyhow::{anyhow, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use russh::{
    client,
    keys::{HashAlg, PublicKeyOrCertificate},
    ChannelMsg,
};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::{mpsc, oneshot},
};
use uuid::Uuid;

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
pub type BoxStream = Box<dyn Stream>;
fn event(app: &AppHandle, id: &str, kind: &str, extra: Value) {
    let _ = app.emit("session-event", json!({"id":id,"kind":kind,"detail":extra}));
}
pub async fn prompt(
    app: &AppHandle,
    state: &Shared,
    session: &str,
    kind: &str,
    detail: Value,
) -> Result<Vec<String>> {
    let id = Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    state
        .prompts
        .lock()
        .map_err(|_| anyhow!("Prompt lock"))?
        .insert(id.clone(), tx);
    app.emit(
        "session-prompt",
        json!({"id":id,"sessionId":session,"kind":kind,"detail":detail}),
    )?;
    let result = tokio::time::timeout(Duration::from_secs(120), rx).await;
    state
        .prompts
        .lock()
        .map_err(|_| anyhow!("Prompt lock"))?
        .remove(&id);
    Ok(result
        .context("Prompt timed out")?
        .context("Prompt cancelled")?)
}
pub struct SshHandler {
    pub app: AppHandle,
    pub state: Shared,
    pub session: String,
    pub address: String,
    pub port: u16,
    pub vault_id: String,
    pub remote_target: Option<(String, u16)>,
}
impl client::Handler for SshHandler {
    type Error = anyhow::Error;
    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool> {
        let public = key.public_key();
        let fingerprint = public.fingerprint(HashAlg::Sha256).to_string();
        let public_key = public.to_openssh()?;
        let address = crate::ssh_compat::endpoint(&self.address, self.port);
        let known: Vec<Record> = {
            let guard = self.state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
            let v = guard.as_ref().ok_or_else(|| anyhow!("Vault is locked"))?;
            ensure!(v.id == self.vault_id, "Vault changed");
            crate::ssh_compat::known_hosts(&v.records()?, &self.address, self.port)
        };
        if crate::ssh_compat::verify_host_key(&known, &address, &public)? {
            return Ok(true);
        }
        let answer=prompt(&self.app,&self.state,&self.session,"hostKey",json!({"address":address,"fingerprint":fingerprint,"algorithm":public.algorithm().to_string()})).await?;
        if answer.first().map(String::as_str) != Some("trust") {
            return Ok(false);
        }
        let mut guard = self.state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        let vault = guard.as_mut().ok_or_else(|| anyhow!("Vault locked"))?;
        ensure!(vault.id == self.vault_id, "Vault changed");
        vault.put(&[Record::new("knownHost",json!({"label":address,"address":address,"publicKey":public_key,"fingerprint":fingerprint}))])?;
        Ok(true)
    }
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<client::Msg>,
        _address: &str,
        _port: u32,
        _origin: &str,
        _origin_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<()> {
        if let Some((host, port)) = self.remote_target.clone() {
            let stream = TcpStream::connect((host.as_str(), port)).await?;
            reply.accept().await;
            tokio::spawn(async move {
                let mut remote = channel.into_stream();
                let mut local = stream;
                let _ = tokio::io::copy_bidirectional(&mut remote, &mut local).await;
            });
        } else {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
        }
        Ok(())
    }
}
pub struct SshConnection {
    pub handle: client::Handle<SshHandler>,
    pub _parents: Vec<client::Handle<SshHandler>>,
}
pub struct SftpConnection {
    pub sftp: russh_sftp::client::SftpSession,
    pub _ssh: SshConnection,
}
async fn proxy_connect(
    mut stream: BoxStream,
    proxy: &Proxy,
    address: &str,
    port: u16,
) -> Result<BoxStream> {
    if proxy.kind == "socks5" {
        let auth = !proxy.username.is_empty();
        stream
            .write_all(if auth { &[5, 1, 2] } else { &[5, 1, 0] })
            .await?;
        let mut res = [0; 2];
        stream.read_exact(&mut res).await?;
        ensure!(
            res == [5, if auth { 2 } else { 0 }],
            "SOCKS5 authentication negotiation failed"
        );
        if auth {
            ensure!(
                proxy.username.len() <= 255 && proxy.password.len() <= 255,
                "SOCKS5 credentials too long"
            );
            let mut req = vec![1, proxy.username.len() as u8];
            req.extend(proxy.username.as_bytes());
            req.push(proxy.password.len() as u8);
            req.extend(proxy.password.as_bytes());
            stream.write_all(&req).await?;
            stream.read_exact(&mut res).await?;
            ensure!(res == [1, 0], "SOCKS5 credentials rejected");
        }
        ensure!(address.len() <= 255, "Host name too long");
        let mut req = vec![5, 1, 0, 3, address.len() as u8];
        req.extend(address.as_bytes());
        req.extend(port.to_be_bytes());
        stream.write_all(&req).await?;
        let mut head = [0; 4];
        stream.read_exact(&mut head).await?;
        ensure!(
            head[0] == 5 && head[1] == 0,
            "SOCKS5 connect failed ({})",
            head[1]
        );
        let size = match head[3] {
            1 => 4,
            4 => 16,
            3 => stream.read_u8().await? as usize,
            _ => anyhow::bail!("Invalid SOCKS5 response"),
        };
        let mut tail = vec![0; size + 2];
        stream.read_exact(&mut tail).await?;
    } else if proxy.kind == "http" {
        ensure!(!address.contains(['\r', '\n']), "Invalid target address");
        let authority = if address.contains(':') {
            format!("[{}]:{}", address, port)
        } else {
            format!("{}:{}", address, port)
        };
        let mut req = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n");
        if !proxy.username.is_empty() {
            req += &format!(
                "Proxy-Authorization: Basic {}\r\n",
                STANDARD.encode(format!("{}:{}", proxy.username, proxy.password))
            );
        }
        req += "\r\n";
        stream.write_all(req.as_bytes()).await?;
        let mut header = vec![];
        while !header.ends_with(b"\r\n\r\n") {
            ensure!(header.len() < 16384, "Proxy response header too large");
            header.push(stream.read_u8().await?);
        }
        let header = String::from_utf8_lossy(&header);
        ensure!(
            header
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                == Some("200"),
            "HTTP CONNECT rejected: {}",
            header.lines().next().unwrap_or("empty response")
        );
    } else {
        anyhow::bail!("Unsupported proxy type")
    };
    Ok(stream)
}
pub async fn connect_ssh(
    app: &AppHandle,
    state: &Shared,
    id: &str,
    host_id: &str,
    remote_target: Option<(String, u16)>,
) -> Result<SshConnection> {
    let (records, vault_id) = {
        let g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        let v = g.as_ref().ok_or_else(|| anyhow!("Unlock a vault first"))?;
        (v.records()?, v.id.clone())
    };
    fn expand(
        records: &[Record],
        id: &str,
        path: &mut Vec<String>,
        result: &mut Vec<Host>,
    ) -> Result<()> {
        ensure!(!path.iter().any(|i| i == id), "Host chain contains a cycle");
        ensure!(result.len() < 16, "Host chain exceeds 16 hops");
        path.push(id.into());
        let host = resolve_host(records, id)?;
        for hop in &host.chain {
            expand(records, hop, path, result)?;
        }
        result.push(host);
        path.pop();
        Ok(())
    }
    let mut hops = vec![];
    expand(&records, host_id, &mut vec![], &mut hops)?;
    let mut handles: Vec<client::Handle<SshHandler>> = vec![];
    for (index, host) in hops.iter().enumerate() {
        event(
            app,
            id,
            "status",
            json!(format!(
                "Connecting hop {}/{} · {}:{}",
                index + 1,
                hops.len(),
                host.address,
                host.port
            )),
        );
        let dest = host
            .proxy
            .as_ref()
            .filter(|p| !p.host.is_empty())
            .map(|p| (p.host.as_str(), p.port))
            .unwrap_or((host.address.as_str(), host.port));
        let stream: BoxStream = if let Some(previous) = handles.last() {
            Box::new(
                previous
                    .channel_open_direct_tcpip(dest.0, dest.1 as u32, "127.0.0.1", 0)
                    .await
                    .with_context(|| format!("Hop {}: forwarding denied", index))?
                    .into_stream(),
            )
        } else {
            Box::new(
                tokio::time::timeout(Duration::from_secs(20), TcpStream::connect(dest))
                    .await
                    .context("TCP connection timed out")??,
            )
        };
        let stream = if let Some(proxy) = host.proxy.as_ref().filter(|p| !p.host.is_empty()) {
            tokio::time::timeout(
                Duration::from_secs(20),
                proxy_connect(stream, proxy, &host.address, host.port),
            )
            .await
            .context("Proxy handshake timed out")??
        } else {
            stream
        };
        let mut config = client::Config {
            keepalive_interval: Some(Duration::from_secs(20)),
            keepalive_max: 3,
            ..Default::default()
        };
        let trusted = crate::ssh_compat::known_hosts(&records, &host.address, host.port);
        config.preferred.key = crate::ssh_compat::preferred_host_keys(&trusted).into();
        let config = Arc::new(config);
        let handler = SshHandler {
            app: app.clone(),
            state: state.clone(),
            session: id.into(),
            address: host.address.clone(),
            port: host.port,
            vault_id: vault_id.clone(),
            remote_target: if index + 1 == hops.len() {
                remote_target.clone()
            } else {
                None
            },
        };
        let mut handle = client::connect_stream(config, stream, handler)
            .await
            .with_context(|| {
                format!("Hop {} ({}): SSH handshake failed", index + 1, host.address)
            })?;
        let username = if host.username.is_empty() {
            prompt(
                app,
                state,
                id,
                "authentication",
                json!({"name":"Username","prompts":[{"prompt":"Username","echo":true}]}),
            )
            .await?
            .first()
            .cloned()
            .unwrap_or_default()
        } else {
            host.username.clone()
        };
        let mut authenticated = false;
        if !host.agent.is_empty() {
            let mut agent = crate::keys::agent(&host.agent)
                .await
                .context("SSH signing agent is unavailable")?;
            for identity in agent.request_identities().await? {
                let key = identity.public_key().into_owned();
                if !host.agent_key.is_empty()
                    && key.fingerprint(HashAlg::Sha256).to_string() != host.agent_key
                {
                    continue;
                }
                let hashes = if key.algorithm().is_rsa() {
                    crate::ssh_compat::rsa_hash_candidates(handle.best_supported_rsa_hash().await?)
                } else {
                    vec![None]
                };
                for hash in hashes {
                    if handle
                        .authenticate_publickey_with(&username, key.clone(), hash, &mut agent)
                        .await?
                        .success()
                    {
                        authenticated = true;
                        break;
                    }
                }
                if authenticated {
                    break;
                }
            }
        }
        if !authenticated && !host.private_key.is_empty() {
            let key = crate::ssh_compat::decode_private_key(&host.private_key, &host.passphrase)?;
            let key = Arc::new(key);
            authenticated = if !host.certificate.is_empty() {
                handle
                    .authenticate_openssh_cert(
                        &username,
                        key,
                        russh::keys::Certificate::from_openssh(&host.certificate)?,
                    )
                    .await?
                    .success()
            } else {
                crate::ssh_compat::authenticate_key(&mut handle, &username, key).await?
            };
        }
        if !authenticated && !host.password.is_empty() {
            authenticated = handle
                .authenticate_password(&username, &host.password)
                .await?
                .success();
        }
        if !authenticated {
            let mut result = handle
                .authenticate_keyboard_interactive_start(&username, None::<String>)
                .await?;
            for _ in 0..8 {
                match result {
                    client::KeyboardInteractiveAuthResponse::Success => {
                        authenticated = true;
                        break;
                    }
                    client::KeyboardInteractiveAuthResponse::Failure { .. } => break,
                    client::KeyboardInteractiveAuthResponse::InfoRequest {
                        name,
                        instructions,
                        prompts,
                    } => {
                        let details = json!({"name":name,"instructions":instructions,"prompts":prompts.iter().map(|p|json!({"prompt":p.prompt,"echo":p.echo})).collect::<Vec<_>>()});
                        let answers = prompt(app, state, id, "authentication", details).await?;
                        ensure!(answers.len() == prompts.len(), "Authentication cancelled");
                        result = handle
                            .authenticate_keyboard_interactive_respond(answers)
                            .await?;
                    }
                }
            }
        }
        if !authenticated && host.password.is_empty() {
            let answers=prompt(app,state,id,"authentication",json!({"name":format!("{}@{}",username,host.address),"prompts":[{"prompt":"Password","echo":false}]})).await?;
            if let Some(password) = answers.first() {
                authenticated = handle
                    .authenticate_password(&username, password)
                    .await?
                    .success();
            }
        }
        ensure!(
            authenticated,
            "Hop {} ({}): authentication failed",
            index + 1,
            host.address
        );
        handles.push(handle);
    }
    let handle = handles.pop().ok_or_else(|| anyhow!("Empty host chain"))?;
    Ok(SshConnection {
        handle,
        _parents: handles,
    })
}
fn output(app: &AppHandle, id: &str, data: &[u8], log: &mut Vec<u8>) {
    if log.len() < 4 * 1024 * 1024 {
        let len = data.len().min(4 * 1024 * 1024 - log.len());
        log.extend_from_slice(&data[..len]);
    }
    event(app, id, "data", json!(STANDARD.encode(data)));
}
pub fn start(
    app: AppHandle,
    state: Shared,
    host_id: Option<String>,
    shell: Option<String>,
) -> Result<String> {
    let mut sessions = state.sessions.lock().map_err(|_| anyhow!("Session lock"))?;
    ensure!(sessions.len() < 16, "Maximum 16 active sessions");
    let id = Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::channel(256);
    sessions.insert(id.clone(), tx);
    drop(sessions);
    let session_id = id.clone();
    tauri::async_runtime::spawn(async move {
        let result = run_session(&app, &state, &session_id, host_id, shell, rx).await;
        crate::metrics::stop(&state, &session_id);
        if let Err(error) = result {
            event(&app, &session_id, "error", json!(format!("{:#}", error)));
        }
        event(&app, &session_id, "closed", Value::Null);
        if let Ok(mut s) = state.sessions.lock() {
            s.remove(&session_id);
        }
    });
    Ok(id)
}
async fn run_session(
    app: &AppHandle,
    state: &Shared,
    id: &str,
    host_id: Option<String>,
    shell: Option<String>,
    mut rx: mpsc::Receiver<SessionInput>,
) -> Result<()> {
    let (vault_id, host) = {
        let g = state.vault.lock().map_err(|_| anyhow!("Vault lock"))?;
        let v = g.as_ref().ok_or_else(|| anyhow!("Unlock a vault first"))?;
        (
            v.id.clone(),
            host_id
                .as_ref()
                .map(|id| resolve_host(&v.records()?, id))
                .transpose()?,
        )
    };
    if host.is_none() {
        crate::metrics::register(
            app,
            state,
            id,
            if shell.as_deref() == Some("wsl.exe") {
                crate::metrics::Target::Wsl
            } else {
                crate::metrics::Target::Local
            },
        );
        return run_local(app.clone(), state.clone(), id.into(), vault_id, shell, rx).await;
    }
    let host = host.unwrap();
    let mut log = vec![];
    let host_id = host_id.unwrap();
    match host.protocol.as_str() {
        "ssh" => {
            let connection = Arc::new(
                tokio::select! {r=connect_ssh(app,state,id,&host_id,None)=>r?,_ = wait_close(&mut rx)=>return Ok(())},
            );
            crate::metrics::register(
                app,
                state,
                id,
                crate::metrics::Target::Ssh(connection.clone(), host.label.clone()),
            );
            let mut channel = connection.handle.channel_open_session().await?;
            channel
                .request_pty(true, "xterm-256color", 100, 30, 0, 0, &[])
                .await?;
            for (key, value) in &host.environment {
                channel.set_env(false, key, value).await?;
            }
            channel.request_shell(true).await?;
            event(app, id, "connected", Value::Null);
            if !host.startup.is_empty() {
                channel
                    .data(format!("{}\r", host.startup).as_bytes())
                    .await?;
            }
            loop {
                tokio::select! {
                    msg=channel.wait()=>match msg {Some(ChannelMsg::Data{data})|Some(ChannelMsg::ExtendedData{data,..})=>output(app,id,&data,&mut log),Some(ChannelMsg::ExitStatus{exit_status})=>event(app,id,"status",json!(format!("Exited ({})",exit_status))),Some(ChannelMsg::Close)|None=>break,_=>{}},
                    msg=rx.recv()=>match msg{Some(SessionInput::Data(data))=>channel.data(data.as_slice()).await?,Some(SessionInput::Resize(cols,rows))=>channel.window_change(cols as u32,rows as u32,0,0).await?,_=>{let _=channel.close().await;break;}}
                }
            }
            crate::metrics::stop(state, id);
            let _ = connection
                .handle
                .disconnect(russh::Disconnect::ByApplication, "TermTerm closed", "en")
                .await;
        }
        "telnet" => {
            crate::metrics::register(
                app,
                state,
                id,
                crate::metrics::Target::Unsupported(
                    "Telnet has no independent resource channel".into(),
                ),
            );
            let mut stream = TcpStream::connect((host.address.as_str(), host.port)).await?;
            event(app, id, "connected", Value::Null);
            let mut buf = [0; 16384];
            let mut telnet = Telnet::default();
            loop {
                tokio::select! {n=stream.read(&mut buf)=>{let n=n?;if n==0{break;}let(data,response)=telnet.feed(&buf[..n]);if !response.is_empty(){stream.write_all(&response).await?;}output(app,id,&data,&mut log);},msg=rx.recv()=>match msg{Some(SessionInput::Data(data))=>{let escaped:Vec<u8>=data.into_iter().flat_map(|b|if b==255{vec![255,255]}else{vec![b]}).collect();stream.write_all(&escaped).await?;},Some(SessionInput::Resize(..))=>{},_=>break}}
            }
        }
        "serial" => {
            crate::metrics::register(
                app,
                state,
                id,
                crate::metrics::Target::Unsupported(
                    "Serial ports do not expose system resource counters".into(),
                ),
            );
            return run_serial(app.clone(), state.clone(), id.into(), vault_id, host, rx).await;
        }
        "mosh" => return run_mosh(app.clone(), state.clone(), id.into(), host_id, host, rx).await,
        _ => anyhow::bail!("Unsupported protocol"),
    }
    save_log(state, &vault_id, id, &host.label, &host_id, &log);
    Ok(())
}
async fn wait_close(rx: &mut mpsc::Receiver<SessionInput>) {
    while let Some(msg) = rx.recv().await {
        if matches!(msg, SessionInput::Close) {
            break;
        }
    }
}
fn save_log(state: &Shared, vault_id: &str, id: &str, label: &str, host_id: &str, log: &[u8]) {
    if let Ok(mut guard) = state.vault.lock() {
        if let Some(v) = guard.as_mut().filter(|v| v.id == vault_id) {
            let records = v.records().unwrap_or_default();
            let settings = records.iter().find(|r| r.kind == "settings");
            if settings.is_some_and(|r| r.data["logging"] == false) {
                return;
            }
            let days = settings
                .and_then(|r| r.data["logRetentionDays"].as_i64())
                .unwrap_or(30)
                .clamp(0, 3650);
            if days > 0 {
                let cutoff = chrono::Utc::now().timestamp_millis() - days * 86_400_000;
                let expired: Vec<_> = records
                    .iter()
                    .filter(|r| r.kind == "log" && r.updated_at < cutoff)
                    .map(|r| r.id.clone())
                    .collect();
                let _ = v.delete(&expired);
            }
            let r = Record::new(
                "log",
                json!({"label":label,"sessionId":id,"hostId":host_id,"content":String::from_utf8_lossy(log),"endedAt":chrono::Utc::now().to_rfc3339(),"truncated":log.len()>=4*1024*1024}),
            );
            let _ = v.put(&[r]);
        }
    }
}
async fn run_local(
    app: AppHandle,
    state: Shared,
    id: String,
    vault_id: String,
    shell: Option<String>,
    mut rx: mpsc::Receiver<SessionInput>,
) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
        use std::io::{Read, Write};
        let shell = shell.unwrap_or_else(crate::platform::default_shell);
        crate::platform::validate_shell(&shell)?;
        let pair = NativePtySystem::default().openpty(PtySize {
            rows: 30,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");
        if shell.contains("powershell") || shell == "pwsh.exe" {
            cmd.arg("-NoLogo");
        }
        let mut child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;
        let app2 = app.clone();
        let id2 = id.clone();
        let read_thread = std::thread::spawn(move || {
            let mut log = vec![];
            let mut buf = [0; 16384];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => output(&app2, &id2, &buf[..n], &mut log),
                }
            }
            log
        });
        event(&app, &id, "connected", Value::Null);
        loop {
            match rx.try_recv() {
                Ok(SessionInput::Data(data)) => {
                    writer.write_all(&data)?;
                    writer.flush()?;
                }
                Ok(SessionInput::Resize(cols, rows)) => pair.master.resize(PtySize {
                    cols,
                    rows,
                    pixel_width: 0,
                    pixel_height: 0,
                })?,
                Ok(SessionInput::Close) | Err(mpsc::error::TryRecvError::Disconnected) => {
                    let _ = child.kill();
                    break;
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    if child.try_wait()?.is_some() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        drop(writer);
        drop(pair.master);
        let _ = child.wait();
        if let Ok(log) = read_thread.join() {
            save_log(&state, &vault_id, &id, &shell, "", &log);
        }
        Ok(())
    })
    .await??;
    Ok(())
}
async fn run_serial(
    app: AppHandle,
    state: Shared,
    id: String,
    vault_id: String,
    host: Host,
    mut rx: mpsc::Receiver<SessionInput>,
) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut port = serialport::new(
            &host.address,
            if host.serial_baud == 0 {
                115200
            } else {
                host.serial_baud
            },
        )
        .timeout(Duration::from_millis(20))
        .data_bits(match host.serial_data_bits {
            5 => serialport::DataBits::Five,
            6 => serialport::DataBits::Six,
            7 => serialport::DataBits::Seven,
            _ => serialport::DataBits::Eight,
        })
        .stop_bits(if host.serial_stop_bits == 2 {
            serialport::StopBits::Two
        } else {
            serialport::StopBits::One
        })
        .parity(match host.serial_parity.as_str() {
            "odd" => serialport::Parity::Odd,
            "even" => serialport::Parity::Even,
            _ => serialport::Parity::None,
        })
        .flow_control(match host.serial_flow_control.as_str() {
            "hardware" => serialport::FlowControl::Hardware,
            "software" => serialport::FlowControl::Software,
            _ => serialport::FlowControl::None,
        })
        .open()?;
        event(&app, &id, "connected", Value::Null);
        let mut log = vec![];
        let mut buf = [0; 4096];
        loop {
            match rx.try_recv() {
                Ok(SessionInput::Data(data)) => port.write_all(&data)?,
                Ok(SessionInput::Close) | Err(mpsc::error::TryRecvError::Disconnected) => break,
                _ => {}
            }
            match port.read(&mut buf) {
                Ok(n) => output(&app, &id, &buf[..n], &mut log),
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(e.into()),
            }
        }
        save_log(&state, &vault_id, &id, &host.label, "", &log);
        Ok(())
    })
    .await??;
    Ok(())
}
async fn run_mosh(
    app: AppHandle,
    state: Shared,
    id: String,
    host_id: String,
    host: Host,
    mut rx: mpsc::Receiver<SessionInput>,
) -> Result<()> {
    let ssh = Arc::new(
        tokio::select! {r=connect_ssh(&app,&state,&id,&host_id,None)=>r?,_=wait_close(&mut rx)=>return Ok(())},
    );
    let mut channel = ssh.handle.channel_open_session().await?;
    // Fixed bootstrap command. The one-use MOSH_KEY never enters the renderer or a log.
    channel
        .exec(true, "mosh-server new -c 256 -l LANG=C.UTF-8")
        .await?;
    let bootstrap = tokio::time::timeout(Duration::from_secs(20), async {
        let mut bytes = Vec::new();
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    ensure!(bytes.len() < 65536, "Mosh bootstrap output too large");
                    bytes.extend_from_slice(&data);
                }
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        Ok::<_, anyhow::Error>(String::from_utf8_lossy(&bytes).into_owned())
    })
    .await
    .context("Mosh server bootstrap timed out")??;
    let tokens=bootstrap.lines().find_map(|l|l.strip_prefix("MOSH CONNECT ")).ok_or_else(||anyhow!("The server did not start Mosh. Install mosh-server and ensure a UTF-8 locale is available."))?;
    let mut parts = tokens.split_whitespace();
    let port = parts
        .next()
        .ok_or_else(|| anyhow!("Missing Mosh UDP port"))?
        .parse::<u16>()?;
    let secret = zeroize::Zeroizing::new(
        parts
            .next()
            .ok_or_else(|| anyhow!("Missing Mosh session key"))?
            .to_string(),
    );
    let address = if host.mosh_address.is_empty() {
        host.address.clone()
    } else {
        host.mosh_address.clone()
    };
    let ip = tokio::net::lookup_host((address.as_str(), port))
        .await?
        .next()
        .ok_or_else(|| anyhow!("Cannot resolve Mosh UDP address"))?
        .ip()
        .to_string();
    let runtime = app.path().resource_dir()?.join("mosh");
    let runtime = if crate::platform::mosh_binary(&runtime).is_file() {
        runtime
    } else {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
            "resources/mosh"
        } else {
            "resources/mosh-native"
        })
    };
    ensure!(
        crate::platform::mosh_binary(&runtime).is_file(),
        "Bundled Mosh runtime is missing. Run scripts/fetch-mosh.mjs before packaging."
    );
    event(
        &app,
        &id,
        "status",
        json!(format!(
            "Mosh UDP · {}:{} · UDP must be reachable separately from SSH proxies",
            ip, port
        )),
    );
    crate::metrics::register(
        &app,
        &state,
        &id,
        crate::metrics::Target::Ssh(ssh.clone(), host.label.clone()),
    );
    tokio::task::spawn_blocking(move || -> Result<()> {
        use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
        use std::io::{Read, Write};
        let pair = NativePtySystem::default().openpty(PtySize {
            rows: 30,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut cmd = CommandBuilder::new(crate::platform::mosh_binary(&runtime));
        cmd.arg(ip);
        cmd.arg(port.to_string());
        cmd.env("MOSH_KEY", secret.as_str());
        cmd.env("TERM", "xterm-256color");
        cmd.env("LANG", "C.UTF-8");
        cmd.env("MOSH_PREDICTION_DISPLAY", "adaptive");
        cmd.env("TERMINFO", runtime.join("usr/share/terminfo"));
        #[cfg(target_os = "linux")]
        cmd.env("LD_LIBRARY_PATH", runtime.join("lib"));
        let mut child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;
        let app2 = app.clone();
        let id2 = id.clone();
        let reader = std::thread::spawn(move || {
            let mut buf = [0; 16384];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => event(&app2, &id2, "data", json!(STANDARD.encode(&buf[..n]))),
                }
            }
        });
        event(&app, &id, "connected", Value::Null);
        loop {
            match rx.try_recv() {
                Ok(SessionInput::Data(data)) => {
                    writer.write_all(&data)?;
                    writer.flush()?;
                }
                Ok(SessionInput::Resize(cols, rows)) => pair.master.resize(PtySize {
                    cols,
                    rows,
                    pixel_width: 0,
                    pixel_height: 0,
                })?,
                Ok(SessionInput::Close) | Err(mpsc::error::TryRecvError::Disconnected) => {
                    let _ = child.kill();
                    break;
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    if child.try_wait()?.is_some() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        drop(writer);
        drop(pair.master);
        let _ = child.wait();
        let _ = reader.join();
        drop(ssh);
        Ok(())
    })
    .await??;
    Ok(())
}
#[derive(Default)]
struct Telnet {
    state: u8,
    command: u8,
}
impl Telnet {
    fn feed(&mut self, bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut data = vec![];
        let mut reply = vec![];
        for &b in bytes {
            match self.state {
                0 => {
                    if b == 255 {
                        self.state = 1
                    } else {
                        data.push(b)
                    }
                }
                1 => match b {
                    255 => {
                        data.push(255);
                        self.state = 0;
                    }
                    251..=254 => {
                        self.command = b;
                        self.state = 2;
                    }
                    250 => self.state = 3,
                    _ => self.state = 0,
                },
                2 => {
                    match self.command {
                        251 => reply.extend([255, if b == 1 || b == 3 { 253 } else { 254 }, b]),
                        253 => reply.extend([255, 252, b]),
                        _ => {}
                    }
                    self.state = 0;
                }
                3 => {
                    if b == 255 {
                        self.state = 4
                    }
                }
                4 => self.state = if b == 240 { 0 } else { 3 },
                _ => self.state = 0,
            }
        }
        (data, reply)
    }
}
