use super::*;
use crate::{
    connections,
    model::{Host, Record},
    state::{SessionInput, Shared},
};
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use russh::{
    keys::{HashAlg, PublicKeyOrCertificate},
    ChannelMsg,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
use termterm_team_client::connection::Authentication;
use uuid::Uuid;
use zeroize::Zeroize;

pub struct TrustState {
    pub epoch: u64,
    store: Mutex<termterm_team_client::local::Store>,
}
#[derive(Default)]
pub struct NativeConnections {
    trust: Mutex<Option<Arc<TrustState>>>,
    pending: Mutex<std::collections::HashSet<String>>,
}
pub(super) struct Reservation<'a> {
    owner: &'a NativeConnections,
    id: String,
}
impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if let Ok(mut p) = self.owner.pending.lock() {
            p.remove(&self.id);
        }
    }
}
impl NativeConnections {
    pub fn clear_trust(&self) {
        if let Ok(mut slot) = self.trust.lock() {
            *slot = None;
        }
    }
    pub(super) fn reserve(&self, id: &str) -> Result<Reservation<'_>> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Connection lock"))?;
        ensure!(
            pending.len() < 16 && pending.insert(id.to_string()),
            "Duplicate or too many pending connections"
        );
        Ok(Reservation {
            owner: self,
            id: id.to_string(),
        })
    }
    async fn trust(&self, app: &AppHandle, epoch: u64) -> Result<Arc<TrustState>> {
        if let Some(existing) = self
            .trust
            .lock()
            .map_err(|_| anyhow::anyhow!("Trust lock"))?
            .as_ref()
            .filter(|s| s.epoch == epoch)
            .cloned()
        {
            return Ok(existing);
        }
        let root = app.path().app_data_dir()?.join("team-v2-cache");
        let store = app
            .state::<DesktopState>()
            .local_store(&root, epoch)
            .await?;
        let mut slot = self
            .trust
            .lock()
            .map_err(|_| anyhow::anyhow!("Trust lock"))?;
        ensure!(
            app.state::<DesktopState>().current_epoch() == epoch,
            "Team session closed"
        );
        if slot.as_ref().is_none_or(|s| s.epoch != epoch) {
            *slot = Some(Arc::new(TrustState {
                epoch,
                store: Mutex::new(store),
            }));
        }
        Ok(slot.as_ref().unwrap().clone())
    }
}
impl TrustState {
    fn records(&self) -> Result<Vec<Record>> {
        let store = self
            .store
            .lock()
            .map_err(|_| anyhow::anyhow!("Trust lock"))?;
        let entry = store.read("known-hosts")?;
        if entry.revision == 0 {
            Ok(vec![])
        } else {
            Ok(serde_json::from_slice(&entry.bytes)?)
        }
    }
    pub async fn verify(
        &self,
        app: &AppHandle,
        state: &Shared,
        session: &str,
        address: &str,
        port: u16,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool> {
        ensure!(
            app.state::<DesktopState>().current_epoch() == self.epoch,
            "Team session closed"
        );
        let public = key.public_key();
        let endpoint = crate::ssh_compat::endpoint(address, port);
        let known = self.records()?;
        let known = crate::ssh_compat::known_hosts(&known, address, port);
        if crate::ssh_compat::verify_host_key(&known, &endpoint, &public)? {
            return Ok(true);
        }
        let fingerprint = public.fingerprint(HashAlg::Sha256).to_string();
        let answer=connections::prompt(app,state,session,"hostKey",serde_json::json!({"address":endpoint,"fingerprint":fingerprint,"algorithm":public.algorithm().to_string()})).await?;
        if answer.first().map(String::as_str) != Some("trust") {
            return Ok(false);
        }
        ensure!(
            app.state::<DesktopState>().current_epoch() == self.epoch,
            "Team session closed"
        );
        let mut store = self
            .store
            .lock()
            .map_err(|_| anyhow::anyhow!("Trust lock"))?;
        for _ in 0..8 {
            let entry = store.read("known-hosts")?;
            let mut known: Vec<Record> = if entry.revision == 0 {
                vec![]
            } else {
                serde_json::from_slice(&entry.bytes)?
            };
            // Re-read after the dialog. Another process may have trusted a different key.
            let existing = crate::ssh_compat::known_hosts(&known, address, port);
            if !existing.is_empty() {
                return crate::ssh_compat::verify_host_key(&existing, &endpoint, &public);
            }
            known.push(Record::new("knownHost",serde_json::json!({"address":endpoint,"publicKey":public.to_openssh()?,"fingerprint":fingerprint})));
            if store.replace("known-hosts", entry.revision, &serde_json::to_vec(&known)?)? {
                return Ok(true);
            }
        }
        anyhow::bail!("Host trust changed concurrently; retry connection")
    }
}
struct ConnectionHosts(Vec<Host>);
impl Drop for ConnectionHosts {
    fn drop(&mut self) {
        for host in &mut self.0 {
            host.password.zeroize();host.private_key.zeroize();host.passphrase.zeroize();
            if let Some(proxy)=host.proxy.as_mut(){proxy.username.zeroize();proxy.password.zeroize();}
        }
    }
}
fn event(app: &AppHandle, id: &str, kind: &str, detail: Value) {
    let _ = app.emit(
        "session-event",
        serde_json::json!({"id":id,"kind":kind,"detail":detail}),
    );
}

#[tauri::command]
pub fn team_v2_start(
    app: AppHandle,
    state: State<Shared>,
    host_id: Uuid,
) -> Result<String, String> {
    start(app, state.inner().clone(), host_id, None).map_err(|e| e.to_string())
}
#[derive(Clone)]
struct StartupDocument { id: Uuid, revision: i64, generation: Option<i64> }
async fn document(app: &AppHandle, id: Uuid) -> Result<(i64,Option<i64>,termterm_team_client::resources::TeamDocument)> {
    let row=app.state::<DesktopState>().call(DesktopAction::Resource {id}).await?;
    ensure!(row["permissions"].as_array().is_some_and(|p|p.iter().any(|p|p=="read")),"Snippet/workspace read permission required");
    let content:termterm_team_client::resources::ResourceContent=serde_json::from_value(row["content"].clone())?;
    ensure!(!content.deleted,"Document deleted");
    Ok((row["revision"].as_i64().context("Document revision missing")?,row["localGeneration"].as_i64(),content.document.context("Not a Team document")?))
}
async fn startup_command(app:&AppHandle, source:&Option<StartupDocument>, host:Uuid)->Result<String> {
    let Some(source)=source else{return Ok(String::new())};
    let (revision,generation,doc)=document(app,source.id).await?;
    ensure!(revision==source.revision && generation==source.generation,"Snippet/workspace changed; reopen the preview");
    match doc {
        termterm_team_client::resources::TeamDocument::Snippet {command}=>Ok(command),
        termterm_team_client::resources::TeamDocument::Workspace {entries,..}=>Ok(entries.into_iter().find(|e|e.host_id==host).context("Host not part of workspace")?.startup_command),
    }
}
/// Accept record identities only. Commands are reopened and authorized natively;
/// neither a frontend startup string nor a workspace grants target access.
#[tauri::command]
pub async fn team_v2_document_start(app:AppHandle,state:State<'_,Shared>,document_id:Uuid,target_id:Option<Uuid>,expected_revision:i64,local_generation:Option<i64>)->Result<Value,String> {
    async {
        use termterm_team_client::resources::{TeamDocument,WorkspaceLayout};
        let epoch=app.state::<DesktopState>().current_epoch();
        let (revision,generation,doc)=document(&app,document_id).await?;
        ensure!(revision==expected_revision && generation==local_generation,"Snippet/workspace changed; reopen the preview");
        let (layout,targets)=match doc {
            TeamDocument::Snippet{..} => (WorkspaceLayout::Tabs,vec![(target_id.context("Choose a target host")?,true)]),
            TeamDocument::Workspace{layout,entries} => {ensure!(target_id.is_none(),"Workspace target override denied");(layout,entries.into_iter().map(|e|(e.host_id,e.stats_enabled)).collect())},
        };
        let mut prepared=Vec::new();
        for (host,stats) in targets {
            let (resolved,plan)=app.state::<DesktopState>().connection_target(host).await?;
            ensure!(resolved==epoch,"Team session changed");
            prepared.push((host,stats,plan.steps.last().context("Target missing")?.label.clone()));
        }
        ensure!(app.state::<DesktopState>().current_epoch()==epoch,"Team session changed");
        let mut opened=Vec::new();
        for (host,stats,label) in prepared {
            match start(app.clone(),state.inner().clone(),host,Some(StartupDocument{id:document_id,revision,generation})) {
                Ok(id)=>opened.push(serde_json::json!({"id":id,"hostId":host,"label":label,"statsEnabled":stats})),
                Err(e)=>{
                    if let Ok(sessions)=state.sessions.lock(){for row in &opened {if let Some(sender)=row["id"].as_str().and_then(|id|sessions.get(id)){let _=sender.try_send(SessionInput::Close);}}}
                    return Err(e);
                }
            }
        }
        Ok(serde_json::json!({"layout":layout,"sessions":opened}))
    }.await.map_err(|e:anyhow::Error|e.to_string())
}
fn start(app: AppHandle, state: Shared, host_id: Uuid, startup:Option<StartupDocument>) -> Result<String> {
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| anyhow::anyhow!("Session lock"))?;
    ensure!(sessions.len() < 16, "Maximum 16 active sessions");
    let id = Uuid::new_v4().to_string();
    let (tx, rx) = tokio::sync::mpsc::channel(256);
    sessions.insert(id.clone(), tx);
    drop(sessions);
    let session = id.clone();
    let epoch = app.state::<DesktopState>().current_epoch();
    tauri::async_runtime::spawn(async move {
        let desktop = app.state::<DesktopState>();
        let result = tokio::select! {r=run(&app,&state,&session,host_id,epoch,rx,startup)=>r,_=desktop.cancelled(epoch)=>Ok(())};
        crate::metrics::stop(&state, &session);
        if let Err(e) = result {
            event(&app, &session, "error", serde_json::json!(format!("{e:#}")));
        }
        event(&app, &session, "closed", Value::Null);
        if let Ok(mut sessions) = state.sessions.lock() {
            sessions.remove(&session);
        }
    });
    Ok(id)
}
async fn run(
    app: &AppHandle,
    state: &Shared,
    id: &str,
    host_id: Uuid,
    epoch: u64,
    mut rx: tokio::sync::mpsc::Receiver<SessionInput>,
    startup:Option<StartupDocument>,
) -> Result<()> {
    let desktop = app.state::<DesktopState>();
    let mut size = (100, 30);
    let (connection, label, protocol, address) = tokio::select! {
     r=connect_transport(app,state,id,host_id,epoch,None)=>r?,
     _=async {while let Some(msg)=rx.recv().await {match msg{SessionInput::Close=>break,SessionInput::Resize(cols,rows)=>size=(cols,rows),_=>{}}}}=>return Ok(())
    };
    let connection = Arc::new(connection);
    let mut check = tokio::time::interval(Duration::from_secs(10));
    check.tick().await;
    let mut permission = Box::pin(async {
        loop {
            check.tick().await;
            match desktop.check_connection(host_id, epoch).await {
                Ok(()) => {}
                Err(e) if e.to_string() == "network_unavailable" => event(
                    app,
                    id,
                    "status",
                    serde_json::json!("Team servisine ulaşılamıyor; bağlantı açık"),
                ),
                Err(e) => return e,
            }
        }
    });
    let command=startup_command(app,&startup,host_id).await?;
    ensure!(desktop.current_epoch()==epoch,"Team session changed");
    if protocol == "mosh" {
        return tokio::select! {
            result=connections::run_authorized_mosh(app.clone(),state.clone(),id.into(),connections::MoshOptions{address,udp_address:String::new(),label,startup:command},connection,size,rx)=>result,
            error=&mut permission=>Err(error).context("Team Mosh permission revoked or login expired"),
        };
    }
    crate::metrics::register(
        app,
        state,
        id,
        crate::metrics::Target::Ssh(connection.clone(), label),
    );
    let mut channel = connection.handle.channel_open_session().await?;
    channel
        .request_pty(
            true,
            "xterm-256color",
            size.0 as u32,
            size.1 as u32,
            0,
            0,
            &[],
        )
        .await?;
    channel.request_shell(true).await?;
    event(app, id, "connected", Value::Null);
    if !command.is_empty() {channel.data(format!("{command}\n").as_bytes()).await?;}
    loop {
        tokio::select! {
         error=&mut permission=>{let _=channel.close().await;return Err(error).context("Team connection permission was revoked or login expired");},
         msg=channel.wait()=>match msg{Some(ChannelMsg::Data{data})|Some(ChannelMsg::ExtendedData{data,..})=>event(app,id,"data",serde_json::json!(STANDARD.encode(&data))),Some(ChannelMsg::Close)|None=>break,_=>{}},
         msg=rx.recv()=>match msg{Some(SessionInput::Data(data))=>channel.data(data.as_slice()).await?,Some(SessionInput::Resize(cols,rows))=>channel.window_change(cols as u32,rows as u32,0,0).await?,_=>{let _=channel.close().await;break;}}
        }
    }
    let _ = connection
        .handle
        .disconnect(russh::Disconnect::ByApplication, "TermTerm closed", "en")
        .await;
    Ok(())
}

