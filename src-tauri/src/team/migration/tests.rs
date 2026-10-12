use super::*;
fn fixture() -> Snapshot {
    let team = Uuid::new_v4().to_string();
    let owner = Uuid::new_v4().to_string();
    let v = Uuid::new_v4().to_string();
    let group = Record::new("group", json!({"label":"Üretim"}));
    let h = Record::new(
        "host",
        json!({"label":"İstanbul DB","groupId":group.id,"address":"private.example.invalid","password":"MIGRATION_SECRET_NEVER_IPC","privateKey":"PRIVATE_MATERIAL_NEVER_IPC","protocol":"ssh","port":22,"proxy":{"host":"proxy.private","password":"PROXY_SECRET_NEVER_IPC"}}),
    );
    Snapshot {
        version: 1,
        team: json!({"id":team,"name":"Kaynak","ownerId":owner,"aclRevision":9}),
        captured_at: "2026-10-08T00:00:00Z".into(),
        members: json!({"members":[{"userId":owner,"username":"leader","role":"owner","active":true}],"acl":[{"user_id":owner,"vault_id":v,"resource_id":h.id,"permission":"export","effect":"deny"}],"delegations":[]}),
        vaults: vec![SourceVault {
            id: v,
            name: "Kasa".into(),
            revision: 2,
            records: vec![group, h]
                .into_iter()
                .map(|r| SourceRecord {
                    id: r.id.clone(),
                    kind: r.kind.clone(),
                    record: Some(r),
                    revision: 1,
                    deleted: false,
                    issue: None,
                })
                .collect(),
        }],
    }
}
#[test]
fn preview_keeps_relationships_denies_and_never_exposes_secrets() -> Result<()> {
    let s = fixture();
    let target = Uuid::new_v4().to_string();
    let p = preview(&s, "hash", &target)?;
    assert_eq!(p["canApply"], false);
    assert_eq!(p["rules"][0]["effect"], "deny");
    assert_eq!(
        p["records"][1]["parentTargetId"],
        p["records"][0]["targetId"]
    );
    assert_eq!(p["records"][1]["unsupportedFields"], json!(["proxy"]));
    let text = p.to_string();
    for secret in [
        "MIGRATION_SECRET_NEVER_IPC",
        "PRIVATE_MATERIAL_NEVER_IPC",
        "PROXY_SECRET_NEVER_IPC",
        "private.example.invalid",
        "proxy.private",
    ] {
        assert!(!text.contains(secret));
    }
    assert_eq!(preview(&s, "hash", &target)?, p);
    assert_ne!(
        preview(&s, "hash", &Uuid::new_v4().to_string())?["records"][0]["targetId"],
        p["records"][0]["targetId"]
    );
    Ok(())
}
#[test]
fn encrypted_archive_failure_never_changes_source() -> Result<()> {
    let s = fixture();
    let d = tempfile::tempdir()?;
    let path = d.path().join("snapshot.ttmigration");
    let password = "Disposable-archive-password";
    write_archive(&path, password, &s)?;
    let before = std::fs::read(&path)?;
    assert!(!String::from_utf8_lossy(&before).contains("MIGRATION_SECRET_NEVER_IPC"));
    let (open, hash) = read_archive(&path, password)?;
    assert_eq!(
        open.vaults[0].records[1].record.as_ref().unwrap().data["password"],
        "MIGRATION_SECRET_NEVER_IPC"
    );
    assert_eq!(hash, hex::encode(Sha256::digest(&before)));
    assert!(read_archive(&path, "wrong-password").is_err());
    assert!(write_archive(&path, password, &s).is_err());
    assert_eq!(std::fs::read(&path)?, before);
    let cut = d.path().join("cut.ttmigration");
    std::fs::write(&cut, &before[..before.len() / 2])?;
    assert!(read_archive(&cut, password).is_err());
    let mut a: Archive = serde_json::from_slice(&before)?;
    a.version = 999;
    std::fs::write(&cut, serde_json::to_vec(&a)?)?;
    assert!(read_archive(&cut, password).is_err());
    assert_eq!(std::fs::read(&path)?, before);
    Ok(())
}
#[test]
fn broken_relations_cannot_be_silently_ignored() -> Result<()> {
    let mut s = fixture();
    let target = Uuid::new_v4().to_string();
    let id = s.vaults[0].records[0].id.clone();
    s.vaults[0].records[0].record.as_mut().unwrap().data["groupId"] = json!(id);
    let p = preview(&s, "h", &target)?;
    assert!(p["issueRecords"].as_u64().unwrap() > 0);
    assert!(p["records"][0]["issues"].to_string().contains("döngüsü"));
    s.members["acl"][0]["resource_id"] = json!(Uuid::new_v4());
    assert!(preview(&s, "h", &target).is_err());
    s.members["acl"] = json!([]);
    s.vaults[0].records[1].record = None;
    s.vaults[0].records[1].issue = Some("secret error must not echo".into());
    let p = preview(&s, "h", &target)?;
    assert!(!p.to_string().contains("secret error"));
    assert_eq!(p["issueRecords"], 2);
    Ok(())
}

