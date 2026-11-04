use super::{identity::{self, Process}, network::OwnedNetwork, pipe, process, store::{DiskStore, ProtectedRoot}, wintun::{Session as TunSession, Wintun}, Handle};
use crate::{policy::{Plan, Registry}, protocol::{Challenge, EngineEvent, Request, SsoResult}, transaction::{DnsSettings, Metric, Journal, Object}};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{collections::{BTreeMap, HashMap}, io::{BufRead, BufReader, Write}, path::PathBuf, sync::Arc, time::Duration};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::windows::named_pipe::NamedPipeServer, sync::{mpsc,oneshot}, task::JoinHandle};
use uuid::Uuid;
use windows_sys::Win32::{Foundation::WAIT_OBJECT_0, System::Threading::{GetCurrentProcess, WaitForSingleObject}};
use zeroize::{Zeroize, Zeroizing};

enum Input { Request(Result<Request>), Engine(Uuid, u64, Result<EngineEvent>), Packet(Uuid, u64, Result<()>), ControlLost, Tick }
struct Output {frame:Zeroizing<Vec<u8>>, delivered:Option<tokio::sync::oneshot::Sender<std::result::Result<(),String>>>}
struct AddressWait {guid:Uuid,luid:u64,deadline:std::time::Instant}
struct PendingStop {error:Option<String>,deadline:std::time::Instant,timed_out:bool}
enum CleanupProof {Quiesced(u64),OwnerExited,Retry}
struct CleanupRetry {error:Option<String>,at:std::time::Instant}
fn stop_error(retry:Option<&CleanupRetry>,pending:Option<&PendingStop>,error:Option<String>)->Option<String> {
    retry.and_then(|retry|retry.error.clone()).or_else(||pending.and_then(|pending|pending.error.clone())).or(error)
}
impl PendingStop {
    fn acknowledge(&self,current:u64,received:u64,stopping:bool)->Result<()> {ensure!(current==received,"stale_session_generation");ensure!(stopping,"session_not_stopping");Ok(())}
    fn expire(&mut self,now:std::time::Instant)->bool {if self.timed_out || now<self.deadline {return false;}self.timed_out=true;true}
    async fn packet_failure(&mut self,prior_stop:bool,error:&anyhow::Error,diagnostic:&mut Option<oneshot::Receiver<String>>) {
        if prior_stop || !error.downcast_ref::<std::io::Error>().is_some_and(|e|e.kind()==std::io::ErrorKind::UnexpectedEof) {return;}
        if let Some(receiver)=diagnostic.take() {
            // Forwarding is already stopped; only the terminal reason waits for stdout to drain.
            if let Ok(Ok(reason))=tokio::time::timeout(Duration::from_secs(1),receiver).await {self.error=Some(reason);}
        }
    }
}
fn read_engine_events(reader:impl BufRead,id:Uuid,generation:u64,events:mpsc::Sender<Input>,mut diagnostic:Option<oneshot::Sender<String>>) {
    let mut reader=reader;
    loop {
        let event:Result<EngineEvent>=crate::protocol::read_sync(&mut reader).and_then(|v|serde_json::from_value(v).map_err(Into::into));
        if let Ok(event)=&event {
            if event.session_id!=id || event.generation!=generation || !matches!(event.kind.as_str(),"progress"|"connecting"|"challenge"|"network-plan"|"tunnel-ready"|"connected"|"failed"|"disconnected") {diagnostic.take();}
            else if event.kind=="failed" {
                if let Some(sender)=diagnostic.take() {if let Some(reason)=event.detail.as_str() {let _=sender.send(format!("engine_failed: {reason}"));}}
            }
        } else {diagnostic.take();}
        let closed=event.is_err();
        if events.blocking_send(Input::Engine(id,generation,event)).is_err() || closed {break;}
    }
}
fn initial_credential_fields(challenge:&Challenge,used:&mut bool)->Option<(&'static str,&'static str)> {
    let first=!*used;
    if challenge.fields.iter().any(|field|field.secret || matches!(field.purpose.as_str(),"secret"|"password"|"otp")) {*used=true;}
    if !first {return None;}
    termterm_vpn_core::password_challenge_fields(&json!({"fields":challenge.fields,"message":challenge.message,"error":challenge.error}))
}
#[derive(Default)]
struct ConnectionReadiness {stopping:bool,engine_connected:bool,addresses_ready:bool}
impl ConnectionReadiness {
    fn engine_connected(&mut self) {if !self.stopping {self.engine_connected=true;}}
    fn addresses_ready(&mut self) {if !self.stopping {self.addresses_ready=true;}}
    fn connected(&self)->bool {!self.stopping && self.engine_connected && self.addresses_ready}
    fn stop(&mut self) {self.stopping=true;}
}
struct Session {
    id: Uuid, generation: u64, public: Value, child: process::Child,
    input: mpsc::Sender<Zeroizing<Vec<u8>>>, packet: Arc<NamedPipeServer>,
    challenge: Option<Challenge>, username: String, password: Zeroizing<String>, remember: bool, credential_autofill_used: bool,
    sso_origin: Option<String>, challenge_created: Option<std::time::Instant>,
    plan: Option<Plan>, journal: Journal, store: DiskStore, reserved: bool,
    peer_paths: Vec<(std::net::IpAddr,super::network::PeerPath)>, expected_plan: Option<Plan>,
    tun: Option<Arc<TunSession>>, pump: Option<JoinHandle<()>>,
    address_wait:Option<AddressWait>,readiness:ConnectionReadiness,
    pending_stop:Option<PendingStop>,
    engine_failure:Option<oneshot::Receiver<String>>,
    cleanup_retry:Option<CleanupRetry>,
}
fn clear(value: &mut Value) {
    match value { Value::String(s) => s.zeroize(), Value::Array(a) => a.iter_mut().for_each(clear), Value::Object(o) => o.values_mut().for_each(clear), _ => {} }
}
fn private_frame(mut value: Value) -> Result<Zeroizing<Vec<u8>>> {
    let result = serde_json::to_vec(&value); clear(&mut value); let mut bytes = Zeroizing::new(result?);
    ensure!(bytes.len() < crate::CONTROL_LIMIT, "control_message_too_large"); bytes.push(b'\n'); Ok(bytes)
}
impl Session {
    fn write(&self, mut value: Value) -> Result<()> {
        value["sessionId"] = self.id.to_string().into(); value["generation"] = self.generation.into();
        self.input.try_send(private_frame(value)?).map_err(|_| anyhow::anyhow!("engine_control_backpressure_or_closed"))?; Ok(())
    }
    fn state(&mut self, state: &str, error: Option<&str>) -> Value {
        self.public["state"] = state.into();
        if let Some(mut challenge) = self.public.as_object_mut().unwrap().remove("challenge") { clear(&mut challenge); }
        if let Some(error) = error { self.public["error"] = error.into(); } else { self.public.as_object_mut().unwrap().remove("error"); }
        json!({"kind":"session","session":self.public})
    }
}
struct Broker {
    owner: Process, root: ProtectedRoot, network: OwnedNetwork, registry: Registry,
    sessions: HashMap<Uuid, Session>, vault: Option<Uuid>, bundle: PathBuf,
    input: mpsc::Sender<Input>, output: mpsc::Sender<Output>,
    validation: bool,
    closing_request:Option<u64>,
    accepting:bool,
}
impl Broker {
    fn send(&self, value: Value) -> Result<()> { self.output.try_send(Output {frame:private_frame(value)?,delivered:None}).map_err(|_| anyhow::anyhow!("manager_control_backpressure_or_closed"))?; Ok(()) }
    async fn start(&mut self, id: Uuid, generation: u64, profile_id: Uuid, mut profile: termterm_vpn_core::Profile) -> Result<Value> {
        ensure!(self.vault.is_some(), "hello_required");
        ensure!(self.accepting && self.closing_request.is_none(),"broker_closing");
        ensure!(generation > 0 && generation < 1_u64 << 53, "invalid_generation");
        ensure!(!self.sessions.contains_key(&id) && self.sessions.len() < crate::MAX_CONNECTIONS, "maximum_three_connections_or_duplicate_session");
        profile.validate()?;
        let sso_origin = if profile.family == termterm_vpn_core::Family::GlobalProtect { Some(termterm_vpn_core::sso::origin(&profile.server)?) } else { None };
        if self.validation {crate::protocol::validate_fixture_profile(&profile)?;}
        ensure!(matches!(profile.family, termterm_vpn_core::Family::OpenVpn | termterm_vpn_core::Family::WireGuard | termterm_vpn_core::Family::Fortinet | termterm_vpn_core::Family::GlobalProtect | termterm_vpn_core::Family::Cisco), "unsupported_protocol");
        if profile.family == termterm_vpn_core::Family::OpenVpn { termterm_vpn_core::import::openvpn_embedded(&profile.config)?; }
        let wireguard = if profile.family == termterm_vpn_core::Family::WireGuard {Some(termterm_vpn_core::wireguard::parse(&profile.config)?.engine_config()?)} else {None};
        let expected_plan = wireguard.as_ref().map(|config|Plan::from_engine(config["networkPlan"].clone())).transpose()?;
        let mut peer_paths=vec![];
        if let Some(config)=&wireguard {
            for endpoint in config["endpoints"].as_array().unwrap() {
                let peer=endpoint.as_str().unwrap().parse::<std::net::SocketAddr>()?.ip();
                let path=super::network::peer_path(peer)?;
                ensure!(!self.network.adapters.values().any(|adapter|adapter.luid==path.luid),"vpn_peer_uses_other_owned_vpn");
                peer_paths.push((peer,path));
            }
        }
        let packet_name = format!(r"\\.\pipe\TermTerm.VpnDev.packet.{}", Uuid::new_v4());
        let packet = pipe::server(&packet_name, &self.owner.identity, true)?;
        let endpoint = pipe::packet_endpoint(&packet_name)?;
        packet.connect().await?;
        let executable = self.bundle.join(match profile.family {termterm_vpn_core::Family::OpenVpn=>"termterm-openvpn.exe",termterm_vpn_core::Family::WireGuard=>"termterm-wireguard.exe",_=>"termterm-openconnect.exe"});
        let child = process::engine(&self.owner, &executable, endpoint.raw())?;
        drop(endpoint);
        let input_file = child.input.try_clone()?; let output_file = child.output.try_clone()?;
        let (input, mut writes) = mpsc::channel::<Zeroizing<Vec<u8>>>(16);
        let events = self.input.clone();
        std::thread::spawn(move || {
            let mut file = input_file;
            while let Some(frame) = writes.blocking_recv() {
                if file.write_all(&frame).and_then(|_| file.flush()).is_err() {
                    let _ = events.blocking_send(Input::Engine(id, generation, Err(anyhow::anyhow!("engine_control_write_failed")))); break;
                }
            }
        });
        let events = self.input.clone();
        let (failure_sender,failure_receiver)=oneshot::channel();
        let is_wireguard=profile.family==termterm_vpn_core::Family::WireGuard;
        std::thread::spawn(move || {
            read_engine_events(BufReader::new(output_file),id,generation,events,is_wireguard.then_some(failure_sender));
        });
        let journal = Journal::new(id, generation)?; let store = self.root.store(&journal);
        let public = json!({"id":id,"generation":generation,"profileId":profile_id,"vaultId":self.vault,"state":"connecting"});
        let session = Session { id, generation, public: public.clone(), child, input, packet: Arc::new(packet), challenge: None, sso_origin, challenge_created: None,
            username: std::mem::take(&mut profile.username), password: Zeroizing::new(std::mem::take(&mut profile.password)), remember: profile.remember, credential_autofill_used: false,
            plan: None, peer_paths,expected_plan,journal, store, reserved: false, tun: None, pump: None,address_wait:None,readiness:ConnectionReadiness::default(),pending_stop:None,engine_failure:is_wireguard.then_some(failure_receiver),cleanup_retry:None };
        let mut initial = json!({});
        if let Some(wireguard)=wireguard { initial["wireguard"]=wireguard;profile.config.zeroize(); }
        else if profile.family == termterm_vpn_core::Family::OpenVpn { initial["config"] = std::mem::take(&mut profile.config).into(); }
        else {
            initial["protocol"] = termterm_vpn_core::protocol(profile.family).unwrap().into(); initial["server"] = std::mem::take(&mut profile.server).into(); initial["realm"] = std::mem::take(&mut profile.realm).into();
            if self.validation {
                let ca = self.bundle.join("fixture-ca.pem");
                ensure!(ca.is_file(),"fixed_validation_fixture_ca_missing");
                initial["caFile"] = ca.to_str().ok_or_else(||anyhow::anyhow!("invalid_fixture_ca_path"))?.into();
            }
        }
        session.write(initial)?;
        self.sessions.insert(id, session); Ok(public)
    }
    async fn stop(&mut self, id: Uuid, generation: u64, error: Option<String>) -> Result<()> {
        let session=self.sessions.get_mut(&id).ok_or_else(||anyhow::anyhow!("session_not_found"))?;
        ensure!(session.generation==generation,"stale_session_generation");
        if session.pending_stop.is_some() {return Ok(());}
        #[cfg(feature="privileged-validation")]
        if let Some(diagnostic)=&self.network.diagnostics {diagnostic.record("stop_requested",Some(&session.journal),None,None,error.as_ref().map(|value|anyhow::anyhow!(value.clone())).as_ref());}
        session.address_wait=None;session.readiness.stop();
        session.pending_stop=Some(PendingStop {error,deadline:std::time::Instant::now()+Duration::from_secs(10),timed_out:false});
        let state=session.state("disconnecting",None);
        // Stop ciphertext immediately but keep the owned adapter/routes until sockets are closed.
        let _=session.child.terminate();
        self.send(state)?;self.send(json!({"kind":"quiesceRequired","id":id,"generation":generation}))
    }
    async fn finish_stop(&mut self, id: Uuid, generation: u64, error: Option<String>,proof:CleanupProof) -> Result<()> {
        let candidate=self.sessions.get(&id).ok_or_else(||anyhow::anyhow!("session_not_found"))?;
        ensure!(candidate.generation==generation,"stale_session_generation");
        match proof {
            CleanupProof::Quiesced(received)=>candidate.pending_stop.as_ref().ok_or_else(||anyhow::anyhow!("quiesce_not_requested"))?.acknowledge(generation,received,candidate.readiness.stopping)?,
            CleanupProof::OwnerExited=>ensure!(unsafe {WaitForSingleObject(self.owner.handle.raw(),0)}==WAIT_OBJECT_0,"live_owner_cleanup_requires_quiescence"),
            CleanupProof::Retry=>ensure!(candidate.cleanup_retry.is_some(),"cleanup_retry_requires_prior_quiescence"),
        }
        let error=stop_error(candidate.cleanup_retry.as_ref(),candidate.pending_stop.as_ref(),error);
        let retrying=matches!(proof,CleanupProof::Retry);
        #[cfg(feature="privileged-validation")]
        if !retrying {if let Some(diagnostic)=&self.network.diagnostics {diagnostic.record(if matches!(proof,CleanupProof::OwnerExited) {"owner_exited"} else {"quiesced"},Some(&candidate.journal),None,None,None);}}
        let mut session = self.sessions.remove(&id).ok_or_else(|| anyhow::anyhow!("session_not_found"))?;
        if session.generation != generation { self.sessions.insert(id, session); anyhow::bail!("stale_session_generation"); }
        session.address_wait=None;session.readiness.stop();
        #[cfg(feature="privileged-validation")]
        if !retrying {if let Some(diagnostic)=&self.network.diagnostics {diagnostic.record("cleanup_begin",Some(&session.journal),None,None,None);}}
        let cleanup = async {
            session.child.terminate()?;
            if let Some(pump) = session.pump.take() { pump.abort(); let _ = pump.await; }
            session.tun.take();
            cleanup_session_journal(&mut session.journal,&mut session.store,&mut self.network)?;
            session.store.remove(&session.journal)?;
            for entry in &session.journal.entries {if let Object::Adapter{guid,..}=entry.object {self.network.forget_closed_adapter(guid);}}
            if session.reserved { self.registry.release_after_cleanup(id, generation)?; }
            Ok::<_, anyhow::Error>(())
        }.await;
        if let Err(cleanup_error) = cleanup {
            #[cfg(feature="privileged-validation")]
            if !retrying || !cleanup_error.is::<super::network::AdapterUnregisterPending>() {if let Some(diagnostic)=&self.network.diagnostics {diagnostic.record("cleanup_failed",Some(&session.journal),None,None,Some(&cleanup_error));}}
            if cleanup_error.is::<super::network::AdapterUnregisterPending>() {
                session.cleanup_retry=Some(CleanupRetry {error:error.clone(),at:std::time::Instant::now()+Duration::from_secs(1)});
                let event=(!retrying).then(||session.state("disconnecting",Some("cleanup_pending: windows_interface_unregister_pending")));
                self.sessions.insert(id,session);if let Some(event)=event {self.send(event)?;}return Ok(());
            }
            session.cleanup_retry=None;
            let event = session.state("failed", Some(&format!("cleanup_required: {cleanup_error}")));
            self.sessions.insert(id, session); self.send(event)?; return Err(cleanup_error);
        }
        #[cfg(feature="privileged-validation")]
        if let Some(diagnostic)=&self.network.diagnostics {diagnostic.record("journal_removed",Some(&session.journal),None,None,None);}
        self.send(session.state(if error.is_some() { "failed" } else { "disconnected" }, error.as_deref()))?;
        Ok(())
    }
    async fn close(&mut self) -> Result<()> {
        let identities: Vec<_> = self.sessions.iter().map(|(id,s)| (*id,s.generation)).collect();
        let mut failure = None;
        for (id,generation) in identities { if let Err(e) = self.finish_stop(id,generation,None,CleanupProof::OwnerExited).await { failure = Some(e); } }
        if let Some(error) = failure { return Err(error); } Ok(())
    }
    async fn begin_close(&mut self,request_id:u64)->Result<()> {
        ensure!(self.closing_request.is_none(),"broker_already_closing");self.closing_request=Some(request_id);
        let sessions=self.sessions.iter().map(|(id,s)|(*id,s.generation)).collect::<Vec<_>>();
        for (id,generation) in sessions {self.stop(id,generation,None).await?;}Ok(())
    }
    fn poll_quiescence(&mut self)->Result<()> {
        let now=std::time::Instant::now();let mut events=vec![];
        for session in self.sessions.values_mut() {if session.cleanup_retry.is_none() && session.public["state"]!="failed" && session.pending_stop.as_mut().is_some_and(|pending|pending.expire(now)) {events.push(session.state("failed",Some("cleanup_required: quiesce_timeout_routes_retained")));}}
        for event in events {self.send(event)?;}Ok(())
    }
    async fn poll_cleanup(&mut self)->Result<()> {
        let now=std::time::Instant::now();
        let pending=self.sessions.iter().filter_map(|(id,session)|session.cleanup_retry.as_ref().filter(|retry|now>=retry.at).map(|retry|(*id,session.generation,retry.error.clone()))).collect::<Vec<_>>();
        for (id,generation,error) in pending {self.finish_stop(id,generation,error,CleanupProof::Retry).await?;}Ok(())
    }
    fn lease(&self,id:Uuid,generation:u64)->Result<Value> {
        let session=self.sessions.get(&id).ok_or_else(||anyhow::anyhow!("session_not_found"))?;
        ensure!(session.generation==generation && session.readiness.connected(),"session_not_connected_or_stale");
        let (guid,luid)=session.journal.entries.iter().find_map(|entry|match &entry.object {Object::Address {guid,luid,..} if !entry.undone=>Some((*guid,*luid)),_=>None}).ok_or_else(||anyhow::anyhow!("owned_interface_missing"))?;
        ensure!(self.network.adapters.get(&guid).is_some_and(|a|a.luid==luid) && OwnedNetwork::verify_interface(guid,luid)?,"interface_not_owned_by_this_process");
        let mut index=0;super::status(unsafe {windows_sys::Win32::NetworkManagement::IpHelper::ConvertInterfaceLuidToIndex(&windows_sys::Win32::NetworkManagement::Ndis::NET_LUID_LH {Value:luid},&mut index)})?;
        let plan=session.plan.as_ref().ok_or_else(||anyhow::anyhow!("network_plan_missing"))?;
        for (peer,path) in &session.peer_paths {super::network::verify_peer_path(*peer,path)?;}
        let families=plan.addresses.iter().map(|a|if a.cidr.address.is_ipv4(){"ipv4"}else{"ipv6"}).collect::<std::collections::BTreeSet<_>>();
        Ok(json!({"id":id,"generation":generation,"interfaceGuid":guid,"interfaceLuid":luid.to_string(),"interfaceIndex":index,"addressFamilies":families,"addresses":plan.addresses.iter().map(|a|a.cidr.text()).collect::<Vec<_>>()}))
    }
    fn publish_connected(&mut self,id:Uuid)->Result<()> {
        let session=self.sessions.get(&id).ok_or_else(||anyhow::anyhow!("session_not_found"))?;
        if !session.readiness.connected() {return Ok(());}
        let generation=session.generation;let lease=self.lease(id,generation)?;
        self.send(json!({"kind":"networkLease","id":id,"generation":generation,"lease":lease}))?;
        let event=self.sessions.get_mut(&id).unwrap().state("connected",None);self.send(event)
    }
    async fn poll_address_readiness(&mut self)->Result<()> {
        let pending=self.sessions.iter().filter_map(|(id,session)|session.address_wait.as_ref().map(|_|(*id,session.generation))).collect::<Vec<_>>();
        for (id,generation) in pending {
            let result=(||->Result<bool> {
                let session=self.sessions.get_mut(&id).ok_or_else(||anyhow::anyhow!("session_not_found"))?;
                let wait=session.address_wait.as_ref().ok_or_else(||anyhow::anyhow!("address_wait_missing"))?;
                ensure!(std::time::Instant::now()<wait.deadline,"owned_address_dad_timeout");
                let plan=session.plan.as_ref().ok_or_else(||anyhow::anyhow!("network_plan_missing"))?;
                let mut ready=true;
                for assignment in &plan.addresses {ready &= self.network.address_ready(wait.guid,wait.luid,&assignment.cidr)?;}
                if ready {for (peer,path) in &session.peer_paths {super::network::verify_peer_path(*peer,path)?;}}
                if ready && !plan.dns_servers.is_empty() {
                    let guid=wait.guid;let luid=wait.luid;
                    super::network::verify_dns_routes(&plan.dns_servers,luid)?;
                    let before=OwnedNetwork::metric(guid,luid)?;
                    session.journal.apply(Object::Metric {guid,luid,before,after:Metric {automatic:false,value:1}},&mut session.store,&mut self.network)?;
                    let before=OwnedNetwork::dns(guid,luid)?;
                    // A newly-created interface must not silently adopt externally configured DNS.
                    ensure!(before==DnsSettings::default(),"new_adapter_dns_already_configured");
                    let after=DnsSettings {servers:plan.dns_servers.clone(),search_domain:plan.search_domain.clone(),search_list:if plan.search_domain.is_empty(){vec![]}else{vec![plan.search_domain.clone()]}};
                    session.journal.apply(Object::Dns {guid,luid,before,after},&mut session.store,&mut self.network)?;
                }
                Ok(ready)
            })();
            match result {
                Ok(true)=>{let session=self.sessions.get_mut(&id).unwrap();session.address_wait=None;session.readiness.addresses_ready();self.publish_connected(id)?;}
                Ok(false)=>{},
                Err(error)=>{let _=self.stop(id,generation,Some(error.to_string())).await;}
            }
        } Ok(())
    }
    fn answer(&mut self, id: Uuid, generation: u64, challenge_id: u64, mut values: BTreeMap<String,String>) -> Result<Value> {
        let session = self.sessions.get_mut(&id).ok_or_else(|| anyhow::anyhow!("session_not_found"))?;
        ensure!(session.generation == generation, "stale_session_generation");
        ensure!(!session.readiness.stopping,"session_cleanup_pending");
        let challenge = session.challenge.as_ref().ok_or_else(|| anyhow::anyhow!("no_pending_challenge"))?;
        ensure!(challenge.id == challenge_id, "stale_challenge"); challenge.answer(&values)?;
        let gateway_origin = if session.sso_origin.is_some() {challenge.gateway_origin(&values)?} else {None};
        let frame = json!({"action":"answer","id":challenge_id,"values":values}); values.values_mut().for_each(Zeroize::zeroize);
        session.write(frame)?; if gateway_origin.is_some() {session.sso_origin = gateway_origin;} session.challenge = None; let event = session.state("connecting",None); self.send(event)?; Ok(json!({}))
    }
    fn sso_answer(&mut self, id: Uuid, generation: u64, challenge_id: u64, result: SsoResult) -> Result<Value> {
        let session = self.sessions.get_mut(&id).ok_or_else(|| anyhow::anyhow!("session_not_found"))?;
        ensure!(session.generation == generation && !session.readiness.stopping, "stale_session_generation");
        let challenge = session.challenge.as_ref().ok_or_else(|| anyhow::anyhow!("no_pending_challenge"))?;
        ensure!(challenge.id == challenge_id && session.challenge_created.is_some_and(|at| at.elapsed() < Duration::from_secs(300)), "stale_challenge");
        challenge.private.as_ref().ok_or_else(||anyhow::anyhow!("no_pending_sso_challenge"))?.answer(&result)?;
        session.write(json!({"action":"ssoAnswer","id":challenge_id,"result":result}))?;
        session.challenge = None; session.challenge_created = None;
        let event = session.state("connecting",None); self.send(event)?; Ok(json!({}))
    }
    async fn request(&mut self, request: Request) -> Result<(Value,bool)> {
        match request {
            Request::Hello { vault_id, .. } => { ensure!(self.vault.is_none(), "hello_already_received"); self.vault = Some(vault_id); Ok((json!({"version":crate::PROTOCOL_VERSION,"labOnly":true,"maxConnections":3,"dns":true,"fullTunnel":true,"networkLease":true,"quiesce":true}),false)) }
            Request::Connect { id,generation,profile_id,profile,.. } => Ok((self.start(id,generation,profile_id,profile).await?,false)),
            Request::Answer { id,generation,challenge_id,values,.. } => Ok((self.answer(id,generation,challenge_id,values)?,false)),
            Request::SsoAnswer { id,generation,challenge_id,result,.. } => Ok((self.sso_answer(id,generation,challenge_id,result)?,false)),
            Request::Disconnect { id,generation,.. } => { self.stop(id,generation,None).await?; Ok((json!({}),false)) }
            Request::Quiesced {id,generation,..} => {
                let session=self.sessions.get(&id).ok_or_else(||anyhow::anyhow!("session_not_found"))?;
                let pending=session.pending_stop.as_ref().ok_or_else(||anyhow::anyhow!("quiesce_not_requested"))?;
                pending.acknowledge(session.generation,generation,session.readiness.stopping)?;
                let error=pending.error.clone();self.finish_stop(id,generation,error,CleanupProof::Quiesced(generation)).await?;Ok((json!({}),false))
            }
            Request::Sessions { .. } => Ok((json!(self.sessions.values().map(|s| s.public.clone()).collect::<Vec<_>>()),false)),
            Request::NetworkReceipt {id,generation,..} => {
                let mut lease=self.lease(id,generation)?;
                if !self.validation {return Ok((lease,false));}
                let session=self.sessions.get(&id).ok_or_else(||anyhow::anyhow!("session_not_found"))?;
                ensure!(session.generation==generation && session.readiness.connected(),"session_not_connected_or_stale");
                let (guid,luid)=session.journal.entries.iter().find_map(|entry|match &entry.object {Object::Address {guid,luid,..} if !entry.undone=>Some((*guid,*luid)),_=>None}).ok_or_else(||anyhow::anyhow!("owned_interface_missing"))?;
                ensure!(self.network.adapters.get(&guid).is_some_and(|a|a.luid==luid),"interface_not_owned_by_this_process");
                let mut index=0;super::status(unsafe {windows_sys::Win32::NetworkManagement::IpHelper::ConvertInterfaceLuidToIndex(&windows_sys::Win32::NetworkManagement::Ndis::NET_LUID_LH {Value:luid},&mut index)})?;
                let plan=session.plan.as_ref().ok_or_else(||anyhow::anyhow!("network_plan_missing"))?;
                for (peer,path) in &session.peer_paths {super::network::verify_peer_path(*peer,path)?;}
                for (key,value) in json!({"guid":guid,"luid":luid.to_string(),"interfaceIndex":index,"dns":OwnedNetwork::dns(guid,luid)?,"metric":OwnedNetwork::metric(guid,luid)?.value,"fullTunnel":plan.full_tunnel,"peerPathVerified":!session.peer_paths.is_empty()}).as_object().unwrap() {lease[key]=value.clone();}
                Ok((lease,false))
            }
            Request::Close { .. } => {
                anyhow::bail!("close_requires_actor_continuation")
            }
        }
    }
    async fn engine_event(&mut self, id: Uuid, event: EngineEvent) -> Result<()> {
        let session = self.sessions.get_mut(&id).ok_or_else(|| anyhow::anyhow!("session_not_found"))?;
        ensure!(event.session_id == id && event.generation == session.generation, "stale_engine_generation");
        // A failed cleanup retains its session and reservation, but that generation
        // must never resume negotiation or announce connected from a queued frame.
        if session.readiness.stopping {return Ok(());}
        match event.kind.as_str() {
            "progress" => crate::protocol::validate_progress(event.detail)?,
            "connecting" => { let event = session.state("connecting",None); self.send(event)?; }
            "challenge" => {
                ensure!(session.challenge.is_none(), "concurrent_challenge");
                let mut challenge: Challenge = serde_json::from_value(event.detail)?; challenge.validate()?;
                if let Some(private) = &challenge.private {
                    ensure!(Some(&private.expected_origin) == session.sso_origin.as_ref(), "sso_origin_changed");
                    session.challenge_created = Some(std::time::Instant::now());
                }
                let credential_fields = initial_credential_fields(&challenge,&mut session.credential_autofill_used);
                if session.remember && !session.password.is_empty() && credential_fields.is_some() {
                    let (user, password) = credential_fields.unwrap();
                    let values = BTreeMap::from([(user.to_owned(),session.username.clone()),(password.to_owned(),session.password.to_string())]);
                    let generation = session.generation; let challenge_id = challenge.id; session.challenge = Some(challenge); self.answer(id,generation,challenge_id,values)?;
                } else {
                    for field in &mut challenge.fields { if matches!((field.name.as_str(),field.purpose.as_str()), ("username","username") | ("user","text")) {field.value = session.username.clone();} }
                    session.public["state"] = "challenge".into(); session.public["challenge"] = serde_json::to_value(&challenge)?; session.challenge = Some(challenge);
                    let event = json!({"kind":"session","session":session.public}); self.send(event)?;
                }
            }
            "network-plan" => {
                ensure!(session.plan.is_none(), "reconnection_requires_fresh_session");
                let plan = Plan::from_engine(event.detail)?;
                ensure!(session.expected_plan.as_ref().is_none_or(|expected|expected==&plan),"wireguard_network_plan_changed");
                let owned: Vec<_> = self.network.adapters.values().map(|a|a.luid).collect();
                super::validation::check_host_plan(&plan,&owned)?;
                if plan.full_tunnel && session.peer_paths.is_empty() {
                    let path=super::network::peer_path(plan.remote.ok_or_else(||anyhow::anyhow!("full_tunnel_peer_missing"))?)?;
                    ensure!(!owned.contains(&path.luid),"vpn_peer_uses_other_owned_vpn");session.peer_paths.push((plan.remote.unwrap(),path));
                }
                for (peer,path) in &session.peer_paths {super::network::verify_peer_path(*peer,path)?;}
                self.registry.reserve(id,session.generation,&plan)?;
                session.reserved = true; session.plan = Some(plan); session.write(json!({"action":"accept-network"}))?;
            }
            "tunnel-ready" => {
                let plan = session.plan.as_ref().ok_or_else(|| anyhow::anyhow!("network_not_reserved"))?;
                ensure!(session.tun.is_none(), "tunnel_already_configured");
                // The engine must not alter its proposal after the reservation was accepted.
                ensure!(&Plan::from_engine(event.detail)? == plan, "network_plan_changed_after_reservation");
                let guid = Uuid::new_v4(); let name = format!("TermTerm VPN {}",guid.simple());
                session.journal.apply(Object::Adapter {guid,name},&mut session.store,&mut self.network)?;
                let adapter = self.network.adapters.get(&guid).ok_or_else(|| anyhow::anyhow!("adapter_missing"))?.clone(); let luid = adapter.luid;
                for assignment in &plan.addresses { session.journal.apply(Object::Address {guid,luid,cidr:assignment.cidr.clone()},&mut session.store,&mut self.network)?; }
                for ipv6 in [false,true] { if plan.addresses.iter().any(|a| a.cidr.address.is_ipv6() == ipv6) {
                    let before = OwnedNetwork::mtu(guid,luid,ipv6)?;
                    session.journal.apply(Object::Mtu {guid,luid,ipv6,before,after:plan.mtu},&mut session.store,&mut self.network)?;
                } }
                for destination in super::network::route_apply_order(&plan.routes) { session.journal.apply(Object::Route {guid,luid,destination:destination.clone(),metric:5},&mut session.store,&mut self.network)?; }
                for (peer,path) in &session.peer_paths {super::network::verify_peer_path(*peer,path)?;}
                let tun = Arc::new(adapter.start(plan.mtu as usize)?); session.tun = Some(tun.clone());
                let packet = session.packet.clone(); let events = self.input.clone(); let mtu = plan.mtu as usize; let generation=session.generation;
                session.pump = Some(tokio::spawn(async move {
                    let result = async {
                        let mut poll = tokio::time::interval(Duration::from_millis(2));
                        loop { tokio::select! {
                            incoming = pipe::read_packet(&packet,mtu) => { tun.send(&incoming?)?; }
                            _ = poll.tick() => { for _ in 0..64 { let Some(packet_bytes) = tun.receive()? else {break;}; tokio::time::timeout(Duration::from_secs(2),pipe::write_packet(&packet,&packet_bytes,mtu)).await??; } }
                        } }
                        #[allow(unreachable_code)] Ok::<_,anyhow::Error>(())
                    }.await;
                    let _ = events.send(Input::Packet(id,generation,result)).await;
                }));
                // Address creation returns before DAD completes. The packet pump
                // must already be running while the actor polls actual usability.
                session.address_wait=Some(AddressWait{guid,luid,deadline:std::time::Instant::now()+Duration::from_secs(10)});
                session.write(json!({"action":"network-configured","packetHandle":session.child.packet_handle}))?;
            }
            "connected" => { ensure!(session.tun.is_some() && session.reserved, "unconfigured_connection");session.readiness.engine_connected();self.publish_connected(id)?; }
            "failed" => anyhow::bail!("engine_failed: {}", event.detail.as_str().unwrap_or("vpn_transport_failed")),
            "disconnected" => { let generation = session.generation; self.stop(id,generation,None).await?; }
            _ => anyhow::bail!("unknown_engine_event"),
        }
        Ok(())
    }
}