pub(super) async fn connect_target(
    app: &AppHandle,
    state: &Shared,
    id: &str,
    host_id: Uuid,
    epoch: u64,
    remote: Option<(String, u16)>,
) -> Result<(connections::SshConnection, String)> {
    let (connection,label,_,_) = connect_transport(app,state,id,host_id,epoch,remote).await?;
    Ok((connection,label))
}
async fn connect_transport(
    app: &AppHandle, state: &Shared, id: &str, host_id: Uuid, epoch: u64,
    remote: Option<(String,u16)>,
) -> Result<(connections::SshConnection,String,String,String)> {
    let desktop = app.state::<DesktopState>();
    let (resolved_epoch, plan) = desktop.connection_target(host_id).await?;
    ensure!(epoch == resolved_epoch, "Team session changed");
    ensure!(plan.target_id == host_id && !plan.steps.is_empty(), "Connection target mismatch");
    let mut hosts=ConnectionHosts(Vec::new());
    for (index, target) in plan.steps.into_iter().enumerate() {
    target.authentication.validate()?;
    let mut host = Host {
        label: target.label,
        address: target.address,
        port: target.port,
        protocol: target.protocol,
        username: target.username.unwrap_or_default(),
        ..Default::default()
    };
    if let Some(config) = target.proxy {
        config.validate()?;
        let mut proxy = crate::model::Proxy {kind:config.kind.clone(),host:config.host.clone(),port:config.port,..Default::default()};
        if config.authenticate {
          if let Some(secret)=target.proxy_auth {
            secret.validate(&config)?; proxy.username=secret.username.clone();proxy.password=secret.password.clone();
          } else {
            let answers = zeroize::Zeroizing::new(connections::prompt(app,state,id,"authentication",
                serde_json::json!({"name":format!("Hop {} · Proxy {}:{}",index+1,proxy.host,proxy.port),
                "instructions":"Proxy kimlik bilgileri yalnız bu bağlantıda kullanılır; ortak host kaydına yazılmaz.",
                "prompts":[{"prompt":"Proxy kullanıcı adı","echo":true},{"prompt":"Proxy parolası","echo":false}]})).await?);
            ensure!(answers.len()==2 && !answers[0].is_empty() && !answers[1].is_empty(),"Proxy authentication cancelled");
            ensure!(answers[0].len()<=255 && answers[1].len()<=255 && !answers[0].contains(':'),"Invalid proxy credentials");
            proxy.username=answers[0].clone();proxy.password=answers[1].clone();
          }
        }
        host.proxy=Some(proxy);
    }
    match target.authentication {
        Authentication::Agent(agent) => {
            agent.validate()?;
            ensure!(if cfg!(windows) { agent.provider != "unix" } else { agent.provider == "unix" }, "Seçilen SSH agent bu platformda kullanılamıyor; kişisel kimliğini bu cihaz için düzenle");
            host.agent = agent.provider;
            host.agent_key = agent.fingerprint;
        }
        Authentication::Prompt => {}
        Authentication::KeyboardInteractive => host.keyboard_interactive = true,
        Authentication::Password(password) => host.password = password.to_string(),
        Authentication::Key(key) => {
            host.private_key = key.private_key.clone();
            host.passphrase = key.passphrase.clone();
            host.certificate = key.certificate.clone();
        }
    }
    hosts.0.push(host);
    }
    let last=hosts.0.last().context("Empty connection plan")?;
    let (label,protocol,address)=(last.label.clone(),last.protocol.clone(),last.address.clone());
    let trust = app.state::<NativeConnections>().trust(app, epoch).await?;
    let known = trust.records()?;
    let connection = connections::connect_hops(
        app,
        state,
        id,
        "team-v2",
        &hosts.0,
        &known,
        remote,
        Some(trust),
        true,
    )
    .await?;
    desktop.check_connection(host_id, epoch).await?;
    Ok((connection, label, protocol, address))
}
