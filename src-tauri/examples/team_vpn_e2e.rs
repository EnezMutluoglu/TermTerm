//! Disposable native Team/WireGuard/SSH/SFTP acceptance; never packaged with the app.
#[cfg(all(any(target_os = "linux", target_os = "windows"), feature = "vpn-webkit-fixture"))]
mod fixture {
    use anyhow::{ensure, Context, Result};
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::Deserialize;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::{path::PathBuf, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::Duration};
    #[cfg(target_os = "linux")]
    use std::os::unix::fs::MetadataExt;
    use tauri::{Listener, Manager};
    use termterm_lib::{commands, files::Endpoint, state::{AppState, Shared}, team_v2, vpn};
    use termterm_team_client::{access::{AccessEffect, AccessPermission, AccessRule}, desktop::{DesktopAction, DesktopState}, keys::Identity, profiles::ProfileSelection, resources::ResourceContent, sharing::ShareRecipient, transport::{ServiceProfile, TeamClient}, vpn::Action};
    use termterm_vpn_core::Profile;
    use uuid::Uuid;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        profile: ServiceProfile, username: String, password: String,
        vpn: Profile, ssh_key: String, host_fingerprint: String, root: PathBuf,
        #[serde(default)]
        host_network_namespace: u64,
        #[serde(default)]
        remote_root: Option<String>,
        #[serde(default)]
        silent_loss: bool,
        #[serde(default)]
        shared_revocation: bool,
    }
    async fn until(mut check: impl FnMut() -> bool, what: &str) -> Result<()> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        while !check() {
            ensure!(tokio::time::Instant::now() < deadline, "Timed out: {what}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok(())
    }
    fn endpoint(connection: &str, path: impl Into<String>) -> Endpoint { Endpoint {connection:connection.into(),path:path.into()} }
    #[cfg(target_os = "windows")]
    const DISCONNECT_WAIT_TIMEOUT: &str = "VPN kapatma işlemi için bekleme süresi doldu; temizliğin tamamlandığı doğrulanamadı.";
    #[cfg(target_os = "windows")]
    fn pending_disconnect_proof(error: &str, current: Option<&Value>, expected: &Value) -> bool {
        error == DISCONNECT_WAIT_TIMEOUT && current.is_some_and(|current|
            current["id"] == expected["id"] && current["generation"] == expected["generation"]
                && (current["state"] == "disconnected" || (current["state"] == "disconnecting"
                    && current["error"] == "cleanup_pending: windows_interface_unregister_pending")))
    }
    async fn exercise(app: tauri::AppHandle, f: Fixture) -> Result<()> {
        let desktop = app.state::<DesktopState>();
        let password = format!("Disposable-{}!", Uuid::new_v4());
        let identity = Identity::generate();
        let mut owner = TeamClient::new(f.profile.clone())?;
        owner.login(&f.username,&f.password,Uuid::new_v4(),"Native Team VPN fixture").await?;
        owner.activate(&password,&identity).await?;
        owner.login(&f.username,&password,Uuid::new_v4(),"Native Team VPN owner").await?;
        owner.enroll_recovery(&password,&Identity::generate().public_key()).await?;
        #[cfg(target_os = "linux")]
        desktop.configure_cache(app.path().app_data_dir()?.join("team-v2-cache"))?;
        #[cfg(target_os = "windows")]
        desktop.configure_cache(f.root.join("client/data/team-v2-cache"))?;
        desktop.login(f.profile.clone(),f.username.clone(),password.clone()).await?;
        let vault = Uuid::new_v4(); let host = Uuid::new_v4(); let key = Uuid::new_v4();
        owner.create_vault(vault,"Disposable native VPN",&identity).await?;
        let folder = Uuid::new_v4();
        if f.shared_revocation {
            let content = ResourceContent {kind:"folder".into(),parent_id:None,name:"Shared VPN fixture".into(),address:None,protocol:None,port:None,username_policy:None,username:None,deleted:false,document:None,proxy:None};
            owner.save_resource(&owner.prepare_resource(vault,folder,0,1,"2026-10-30T00:00:00Z",&content).await?).await?;
        }
        let content = ResourceContent {kind:"host".into(),parent_id:f.shared_revocation.then_some(folder),name:"Tunnel-only SSH".into(),address:Some("10.97.0.1".into()),protocol:Some("ssh".into()),port:Some(41843),username_policy:Some("shared_required".into()),username:Some("termterm_test".into()),deleted:false,document:None,proxy:None};
        let change = owner.prepare_resource(vault,host,0,if f.shared_revocation {2} else {1},"2026-10-30T00:00:00Z",&content).await?;
        owner.save_resource(&change).await?;
        let snapshot = desktop.call(DesktopAction::Vpn{context:None,action:Action::List}).await?;
        let context = snapshot["context"].as_str().context("VPN context")?.to_owned();
        let original = f.vpn.clone();
        let saved = desktop.save_vpn(&context,None,0,f.vpn).await?;
        let mut profile_id = serde_json::from_value(saved["id"].clone())?;
        #[cfg(target_os = "windows")]
        let private_id=profile_id;
        #[cfg(target_os = "windows")]
        desktop.call(DesktopAction::Vpn{context:Some(context.clone()),action:Action::Authenticate{id:profile_id,authentication:termterm_team_client::vpn::Authentication{username:"disposable-owner".into(),password:"disposable-owner-secret".into(),remember:true}}}).await?;
        if f.shared_revocation {
            let member = owner.create_member("native-vpn-recipient","Native VPN recipient","operator").await?;
            let mut recipient = TeamClient::new(f.profile.clone())?;
            recipient.login("native-vpn-recipient",&member.password,Uuid::new_v4(),"Native recipient activation").await?;
            let recipient_key = Identity::generate();
            recipient.activate(&password,&recipient_key).await?;
            recipient.login("native-vpn-recipient",&password,Uuid::new_v4(),"Native recipient").await?;
            let grant = owner.prepare_access(member.account_id,folder,&[AccessRule{permission:AccessPermission::Read,effect:AccessEffect::Allow},AccessRule{permission:AccessPermission::Connect,effect:AccessEffect::Allow}]).await?;
            let mut cursor = 0;
            loop {let progress=owner.stage_access_keys(grant.id,cursor,&identity).await?;if let Some(next)=progress.next_after {cursor=next;}else{break;}}
            owner.finish_access(grant.id,false).await?;
            let plan = owner.prepare_share(profile_id,folder,&[ShareRecipient{account_id:member.account_id,view:Some(true),connect:true,edit:Some(false),reveal:false,export:false}]).await?;
            let shared = owner.finish_share(serde_json::from_value(plan["id"].clone())?,&identity).await?;
            let shared_id = serde_json::from_value(shared["credentialId"].clone())?;
            ensure!(shared_id != profile_id,"Shared VPN reused private identity");
            let mut changed = original.clone(); changed.label="Owner changed private VPN".into(); changed.config=changed.config.replace(":31820",":31821");
            owner.save_vpn_configuration(profile_id,1,&changed,&identity).await?;
            let shared = TeamClient::decode_vpn(&recipient.open_shared_credential(shared_id,"vpn",None,&recipient_key).await?)?;
            ensure!(shared.config==original.config && shared.label==original.label && shared.username.is_empty() && shared.password.is_empty(),"Shared VPN copy or personal authentication isolation failed");
            for purpose in ["reveal","export","edit"] {ensure!(recipient.open_shared_credential(shared_id,purpose,None,&recipient_key).await.is_err(),"Recipient gained {purpose} permission");}
            profile_id=shared_id;
        }
        desktop.call(DesktopAction::Vpn{context:Some(context),action:Action::Bind{resource_id:host,expected_revision:0,vpn_id:Some(profile_id),personal:!f.shared_revocation}}).await?;
        if f.shared_revocation {
            desktop.logout()?;desktop.login(f.profile,"native-vpn-recipient".into(),password).await?;
            #[cfg(target_os = "windows")]
            {
                let (_,recipient_context)=desktop.vpn_scope().await?;
                desktop.call(DesktopAction::Vpn{context:Some(recipient_context),action:Action::Authenticate{id:profile_id,authentication:termterm_team_client::vpn::Authentication{username:"disposable-recipient".into(),password:"disposable-recipient-secret".into(),remember:true}}}).await?;
                let (_,recipient_profile)=desktop.vpn_material(profile_id,desktop.current_epoch()).await?;
                ensure!(recipient_profile.username=="disposable-recipient" && recipient_profile.password=="disposable-recipient-secret","Recipient personal VPN authentication missing");
                let owner_auth=owner.vpn_authentication(private_id,&identity).await?;
                ensure!(owner_auth.username=="disposable-owner" && owner_auth.password=="disposable-owner-secret","Recipient authentication replaced owner's private authentication");
                let shared_owner_auth=owner.vpn_authentication(profile_id,&identity).await?;
                ensure!(shared_owner_auth.username.is_empty() && shared_owner_auth.password.is_empty(),"Recipient authentication leaked to shared VPN owner");
            }
        }
        desktop.call(DesktopAction::SaveCredential{id:key,expected_revision:0,credential_kind:"ssh_key".into(),label:"Public disposable test key".into(),secret:f.ssh_key,passphrase:None,certificate:None}).await?;
        desktop.call(DesktopAction::SaveProfile{selection:ProfileSelection{host_id:Some(host),expected_revision:0,method:"credential".into(),credential_id:Some(key),username:None}}).await?;
        let public_snapshot=desktop.call(DesktopAction::Vpn{context:None,action:Action::List}).await?;
        let wireguard_key=original.config.lines().find_map(|line|line.strip_prefix("PrivateKey = ")).context("Fixture WireGuard key")?;
        ensure!(!serde_json::to_string(&public_snapshot)?.contains(wireguard_key),"VPN key leaked in public snapshot");
        desktop.connection_target(host).await?;
        ensure!(desktop.required_vpn_for_plan(host,host,desktop.current_epoch()).await? == Some(profile_id),"Required VPN proof missing");
        let events = Arc::new(Mutex::new(Vec::<(String,Value)>::new()));
        for name in ["session-event","sftp-closed","vpn-event"] {
            let events = events.clone();
            app.listen(name,move |event| { if let Ok(value) = serde_json::from_str(event.payload()) {events.lock().unwrap().push((name.into(),value));} });
        }
        let trust_app = app.clone();
        app.listen("session-prompt",move |event| {
            let value:Value = serde_json::from_str(event.payload()).unwrap();
            let trusted = value["kind"] == "hostKey" && value["detail"]["fingerprint"] == f.host_fingerprint;
            commands::prompt_answer(trust_app.state(),value["id"].as_str().unwrap().into(),vec![if trusted {"trust"} else {"reject"}.into()]).unwrap();
        });
        let terminal = team_v2::terminal::team_v2_start(app.clone(),app.state(),host).map_err(anyhow::Error::msg)?;
        until(||events.lock().unwrap().iter().any(|(name,v)|name=="session-event" && v["id"]==terminal && (v["kind"]=="connected" || v["kind"]=="error")),"real Team SSH connection").await?;
        let connection_error=events.lock().unwrap().iter().find(|(name,v)|name=="session-event" && v["id"]==terminal && v["kind"]=="error").map(|(_,v)|v["detail"].clone());
        ensure!(connection_error.is_none(),"Real Team SSH failed: {}",connection_error.unwrap_or(Value::Null));
        commands::session_input(app.state(),terminal.clone(),Some("printf '\\124\\124\\137\\126\\120\\116\\137\\123\\123\\110\\137\\117\\113\\n'\n".into()),None,None,None).await.map_err(anyhow::Error::msg)?;
        until(||events.lock().unwrap().iter().filter(|(name,v)|name=="session-event" && v["id"]==terminal && v["kind"]=="data").filter_map(|(_,v)|STANDARD.decode(v["detail"].as_str()?).ok()).flatten().collect::<Vec<_>>().windows(b"TT_VPN_SSH_OK".len()).any(|v|v==b"TT_VPN_SSH_OK"),"SSH command output").await?;
        let sftp = Uuid::new_v4();
        team_v2::sftp::team_v2_sftp(app.clone(),app.state(),sftp,host).await.map_err(anyhow::Error::msg)?;
        let local = f.root.join("client");
        let remote = f.remote_root.unwrap_or_else(||f.root.join("remote").to_string_lossy().into_owned());
        let payload = local.join("payload"); let download = local.join("download"); let upload = format!("{remote}/uploaded");
        let source = std::fs::read(&payload)?;
        for (from,to) in [(endpoint("local",payload.to_string_lossy()),endpoint(&sftp.to_string(),upload.clone())),(endpoint(&sftp.to_string(),upload),endpoint("local",download.to_string_lossy()))] {
            ensure!(commands::file_transfer(app.clone(),app.state(),Uuid::new_v4().to_string(),from,to,false).await.map_err(anyhow::Error::msg)? == source.len() as u64,"Transfer size mismatch");
        }
        ensure!(Sha256::digest(&source)==Sha256::digest(std::fs::read(&download)?),"SFTP roundtrip SHA256 mismatch");
        let transfer_id = Uuid::new_v4().to_string(); let paused = Arc::new(AtomicBool::new(false));
        let transfer_app = app.clone(); let transfer_events = paused.clone(); let pause_id = transfer_id.clone();
        app.listen("transfer-progress",move |event| {
            let v:Value=serde_json::from_str(event.payload()).unwrap();
            if v["id"]==pause_id && v["transferred"].as_u64().unwrap_or(0)>0 && !transfer_events.swap(true,Ordering::SeqCst) {
                assert!(commands::transfer_control(transfer_app.clone(),transfer_app.state(),pause_id.clone(),"pause".into()).unwrap());
            }
        });
        let target = local.join("preserved"); std::fs::write(&target,b"preserve-existing-target")?;
        let task_app=app.clone(); let target_path=target.to_string_lossy().into_owned();
        let task=tokio::spawn(async move {commands::file_transfer(task_app.clone(),task_app.state(),transfer_id,endpoint(&sftp.to_string(),format!("{remote}/large")),endpoint("local",target_path),true).await});
        until(||paused.load(Ordering::SeqCst),"nonempty transfer paused before commit").await?;
        let sessions = vpn::vpn_sessions(app.clone(),app.state()).map_err(anyhow::Error::msg)?;
        let session = sessions.iter().find(|v|v["state"]=="connected").context("Actual native VPN session")?;
        #[cfg(target_os = "windows")]
        std::fs::write(local.join("native-live-ready"),b"required-vpn-ssh-sftp-hash-and-paused-transfer")?;
        let mut revocation_started=None;
        #[allow(unused_mut)]
        let mut disconnect_wait_pending=false;
        if f.shared_revocation {
            let update=owner.prepare_shared_update(profile_id,1,None,&[]).await?;
            owner.finish_shared_update(serde_json::from_value(update["id"].clone())?,&identity).await?;
            revocation_started=Some(tokio::time::Instant::now());
            tokio::time::timeout(Duration::from_secs(15),until(||events.lock().unwrap().iter().any(|(name,v)|name=="vpn-event" && v["id"]==session["id"] && v["error"].as_str().unwrap_or("").contains("Team VPN authorization changed")),"shared VPN revocation monitor")).await??;
        } else if f.silent_loss {
            std::fs::write(local.join("network-loss-ready"), b"paused-transfer-and-live-ssh-sftp")?;
            until(||events.lock().unwrap().iter().any(|(name,v)|name=="vpn-event" && v["id"]==session["id"] && v["state"]=="failed" && v["error"].as_str().unwrap_or("").contains("wireguard_peer_unresponsive")),"silent UDP loss failure").await?;
        } else {
            let result=vpn::vpn_disconnect(app.clone(),app.state(),session["id"].as_str().unwrap().into(),session["generation"].as_u64().unwrap()).await;
            #[cfg(target_os = "linux")]
            result.map_err(anyhow::Error::msg)?;
            #[cfg(target_os = "windows")]
            if let Err(error)=result {
                ensure!(error == DISCONNECT_WAIT_TIMEOUT,"Unexpected native disconnect error: {error}");
                until(||events.lock().unwrap().iter().any(|(name,v)|name=="vpn-event" && (pending_disconnect_proof(&error,Some(v),session)
                    || (v["id"]==session["id"] && v["generation"]==session["generation"] && v["state"]=="failed"))),"native pending-removal state after the short desktop wait").await?;
                let current=vpn::vpn_sessions(app.clone(),app.state()).map_err(anyhow::Error::msg)?;
                ensure!(pending_disconnect_proof(&error,current.iter().find(|v|v["id"]==session["id"]),session),"Native disconnect timeout lacked same-generation pending or terminal proof");
                disconnect_wait_pending=true;
            }
        }
        ensure!(tokio::time::timeout(Duration::from_secs(15),task).await??.is_err(),"Interrupted transfer committed");
        until(||events.lock().unwrap().iter().any(|(name,v)|name=="session-event" && v["id"]==terminal && v["kind"]=="closed"),"SSH cancellation").await?;
        until(||events.lock().unwrap().iter().any(|(name,v)|name=="sftp-closed" && v["id"]==sftp.to_string()),"SFTP cancellation").await?;
        #[cfg(target_os = "windows")]
        let revocation_seconds=revocation_started.map(|started|started.elapsed().as_secs_f64());
        #[cfg(target_os = "windows")]
        ensure!(revocation_seconds.is_none_or(|seconds|seconds<=15.0),"Revocation VPN/dependent shutdown exceeded 15 seconds");
        #[cfg(target_os = "linux")]
        if f.shared_revocation {
            until(||events.lock().unwrap().iter().any(|(name,v)|name=="vpn-event" && v["id"]==session["id"] && v["generation"]==session["generation"] && v["state"]=="disconnected"),"revoked VPN automatic teardown before logout").await?;
        }
        #[cfg(target_os = "windows")]
        tokio::time::timeout(Duration::from_secs(720),async {
                loop {
                    {
                        let events=events.lock().unwrap();
                        ensure!(!events.iter().any(|(name,v)|name=="vpn-event" && v["id"]==session["id"] && v["generation"]==session["generation"] && v["state"]=="failed"),"Windows native teardown failed; no recovery performed");
                        if events.iter().any(|(name,v)|name=="vpn-event" && v["id"]==session["id"] && v["generation"]==session["generation"] && v["state"]=="disconnected") {break;}
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Ok::<_,anyhow::Error>(())
            }).await.context("Windows native cleanup observation limit; no recovery performed")??;
        #[cfg(target_os = "linux")]
        let revocation_seconds=revocation_started.map(|started|started.elapsed().as_secs_f64());
        #[cfg(target_os = "linux")]
        ensure!(revocation_seconds.is_none_or(|seconds|seconds<=15.0),"Revocation VPN/dependent shutdown exceeded 15 seconds");
        if f.shared_revocation || cfg!(windows) {
            std::fs::write(local.join("revocation-cleanup-ready"),b"disconnected-before-logout")?;
            #[cfg(target_os = "linux")]
            let audit_seconds=15;
            #[cfg(target_os = "windows")]
            let audit_seconds=120;
            tokio::time::timeout(Duration::from_secs(audit_seconds),async {
                while !local.join("revocation-cleanup-verified").exists() {tokio::time::sleep(Duration::from_millis(50)).await;}
            }).await.context("external pre-logout journal/native verification")?;
        }
        ensure!(commands::file_list(app.state(),endpoint(&sftp.to_string(),"/")).await.is_err(),"Closed SFTP still readable");
        ensure!(commands::session_input(app.state(),terminal,None,None,None,None).await.is_err(),"Closed SSH accepted input");
        ensure!(std::fs::read(&target)?==b"preserve-existing-target","Interrupted download replaced target");
        ensure!(!std::fs::read_dir(&local)?.any(|e|e.unwrap().file_name().to_string_lossy().ends_with(".part")),"Partial local staging file survived");
        if f.shared_revocation {
            let retained=TeamClient::decode_vpn(&owner.open_shared_credential(profile_id,"vpn",None,&identity).await?)?;
            ensure!(retained.config==original.config,"Recipient ACL revocation removed owner's shared VPN access");
            ensure!(desktop.vpn_material(profile_id,desktop.current_epoch()).await.is_err(),"Revoked VPN credential remained readable");
            ensure!(desktop.required_vpn_for_plan(host,host,desktop.current_epoch()).await.is_err(),"Revoked requirement allowed direct fallback");
            ensure!(team_v2::sftp::team_v2_sftp(app.clone(),app.state(),Uuid::new_v4(),host).await.is_err(),"Revoked host reconnected");
        }
        ensure!(!serde_json::to_string(&*events.lock().unwrap())?.contains(wireguard_key),"VPN key leaked in public native events");
        team_v2::team_v2_logout(app.state(),app.state(),app.state()).await.map_err(anyhow::Error::msg)?;
        std::fs::write(local.join("native-result.json"),serde_json::to_vec_pretty(&json!({"result":"passed","realTeamRequiredVpn":true,"realSSH":true,"sftpSha256":hex::encode(Sha256::digest(source)),"vpnDisconnectClosedSSH":true,"vpnDisconnectClosedSFTP":true,"silentUdpLoss":f.silent_loss,"sharedRevocation":f.shared_revocation,"recipientAclRevocation":f.shared_revocation,"ownerSharedAccessRetained":f.shared_revocation,"revocationShutdownSeconds":revocation_seconds,"preLogoutCleanupVerified":f.shared_revocation || cfg!(windows),"disconnectWaitPending":disconnect_wait_pending,"confirmedNativeCleanup":true,"personalAuthenticationStorageIsolated":cfg!(windows) && f.shared_revocation,"publicVpnKeyLeaks":0,"sharedCopyIndependent":f.shared_revocation,"recipientConnectOnly":f.shared_revocation,"revokedMaterialAndReconnectDenied":f.shared_revocation,"pausedTransferCancelled":true,"existingTargetPreserved":true,"localPartialFiles":0}))?)?;
        Ok(())
    }
    pub fn main() -> Result<()> {
        let path = std::env::var_os("TERMTERM_TEAM_VPN_NATIVE_FIXTURE").context("Disposable fixture path required")?;
        let fixture:Fixture=serde_json::from_slice(&std::fs::read(path)?)?;
        #[cfg(target_os = "linux")]
        ensure!(std::fs::metadata("/proc/self/ns/net")?.ino() != fixture.host_network_namespace,"Private network namespace required");
        #[cfg(target_os = "windows")]
        {
            let executable=std::env::current_exe()?.canonicalize()?;
            let worktree=executable.ancestors().find(|path|path.file_name().is_some_and(|name|name==".tools")).and_then(|path|path.parent()).context("Windows fixture executable must be staged under this worktree .tools")?;
            ensure!(fixture.root.canonicalize()?.starts_with(worktree.join(".lab").canonicalize()?),"Windows fixture data must stay under this worktree .lab");
            ensure!(fixture.remote_root.as_ref().is_some_and(|path|path.starts_with("/run/termterm-team-win-") && !path.contains("..") && !path.contains('\\')),"Disposable remote fixture root required");
        }
        let mut context = tauri::generate_context!();
        context.config_mut().identifier="local.termterm.desktop.vpndev".into();
        context.config_mut().app.windows.clear();
        let outcome = Arc::new(Mutex::new(None)); let result = outcome.clone();
        let app=tauri::Builder::default().manage(Arc::new(AppState::default()) as Shared).manage(DesktopState::default()).manage(team_v2::NativeConnections::default()).setup(move |app| {
            let handle=app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let value=tokio::time::timeout(Duration::from_secs(if cfg!(windows) {1050} else {180}),exercise(handle.clone(),fixture)).await.context("Native fixture deadline").and_then(|r|r);
                let code=if value.is_ok(){0}else{1}; *result.lock().unwrap()=Some(value);handle.exit(code);
            });Ok(())
        }).build(context)?;
        let pending=outcome.clone();
        app.run_return(move |_,event| {
            if let tauri::RunEvent::ExitRequested {api,..}=event {
                if pending.lock().unwrap().is_none() {api.prevent_exit();}
            }
        });
        let result=outcome.lock().unwrap().take().context("Native fixture ended without result")?;
        result
    }
    #[cfg(all(test, target_os = "windows"))]
    mod tests {
        use super::*;
        #[test]
        fn disconnect_timeout_needs_same_generation_pending_or_terminal_proof() {
            assert!(include_str!("../src/vpn.rs").contains(DISCONNECT_WAIT_TIMEOUT));
            let expected=json!({"id":"fixture","generation":2});
            let pending=json!({"id":"fixture","generation":2,"state":"disconnecting","error":"cleanup_pending: windows_interface_unregister_pending"});
            assert!(pending_disconnect_proof(DISCONNECT_WAIT_TIMEOUT,Some(&pending),&expected));
            assert!(!pending_disconnect_proof("VPN lock",Some(&pending),&expected));
            assert!(!pending_disconnect_proof(DISCONNECT_WAIT_TIMEOUT,None,&expected));
            for (field,value) in [("id",json!("other")),("generation",json!(1)),("state",json!("failed")),("error",json!("cleanup_required: fixture"))] {
                let mut changed=pending.clone();changed[field]=value;
                assert!(!pending_disconnect_proof(DISCONNECT_WAIT_TIMEOUT,Some(&changed),&expected));
            }
            let mut terminal=pending;terminal["state"]=json!("disconnected");terminal["error"]=Value::Null;
            assert!(pending_disconnect_proof(DISCONNECT_WAIT_TIMEOUT,Some(&terminal),&expected));
        }
    }
}
fn main() {
    #[cfg(all(any(target_os = "linux", target_os = "windows"), feature = "vpn-webkit-fixture"))]
    if let Err(error) = fixture::main() {eprintln!("Native Team VPN fixture failed: {error:#}");std::process::exit(1);}
    #[cfg(not(all(any(target_os = "linux", target_os = "windows"), feature = "vpn-webkit-fixture")))]
    eprintln!("Disposable Linux/Windows fixture requires --features vpn-webkit-fixture");
}