fn closed_adapter_only(journal:&Journal,closed:impl FnOnce(Uuid,Option<u64>)->bool)->bool {
    let pending=journal.entries.iter().filter(|entry|!entry.undone).collect::<Vec<_>>();
    let [entry]=pending.as_slice() else {return false;};
    let Object::Adapter{guid,..}=entry.object else {return false;};
    let mut expected=None;
    for entry in &journal.entries {
        if let Object::Address{guid:other,luid,..}|Object::Route{guid:other,luid,..}|Object::Mtu{guid:other,luid,..}|Object::Dns{guid:other,luid,..}|Object::Metric{guid:other,luid,..}=entry.object {
            if guid!=other || luid==0 || expected.is_some_and(|old|old!=luid) {return false;}expected=Some(luid);
        }
    }
    closed(guid,expected)
}
pub(super) fn cleanup_session_journal(journal:&mut Journal,store:&mut DiskStore,network:&mut OwnedNetwork)->Result<()> {
    let error=match journal.cleanup(store,network) {Ok(())=>return Ok(()),Err(error)=>error};
    if error.is::<super::network::AdapterUnregisterPending>() {
        ensure!(closed_adapter_only(journal,|guid,luid|network.closed_adapter(guid,luid)),"pending_cleanup_requires_closed_owned_adapter");return Err(error);
    }
    // Close is void and can leave a nonpresent devnode. Reuse the fully verified
    // phantom path only after our own raw Close returned and reverse network undo finished.
    if !closed_adapter_only(journal,|guid,luid|network.closed_adapter(guid,luid)) {return Err(error);}
    let candidates=super::orphan::preflight(std::slice::from_ref(journal)).with_context(||format!("normal_close_owned_phantom_preflight after {error:#}"))?;
    super::orphan::remove(candidates).with_context(||format!("normal_close_owned_phantom_cleanup after {error:#}"))?;
    journal.cleanup(store,network).context("normal_close_journal_retry")
}