#[test]
fn account_mapping_is_explicit_role_preserving_and_redacted() -> Result<()> {
    let s = fixture();
    let target = Uuid::new_v4().to_string();
    let account = Uuid::new_v4().to_string();
    let owner = json!({"teamId":target,"accountId":account,"role":"owner","aclRevision":2});
    let mut members = json!([{"accountId":account,"username":"leader","displayName":"Lider","role":"owner","status":"active","secret":"MUST_NOT_LEAK"}]);
    let fp = accounts::fingerprint(&owner,&members)?;
    let mut p = preview(&s,"hash",&target)?;
    accounts::map(&mut p,&members,&fp,&[])?;
    assert_eq!(p["accountsReady"],false); // Matching username is not authorization.
    assert!(p["members"][0]["targetAccountId"].is_null());
    let mapping = AccountMapping { source_account_id: s.team["ownerId"].as_str().unwrap().into(), target_account_id: account.clone() };
    accounts::map(&mut p,&members,&fp,&[mapping.clone()])?;
    assert_eq!(p["accountsReady"],true);
    assert_eq!(p["canApply"],false);
    assert_eq!(p["rules"][0]["targetAccountId"],account);
    assert_eq!(p["rules"][0]["effect"],"deny");
    assert!(!p.to_string().contains("MUST_NOT_LEAK"));
    members[0]["status"] = json!("pending");
    assert!(accounts::fingerprint(&owner,&members).is_err());
    accounts::map(&mut p,&members,"changed",&[mapping.clone()])?;
    assert_eq!(p["accountsReady"],false);
    members[0]["status"] = json!("active");
    members[0]["role"] = json!("operator");
    accounts::map(&mut p,&members,"changed",&[mapping.clone()])?;
    assert_eq!(p["accountsReady"],false);
    assert!(accounts::map(&mut p,&members,"changed",&[AccountMapping {target_account_id:Uuid::new_v4().to_string(),..mapping}]).is_err());
    Ok(())
}

#[test]
fn account_mapping_rejects_merging_accounts_and_reactivating_disabled_members() -> Result<()> {
    let mut s = fixture();
    let other = Uuid::new_v4().to_string();
    s.members["members"].as_array_mut().unwrap().push(json!({"userId":other,"username":"disabled","role":"operator","active":false}));
    let owner_id = Uuid::new_v4().to_string();
    let target_id = Uuid::new_v4().to_string();
    let mut members = json!([
        {"accountId":owner_id,"username":"leader","displayName":"Lider","role":"owner","status":"active"},
        {"accountId":target_id,"username":"disabled","displayName":"Kapalı","role":"operator","status":"active"}
    ]);
    let owner = json!({"teamId":Uuid::new_v4(),"accountId":owner_id,"role":"owner","aclRevision":1});
    let fp = accounts::fingerprint(&owner,&members)?;
    let mut p = preview(&s,"hash",owner["teamId"].as_str().unwrap())?;
    let mut bindings = vec![AccountMapping {source_account_id:s.team["ownerId"].as_str().unwrap().into(),target_account_id:owner_id.clone()},
        AccountMapping {source_account_id:other,target_account_id:target_id.clone()}];
    accounts::map(&mut p,&members,&fp,&bindings)?;
    assert_eq!(p["unresolvedAccounts"],1);
    members[1]["status"] = json!("disabled");
    accounts::map(&mut p,&members,&fp,&bindings)?;
    assert_eq!(p["accountsReady"],true);
    assert_ne!(accounts::fingerprint(&owner,&members)?,fp);
    let mut reordered=members.as_array().unwrap().clone();reordered.reverse();
    assert_eq!(accounts::fingerprint(&owner,&json!(reordered))?,accounts::fingerprint(&owner,&members)?);
    bindings[1].target_account_id=owner_id;
    assert!(accounts::map(&mut p,&members,&fp,&bindings).is_err());
    Ok(())
}
