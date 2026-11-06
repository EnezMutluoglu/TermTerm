//! Real encryption/serialization/HTTPS/cache test, using only a fresh private service fixture.
use serde::Deserialize;
use serde_json::Value;
use termterm_team_client::{access::{AccessEffect,AccessPermission,AccessRule},desktop::{DesktopAction,DesktopState},keys::Identity,profiles::ProfileSelection,resources::ResourceContent,sharing::ShareRecipient,transport::{ServiceProfile,TeamClient},vpn::{Action,Authentication}};
use termterm_vpn_core::{Family,Profile};
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct Fixture{profile:ServiceProfile,username:String,password:String,control_url:String}
async fn control(f:&Fixture,path:&str)->Value {
    reqwest::Client::new().post(format!("{}{path}",f.control_url)).send().await.unwrap().json().await.unwrap()
}
async fn list(state:&DesktopState)->Value {state.call(DesktopAction::Vpn{context:None,action:Action::List}).await.unwrap()}
fn profile()->Profile {Profile{version:1,label:"Private fixture".into(),family:Family::OpenVpn,source:"fixture.ovpn".into(),server:String::new(),group_id:Some("Lab".into()),username:"owner-login".into(),password:"Disposable-Private-VPN-Password".into(),realm:String::new(),config:"client\ndev tun\nproto udp\nremote vpn-fixture.example 1194\nauth-user-pass\n".into(),remember:true}}
#[tokio::test]
#[ignore="requires tests/team_vpn_http.py disposable PostgreSQL/HTTPS fixture"]
async fn encrypted_team_vpn_lifecycle_and_offline_replay() {
    let f:Fixture=serde_json::from_slice(&std::fs::read(std::env::var("TERMTERM_TEAM_VPN_FIXTURE").unwrap()).unwrap()).unwrap();
    let password="Disposable-Team-VPN-Account-Password-123!";
    let mut owner=TeamClient::new(f.profile.clone()).unwrap();owner.login(&f.username,&f.password,Uuid::new_v4(),"VPN crypto fixture").await.unwrap();
    let owner_key=Identity::generate();owner.activate(password,&owner_key).await.unwrap();owner.login(&f.username,password,Uuid::new_v4(),"VPN owner fixture").await.unwrap();
    owner.enroll_recovery(password,&Identity::generate().public_key()).await.unwrap();
    let root=tempfile::tempdir().unwrap();let state=DesktopState::default();state.configure_cache(root.path().to_path_buf()).unwrap();state.login(f.profile.clone(),f.username.clone(),password.into()).await.unwrap();
    let snapshot=list(&state).await;let context=snapshot["context"].as_str().unwrap().to_owned();
    let saved=state.save_vpn(&context,None,0,profile()).await.unwrap();let id:Uuid=serde_json::from_value(saved["id"].clone()).unwrap();assert!(saved["pending"]==false);
    let (_,material)=state.vpn_material(id,state.current_epoch()).await.unwrap();assert!(material.password=="Disposable-Private-VPN-Password" && material.username=="owner-login");
    let private=owner.open_private_credential(id,&owner_key).await.unwrap();assert!(private.kind=="vpn");
    let metadata:Value=serde_json::from_slice(&private.data).unwrap();assert!(metadata.get("password").is_none() && metadata.get("username").is_none());assert!(std::str::from_utf8(&private.secret).unwrap()==profile().config);
    let member=owner.create_member("vpn-member","VPN member","operator").await.unwrap();
    let mut user=TeamClient::new(f.profile.clone()).unwrap();user.login("vpn-member",&member.password,Uuid::new_v4(),"VPN recipient").await.unwrap();
    let user_key=Identity::generate();user.activate(password,&user_key).await.unwrap();user.login("vpn-member",password,Uuid::new_v4(),"VPN recipient").await.unwrap();
    let vault=Uuid::new_v4();let folder=Uuid::new_v4();let host=Uuid::new_v4();owner.create_vault(vault,"VPN fixture vault",&owner_key).await.unwrap();
    let folder_data=ResourceContent{kind:"folder".into(),parent_id:None,name:"VPN fixture folder".into(),address:None,protocol:None,port:None,username_policy:None,username:None,deleted:false,document:None,proxy:None};
    let change=owner.prepare_resource(vault,folder,0,1,"2026-10-29T00:00:00Z",&folder_data).await.unwrap();owner.save_resource(&change).await.unwrap();
    let host_data=ResourceContent{kind:"host".into(),parent_id:Some(folder),name:"VPN fixture target".into(),address:Some("localhost".into()),protocol:Some("ssh".into()),port:Some(22),username_policy:Some("personal".into()),username:None,deleted:false,document:None,proxy:None};
    let change=owner.prepare_resource(vault,host,0,2,"2026-10-29T00:00:00Z",&host_data).await.unwrap();owner.save_resource(&change).await.unwrap();
    let grant=owner.prepare_access(member.account_id,folder,&[AccessRule{permission:AccessPermission::Read,effect:AccessEffect::Allow},AccessRule{permission:AccessPermission::Connect,effect:AccessEffect::Allow}]).await.unwrap();
    let mut cursor=0;loop {let progress=owner.stage_access_keys(grant.id,cursor,&owner_key).await.unwrap();if let Some(next)=progress.next_after {cursor=next;}else{break;}}owner.finish_access(grant.id,false).await.unwrap();
    let plan=owner.prepare_share(id,folder,&[ShareRecipient{account_id:member.account_id,view:Some(true),connect:true,edit:Some(false),reveal:false,export:false}]).await.unwrap();
    let shared=owner.finish_share(serde_json::from_value(plan["id"].clone()).unwrap(),&owner_key).await.unwrap();let shared_id:Uuid=serde_json::from_value(shared["credentialId"].clone()).unwrap();
    let shared_secret=user.open_shared_credential(shared_id,"vpn",None,&user_key).await.unwrap();let shared_profile=TeamClient::decode_vpn(&shared_secret).unwrap();assert!(shared_profile.username.is_empty() && shared_profile.password.is_empty());
    assert!(user.open_shared_credential(shared_id,"reveal",None,&user_key).await.is_err());assert!(user.open_shared_credential(shared_id,"export",None,&user_key).await.is_err());assert!(user.open_shared_credential(shared_id,"edit",None,&user_key).await.is_err());
    // A successful apply followed by a lost reply must settle by receipt, never re-encrypt/rewrite.
    let before=control(&f,"/stats").await["applyRequests"].as_u64().unwrap();control(&f,"/drop-reply").await;
    let mut edited=profile();edited.label="Private edited".into();edited.username.clear();edited.password.clear();edited.remember=false;
    let saved=state.save_vpn(&context,Some(id),1,edited).await.unwrap();assert!(saved["pending"]==true);
    let snapshot=list(&state).await;assert!(snapshot["journal"]["pending"]==0);
    assert!(control(&f,"/stats").await["applyRequests"].as_u64().unwrap()==before+1);
    let (_,material)=state.vpn_material(id,state.current_epoch()).await.unwrap();assert!(material.username.is_empty() && material.password.is_empty() && !material.remember);
    let independent=TeamClient::decode_vpn(&user.open_shared_credential(shared_id,"vpn",None,&user_key).await.unwrap()).unwrap();assert!(independent.label=="Private fixture");
    owner.save_profile(&ProfileSelection{host_id:None,expected_revision:0,method:"prompt".into(),credential_id:None,username:Some("fixture-user".into())},&owner_key).await.unwrap();
    state.call(DesktopAction::Vpn{context:Some(context.clone()),action:Action::Bind{resource_id:folder,expected_revision:0,vpn_id:Some(shared_id),personal:false}}).await.unwrap();
    state.connection_target(host).await.unwrap();assert!(state.required_vpn_for_plan(host,host,state.current_epoch()).await.unwrap()==Some(shared_id));state.vpn_material(shared_id,state.current_epoch()).await.unwrap();
    control(&f,"/offline").await;
    assert!(state.required_vpn_for_plan(host,host,state.current_epoch()).await.unwrap()==Some(shared_id));
    let mut offline=profile();offline.label="Offline private edit".into();offline.username="offline-user".into();offline.password="Disposable-Offline-Password".into();
    let saved=state.save_vpn(&context,Some(id),2,offline).await.unwrap();assert!(saved["pending"]==true);
    let mut shared_edit=profile();shared_edit.label="Offline shared candidate".into();shared_edit.username.clear();shared_edit.password.clear();shared_edit.remember=false;
    let saved=state.save_vpn(&context,Some(shared_id),1,shared_edit).await.unwrap();assert!(saved["pending"]==true);
    state.logout().unwrap();let reopened=DesktopState::default();reopened.configure_cache(root.path().to_path_buf()).unwrap();
    let login=reopened.login(f.profile.clone(),f.username.clone(),password.into()).await.unwrap();assert!(login["offline"]==true);
    let (_,cached)=reopened.vpn_material(id,reopened.current_epoch()).await.unwrap();assert!(cached.label=="Offline private edit" && cached.password=="Disposable-Offline-Password");
    assert!(reopened.required_vpn_for_plan(host,host,reopened.current_epoch()).await.unwrap()==Some(shared_id));assert!(reopened.required_vpn_for_plan(host,Uuid::new_v4(),reopened.current_epoch()).await.is_err());
    control(&f,"/online").await;
    let mut remote=profile();remote.label="Remote shared revision".into();owner.save_vpn_configuration(shared_id,1,&remote,&owner_key).await.unwrap();
    reopened.call(DesktopAction::SyncJournal{}).await.unwrap();reopened.call(DesktopAction::SyncJournal{}).await.unwrap();
    let snapshot=list(&reopened).await;assert!(snapshot["journal"]["pending"]==0 && snapshot["journal"]["conflicts"]==1);
    let restored=TeamClient::decode_vpn(&owner.open_private_credential(id,&owner_key).await.unwrap()).unwrap();assert!(restored.label=="Offline private edit");
    let recipient_root=tempfile::tempdir().unwrap();let recipient=DesktopState::default();recipient.configure_cache(recipient_root.path().to_path_buf()).unwrap();recipient.login(f.profile.clone(),"vpn-member".into(),password.into()).await.unwrap();list(&recipient).await;
    let (_,recipient_material)=recipient.vpn_material(shared_id,recipient.current_epoch()).await.unwrap();assert!(recipient_material.username.is_empty() && recipient_material.password.is_empty());
    owner.remove_shared(shared_id,2).await.unwrap();assert!(recipient.check_vpn(shared_id,recipient.current_epoch()).await.is_err());
    control(&f,"/offline").await;assert!(recipient.vpn_material(shared_id,recipient.current_epoch()).await.is_err());control(&f,"/online").await;
    // Authentication changes are a separate typed action, without shared configuration edit rights.
    let snapshot=list(&reopened).await;let context=snapshot["context"].as_str().unwrap().to_owned();reopened.call(DesktopAction::Vpn{context:Some(context),action:Action::Authenticate{id,authentication:Authentication{username:String::new(),password:String::new(),remember:false}}}).await.unwrap();
    assert!(control(&f,"/stats").await["applyRequests"].as_u64().unwrap()>=4);
}