pub fn recover(root: &ProtectedRoot, network: &mut OwnedNetwork) -> Result<()> {
    let journals=root.journals()?;
    #[cfg(feature="privileged-validation")]
    if let Some(diagnostic)=&network.diagnostics {for (journal,_) in &journals {diagnostic.record("recovery_journal",Some(journal),None,None,None);}}
    let snapshots=journals.iter().map(|(journal,_)|journal.clone()).collect::<Vec<_>>();
    let (candidates,inactive)=super::orphan::recovery_preflight(&snapshots).context("owned_phantom_preflight")?;
    network.recovery_identities(inactive)?;
    super::orphan::remove(candidates).context("owned_phantom_cleanup")?;
    let mut pending=false;
    for (mut journal,mut store) in journals {match journal.cleanup(&mut store,network) {
        Ok(())=>store.remove(&journal)?,
        Err(error) if error.is::<super::network::AdapterUnregisterPending>()=>pending=true,
        Err(error)=>return Err(error),
    }}
    if pending {return Err(super::network::AdapterUnregisterPending.into());}Ok(())
}
fn cleanup_receipt(remaining_sessions: usize, remaining_journals: usize) -> Result<Value> {
    ensure!(remaining_sessions == 0 && remaining_journals == 0,"cleanup_verification_pending");
    Ok(json!({"cleanupVerified":true,"remainingJournals":remaining_journals,"remainingSessions":remaining_sessions}))
}
async fn final_reply(sender: &mpsc::Sender<Output>, value: Value) -> Result<()> {
    let (ack,delivered)=tokio::sync::oneshot::channel();
    sender.try_send(Output {frame:private_frame(value)?,delivered:Some(ack)}).map_err(|_|anyhow::anyhow!("manager_control_backpressure_or_closed"))?;
    tokio::time::timeout(Duration::from_secs(5),delivered).await.map_err(|_|anyhow::anyhow!("final_reply_delivery_timeout"))??.map_err(anyhow::Error::msg)
}
async fn output_loop<W:tokio::io::AsyncWrite+Unpin>(mut writer: W, mut output:mpsc::Receiver<Output>, events:mpsc::Sender<Input>) {
    while let Some(frame)=output.recv().await {
        let result=writer.write_all(&frame.frame).await;
        if let Some(delivered)=frame.delivered {let _=delivered.send(result.as_ref().map(|_|()).map_err(|_|"manager_control_write_failed".into()));}
        if result.is_err() {let _=events.send(Input::ControlLost).await;break;}
    }
}

