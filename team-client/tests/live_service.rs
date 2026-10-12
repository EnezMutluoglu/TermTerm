//! Requires the disposable TLS service fixture started by tests/team_v2_http.py.
use serde::Deserialize;
use termterm_team_client::access::{AccessEffect, AccessPermission, AccessRule};
use termterm_team_client::resources::ResourceContent;
use termterm_team_client::{
    keys::Identity,
    transport::{ServiceProfile, TeamClient},
};
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    profile: ServiceProfile,
    username: String,
    password: String,
}
#[tokio::test]
#[ignore = "requires disposable real TLS service and PostgreSQL"]
async fn real_encrypted_credential_and_two_user_isolation() {
    let path = std::env::var("TERMTERM_TEAM_V2_FIXTURE").expect("fixture path required");
    let fixture: Fixture = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut untrusted_profile = fixture.profile.clone();
    untrusted_profile.ca_pem = None;
    let mut untrusted = TeamClient::new(untrusted_profile).unwrap();
    assert!(
        untrusted
            .login("unused", "unused", Uuid::new_v4(), "Untrusted TLS")
            .await
            .err()
            .unwrap()
            .to_string()
            == "secure_transport_failed"
    );
    let mut leader = TeamClient::new(fixture.profile.clone()).unwrap();
    let session = leader
        .login(
            &fixture.username,
            &fixture.password,
            Uuid::new_v4(),
            "Rust TLS test",
        )
        .await
        .unwrap();
    assert!(session.activation_required);
    let leader_key = Identity::generate();
    let password = "Test-Only-Real-Crypto-123!";
    leader.activate(password, &leader_key).await.unwrap();
    leader
        .login(&fixture.username, password, Uuid::new_v4(), "Rust TLS test")
        .await
        .unwrap();
    assert_eq!(
        leader.unlock_identity(password).unwrap().public_key(),
        leader_key.public_key()
    );
    let recovery = Identity::generate();
    leader.renew().await.unwrap();
    assert_eq!(
        leader.unlock_identity(password).unwrap().public_key(),
        leader_key.public_key()
    );
    leader
        .enroll_recovery(password, &recovery.public_key())
        .await
        .unwrap();
    let recovery_file = recovery
        .recovery_file(fixture.profile.team_id, "Recovery-Only-Test-123")
        .unwrap();
    assert_eq!(
        recovery_file
            .unlock(fixture.profile.team_id, "Recovery-Only-Test-123")
            .unwrap()
            .public_key(),
        recovery.public_key()
    );
    let member = leader
        .create_member("member", "Türkçe üye", "operator")
        .await
        .unwrap();
    let vault = Uuid::new_v4();
    let folder = Uuid::new_v4();
    let host = Uuid::new_v4();
    leader
        .create_vault(vault, "Ortak üretim kasası", &leader_key)
        .await
        .unwrap();
    let folder_data = ResourceContent {
        document: None, proxy:None,
            kind: "folder".into(),
        parent_id: None,
        name: "İstanbul".into(),
        address: None,
        protocol: None,
        port: None,
        username_policy: None,
        username: None,
        deleted: false,
    };
    let first = leader
        .prepare_resource(vault, folder, 0, 1, "2026-10-06T12:00:00Z", &folder_data)
        .await
        .unwrap();
    assert_eq!(
        leader.save_resource(&first).await.unwrap().status,
        "applied"
    );
    assert_eq!(
        leader.save_resource(&first).await.unwrap().revision,
        Some(1)
    );
    assert_eq!(
        leader
            .open_resource(folder, &leader_key)
            .await
            .unwrap()
            .content,
        folder_data
    );
    let mut host_data = ResourceContent {
        document: None, proxy:None,
            kind: "host".into(),
        parent_id: Some(folder),
        name: "Laboratuvar SSH".into(),
        address: Some("127.0.0.1".into()),
        protocol: Some("ssh".into()),
        port: Some(22),
        username_policy: Some("shared_required".into()),
        username: Some("company_admin".into()),
        deleted: false,
    };
    let second = leader
        .prepare_resource(vault, host, 0, 2, "2026-10-06T12:00:01Z", &host_data)
        .await
        .unwrap();
    assert_eq!(
        leader.save_resource(&second).await.unwrap().status,
        "applied"
    );
    assert_eq!(
        leader
            .open_resource(host, &leader_key)
            .await
            .unwrap()
            .content,
        host_data
    );
    assert_eq!(
        leader.resource_page(vault, None).await.unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let stale = leader
        .prepare_resource(vault, host, 1, 3, "2026-10-06T12:00:02Z", &host_data)
        .await
        .unwrap();
    host_data.port = Some(2222);
    let update = leader
        .prepare_resource(vault, host, 1, 4, "2026-10-06T12:00:03Z", &host_data)
        .await
        .unwrap();
    assert_eq!(
        leader.save_resource(&update).await.unwrap().revision,
        Some(2)
    );
    assert_eq!(
        leader.save_resource(&stale).await.unwrap().status,
        "conflict"
    );
    assert_eq!(
        leader
            .open_resource(host, &leader_key)
            .await
            .unwrap()
            .content
            .port,
        Some(2222)
    );
    let pending = leader.pending_conflicts(&leader_key, None).await.unwrap();
    assert_eq!(pending.conflicts.len(), 1);
    assert_eq!(
        pending.conflicts[0].content.as_ref().unwrap().port,
        Some(22)
    );
    assert!(pending.conflicts[0].error.is_none());
    assert!(leader
        .decide_conflict(pending.conflicts[0].id, 1, None)
        .await
        .is_err());
    assert_eq!(
        leader
            .decide_conflict(pending.conflicts[0].id, 2, None)
            .await
            .unwrap()["status"],
        "resolved"
    );
    assert!(leader
        .pending_conflicts(&leader_key, None)
        .await
        .unwrap()
        .conflicts
        .is_empty());
    let mut user = TeamClient::new(fixture.profile.clone()).unwrap();
    user.login("member", &member.password, Uuid::new_v4(), "Second device")
        .await
        .unwrap();
    let user_key = Identity::generate();
    user.activate(password, &user_key).await.unwrap();
    user.login("member", password, Uuid::new_v4(), "Second device")
        .await
        .unwrap();
    assert!(user.vaults().await.unwrap().as_array().unwrap().is_empty());
    assert!(user.open_resource(host, &user_key).await.is_err());
    assert!(user.resource_page(vault, None).await.unwrap()["records"]
        .as_array()
        .unwrap()
        .is_empty());
    let grant = leader
        .prepare_access(
            member.account_id,
            folder,
            &[
                AccessRule {
                    permission: AccessPermission::Read,
                    effect: AccessEffect::Allow,
                },
                AccessRule {
                    permission: AccessPermission::Connect,
                    effect: AccessEffect::Allow,
                },
            ],
        )
        .await
        .unwrap();
    assert_eq!(grant.affected_resources, 2);
    let impact = leader.access_impact(grant.id, None).await.unwrap();
    assert_eq!(impact.records.len(), 2);
    assert!(impact
        .records
        .iter()
        .all(|row| row.before.is_empty() && row.after.len() == 2));
    assert_eq!(grant.required_keys, 4); // folder + host revisions 1/2 + vault label
    assert!(user
        .prepare_access(
            member.account_id,
            folder,
            &[AccessRule {
                permission: AccessPermission::Edit,
                effect: AccessEffect::Allow
            },]
        )
        .await
        .is_err());
    assert!(leader.finish_access(grant.id, false).await.is_err());
    assert!(leader
        .stage_access_keys(grant.id, 0, &user_key)
        .await
        .is_err());
    let progress = leader
        .stage_access_keys(grant.id, 0, &leader_key)
        .await
        .unwrap();
    assert_eq!(progress.staged, 4);
    assert_eq!(
        leader
            .stage_access_keys(grant.id, progress.next_after.unwrap(), &leader_key)
            .await
            .unwrap()
            .staged,
        0
    );
    assert!(user.open_resource(host, &user_key).await.is_err());
    leader.finish_access(grant.id, false).await.unwrap();
    assert_eq!(
        user.open_resource(host, &user_key).await.unwrap().content,
        host_data
    );
    assert_eq!(
        user.open_resource(folder, &user_key).await.unwrap().content,
        folder_data
    );
    assert!(user.vaults().await.unwrap()[0]["envelope"].is_string());
    let revoke = leader
        .prepare_access(
            member.account_id,
            folder,
            &[
                AccessRule {
                    permission: AccessPermission::Read,
                    effect: AccessEffect::Deny,
                },
                AccessRule {
                    permission: AccessPermission::Connect,
                    effect: AccessEffect::Deny,
                },
            ],
        )
        .await
        .unwrap();
    assert_eq!(revoke.required_keys, 0);
    leader.finish_access(revoke.id, false).await.unwrap();
    assert!(user.open_resource(host, &user_key).await.is_err());
    assert!(user.vaults().await.unwrap().as_array().unwrap().is_empty());
    let id = Uuid::new_v4();
    // A delegated manager rewraps real keys from its own identity, not the owner.
    let managed = leader
        .create_member("manager", "Scope manager", "manager")
        .await
        .unwrap();
    let mut manager = TeamClient::new(fixture.profile.clone()).unwrap();
    manager
        .login("manager", &managed.password, Uuid::new_v4(), "Manager PC")
        .await
        .unwrap();
    let manager_key = Identity::generate();
    manager.activate(password, &manager_key).await.unwrap();
    manager
        .login("manager", password, Uuid::new_v4(), "Manager PC")
        .await
        .unwrap();
    let rules = || {
        vec![
            AccessRule {
                permission: AccessPermission::Read,
                effect: AccessEffect::Allow,
            },
            AccessRule {
                permission: AccessPermission::Connect,
                effect: AccessEffect::Allow,
            },
        ]
    };
    let p = leader
        .prepare_access(managed.account_id, folder, &rules())
        .await
        .unwrap();
    leader
        .stage_access_keys(p.id, 0, &leader_key)
        .await
        .unwrap();
    leader.finish_access(p.id, false).await.unwrap();
    let rev = leader.delegations().await.unwrap()["aclRevision"]
        .as_i64()
        .unwrap();
    leader
        .set_delegation(
            managed.account_id,
            folder,
            &[AccessPermission::Read, AccessPermission::Connect],
            rev,
        )
        .await
        .unwrap();
    let p = manager
        .prepare_access(member.account_id, folder, &rules())
        .await
        .unwrap();
    manager
        .stage_access_keys(p.id, 0, &manager_key)
        .await
        .unwrap();
    manager.finish_access(p.id, false).await.unwrap();
    assert_eq!(
        user.open_resource(host, &user_key).await.unwrap().content,
        host_data
    );
    let rev = leader.delegations().await.unwrap()["aclRevision"]
        .as_i64()
        .unwrap();
    leader
        .set_delegation(managed.account_id, folder, &[], rev)
        .await
        .unwrap();
    assert!(manager
        .prepare_access(member.account_id, folder, &rules())
        .await
        .is_err());
    permanent_route_restore(&leader,&leader_key).await;
    delegated_chain_crypto(&leader, &leader_key, &manager, &manager_key, managed.account_id, &user, &user_key, member.account_id).await;
    documents_crypto(&leader,&leader_key,&user,&user_key,member.account_id).await;
    manager.logout().await.unwrap();
    assert_eq!(
        user.save_private_credential(
            id,
            0,
            "password",
            b"Private account label",
            b"private-test-password",
            &user_key
        )
        .await
        .unwrap(),
        1
    );
    assert!(leader
        .private_credentials()
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
    assert!(leader
        .open_private_credential(id, &leader_key)
        .await
        .is_err());
    assert!(leader.set_default_credential(id, 0).await.is_err());
    assert!(user
        .save_private_credential(id, 0, "password", b"overwrite", b"not allowed", &user_key)
        .await
        .is_err());
    user.set_default_credential(id, 0).await.unwrap();
    assert_eq!(
        user.profiles().await.unwrap()["default"]["credentialId"],
        id.to_string()
    );
    let before = user.open_private_credential(id, &user_key).await.unwrap();
    assert_eq!(&**before.secret, b"private-test-password");
    // Independent, context-bound encrypted shared copy; connect does not imply reveal/export.
    let source = Uuid::new_v4();
    leader
        .save_private_credential(
            source,
            0,
            "password",
            b"Shared source",
            b"copy-original",
            &leader_key,
        )
        .await
        .unwrap();
    let plan = leader
        .prepare_share(
            source,
            folder,
            &[termterm_team_client::sharing::ShareRecipient {
                account_id: member.account_id,
                connect: true,
                reveal: false,
                export: false,
            }],
        )
        .await
        .unwrap();
    let plan_id = Uuid::parse_str(plan["id"].as_str().unwrap()).unwrap();
    let receipt = leader.finish_share(plan_id, &leader_key).await.unwrap();
    assert_eq!(
        leader.finish_share(plan_id, &leader_key).await.unwrap(),
        receipt
    );
    let shared = Uuid::parse_str(receipt["credentialId"].as_str().unwrap()).unwrap();
    assert_eq!(
        &**user
            .open_connection_credential(shared, host, &user_key)
            .await
            .unwrap()
            .secret,
        b"copy-original"
    );
    assert!(user
        .open_shared_credential(shared, "reveal", None, &user_key)
        .await
        .is_err());
    assert!(user
        .open_shared_credential(shared, "export", None, &user_key)
        .await
        .is_err());
    assert!(user
        .open_private_credential(source, &user_key)
        .await
        .is_err());
    assert_eq!(
        &**leader
            .open_shared_credential(shared, "export", None, &leader_key)
            .await
            .unwrap()
            .secret,
        b"copy-original"
    );
    leader
        .save_private_credential(
            source,
            1,
            "password",
            b"Changed source",
            b"copy-updated",
            &leader_key,
        )
        .await
        .unwrap();
    assert_eq!(
        &**user
            .open_connection_credential(shared, host, &user_key)
            .await
            .unwrap()
            .secret,
        b"copy-original"
    );
    assert!(user.remove_shared(shared, 1).await.is_err());
    let recipients = [termterm_team_client::sharing::ShareRecipient {
        account_id: member.account_id,
        connect: true,
        reveal: false,
        export: false,
    }];
    let update = leader
        .prepare_shared_update(shared, 1, Some(source), &recipients)
        .await
        .unwrap();
    let update_id = Uuid::parse_str(update["id"].as_str().unwrap()).unwrap();
    assert!(user.shared_update_options(shared).await.is_err());
    assert_eq!(
        leader
            .finish_shared_update(update_id, &leader_key)
            .await
            .unwrap()["revision"],
        2
    );
    assert_eq!(
        leader
            .finish_shared_update(update_id, &leader_key)
            .await
            .unwrap()["revision"],
        2
    );
    assert_eq!(
        &**user
            .open_connection_credential(shared, host, &user_key)
            .await
            .unwrap()
            .secret,
        b"copy-updated"
    );
    let update = leader
        .prepare_shared_update(shared, 2, None, &[])
        .await
        .unwrap();
    leader
        .finish_shared_update(
            Uuid::parse_str(update["id"].as_str().unwrap()).unwrap(),
            &leader_key,
        )
        .await
        .unwrap();
    assert!(user
        .open_connection_credential(shared, host, &user_key)
        .await
        .is_err());
    assert_eq!(
        &**leader
            .open_shared_credential(shared, "reveal", None, &leader_key)
            .await
            .unwrap()
            .secret,
        b"copy-updated"
    );
    assert!(termterm_team_client::shared_recovery::SharedRecovery::begin(&user,shared,&recovery,password,"Unauthorized shared recovery").await.is_err());
    assert!(termterm_team_client::shared_recovery::SharedRecovery::begin(&leader,shared,&Identity::generate(),password,"Wrong recovery file").await.is_err());
    let shared_recovery=termterm_team_client::shared_recovery::SharedRecovery::begin(&leader,shared,&recovery,password,"Recover only for current recipients").await.unwrap();
    assert!(shared_recovery.preview()["recipients"].as_array().unwrap().iter().all(|r|r["accountId"]!=member.account_id.to_string()));
    assert_eq!(shared_recovery.finish(&leader,&leader_key).await.unwrap()["revision"],4);
    assert_eq!(shared_recovery.finish(&leader,&leader_key).await.unwrap()["revision"],4);
    assert_eq!(&**leader.open_shared_credential(shared,"reveal",None,&leader_key).await.unwrap().secret,b"copy-updated");
    assert!(user.open_connection_credential(shared,host,&user_key).await.is_err());
    leader.remove_shared(shared, 4).await.unwrap();
    assert!(user
        .open_connection_credential(shared, host, &user_key)
        .await
        .is_err());
    assert_eq!(
        &**leader
            .open_private_credential(source, &leader_key)
            .await
            .unwrap()
            .secret,
        b"copy-updated"
    );
    user.logout().await.unwrap();
    let mut other_device = TeamClient::new(fixture.profile.clone()).unwrap();
    other_device
        .login("member", password, Uuid::new_v4(), "New PC")
        .await
        .unwrap();
    let restored_key = other_device.unlock_identity(password).unwrap();
    let saved = other_device
        .open_private_credential(id, &restored_key)
        .await
        .unwrap();
    assert_eq!(&**saved.data, b"Private account label");
    assert_eq!(&**saved.secret, b"private-test-password");
    let old_profile = other_device
        .open_profiles(&restored_key)
        .await
        .unwrap()
        .default
        .unwrap();
    other_device
        .save_profile(
            &termterm_team_client::profiles::ProfileSelection {
                host_id: None,
                expected_revision: old_profile.revision,
                method: "credential".into(),
                credential_id: Some(id),
                username: Some("old-personal-user".into()),
            },
            &restored_key,
        )
        .await
        .unwrap();
    let reset = leader.reset_member(member.account_id).await.unwrap();
    assert!(other_device.overview().await.is_err());
    other_device
        .login("member", &reset.password, Uuid::new_v4(), "Reset device")
        .await
        .unwrap();
    let new_key = Identity::generate();
    other_device.activate(password, &new_key).await.unwrap();
    other_device
        .login("member", password, Uuid::new_v4(), "Reset device")
        .await
        .unwrap();
    assert!(other_device
        .open_private_credential(id, &new_key)
        .await
        .is_err());
    assert_eq!(
        other_device
            .private_credentials()
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(leader
        .begin_recovery_job(
            member.account_id,
            password,
            "Disposable recovery test",
            Identity::generate()
        )
        .await
        .is_err());
    let recovery_key = recovery_file
        .unlock(fixture.profile.team_id, "Recovery-Only-Test-123")
        .unwrap();
    let mut recovery_job = leader
        .begin_recovery_job(
            member.account_id,
            password,
            "Reset recovery integration test",
            recovery_key,
        )
        .await
        .unwrap();
    let operation: Uuid =
        serde_json::from_value(recovery_job.preview()["operationId"].clone()).unwrap();
    assert!(leader
        .verify_recovery_job(operation, &leader_key)
        .await
        .is_err());
    let mut verify = other_device
        .verify_recovery_job(operation, &new_key)
        .await
        .unwrap();
    assert!(verify.step(&other_device, &new_key).await.is_err());
    loop {
        if recovery_job.step(&leader, &leader_key).await.unwrap()["completed"] == true {
            break;
        }
    }
    assert!(leader
        .open_private_credential(id, &leader_key)
        .await
        .is_err());
    loop {
        if verify.step(&other_device, &new_key).await.unwrap()["completed"] == true {
            break;
        }
    }
    assert_eq!(
        verify.step(&other_device, &new_key).await.unwrap()["completed"],
        true
    );
    assert_eq!(
        &**other_device
            .open_private_credential(id, &new_key)
            .await
            .unwrap()
            .secret,
        b"private-test-password"
    );
    let broken = other_device
        .open_profiles(&new_key)
        .await
        .unwrap()
        .default
        .unwrap();
    assert!(!broken.unavailable);
    assert_eq!(broken.username.as_deref(),Some("old-personal-user"));
    other_device
        .save_profile(
            &termterm_team_client::profiles::ProfileSelection {
                host_id: None,
                expected_revision: broken.revision,
                method: "credential".into(),
                credential_id: Some(id),
                username: Some("renewed-personal-user".into()),
            },
            &new_key,
        )
        .await
        .unwrap();
    assert_eq!(
        other_device
            .open_profiles(&new_key)
            .await
            .unwrap()
            .default
            .unwrap()
            .username
            .as_deref(),
        Some("renewed-personal-user")
    );
    let archive=other_device.profile_history(None,&new_key).await.unwrap();
    let old=archive.iter().find(|p|p.username.as_deref()==Some("old-personal-user")).unwrap();
    let current=other_device.open_profiles(&new_key).await.unwrap().default.unwrap();
    assert!(other_device.restore_profile(None,old.revision,current.revision-1,&new_key).await.is_err());
    other_device.restore_profile(None,old.revision,current.revision,&new_key).await.unwrap();
    assert_eq!(other_device.open_profiles(&new_key).await.unwrap().default.unwrap().username.as_deref(),Some("old-personal-user"));
    assert!(leader.profile_history(None,&leader_key).await.unwrap().iter().all(|p|p.username.as_deref()!=Some("old-personal-user")));
    chain_crypto(&fixture.profile, &fixture.username, password, &leader_key).await;
    durable_outbox(&fixture.profile, &fixture.username, password, &leader_key).await;
    desktop_outbox(&fixture.profile, &fixture.username, password).await;
    cold_offline_reopen(&fixture.profile, &fixture.username, password).await;
    for disabled in [false, true] {
        revoked_offline_creation(
            &fixture.profile,
            &fixture.username,
            password,
            &leader_key,
            disabled,
        )
        .await;
    }
    // Recovery rotation: real ciphertext from all private/shared versions,
    // interruption/resume, fresh writes during rotation and the new file.
    let rotation_dir=tempfile::tempdir().unwrap();
    let rotation_path=rotation_dir.path().join("new.ttrecovery");
    let new_recovery=termterm_team_client::rotation::create_file(rotation_path.to_str().unwrap(),fixture.profile.team_id,"New-Recovery-File-123!").unwrap();
    assert!(termterm_team_client::rotation::open_file(rotation_path.to_str().unwrap(),fixture.profile.team_id,"Wrong-Password-123!").is_err());
    let rotation=termterm_team_client::rotation::Rotation::begin(&leader,recovery_file.unlock(fixture.profile.team_id,"Recovery-Only-Test-123").unwrap(),new_recovery,password,"Real interrupted rotation test").await.unwrap();
    assert!(rotation.finish(&leader).await.is_err());
    rotation.step(&leader).await.unwrap();drop(rotation);
    leader.save_private_credential(Uuid::new_v4(),0,"password",b"written during rotation",b"fresh test secret",&leader_key).await.unwrap();
    let rotated_key=termterm_team_client::rotation::open_file(rotation_path.to_str().unwrap(),fixture.profile.team_id,"New-Recovery-File-123!").unwrap();
    let rotation=termterm_team_client::rotation::Rotation::begin(&leader,recovery_file.unlock(fixture.profile.team_id,"Recovery-Only-Test-123").unwrap(),rotated_key,password,"Resume actual crypto rotation").await.unwrap();
    while rotation.status(&leader).await.unwrap()["remaining"].as_i64().unwrap()>0 {rotation.step(&leader).await.unwrap();}
    assert_eq!(rotation.finish(&leader).await.unwrap()["completed"],true);
    assert_eq!(rotation.finish(&leader).await.unwrap()["completed"],true);
    leader.save_private_credential(Uuid::new_v4(),0,"password",b"new active recovery",b"post-rotation test secret",&leader_key).await.unwrap();
    let mut recovered=leader.begin_recovery_job(member.account_id,password,"Check new company file after rotation",termterm_team_client::rotation::open_file(rotation_path.to_str().unwrap(),fixture.profile.team_id,"New-Recovery-File-123!").unwrap()).await.unwrap();
    while recovered.step(&leader,&leader_key).await.unwrap()["completed"]!=true {}
    leader.logout().await.unwrap();
    other_device.logout().await.unwrap();
}

async fn desktop_outbox(profile: &ServiceProfile, username: &str, password: &str) {
    use termterm_team_client::desktop::{DesktopAction, DesktopState};
    let dir = tempfile::tempdir().unwrap();
    let state = DesktopState::default();
    state.configure_cache(dir.path().to_path_buf()).unwrap();
    state
        .login(profile.clone(), username.into(), password.into())
        .await
        .unwrap();
    let vault = Uuid::new_v4();
    let host = Uuid::new_v4();
    state
        .call(DesktopAction::CreateVault {
            id: vault,
            name: "Desktop persistent outbox".into(),
        })
        .await
        .unwrap();
    let mut content = ResourceContent {
        document: None, proxy:None,
            kind: "host".into(),
        parent_id: None,
        name: "native original".into(),
        address: Some("127.0.0.1".into()),
        protocol: Some("ssh".into()),
        port: Some(22),
        username_policy: Some("personal".into()),
        username: None,
        deleted: false,
    };
    state
        .call(DesktopAction::SaveResource {
            vault_id: vault,
            id: host,
            expected_revision: 0,
            client_at: "2026-10-07T00:00:00Z".into(),
            content: content.clone(),
            local_generation: None,
        })
        .await
        .unwrap();
    let record = state
        .call(DesktopAction::Resource { id: host })
        .await
        .unwrap();
    content.name = "native durable edit".into();
    let result = state
        .call(DesktopAction::SaveResource {
            vault_id: vault,
            id: host,
            expected_revision: 1,
            client_at: "2026-10-07T00:00:01Z".into(),
            content: content.clone(),
            local_generation: record["localGeneration"].as_i64(),
        })
        .await
        .unwrap();
    assert_eq!(result["status"], "applied");
    state.logout().unwrap();
    state
        .login(profile.clone(), username.into(), password.into())
        .await
        .unwrap();
    let page = state
        .call(DesktopAction::Resources {
            vault_id: vault,
            after: None,
        })
        .await
        .unwrap();
    assert_eq!(page["records"][0]["content"]["name"], content.name);
    assert_eq!(page["records"][0]["revision"], 2);
    assert_eq!(
        state.call(DesktopAction::JournalStatus {}).await.unwrap()["pending"],
        0
    );
    assert_eq!(
        state.call(DesktopAction::SyncJournal {}).await.unwrap()["delivered"],
        0
    );
    // A real second native client changes the server while the first retains
    // its original CAS snapshot. No direct-save IPC bypass simulates this race.
    let second_dir = tempfile::tempdir().unwrap();
    let second = DesktopState::default();
    second
        .configure_cache(second_dir.path().to_path_buf())
        .unwrap();
    second
        .login(profile.clone(), username.into(), password.into())
        .await
        .unwrap();
    let original = state
        .call(DesktopAction::Resource { id: host })
        .await
        .unwrap();
    second
        .call(DesktopAction::Resource { id: host })
        .await
        .unwrap();
    let mut remote = content.clone();
    remote.name = "other device committed".into();
    let saved = second
        .call(DesktopAction::SaveResource {
            vault_id: vault,
            id: host,
            expected_revision: 2,
            client_at: "2026-10-07T00:00:02Z".into(),
            content: remote,
            local_generation: None,
        })
        .await
        .unwrap();
    assert_eq!(saved["revision"], 3);
    // Reproduce a lease invalidated by a wall-clock jump without waiting a day.
    // The authenticated server must renew authority without replacing our
    // original CAS revision; the concurrent remote write must remain a conflict.
    {
        let mut cache = state.local_store(dir.path(), state.current_epoch()).await.unwrap();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        assert_eq!(cache.journal_status(now + 86_400_001).unwrap().edit_remaining_ms, 0);
    }
    let candidate = state
        .call(DesktopAction::SaveResource {
            vault_id: vault,
            id: host,
            expected_revision: 2,
            client_at: "2026-10-07T00:00:03Z".into(),
            content: content.clone(),
            local_generation: original["localGeneration"].as_i64(),
        })
        .await
        .unwrap();
    assert_eq!(candidate["status"], "conflict");
    assert_eq!(
        state.call(DesktopAction::JournalStatus {}).await.unwrap()["conflicts"],
        1
    );
    let server = state
        .call(DesktopAction::ServerResource { id: host })
        .await
        .unwrap();
    assert_eq!(server["revision"], 3);
    assert_eq!(server["content"]["name"], "other device committed");
    let local = state
        .call(DesktopAction::Resource { id: host })
        .await
        .unwrap();
    assert_eq!(local["content"]["name"], content.name);
    let pending = state
        .call(DesktopAction::Conflicts { after: None })
        .await
        .unwrap();
    let decision = pending["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["resourceId"] == host.to_string())
        .unwrap();
    state
        .call(DesktopAction::DecideConflict {
            id: serde_json::from_value(decision["id"].clone()).unwrap(),
            resource_id: host,
            vault_id: vault,
            current_revision: 3,
            client_at: "2026-10-07T00:00:04Z".into(),
            replacement: None,
        })
        .await
        .unwrap();
    assert_eq!(
        state.call(DesktopAction::JournalStatus {}).await.unwrap()["conflicts"],
        0
    );
    assert_eq!(
        state
            .call(DesktopAction::Resource { id: host })
            .await
            .unwrap()["content"]["name"],
        "other device committed"
    );
    second.logout().unwrap();
    state.logout().unwrap();
}

async fn durable_outbox(
    profile: &ServiceProfile,
    username: &str,
    password: &str,
    identity: &Identity,
) {
    use termterm_team_client::{
        local::{
            journal::{CachedResource, Permissions},
            Store,
        },
        offline::Delivery,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut writer = TeamClient::new(profile.clone()).unwrap();
    let view = writer
        .login(
            username,
            password,
            Uuid::new_v4(),
            "Online concurrent writer",
        )
        .await
        .unwrap();
    let scope = profile.cache_scope(view.account_id).unwrap();
    let mut store = Store::open(dir.path(), &scope, identity).unwrap();
    let now = 1_791_302_400_000_i64;
    let device = store.journal_device(now).unwrap();
    let mut sender = TeamClient::new(profile.clone()).unwrap();
    sender
        .login(username, password, device, "Persistent outbox device")
        .await
        .unwrap();
    let vault = Uuid::new_v4();
    let host = Uuid::new_v4();
    writer
        .create_vault(vault, "Outbox acceptance", identity)
        .await
        .unwrap();
    let mut content = ResourceContent {
        document: None, proxy:None,
            kind: "host".into(),
        parent_id: None,
        name: "original".into(),
        address: Some("127.0.0.1".into()),
        protocol: Some("ssh".into()),
        port: Some(22),
        username_policy: Some("personal".into()),
        username: None,
        deleted: false,
    };
    let initial = writer
        .prepare_resource(vault, host, 0, 1, "2026-10-07T00:00:00Z", &content)
        .await
        .unwrap();
    writer.save_resource(&initial).await.unwrap();
    store
        .refresh_cached_resource(CachedResource {
            navigation_only: false,
            id: host,
            vault_id: vault,
            server_revision: 1,
            generation: 0,
            permissions: Permissions {
                read: true,
                connect: true,
                edit: true,
            },
            write_capability: Some(
                sender
                    .offline_edit_capability(vault, host, None)
                    .await
                    .unwrap(),
            ),
            verified_at_ms: now,
            content: content.clone(),
        })
        .unwrap();
    store.authorize_offline_edits(now).unwrap();
    content.name = "İlk çevrimdışı düzenleme".into();
    let first = store.queue_edit(host, 0, content.clone(), now + 1).unwrap();
    content.name = "İkinci çevrimdışı düzenleme".into();
    let second = store.queue_edit(host, 1, content.clone(), now + 2).unwrap();
    drop(store);
    sender.logout().await.unwrap();
    let mut store = Store::open(dir.path(), &scope, identity).unwrap();
    sender
        .login(
            username,
            password,
            store.journal_device(now + 3).unwrap(),
            "Reopened same device",
        )
        .await
        .unwrap();
    assert!(
        matches!(sender.deliver_next(&mut store).await.unwrap(),Delivery::Applied(id) if id==first)
    );
    assert!(
        matches!(sender.deliver_next(&mut store).await.unwrap(),Delivery::Applied(id) if id==second)
    );
    assert!(matches!(
        sender.deliver_next(&mut store).await.unwrap(),
        Delivery::Empty
    ));
    let current = writer.open_resource(host, identity).await.unwrap();
    assert_eq!(current.revision, 3);
    assert_eq!(current.content, content);
    assert_eq!(
        sender
            .operation_receipt(first)
            .await
            .unwrap()
            .unwrap()
            .revision,
        Some(2)
    );
    assert!(writer.operation_receipt(first).await.unwrap().is_none()); // same account, different device
    content.name = "Yerel aday".into();
    let conflict = store.queue_edit(host, 2, content.clone(), now + 4).unwrap();
    content.name = "Kaybeden sürümün sonraki yerel düzenlemesi".into();
    let descendant = store.queue_edit(host, 3, content.clone(), now + 5).unwrap();
    content.name = "Sunucudaki eşzamanlı değişiklik".into();
    let concurrent = writer
        .prepare_resource(vault, host, 3, 2, "2026-10-07T00:00:05Z", &content)
        .await
        .unwrap();
    writer.save_resource(&concurrent).await.unwrap();
    assert!(
        matches!(sender.deliver_next(&mut store).await.unwrap(),Delivery::Conflict(id) if id==conflict)
    );
    assert_eq!(
        writer.open_resource(host, identity).await.unwrap().content,
        content
    );
    assert_eq!(store.journal_status(now + 6).unwrap().conflicts, 1);
    let history = writer.history(host, identity).await.unwrap();
    assert_eq!(history.len(), 4);
    let receipt = sender.operation_receipt(conflict).await.unwrap().unwrap();
    writer
        .decide_conflict(receipt.conflict_id.unwrap(), 4, None)
        .await
        .unwrap();
    assert_eq!(sender.reconcile_journal(&mut store).await.unwrap(), 1);
    assert_eq!(store.journal_status(now + 7).unwrap().quarantined, 1);
    assert!(
        matches!(sender.deliver_next(&mut store).await.unwrap(),Delivery::Conflict(id) if id==descendant)
    );
    assert_eq!(
        writer.open_resource(host, identity).await.unwrap().content,
        content
    );
    let decisions = writer.pending_conflicts(identity, None).await.unwrap();
    assert_eq!(decisions.conflicts.len(), 1);
    assert_eq!(
        decisions.conflicts[0].content.as_ref().unwrap().name,
        "Kaybeden sürümün sonraki yerel düzenlemesi"
    );
    sender.logout().await.unwrap();
    writer.logout().await.unwrap();
}

async fn chain_crypto(
    profile: &ServiceProfile,
    username: &str,
    password: &str,
    identity: &Identity,
) {
    use termterm_team_client::{connection::Authentication, profiles::ProfileSelection};
    let mut owner = TeamClient::new(profile.clone()).unwrap();
    owner
        .login(username, password, Uuid::new_v4(), "Chain owner")
        .await
        .unwrap();
    let v = Uuid::new_v4();
    owner
        .create_vault(v, "Chain isolation", identity)
        .await
        .unwrap();
    let ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    for (n, id) in ids.iter().enumerate() {
        let content = ResourceContent {
            document: None, proxy:None,
            kind: "host".into(),
            name: format!("Hop {n}"),
            parent_id: None,
            address: Some(format!("server-{n}.invalid")),
            protocol: Some("ssh".into()),
            port: Some(22),
            username_policy: Some("personal".into()),
            username: None,
            deleted: false,
        };
        let change = owner
            .prepare_resource(v, *id, 0, n as i64 + 1, "2026-10-07T01:00:00Z", &content)
            .await
            .unwrap();
        owner.save_resource(&change).await.unwrap();
    }
    let member = owner
        .create_member("chain.member", "Chain member", "operator")
        .await
        .unwrap();
    let mut user = TeamClient::new(profile.clone()).unwrap();
    let key = Identity::generate();
    user.login(
        "chain.member",
        &member.password,
        Uuid::new_v4(),
        "Chain member",
    )
    .await
    .unwrap();
    user.activate(password, &key).await.unwrap();
    user.login("chain.member", password, Uuid::new_v4(), "Chain member")
        .await
        .unwrap();
    let grant = owner
        .prepare_access(
            member.account_id,
            ids[2],
            &[
                AccessRule {
                    permission: AccessPermission::Read,
                    effect: AccessEffect::Allow,
                },
                AccessRule {
                    permission: AccessPermission::Connect,
                    effect: AccessEffect::Allow,
                },
            ],
        )
        .await
        .unwrap();
    let mut after = 0;
    loop {
        let page = owner
            .stage_access_keys(grant.id, after, identity)
            .await
            .unwrap();
        match page.next_after {
            Some(n) => after = n,
            None => break,
        }
    }
    owner.finish_access(grant.id, false).await.unwrap();
    let credential = Uuid::new_v4();
    user.save_private_credential(
        credential,
        0,
        "password",
        br#"{"label":"Personal chain password"}"#,
        b"Disposable-Personal-Chain!",
        &key,
    )
    .await
    .unwrap();
    user.save_profile(
        &ProfileSelection {
            host_id: None,
            expected_revision: 0,
            method: "credential".into(),
            credential_id: Some(credential),
            username: Some("personal-hop-user".into()),
        },
        &key,
    )
    .await
    .unwrap();
    let change = owner
        .prepare_chain(ids[2], &ids[..2], identity, 55001, "2026-10-12T00:00:00Z")
        .await
        .unwrap();
    assert_eq!(change.preview["recipientCount"], 2);
    owner.finish_chain(&change).await.unwrap();
    owner.finish_chain(&change).await.unwrap();
    assert_eq!(owner.open_resource(ids[2], identity).await.unwrap().revision, 2);

    user.save_profile(
        &ProfileSelection {
            host_id: Some(ids[0]),
            expected_revision: 0,
            method: "credential".into(),
            credential_id: Some(credential),
            username: Some("hop-override".into()),
        },
        &key,
    )
    .await
    .unwrap();
    let labels = user.connection_profiles(ids[2], &key).await.unwrap();
    assert_eq!(labels.as_array().unwrap().len(), 3);
    assert!(!labels.to_string().contains("server-"));
    let plan = user.resolve_plan(ids[2], &key).await.unwrap();
    assert_eq!(plan.steps.iter().map(|h| h.id).collect::<Vec<_>>(), ids);
    // Personal proxy credentials for a dependency remain private to this account.
    use termterm_team_client::{proxy::ProxySecret,resources::ProxyConfig};
    let endpoint=ProxyConfig{kind:"http".into(),host:"proxy.invalid".into(),port:8080,authenticate:true};
    user.save_proxy(ids[2],ids[0],None,0,Some(ProxySecret{endpoint:endpoint.clone(),username:"private-proxy-user".into(),password:"Private-Proxy-Fixture!".into()}),&key).await.unwrap();
    let status=user.proxy_status(ids[2],ids[0],&key).await.unwrap();
    let proxy_id=serde_json::from_value(status["credentialId"].clone()).unwrap();
    assert!(!status.to_string().contains("Private-Proxy-Fixture!"));
    assert!(owner.proxy_status(ids[2],ids[0],identity).await.unwrap()["credentialId"].is_null());
    assert!(owner.open_private_credential(proxy_id,identity).await.is_err());
    assert!(user.proxy_status(ids[0],ids[0],&key).await.is_err());
    assert_eq!(user.resolve_proxy(ids[2],ids[0],&endpoint,&key).await.unwrap().unwrap().password,"Private-Proxy-Fixture!");
    let mut changed_endpoint=endpoint.clone();changed_endpoint.port+=1;
    assert!(user.resolve_proxy(ids[2],ids[0],&changed_endpoint,&key).await.is_err());
    assert!(user.save_proxy(ids[2],ids[0],Some(proxy_id),0,None,&key).await.is_err());
    user.save_proxy(ids[2],ids[0],Some(proxy_id),1,None,&key).await.unwrap();
    assert!(user.resolve_proxy(ids[2],ids[0],&endpoint,&key).await.unwrap().is_none());
    // Historical direct route must not silently pick up the current chain.
    user.select_history(ids[2], Some(1), &key).await.unwrap();
    assert_eq!(user.resolve_plan(ids[2], &key).await.unwrap().steps.len(), 1);
    user.select_history(ids[2], Some(2), &key).await.unwrap();
    assert_eq!(user.resolve_plan(ids[2], &key).await.unwrap().steps.len(), 3);
    user.select_history(ids[2], None, &key).await.unwrap();

    for (n, step) in plan.steps.iter().enumerate() {
        assert_eq!(step.address, format!("server-{n}.invalid"));
        assert_eq!(
            step.username.as_deref(),
            Some(if n == 0 {
                "hop-override"
            } else {
                "personal-hop-user"
            })
        );
        assert!(
            matches!(&step.authentication,Authentication::Password(p) if p.as_str()=="Disposable-Personal-Chain!")
        );
    }
    user.save_profile(
        &ProfileSelection {
            host_id: Some(ids[0]),
            expected_revision: 1,
            method: "keyboard_interactive".into(),
            credential_id: None,
            username: Some("mfa-user".into()),
        },
        &key,
    )
    .await
    .unwrap();
    assert!(matches!(
        user.resolve_plan(ids[2], &key).await.unwrap().steps[0].authentication,
        Authentication::KeyboardInteractive
    ));
    assert!(user.resolve_plan(ids[0], &key).await.is_err());
    assert!(user.open_resource(ids[0], &key).await.is_err());
    assert!(user
        .resolve_plan(ids[2], &Identity::generate())
        .await
        .is_err());
    // A moved jump uses a NEW resource key and must be rewrapped for each
    // authorized root, without turning dependency access into standalone access.
    let dest=Uuid::new_v4();
    let folder=ResourceContent{document: None, proxy:None,
            kind:"folder".into(),parent_id:None,name:"Taşıma hedefi".into(),address:None,protocol:None,port:None,username_policy:None,username:None,deleted:false};
    let create=owner.prepare_resource(v,dest,0,4,"2026-10-07T01:00:00Z",&folder).await.unwrap();
    owner.save_resource(&create).await.unwrap();
    let mut moved=owner.prepare_move(ids[0],Some(dest),5,"2026-10-07T01:00:00Z",identity).await.unwrap();
    assert!(owner.open_resource(ids[0],identity).await.unwrap().content.parent_id.is_none());
    loop {if moved.stage(&owner,identity).await.unwrap()["ready"]==true {break;}}
    let receipt=moved.finish(&owner,false).await.unwrap();
    assert_eq!(receipt,moved.finish(&owner,false).await.unwrap());
    assert_eq!(owner.open_resource(ids[0],identity).await.unwrap().content.parent_id,Some(dest));
    assert_eq!(user.resolve_plan(ids[2],&key).await.unwrap().steps[0].address,"server-0.invalid");
    // Editing the jump refreshes dependent route envelopes in the SAME write,
    // without a manual chain save or any standalone permission grant.
    let mut edited=owner.open_resource(ids[0],identity).await.unwrap().content;
    edited.address=Some("server-0-updated.invalid".into());
    let change=owner.prepare_resource(v,ids[0],2,9,"2026-10-08T00:00:00Z",&edited).await.unwrap();
    owner.save_resource(&change).await.unwrap();
    assert_eq!(user.resolve_plan(ids[2],&key).await.unwrap().steps[0].address,"server-0-updated.invalid");
    // Grant access AFTER a chain already exists. A target-only manager uses its
    // root-bound envelopes, without independent read/connect rights on the hops.
    let delegated=owner.create_member("chain.delegator","Target-only manager","manager").await.unwrap();
    let mut delegator=TeamClient::new(profile.clone()).unwrap();let delegator_key=Identity::generate();
    delegator.login("chain.delegator",&delegated.password,Uuid::new_v4(),"Delegator").await.unwrap();
    delegator.activate(password,&delegator_key).await.unwrap();delegator.login("chain.delegator",password,Uuid::new_v4(),"Delegator").await.unwrap();
    let grant=owner.prepare_access(delegated.account_id,ids[2],&[
        AccessRule{permission:AccessPermission::Read,effect:AccessEffect::Allow},
        AccessRule{permission:AccessPermission::Connect,effect:AccessEffect::Allow},
        AccessRule{permission:AccessPermission::Edit,effect:AccessEffect::Allow}]).await.unwrap();
    let mut cursor=0;loop {match owner.stage_access_keys(grant.id,cursor,identity).await.unwrap().next_after{Some(n)=>cursor=n,None=>break}}
    owner.finish_access(grant.id,false).await.unwrap();
    assert!(delegator.open_resource(ids[0],&delegator_key).await.is_err());
    assert_eq!(delegator.resolve_plan(ids[2],&delegator_key).await.unwrap().steps.len(),3);
    let rev=owner.delegations().await.unwrap()["aclRevision"].as_i64().unwrap();
    owner.set_delegation(delegated.account_id,ids[2],&[AccessPermission::Read,AccessPermission::Connect],rev).await.unwrap();
    let late=owner.create_member("chain.late","Late chain member","operator").await.unwrap();
    let mut late_client=TeamClient::new(profile.clone()).unwrap();let late_key=Identity::generate();
    late_client.login("chain.late",&late.password,Uuid::new_v4(),"Late").await.unwrap();late_client.activate(password,&late_key).await.unwrap();
    late_client.login("chain.late",password,Uuid::new_v4(),"Late").await.unwrap();
    let grant=delegator.prepare_access(late.account_id,ids[2],&[
        AccessRule{permission:AccessPermission::Read,effect:AccessEffect::Allow},AccessRule{permission:AccessPermission::Connect,effect:AccessEffect::Allow}]).await.unwrap();
    let mut cursor=0;loop {match delegator.stage_access_keys(grant.id,cursor,&delegator_key).await.unwrap().next_after{Some(n)=>cursor=n,None=>break}}
    delegator.finish_access(grant.id,false).await.unwrap();
    let late_plan=late_client.resolve_plan(ids[2],&late_key).await.unwrap();
    assert_eq!(late_plan.steps[0].address,"server-0-updated.invalid");assert_eq!(late_plan.steps.len(),3);
    assert!(late_client.resolve_plan(ids[0],&late_key).await.is_err());
    late_client.logout().await.unwrap();delegator.logout().await.unwrap();


    assert!(user.resolve_plan(ids[0],&key).await.is_err());
    // New recipients can open the moved snapshot and retained historical keys.
    let grant=owner.prepare_access(member.account_id,dest,&[AccessRule{permission:AccessPermission::Read,effect:AccessEffect::Allow}],).await.unwrap();
    let mut after=0;
    loop {match owner.stage_access_keys(grant.id,after,identity).await.unwrap().next_after{Some(n)=>after=n,None=>break}}
    owner.finish_access(grant.id,false).await.unwrap();
    let probe=Uuid::new_v4();
    let create=owner.prepare_resource(v,probe,0,6,"2026-10-07T01:00:00Z",&ResourceContent{document: None, proxy:None,
            name:"Taşınan bağımsız kayıt".into(),..folder.clone()}).await.unwrap();
    owner.save_resource(&create).await.unwrap();
    assert!(user.open_resource(probe,&key).await.is_err());
    let mut moved=owner.prepare_move(probe,Some(dest),7,"2026-10-07T01:00:00Z",identity).await.unwrap();
    assert!(moved.impact(&owner,0).await.unwrap()["items"].as_array().unwrap().iter().any(|x|x["accountId"]==member.account_id.to_string()));
    loop {if moved.stage(&owner,identity).await.unwrap()["ready"]==true {break;}}
    moved.finish(&owner,false).await.unwrap();
    assert_eq!(user.open_resource(probe,&key).await.unwrap().content.name,"Taşınan bağımsız kayıt");
    assert!(user.history(probe,&key).await.unwrap().iter().all(|v|v.content.is_some()));
    let mut moved=owner.prepare_move(probe,None,8,"2026-10-07T01:00:00Z",identity).await.unwrap();
    loop {if moved.stage(&owner,identity).await.unwrap()["ready"]==true {break;}}
    moved.finish(&owner,false).await.unwrap();assert!(user.open_resource(probe,&key).await.is_err());
    let deny = owner
        .prepare_access(
            member.account_id,
            ids[2],
            &[AccessRule {
                permission: AccessPermission::Connect,
                effect: AccessEffect::Deny,
            }],
        )
        .await
        .unwrap();
    let mut after = 0;
    loop {
        let page = owner
            .stage_access_keys(deny.id, after, identity)
            .await
            .unwrap();
        match page.next_after {
            Some(n) => after = n,
            None => break,
        }
    }
    owner.finish_access(deny.id, false).await.unwrap();
    assert!(user.resolve_plan(ids[2], &key).await.is_err());
    user.logout().await.unwrap();
    owner.logout().await.unwrap();
}

// TLS is passed through unchanged. Stopping the loopback relay models a real
// connection refusal; no production bypass or test endpoint exists in the app.
async fn relay(
    port: u16,
    upstream: u16,
) -> (
    u16,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    socket.set_reuseaddr(true).unwrap();
    socket.bind(([127, 0, 0, 1], port).into()).unwrap();
    let listener = socket.listen(64).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop, mut stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _=&mut stopped=>break,
                accepted=listener.accept()=>{
                    let (mut downstream,_)=accepted.unwrap();
                    connections.spawn(async move {
                        if let Ok(mut upstream)=tokio::net::TcpStream::connect(("127.0.0.1",upstream)).await {
                            let _=tokio::io::copy_bidirectional(&mut downstream,&mut upstream).await;
                        }
                    });
                }
            }
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    });
    (port, stop, task)
}
async fn cold_offline_reopen(profile: &ServiceProfile, username: &str, password: &str) {
    use termterm_team_client::desktop::{DesktopAction, DesktopState};
    let upstream = url::Url::parse(&profile.service_url)
        .unwrap()
        .port_or_known_default()
        .unwrap();
    let (port, stop, task) = relay(0, upstream).await;
    let mut profile = profile.clone();
    profile.service_url = format!("https://localhost:{port}");
    let dir = tempfile::tempdir().unwrap();
    let state = DesktopState::default();
    state.configure_cache(dir.path().to_path_buf()).unwrap();
    state
        .login(profile.clone(), username.into(), password.into())
        .await
        .unwrap();
    state.call(DesktopAction::Overview {}).await.unwrap();
    let vault = Uuid::new_v4();
    let host = Uuid::new_v4();
    state
        .call(DesktopAction::CreateVault {
            id: vault,
            name: "Cold offline vault".into(),
        })
        .await
        .unwrap();
    state.call(DesktopAction::Vaults {}).await.unwrap();
    let mut content = ResourceContent {
        document: None, proxy:None,
            kind: "host".into(),
        parent_id: None,
        name: "Cold offline host".into(),
        address: Some("127.0.0.1".into()),
        protocol: Some("ssh".into()),
        port: Some(22),
        username_policy: Some("personal".into()),
        username: None,
        deleted: false,
    };
    state
        .call(DesktopAction::SaveResource {
            vault_id: vault,
            id: host,
            expected_revision: 0,
            client_at: "2026-10-07T00:00:00Z".into(),
            content: content.clone(),
            local_generation: None,
        })
        .await
        .unwrap();
    state
        .call(DesktopAction::Resource { id: host })
        .await
        .unwrap();
    let (_, original_plan) = state.connection_target(host).await.unwrap();
    state
        .call(DesktopAction::Resources {
            vault_id: vault,
            after: None,
        })
        .await
        .unwrap();
    stop.send(()).unwrap();
    task.await.unwrap();
    drop(state);
    let state = DesktopState::default();
    state.configure_cache(dir.path().to_path_buf()).unwrap();
    assert!(state
        .login(profile.clone(), username.into(), "wrong-password".into())
        .await
        .is_err());
    let view = state
        .login(profile.clone(), username.into(), password.into())
        .await
        .unwrap();
    assert_eq!(view["offline"], true);
    assert!(view.get("accessToken").is_none());
    assert!(view.get("refreshToken").is_none());
    assert_eq!(
        state.call(DesktopAction::Overview {}).await.unwrap()["offline"],
        true
    );
    assert!(state
        .call(DesktopAction::Vaults {})
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["id"] == vault.to_string()));
    let row = state
        .call(DesktopAction::Resource { id: host })
        .await
        .unwrap();
    let (epoch, offline_plan) = state.connection_target(host).await.unwrap();
    assert_eq!(
        original_plan.steps[0].address,
        offline_plan.steps[0].address
    );
    state.check_connection(host, epoch).await.unwrap();
    assert!(state.connection_target(Uuid::new_v4()).await.is_err());
    content.name = "Offline edit survived restart".into();
    let queued = state
        .call(DesktopAction::SaveResource {
            vault_id: vault,
            id: host,
            expected_revision: 1,
            client_at: "2026-10-07T00:00:01Z".into(),
            content: content.clone(),
            local_generation: row["localGeneration"].as_i64(),
        })
        .await
        .unwrap();
    assert_eq!(queued["status"], "queued");
    let created = Uuid::new_v4();
    let mut new_content = content.clone();
    new_content.name = "Offline created host".into();
    let result = state
        .call(DesktopAction::SaveResource {
            vault_id: vault,
            id: created,
            expected_revision: 0,
            client_at: "2026-10-07T00:00:02Z".into(),
            content: new_content.clone(),
            local_generation: None,
        })
        .await
        .unwrap();
    assert_eq!(result["status"], "queued");
    let new_row = state
        .call(DesktopAction::Resource { id: created })
        .await
        .unwrap();
    new_content.name = "Offline created then edited".into();
    state
        .call(DesktopAction::SaveResource {
            vault_id: vault,
            id: created,
            expected_revision: 0,
            client_at: "2026-10-07T00:00:03Z".into(),
            content: new_content.clone(),
            local_generation: new_row["localGeneration"].as_i64(),
        })
        .await
        .unwrap();
    assert_eq!(
        state.call(DesktopAction::JournalStatus {}).await.unwrap()["pending"],
        3
    );
    assert!(state
        .call(DesktopAction::CreateVault {
            id: Uuid::new_v4(),
            name: "Must fail offline".into()
        })
        .await
        .is_err());
    let (_, stop, task) = relay(port, upstream).await;
    let mut delivered = 0;
    for _ in 0..3 {
        delivered += state.call(DesktopAction::SyncJournal {}).await.unwrap()["delivered"]
            .as_i64()
            .unwrap();
    }
    assert_eq!(delivered, 3);
    let created_row = state
        .call(DesktopAction::ServerResource { id: created })
        .await
        .unwrap();
    assert_eq!(created_row["revision"], 2);
    assert_eq!(created_row["content"]["name"], new_content.name);
    let row = state
        .call(DesktopAction::Resource { id: host })
        .await
        .unwrap();
    assert_eq!(row["content"]["name"], content.name);
    assert_eq!(row["revision"], 2);
    assert_eq!(
        state.call(DesktopAction::JournalStatus {}).await.unwrap()["pending"],
        0
    );
    state.logout().unwrap();
    stop.send(()).unwrap();
    task.await.unwrap();
}

async fn revoked_offline_creation(
    profile: &ServiceProfile,
    username: &str,
    password: &str,
    identity: &Identity,
    disabled: bool,
) {
    use termterm_team_client::desktop::{DesktopAction, DesktopState};
    let member_name = if disabled {
        "disabled-creator"
    } else {
        "offline-creator"
    };
    let mut owner = TeamClient::new(profile.clone()).unwrap();
    owner
        .login(
            username,
            password,
            Uuid::new_v4(),
            "Creation decision owner",
        )
        .await
        .unwrap();
    let user = owner
        .create_member(member_name, "Çevrimdışı kayıt üyesi", "editor")
        .await
        .unwrap();
    let mut member = TeamClient::new(profile.clone()).unwrap();
    member
        .login(member_name, &user.password, Uuid::new_v4(), "Activation")
        .await
        .unwrap();
    member
        .activate(password, &Identity::generate())
        .await
        .unwrap();
    let vault = Uuid::new_v4();
    let folder = Uuid::new_v4();
    let created = Uuid::new_v4();
    owner
        .create_vault(vault, "Creation decision lab", identity)
        .await
        .unwrap();
    let folder_content = ResourceContent {
        document: None, proxy:None,
            kind: "folder".into(),
        parent_id: None,
        name: "Offline creation scope".into(),
        address: None,
        protocol: None,
        port: None,
        username_policy: None,
        username: None,
        deleted: false,
    };
    let change = owner
        .prepare_resource(vault, folder, 0, 1, "2026-10-07T00:00:00Z", &folder_content)
        .await
        .unwrap();
    owner.save_resource(&change).await.unwrap();
    let access = owner
        .prepare_access(
            user.account_id,
            folder,
            &[
                AccessRule {
                    permission: AccessPermission::Read,
                    effect: AccessEffect::Allow,
                },
                AccessRule {
                    permission: AccessPermission::Edit,
                    effect: AccessEffect::Allow,
                },
            ],
        )
        .await
        .unwrap();
    owner
        .stage_access_keys(access.id, 0, identity)
        .await
        .unwrap();
    owner.finish_access(access.id, false).await.unwrap();
    let upstream = url::Url::parse(&profile.service_url)
        .unwrap()
        .port_or_known_default()
        .unwrap();
    let (port, stop, task) = relay(0, upstream).await;
    let mut offline_profile = profile.clone();
    offline_profile.service_url = format!("https://localhost:{port}");
    let cache = tempfile::tempdir().unwrap();
    let state = DesktopState::default();
    state.configure_cache(cache.path().to_path_buf()).unwrap();
    state
        .login(offline_profile.clone(), member_name.into(), password.into())
        .await
        .unwrap();
    state
        .call(DesktopAction::Resources {
            vault_id: vault,
            after: None,
        })
        .await
        .unwrap();
    stop.send(()).unwrap();
    task.await.unwrap();
    let content = ResourceContent {
        document: None, proxy:None,
            kind: "host".into(),
        parent_id: Some(folder),
        name: "Yetkisi kaldırılan üyenin yeni adayı".into(),
        address: Some("127.0.0.1".into()),
        protocol: Some("ssh".into()),
        port: Some(22),
        username_policy: Some("personal".into()),
        username: None,
        deleted: false,
    };
    state
        .call(DesktopAction::SaveResource {
            vault_id: vault,
            id: created,
            expected_revision: 0,
            client_at: "2026-10-07T00:00:01Z".into(),
            content: content.clone(),
            local_generation: None,
        })
        .await
        .unwrap();
    if disabled {
        owner.disable_member(user.account_id).await.unwrap();
    } else {
        let revoke = owner
            .prepare_access(
                user.account_id,
                folder,
                &[AccessRule {
                    permission: AccessPermission::Edit,
                    effect: AccessEffect::Deny,
                }],
            )
            .await
            .unwrap();
        owner
            .stage_access_keys(revoke.id, 0, identity)
            .await
            .unwrap();
        owner.finish_access(revoke.id, false).await.unwrap();
    }
    let (_, stop, task) = relay(port, upstream).await;
    if disabled {
        assert!(state
            .login(offline_profile.clone(), member_name.into(), password.into())
            .await
            .is_err());
        assert!(state.call(DesktopAction::Overview {}).await.is_err());
        assert!(state.connection_target(created).await.is_err());
        assert!(state
            .submit_drafts(
                offline_profile.clone(),
                member_name.into(),
                "wrong-password".into()
            )
            .await
            .is_err());
        let received = state
            .submit_drafts(offline_profile.clone(), member_name.into(), password.into())
            .await
            .unwrap();
        assert_eq!(received["delivered"], 1);
        assert_eq!(received["awaitingDecision"], 1);
        let again = state
            .submit_drafts(offline_profile.clone(), member_name.into(), password.into())
            .await
            .unwrap();
        assert_eq!(again["delivered"], 0);
        assert_eq!(again["remaining"], 0);
        assert!(state.call(DesktopAction::Overview {}).await.is_err());
    } else {
        state.call(DesktopAction::SyncJournal {}).await.unwrap();
        state.call(DesktopAction::SyncJournal {}).await.unwrap();
    }
    assert!(owner.open_resource(created, identity).await.is_err());
    let pending = owner.pending_conflicts(identity, None).await.unwrap();
    let decision = pending
        .conflicts
        .iter()
        .find(|d| d.resource_id == created)
        .unwrap();
    assert_eq!(decision.content.as_ref().unwrap(), &content);
    assert_eq!(decision.expected_revision, 0);
    let change = owner
        .prepare_resource(vault, created, 0, 2, "2026-10-07T00:00:02Z", &content)
        .await
        .unwrap();
    owner
        .decide_conflict(decision.id, 0, Some(&change))
        .await
        .unwrap();
    assert_eq!(
        owner
            .open_resource(created, identity)
            .await
            .unwrap()
            .content,
        content
    );
    if disabled {
        let received = state
            .submit_drafts(offline_profile.clone(), member_name.into(), password.into())
            .await
            .unwrap();
        assert_eq!(received["awaitingDecision"], 0);
        assert_eq!(received["remaining"], 0);
    } else {
        state.call(DesktopAction::SyncJournal {}).await.unwrap();
        let journal = state.call(DesktopAction::JournalStatus {}).await.unwrap();
        assert_eq!(journal["pending"], 0);
        assert_eq!(journal["conflicts"], 0);
    }
    state.logout().unwrap();
    owner.logout().await.unwrap();
    stop.send(()).unwrap();
    task.await.unwrap();
}

// Manager-authored dependency envelopes are decrypted by the target-only user.
async fn delegated_chain_crypto(owner:&TeamClient, owner_key:&Identity, manager:&TeamClient, manager_key:&Identity, manager_id:Uuid,
    user:&TeamClient,user_key:&Identity,user_id:Uuid) {
    let v=Uuid::new_v4();let folder=Uuid::new_v4();let host=Uuid::new_v4();let jump=Uuid::new_v4();
    owner.create_vault(v,"Delegated routes",owner_key).await.unwrap();
    for (i,id) in [folder,host,jump].iter().enumerate() {
        let data=ResourceContent{document: None, proxy:None,
            kind:if i==0 {"folder"}else{"host"}.into(),parent_id:if i==0 {None}else{Some(folder)},
            name:format!("Delegated {i}"),address:if i==0 {None}else{Some(format!("delegated-{i}.invalid"))},
            protocol:if i==0 {None}else{Some("ssh".into())},port:if i==0 {None}else{Some(22)},
            username_policy:if i==0 {None}else{Some("personal".into())},username:None,deleted:false};
        let change=owner.prepare_resource(v,*id,0,40001+i as i64,"2026-10-08T00:00:00Z",&data).await.unwrap();
        owner.save_resource(&change).await.unwrap();
    }
    for (account,scope,perms) in [(manager_id,folder,vec![AccessPermission::Read,AccessPermission::Connect,AccessPermission::Edit]),
        (user_id,host,vec![AccessPermission::Read,AccessPermission::Connect])] {
        let rules=perms.into_iter().map(|permission|AccessRule{permission,effect:AccessEffect::Allow}).collect::<Vec<_>>();
        let grant=owner.prepare_access(account,scope,&rules).await.unwrap();let mut after=0;
        loop {match owner.stage_access_keys(grant.id,after,owner_key).await.unwrap().next_after {Some(n)=>after=n,None=>break}}
        owner.finish_access(grant.id,false).await.unwrap();
    }
    assert!(manager.prepare_chain(host,&[jump],manager_key,55002,"2026-10-12T00:00:00Z").await.is_err());
    let rev=owner.delegations().await.unwrap()["aclRevision"].as_i64().unwrap();
    owner.set_delegation(manager_id,folder,&[AccessPermission::Connect],rev).await.unwrap();
    let plan=manager.prepare_chain(host,&[jump],manager_key,55002,"2026-10-12T00:00:00Z").await.unwrap();
    manager.finish_chain(&plan).await.unwrap();manager.finish_chain(&plan).await.unwrap();
    let resolved=user.resolve_plan(host,user_key).await.unwrap();
    assert_eq!(resolved.steps.len(),2);assert_eq!(resolved.steps[0].address,"delegated-2.invalid");
    assert_eq!(resolved.steps[1].address,"delegated-1.invalid");assert!(user.resolve_plan(jump,user_key).await.is_err());
    let stale=manager.prepare_chain(host,&[],manager_key,55003,"2026-10-12T00:00:00Z").await.unwrap();
    let rev=owner.delegations().await.unwrap()["aclRevision"].as_i64().unwrap();
    owner.set_delegation(manager_id,folder,&[],rev).await.unwrap();
    assert!(manager.finish_chain(&stale).await.is_err());
    assert_eq!(user.resolve_plan(host,user_key).await.unwrap().steps.len(),2);
}

async fn documents_crypto(owner:&TeamClient,key:&Identity,user:&TeamClient,user_key:&Identity,account:Uuid) {
 use termterm_team_client::resources::{TeamDocument,WorkspaceEntry,WorkspaceLayout};
 let vault=Uuid::new_v4();let host=Uuid::new_v4();let snippet=Uuid::new_v4();let workspace=Uuid::new_v4();
 owner.create_vault(vault,"Belgeler",key).await.unwrap();
 let c=ResourceContent{kind:"host".into(),parent_id:None,name:"Target".into(),address:Some("document-target.invalid".into()),port:Some(22),protocol:Some("ssh".into()),username_policy:Some("personal".into()),username:None,deleted:false,document:None,proxy:None};
 let change=owner.prepare_resource(vault,host,0,45001,"2026-10-08T00:00:00Z",&c).await.unwrap();owner.save_resource(&change).await.unwrap();
 for (offset,id,kind,document) in [(0,snippet,"snippet",TeamDocument::Snippet{command:"printf 'Özel komut'".into()}),(1,workspace,"workspace",TeamDocument::Workspace{layout:WorkspaceLayout::Grid,entries:vec![WorkspaceEntry{host_id:host,startup_command:"printf 'Çalışma alanı'".into(),stats_enabled:false}]})] {
  let mut content=ResourceContent{kind:kind.into(),parent_id:None,name:format!("Belge {kind}"),address:None,protocol:None,port:None,username_policy:None,username:None,deleted:false,document:Some(document),proxy:None};
  let change=owner.prepare_resource(vault,id,0,45002+offset,"2026-10-08T00:00:00Z",&content).await.unwrap();
  let receipt=owner.save_resource(&change).await.unwrap();assert_eq!(receipt.status,"applied");assert_eq!(owner.save_resource(&change).await.unwrap().revision,Some(1));
  assert_eq!(owner.open_resource(id,key).await.unwrap().content,content);
  assert!(user.open_resource(id,user_key).await.is_err());
  let grant=owner.prepare_access(account,id,&[AccessRule{permission:AccessPermission::Read,effect:AccessEffect::Allow}]).await.unwrap();let mut after=0;
  loop {match owner.stage_access_keys(grant.id,after,key).await.unwrap().next_after {Some(n)=>after=n,None=>break}}
  owner.finish_access(grant.id,false).await.unwrap();
  assert_eq!(user.open_resource(id,user_key).await.unwrap().content,content);
  assert!(user.resolve_plan(host,user_key).await.is_err());
  content.name="Belge güncellendi".into();
  let change=owner.prepare_resource(vault,id,1,45010+offset,"2026-10-08T00:00:00Z",&content).await.unwrap();owner.save_resource(&change).await.unwrap();
  assert_eq!(user.history(id,user_key).await.unwrap().len(),2);
  let restored=owner.restore_version(id,1,2,45020+offset,"2026-10-08T00:00:00Z",key).await.unwrap();assert_eq!(restored["status"],"applied");
  assert_eq!(owner.open_resource(id,key).await.unwrap().content.name,format!("Belge {kind}"));
 }
}

async fn permanent_route_restore(owner:&TeamClient,key:&Identity) {
 let v=Uuid::new_v4();owner.create_vault(v,"Route restore",key).await.unwrap();
 let root=Uuid::new_v4();let hop=Uuid::new_v4();
 for (n,id) in [hop,root].into_iter().enumerate() {
  let c=ResourceContent{kind:"host".into(),parent_id:None,name:"Restore host".into(),address:Some("fixture.invalid".into()),port:Some(22),protocol:Some("ssh".into()),username_policy:Some("personal".into()),username:None,deleted:false,document:None,proxy:None};
  let change=owner.prepare_resource(v,id,0,96000+n as i64,"2026-10-12T00:00:00Z",&c).await.unwrap();owner.save_resource(&change).await.unwrap();
 }
 let chain=owner.prepare_chain(root,&[hop],key,96002,"2026-10-12T00:00:00Z").await.unwrap();owner.finish_chain(&chain).await.unwrap();
 let direct=owner.prepare_chain(root,&[],key,96003,"2026-10-12T00:00:00Z").await.unwrap();owner.finish_chain(&direct).await.unwrap();
 let restore=owner.prepare_host_restore(root,2,3,96004,"2026-10-12T00:00:00Z",key).await.unwrap();
 assert_eq!(restore.preview["hops"].as_array().unwrap().len(),1);
 let receipt=owner.finish_chain(&restore).await.unwrap();assert_eq!(receipt["revision"],4);
 let replay=owner.finish_chain(&restore).await.unwrap();for field in ["operationId","planId","revision","sourceRevision","returnRevision","status"] {assert_eq!(receipt[field],replay[field]);}
 assert_eq!(owner.authorize_plan(root).await.unwrap()["steps"].as_array().unwrap().len(),2);
 assert!(owner.prepare_host_restore(root,1,3,96005,"2026-10-12T00:00:00Z",key).await.is_err());
 let stale=owner.prepare_host_restore(root,1,4,96006,"2026-10-12T00:00:00Z",key).await.unwrap();
 let current=owner.open_resource(root,key).await.unwrap();let mut c=current.content;c.name="Concurrent edit".into();
 let change=owner.prepare_resource(v,root,4,96007,"2026-10-12T00:00:00Z",&c).await.unwrap();owner.save_resource(&change).await.unwrap();
 assert!(owner.finish_chain(&stale).await.is_err());
 assert_eq!(owner.authorize_plan(root).await.unwrap()["steps"].as_array().unwrap().len(),2);
 let restore=owner.prepare_host_restore(root,1,5,96008,"2026-10-12T00:00:00Z",key).await.unwrap();owner.finish_chain(&restore).await.unwrap();
 assert_eq!(owner.authorize_plan(root).await.unwrap()["steps"].as_array().unwrap().len(),1);
}