pub async fn serve(pipe_id: Uuid, parent: u32, birth: u64, validation: bool) -> Result<()> {
    ensure!(!validation || cfg!(feature="privileged-validation"),"validation_build_required");
    ensure!(crate::PRIVILEGED_PATH_VALIDATED || (cfg!(feature="privileged-validation") && validation), "privileged_windows_path_not_validated");
    let owner = Process::open(parent)?; ensure!(owner.birth == birth && !owner.identity.elevated, "invalid_manager_process");
    let current = identity::process_identity(unsafe {GetCurrentProcess()})?; ensure!(current.elevated,"elevated_broker_required"); identity::verify_elevated_peer(&owner.identity,unsafe {GetCurrentProcess()})?;
    let lock = super::store::machine_lock()?; let root = ProtectedRoot::open()?;
    if validation {ensure!(root.journal_count()? == 0,"validation_refuses_existing_owned_journals");}
    let helper = std::env::current_exe()?.canonicalize()?; let bundle = helper.parent().unwrap().to_path_buf();
    let watchdog = process::watchdog(&helper,&lock)?;
    if !validation {recover(&root,&mut OwnedNetwork::recovering())?;}
    let api = Wintun::load(&bundle.join("wintun.dll"))?; let network = OwnedNetwork::new(api);
    #[cfg(feature="privileged-validation")]
    let network={let mut network=network;if validation {network.diagnostics=Some(Arc::new(super::diagnostics::Diagnostics::open()?));}network};
    let mut server = pipe::server(&format!(r"\\.\pipe\TermTerm.VpnDev.control.{pipe_id}"),&owner.identity,false)?;
    tokio::time::timeout(Duration::from_secs(30),server.connect()).await??;
    let mut preamble = [0]; tokio::time::timeout(Duration::from_secs(5),server.read_exact(&mut preamble)).await??;
    ensure!(preamble == [1],"invalid_authentication_preamble"); pipe::verify_client(&server,&owner)?; server.write_all(&[1]).await?;
    let (reader,writer) = tokio::io::split(server); let (events,mut inputs) = mpsc::channel(64); let (outgoing,output) = mpsc::channel::<Output>(64);
    let events_copy = events.clone();
    tokio::spawn(async move { let mut reader = tokio::io::BufReader::new(reader); loop { let request = crate::protocol::read_async(&mut reader).await; let closed = request.is_err(); if events_copy.send(Input::Request(request)).await.is_err() || closed {break;} } });
    let events_copy = events.clone();
    tokio::spawn(output_loop(writer,output,events_copy));
    let mut broker = Broker { owner,root,network,registry:Registry::default(),sessions:HashMap::new(),vault:None,bundle,input:events,output:outgoing,validation,closing_request:None,accepting:true };
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    loop {
        let event = tokio::select! { event = inputs.recv() => event, _ = tick.tick() => Some(Input::Tick) };
        match event {
            Some(Input::Request(Ok(Request::Close {request_id}))) => {
                if let Err(error)=broker.begin_close(request_id).await {if broker.send(json!({"kind":"reply","requestId":request_id,"ok":false,"error":error.to_string()})).is_err(){break;}}
            }
            Some(Input::Request(Ok(request))) => {
                let request_id = request.request_id(); let response = broker.request(request).await;
                let close = response.as_ref().is_ok_and(|(_,close)| *close);
                let reply = match response { Ok((result,_)) => json!({"kind":"reply","requestId":request_id,"ok":true,"result":result}), Err(error) => json!({"kind":"reply","requestId":request_id,"ok":false,"error":error.to_string()}) };
                if close {final_reply(&broker.output,reply).await?;break;}
                if broker.send(reply).is_err() {break;}
            }
            Some(Input::Engine(id,generation,event)) => {
                if !broker.sessions.get(&id).is_some_and(|s|s.generation == generation) {continue;}
                let result = match event { Ok(event) => broker.engine_event(id,event).await, Err(error) => Err(error) };
                if let Err(error) = result { if let Some(session) = broker.sessions.get(&id) { let generation = session.generation; let _ = broker.stop(id,generation,Some(error.to_string())).await; } }
            }
            Some(Input::Packet(id,generation,result)) => {
                if let Some(session)=broker.sessions.get(&id).filter(|s|s.generation==generation) {
                    let prior_stop=session.pending_stop.is_some();
                    let error=result.err().unwrap_or_else(||anyhow::anyhow!("packet_channel_closed"));
                    let _=broker.stop(id,generation,Some(error.to_string())).await;
                    if let Some(session)=broker.sessions.get_mut(&id) {
                        if let Some(pending)=&mut session.pending_stop {pending.packet_failure(prior_stop,&error,&mut session.engine_failure).await;}
                    }
                }
            }
            Some(Input::Tick) => {
                if unsafe {WaitForSingleObject(broker.owner.handle.raw(),0)}==WAIT_OBJECT_0 {break;}
                if unsafe {WaitForSingleObject(watchdog.raw(),0)}==WAIT_OBJECT_0 {broker.accepting=false;}
                if broker.poll_address_readiness().await.is_err() || broker.poll_quiescence().is_err() || broker.poll_cleanup().await.is_err() {broker.accepting=false;}
            }
            Some(Input::ControlLost) | Some(Input::Request(Err(_))) | None => {broker.accepting=false;}
        }
        if !broker.accepting {
            let sessions=broker.sessions.iter().map(|(id,s)|(*id,s.generation)).collect::<Vec<_>>();
            for (id,generation) in sessions {let _=broker.stop(id,generation,Some("broker_control_or_watchdog_lost".into())).await;}
            if broker.sessions.is_empty() {break;}
        }
        if let Some(request_id)=broker.closing_request {
            if broker.sessions.is_empty() {
                let receipt=cleanup_receipt(0,broker.root.journal_count()?);
                let reply=match receipt {Ok(result)=>json!({"kind":"reply","requestId":request_id,"ok":true,"result":result}),Err(error)=>json!({"kind":"reply","requestId":request_id,"ok":false,"error":error.to_string()})};
                final_reply(&broker.output,reply).await?;break;
            }
            if broker.sessions.values().any(|s|s.readiness.stopping && s.public["state"]=="failed") {
                broker.closing_request=None;
                if broker.send(json!({"kind":"reply","requestId":request_id,"ok":false,"error":"cleanup_required: routes_retained_until_quiescence"})).is_err(){break;}
            }
            else if broker.sessions.values().any(|s|s.cleanup_retry.is_some()) {
                broker.closing_request=None;
                if broker.send(json!({"kind":"reply","requestId":request_id,"ok":false,"error":"cleanup_pending: windows_interface_unregister_pending"})).is_err(){break;}
            }
        }
    }
    // Losing a pipe is not proof that the live application closed its sockets.
    // Retain the blackhole until the actual authenticated process handle signals exit.
    if !broker.sessions.is_empty() {
        let sessions=broker.sessions.iter().map(|(id,s)|(*id,s.generation)).collect::<Vec<_>>();
        for (id,generation) in sessions {let _=broker.stop(id,generation,None).await;}
        while unsafe {WaitForSingleObject(broker.owner.handle.raw(),0)}!=WAIT_OBJECT_0 {tokio::time::sleep(Duration::from_millis(200)).await;}
    }
    broker.close().await
}

pub fn watchdog(broker: Handle, _lock: Handle, ready: Handle) -> Result<()> {
    let current = identity::process_identity(unsafe {GetCurrentProcess()})?;
    ensure!(current.elevated,"elevated_watchdog_required");
    identity::same_logon(&current,&identity::process_identity(broker.raw())?)?;
    let root = ProtectedRoot::open()?;
    let mut network = OwnedNetwork::recovering();
    #[cfg(feature="privileged-validation")]
    {network.diagnostics=Some(Arc::new(super::diagnostics::Diagnostics::open()?));}
    super::win(unsafe {windows_sys::Win32::System::Threading::SetEvent(ready.raw())})?;
    ensure!(unsafe {WaitForSingleObject(broker.raw(),u32::MAX)} == WAIT_OBJECT_0,"broker_wait_failed");
    reconcile_cleanup(|| {
        #[cfg(feature="privileged-validation")]
        if let Some(diagnostic)=&network.diagnostics {diagnostic.record("watchdog_recovery_begin",None,None,None,None);}
        let result=recover(&root,&mut network);
        #[cfg(feature="privileged-validation")]
        if let Some(diagnostic)=&network.diagnostics {diagnostic.record(if result.is_ok() {"watchdog_recovery_finished"} else {"watchdog_recovery_failed"},None,None,None,result.as_ref().err());}
        result
    },std::thread::sleep)
}
fn reconcile_cleanup(mut recover:impl FnMut()->Result<()>,mut pause:impl FnMut(Duration))->Result<()> {
    let mut failures=0;
    loop {match recover() {
        Ok(())=>return Ok(()),
        Err(error) if error.is::<super::network::AdapterUnregisterPending>()=>pause(Duration::from_secs(1)),
        Err(error)=>{failures+=1;if failures==3 {return Err(error);}pause(Duration::from_millis(500));}
    }}
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet_eof()->anyhow::Error {std::io::Error::new(std::io::ErrorKind::UnexpectedEof,"packet_channel_closed").into()}
    fn pending_stop(error:Option<&str>)->PendingStop {PendingStop{error:error.map(str::to_owned),deadline:std::time::Instant::now()+Duration::from_secs(10),timed_out:false}}
    #[tokio::test]
    async fn wireguard_eof_waits_for_held_reader_before_blocked_actor_delivery() {
        let id=Uuid::new_v4();
        let raw=serde_json::to_vec(&json!({"kind":"failed","sessionId":id,"generation":7,"detail":"wireguard_peer_handshake_timeout"})).unwrap();
            let (events,inputs)=mpsc::channel(1);events.send(Input::Tick).await.unwrap();
            let (diagnostic_sender,diagnostic)=oneshot::channel();let mut diagnostic=Some(diagnostic);
            let (release,held)=std::sync::mpsc::channel();let (done,mut finished)=oneshot::channel();
            let mut frame=raw.clone();frame.push(b'\n');
            let reader=std::thread::spawn(move||{held.recv().unwrap();read_engine_events(std::io::Cursor::new(frame),id,7,events,Some(diagnostic_sender));let _=done.send(());});
            let mut pending=pending_stop(Some("packet_channel_closed"));let mut readiness=ConnectionReadiness::default();readiness.stop();
            let eof=packet_eof();let resolution=pending.packet_failure(false,&eof,&mut diagnostic);
            tokio::join!(resolution,async {tokio::task::yield_now().await;release.send(()).unwrap();});
            assert_eq!(pending.error.as_deref(),Some("engine_failed: wireguard_peer_handshake_timeout"));assert!(!readiness.connected());
            assert!(matches!(finished.try_recv(),Err(oneshot::error::TryRecvError::Empty)));
            drop(inputs);reader.join().unwrap();
    }
    #[tokio::test]
    async fn wireguard_failure_first_preserves_its_reason_when_packet_eof_follows() {
        let id=Uuid::new_v4();let frame=format!("{}\n",json!({"kind":"failed","sessionId":id,"generation":7,"detail":"wireguard_peer_handshake_timeout"}));
        let (events,mut inputs)=mpsc::channel(1);let (sender,receiver)=oneshot::channel();let mut diagnostic=Some(receiver);
        let reader=std::thread::spawn(move||read_engine_events(std::io::Cursor::new(frame),id,7,events,Some(sender)));
        let Some(Input::Engine(_,_,Ok(event)))=inputs.recv().await else {panic!("missing engine failure");};
        let reason=format!("engine_failed: {}",event.detail.as_str().unwrap());let mut pending=pending_stop(Some(&reason));
        pending.packet_failure(true,&packet_eof(),&mut diagnostic).await;
        assert_eq!(pending.error.as_deref(),Some(reason.as_str()));assert!(diagnostic.is_some());
        drop(inputs);reader.join().unwrap();
    }
    #[tokio::test]
    async fn packet_diagnostics_preserve_prior_stops_corruption_and_bounded_missing_failure() {
        for (prior,error,original) in [(true,packet_eof(),None),(true,packet_eof(),Some("first_failure")),(false,anyhow::anyhow!("ipv4_header_truncated"),Some("ipv4_header_truncated"))] {
            let (sender,receiver)=oneshot::channel();sender.send("engine_failed: replacement".into()).unwrap();let mut diagnostic=Some(receiver);
            let mut pending=pending_stop(original);pending.packet_failure(prior,&error,&mut diagnostic).await;
            assert_eq!(pending.error.as_deref(),original);assert!(diagnostic.is_some());
        }
        let (_sender,receiver)=oneshot::channel::<String>();let mut diagnostic=Some(receiver);let mut pending=pending_stop(Some("packet_channel_closed"));
        let clock=std::time::Instant::now();pending.packet_failure(false,&packet_eof(),&mut diagnostic).await;
        assert!(clock.elapsed()>=Duration::from_millis(950) && clock.elapsed()<Duration::from_secs(2));
        assert_eq!(pending.error.as_deref(),Some("packet_channel_closed"));assert!(diagnostic.is_none());
    }
    #[tokio::test]
    async fn malformed_stale_and_nonfailure_frames_cannot_replace_packet_closure() {
        let id=Uuid::new_v4();
        for frame in [json!({"kind":"failed","sessionId":Uuid::new_v4(),"generation":7,"detail":"wrong_session"}),json!({"kind":"failed","sessionId":id,"generation":6,"detail":"stale"}),json!({"kind":"failed","sessionId":id,"generation":7,"detail":{"unsafe":"object"}}),json!({"kind":"connected","sessionId":id,"generation":7}),json!({"kind":"unknown","sessionId":id,"generation":7}),json!({"kind":"failed","sessionId":id,"generation":7,"detail":"bad","unknown":true})] {
            let valid_success=frame["kind"]=="connected";
            let mut raw=serde_json::to_vec(&frame).unwrap();raw.push(b'\n');
            if !valid_success {raw.extend(format!("{}\n",json!({"kind":"failed","sessionId":id,"generation":7,"detail":"must_not_mask_an_earlier_invalid_frame"})).as_bytes());}
            let (events,mut inputs)=mpsc::channel(8);let (sender,receiver)=oneshot::channel();let mut diagnostic=Some(receiver);
            let reader=std::thread::spawn(move||read_engine_events(std::io::Cursor::new(raw),id,7,events,Some(sender)));
            let mut pending=pending_stop(Some("packet_channel_closed"));pending.packet_failure(false,&packet_eof(),&mut diagnostic).await;
            assert_eq!(pending.error.as_deref(),Some("packet_channel_closed"));
            while inputs.recv().await.is_some() {}reader.join().unwrap();
        }
    }
    #[test]
    fn quiesce_acknowledgement_is_generation_bound_and_timeout_never_authorizes_cleanup() {
        let now=std::time::Instant::now();let mut pending=PendingStop{error:None,deadline:now+Duration::from_secs(10),timed_out:false};
        assert!(pending.acknowledge(7,6,true).is_err());assert!(pending.acknowledge(7,7,false).is_err());assert!(pending.acknowledge(7,7,true).is_ok());
        assert!(!pending.expire(now));assert!(pending.expire(now+Duration::from_secs(10)));assert!(!pending.expire(now+Duration::from_secs(20)));assert!(pending.timed_out);
        // A late exact-generation ACK still proves closure; timeout alone grants nothing.
        assert!(pending.acknowledge(7,7,true).is_ok());assert!(pending.acknowledge(8,7,true).is_err());
    }
    #[test]
    fn initial_verification_form_never_receives_saved_password_or_enables_later_autofill() {
        let form=|message:&str,fields:Value|serde_json::from_value::<Challenge>(json!({"id":1,"message":message,"fields":fields})).unwrap();
        let fields=json!([{"name":"user","purpose":"text","label":"Username:","secret":false},{"name":"passwd","purpose":"secret","label":"Password:","secret":true}]);
        let normal=form("Please log in",fields.clone());let mut used=false;
        assert_eq!(initial_credential_fields(&normal,&mut used),Some(("user","passwd")));assert!(used);
        assert_eq!(initial_credential_fields(&normal,&mut used),None);
        for otp in [form("Enter verification token",fields),form("",json!([{"name":"passwd","purpose":"secret","label":"Challenge:","secret":true}]))] {
            let mut used=false;assert_eq!(initial_credential_fields(&otp,&mut used),None);assert!(used);
            assert_eq!(initial_credential_fields(&normal,&mut used),None);
        }
        let mut used=false;let gateway=form("Select gateway",json!([{"name":"gateway","purpose":"choice","label":"Gateway:","secret":false}]));
        assert_eq!(initial_credential_fields(&gateway,&mut used),None);assert!(!used);
        assert_eq!(initial_credential_fields(&normal,&mut used),Some(("user","passwd")));
    }
    #[test]
    fn readiness_requires_both_proofs_and_cancellation_timeout_or_failed_cleanup_stays_terminal() {
        let mut engine_first=ConnectionReadiness::default();engine_first.engine_connected();assert!(!engine_first.connected());engine_first.addresses_ready();assert!(engine_first.connected());
        let mut address_first=ConnectionReadiness::default();address_first.addresses_ready();assert!(!address_first.connected());address_first.engine_connected();assert!(address_first.connected());
        for proofs in 0..4 {
            let mut retained=ConnectionReadiness::default();if proofs&1!=0 {retained.engine_connected();}if proofs&2!=0 {retained.addresses_ready();}
            retained.stop();assert!(!retained.connected());
            // Applies equally to cancel, DAD failure/timeout, and early terminate failure:
            // retained tun/reservation or queued same-generation frames cannot revive it.
            retained.engine_connected();retained.addresses_ready();assert!(!retained.connected());assert!(retained.stopping);
        }
    }
    #[test]
    fn normal_phantom_completion_requires_our_closed_adapter_and_finished_network_undo() {
        use crate::{policy::Cidr,transaction::Entry};
        let guid=Uuid::new_v4();let luid=53_u64<<48|32772<<24;
        let adapter=Entry{object:Object::Adapter{guid,name:format!("TermTerm VPN {}",guid.simple())},undone:false};
        let address=Entry{object:Object::Address{guid,luid,cidr:Cidr::parse("192.0.2.241/32").unwrap()},undone:true};
        let mut journal=Journal::new(Uuid::new_v4(),1).unwrap();journal.entries=vec![adapter.clone(),address.clone()];
        assert!(closed_adapter_only(&journal,|g,l|g==guid && l==Some(luid)));
        assert!(!closed_adapter_only(&journal,|_,_|false));
        journal.entries[1].undone=false;assert!(!closed_adapter_only(&journal,|_,_|panic!("network undo is pending")));journal.entries[1].undone=true;
        journal.entries=vec![adapter.clone()];assert!(!closed_adapter_only(&journal,|_,_|false));
        assert!(closed_adapter_only(&journal,|g,l|g==guid && l.is_none()));
        journal.entries=vec![adapter.clone(),Entry{object:Object::Address{guid:Uuid::new_v4(),luid,cidr:Cidr::parse("192.0.2.241/32").unwrap()},undone:true}];assert!(!closed_adapter_only(&journal,|_,_|panic!("wrong GUID")));
        journal.entries=vec![adapter,address,Entry{object:Object::Mtu{guid,luid:luid+1,ipv6:false,before:1500,after:1400},undone:true}];assert!(!closed_adapter_only(&journal,|_,_|panic!("inconsistent LUID")));
        for object in [Object::Dns {guid,luid,before:DnsSettings::default(),after:DnsSettings::default()},Object::Metric {guid,luid,before:Metric {automatic:true,value:25},after:Metric {automatic:false,value:1}}] {
            journal.entries=vec![Entry {object:Object::Adapter {guid,name:format!("TermTerm VPN {}",guid.simple())},undone:false},Entry {object,undone:false}];
            assert!(!closed_adapter_only(&journal,|_,_|panic!("settings undo pending")));journal.entries[1].undone=true;assert!(closed_adapter_only(&journal,|g,l|g==guid && l==Some(luid)));
        }
    }
    #[test]
    fn cleanup_receipt_never_claims_pending_sessions_or_global_journals_are_clean() {
        assert!(cleanup_receipt(1,0).is_err());assert!(cleanup_receipt(0,1).is_err());assert_eq!(cleanup_receipt(0,0).unwrap()["cleanupVerified"],true);
    }
    #[test]
    fn original_stop_failure_survives_pending_hard_cleanup_and_owner_exit() {
        let pending=pending_stop(Some("engine_failed: wireguard_peer_unresponsive"));
        let retry=CleanupRetry {error:pending.error.clone(),at:std::time::Instant::now()};
        assert_eq!(stop_error(Some(&retry),Some(&pending),None),pending.error);
        assert_eq!(stop_error(None,Some(&pending),None),pending.error);
        assert_eq!(stop_error(None,Some(&pending),Some("broker_control_or_watchdog_lost".into())),pending.error);
        assert!(stop_error(None,Some(&pending_stop(None)),None).is_none());
        assert_eq!(stop_error(None,None,Some("engine_failed".into())).as_deref(),Some("engine_failed"));
    }
    #[test]
    fn crash_recovery_persists_network_undo_before_pending_and_resumes_a_failed_save() {
        use crate::{policy::Cidr,transaction::{Entry,Network,Store}};
        struct Memory {last:Journal,writes:usize,fail_at:usize}
        impl Store for Memory {fn save(&mut self,journal:&Journal)->Result<()> {self.writes+=1;ensure!(self.writes!=self.fail_at,"disk_failure");self.last=journal.clone();Ok(())}}
        struct Inactive {present:bool,changed:bool,undone:Vec<Object>}
        impl Network for Inactive {
            fn prepare(&self,_:&Object)->Result<()> {panic!("recovery must not prepare new mutations")}
            fn apply(&mut self,_:&Object)->Result<()> {panic!("recovery must not create new objects")}
            fn undo(&mut self,object:&Object)->Result<()> {
                if matches!(object,Object::Adapter {..}) {ensure!(!self.present,super::super::network::AdapterUnregisterPending);return Ok(());}
                ensure!(!self.changed,"owned_setting_changed");if !self.undone.contains(object) {self.undone.push(object.clone());}Ok(())
            }
        }
        let guid=Uuid::new_v4();let luid=53_u64<<48|32772<<24;let mut journal=Journal::new(Uuid::new_v4(),1).unwrap();
        journal.entries=vec![Entry {object:Object::Adapter {guid,name:format!("TermTerm VPN {}",guid.simple())},undone:false},Entry {object:Object::Address {guid,luid,cidr:Cidr::parse("192.0.2.241/32").unwrap()},undone:false},Entry {object:Object::Dns {guid,luid,before:DnsSettings::default(),after:DnsSettings {servers:vec!["192.0.2.53".parse().unwrap()],..Default::default()}},undone:false}];
        let mut store=Memory {last:journal.clone(),writes:0,fail_at:2};let mut network=Inactive {present:true,changed:false,undone:vec![]};
        assert_eq!(journal.cleanup(&mut store,&mut network).unwrap_err().to_string(),"disk_failure");
        assert!(!store.last.entries[1].undone && store.last.entries[2].undone);journal=store.last.clone();
        let error=journal.cleanup(&mut store,&mut network).unwrap_err();assert!(error.is::<super::super::network::AdapterUnregisterPending>());
        assert!(!store.last.entries[0].undone && store.last.entries[1..].iter().all(|entry|entry.undone));assert_eq!(network.undone.len(),2);
        journal=store.last.clone();network.present=false;journal.cleanup(&mut store,&mut network).unwrap();assert!(journal.complete());
        journal.entries.iter_mut().for_each(|entry|entry.undone=false);network.changed=true;
        assert_eq!(journal.cleanup(&mut store,&mut network).unwrap_err().to_string(),"owned_setting_changed");assert!(journal.entries.iter().all(|entry|!entry.undone));
    }
    #[test]
    fn watchdog_waits_past_ten_minutes_only_for_narrow_pending_and_keeps_errors_bounded() {
        use std::cell::Cell;
        let attempts=Cell::new(0);let elapsed=Cell::new(Duration::ZERO);
        reconcile_cleanup(|| {attempts.set(attempts.get()+1);if attempts.get()<=660 {Err(super::super::network::AdapterUnregisterPending.into())} else {Ok(())}},|delay| {assert_eq!(delay,Duration::from_secs(1));elapsed.set(elapsed.get()+delay);}).unwrap();
        assert_eq!(attempts.get(),661);assert_eq!(elapsed.get(),Duration::from_secs(660));
        for pending_first in [false,true] {
            attempts.set(0);let mut pauses=vec![];
            let error=reconcile_cleanup(|| {attempts.set(attempts.get()+1);if pending_first && attempts.get()==1 {Err(super::super::network::AdapterUnregisterPending.into())} else {anyhow::bail!("interface_identity_changed")}},|delay|pauses.push(delay)).unwrap_err();
            assert_eq!(error.to_string(),"interface_identity_changed");assert_eq!(attempts.get(),3+usize::from(pending_first));
            assert_eq!(pauses,if pending_first {vec![Duration::from_secs(1),Duration::from_millis(500),Duration::from_millis(500)]} else {vec![Duration::from_millis(500),Duration::from_millis(500)]});
        }
    }
    #[tokio::test]
    async fn final_reply_is_acknowledged_after_write_and_peer_loss_fails_without_deadlock() {
        let (writer,mut reader)=tokio::io::duplex(4096);let (sender,output)=mpsc::channel(4);let (events,_inputs)=mpsc::channel(4);
        let writer_task=tokio::spawn(output_loop(writer,output,events));
        let delivered=final_reply(&sender,json!({"ok":true}));
        let received=async {let mut data=[0;12];reader.read_exact(&mut data).await.unwrap();assert_eq!(&data,b"{\"ok\":true}\n");};
        let (result,_)=tokio::join!(delivered,received);result.unwrap();drop(reader);
        let result=tokio::time::timeout(Duration::from_secs(1),final_reply(&sender,json!({"ok":true}))).await.unwrap();assert!(result.is_err());
        drop(sender);writer_task.await.unwrap();
    }
}
