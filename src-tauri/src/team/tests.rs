use super::*;

#[tokio::test]
#[ignore = "requires isolated TERMTERM_TEAM_PROFILE PostgreSQL TLS lab"]
async fn postgres_sql_permissions_and_secret_envelopes() -> Result<()> {
    let path=std::env::var("TERMTERM_TEAM_PROFILE")?;
    let mut profile:SyncProfile=serde_json::from_slice(&std::fs::read(path)?)?;
    profile.database="termterm_team_e2e".into();
    let client=connect_checked(&profile).await?;
    let password="Disposable-Team-SQL-1234";
    let mut accounts=vec![];
    for role in ["sqlowner","sshonly","sqlmonitor"] {
        let name=format!("{role}_{}",&Uuid::new_v4().simple().to_string()[..10]);
        let (public,private)=crypto::identity(password)?; let device=Uuid::new_v4();
        let response:Value=client.query_one("SELECT termterm_team.register_account($1,$2,$3,$4,$5,$6,$7)",
            &[&name,&format!("{name}@example.invalid"),&password,&hex::decode(public)?,&hex::decode(&private)?,&device,&"SQL ACL regression"]).await?.get(0);
        accounts.push(Auth {profile:profile.clone(),user_id:response["userId"].as_str().unwrap().into(),username:name,device_id:device.to_string(),
            access:response["accessToken"].as_str().unwrap().into(),refresh:response["refreshToken"].as_str().unwrap().into(),delivery:response["deliveryToken"].as_str().unwrap().into(),
            identity:crypto::unlock_identity(password,&private)?.to_vec(),expires:now()+3_600_000});
    }
    let owner=&accounts[0];let ssh=&accounts[1];let monitor=&accounts[2];
    let team=raw_rpc(owner,"create_team",&json!({"name":"SQL permissions lab"})).await?["id"].clone();
    let vault=raw_rpc(owner,"create_vault",&json!({"teamId":team,"name":"SQL encrypted lab"})).await?["id"].as_str().unwrap().to_owned();
    for account in [ssh,monitor] {
        raw_rpc(owner,"member_set",&json!({"teamId":team,"userId":account.user_id,"role":"operator","active":true})).await?;
        for permission in ["read",if account.user_id==ssh.user_id {"connect"} else {"sql_monitor"}] {
            let overview=raw_rpc(owner,"overview",&json!({})).await?;
            raw_rpc(owner,"acl_set",&json!({"vaultId":vault,"userId":account.user_id,"permission":permission,"effect":"allow","aclRevision":overview["teams"][0]["aclRevision"]})).await?;
        }
    }
    let record=Record::new("databaseConnection",json!({"label":"Paylaşılan SQL İstanbul","engine":"postgresql","host":"localhost","password":"secret-not-for-ssh"}));
    // New records have no id on the server: the kind hint must prevent the
    // inherited terminal connect grant from receiving this first secret key.
    let recipients=raw_rpc(owner,"recipients",&json!({"vaultId":vault,"kind":record.kind})).await?;
    let recipient=|id:&str|recipients["recipients"].as_array().unwrap().iter().find(|r|r["userId"]==id).unwrap();
    assert_eq!(recipient(&ssh.user_id)["data"],true);assert_eq!(recipient(&ssh.user_id)["secret"],false);
    assert_eq!(recipient(&monitor.user_id)["secret"],true);
    let mut encrypted=crypto::encrypt(&vault,&record,1,&recipients["recipients"])?;
    for(k,v) in json!({"operationId":Uuid::new_v4().to_string(),"vaultId":vault,"recordId":record.id,"kind":record.kind,"sequence":1,"expectedRevision":0,"parentId":null,"clientAt":chrono::Utc::now().to_rfc3339(),"aclRevision":recipients["aclRevision"],"deleted":false,"links":[]}).as_object().unwrap(){encrypted[k]=v.clone();}
    // Extra encrypted envelopes are refused even if the caller holds edit.
    let mut forged=encrypted.clone();forged["envelopes"].as_array_mut().unwrap().push(json!({"userId":ssh.user_id,"purpose":"secret","envelope":"00"}));
    assert!(raw_rpc(owner,"apply",&forged).await.is_err());
    assert_eq!(raw_rpc(owner,"apply",&encrypted).await?["status"],"applied");
    let rows=raw_rpc(ssh,"records",&json!({"vaultId":vault})).await?;
    assert!(crypto::decrypt(&*identity(ssh)?,&vault,&rows["records"][0])?.data["password"].is_null());
    assert!(!rows.to_string().contains("secret-not-for-ssh"));
    assert!(raw_rpc(ssh,"connection_records",&json!({"vaultId":vault,"recordId":record.id})).await.is_err());
    for permission in ["sql_access","sql_run","sql_monitor","sql_manage","sql_export","connect","anything"] {
        assert!(raw_rpc(ssh,"sql_authorize",&json!({"vaultId":vault,"recordId":record.id,"permission":permission,"userId":owner.user_id})).await.is_err(),"SSH actor spoofed {permission}");
    }
    let response=raw_rpc(monitor,"sql_authorize",&json!({"vaultId":vault,"recordId":record.id,"permission":"sql_monitor"})).await?;
    assert_eq!(crypto::decrypt(&*identity(monitor)?,&vault,&response)?.data["password"],"secret-not-for-ssh");
    for permission in ["sql_run","sql_manage","sql_export"] {
        assert!(raw_rpc(monitor,"sql_authorize",&json!({"vaultId":vault,"recordId":record.id,"permission":permission})).await.is_err());
    }
    let overview=raw_rpc(owner,"overview",&json!({})).await?;
    raw_rpc(owner,"acl_set",&json!({"vaultId":vault,"recordId":record.id,"userId":monitor.user_id,"permission":"sql_export","effect":"allow","aclRevision":overview["teams"][0]["aclRevision"]})).await?;
    assert!(raw_rpc(monitor,"sql_authorize",&json!({"vaultId":vault,"recordId":record.id,"permission":"sql_export"})).await.is_ok());
    assert!(raw_rpc(monitor,"sql_authorize",&json!({"vaultId":vault,"recordId":record.id,"permission":"sql_export","recordExport":true,"includeSecrets":false})).await.is_err());
    assert!(client.query("SELECT * FROM termterm_team.team_records",&[]).await.is_err());
    assert!(client.query("SELECT termterm_team.sql_use($1,$2,$3)",&[&Uuid::parse_str(&owner.user_id)?,&Uuid::parse_str(&vault)?,&Uuid::parse_str(&record.id)?]).await.is_err());
    let overview=raw_rpc(owner,"overview",&json!({})).await?;
    raw_rpc(owner,"acl_set",&json!({"vaultId":vault,"recordId":record.id,"userId":monitor.user_id,"permission":"sql_monitor","effect":"deny","aclRevision":overview["teams"][0]["aclRevision"]})).await?;
    assert!(raw_rpc(monitor,"sql_authorize",&json!({"vaultId":vault,"recordId":record.id,"permission":"sql_access"})).await.is_err());
    let revoked=raw_rpc(monitor,"records",&json!({"vaultId":vault})).await?;
    assert!(crypto::decrypt(&*identity(monitor)?,&vault,&revoked["records"][0])?.data["password"].is_null());
    // SQL documents have a single, server-checked DB link. Reading instructions
    // neither grants SQL execution nor reveals connection secrets.
    for (i,kind) in ["savedQuery","databaseRunbook"].iter().enumerate() {
        let document=Record::new(kind,json!({"label":"Türkçe belge","connectionId":record.id,"sql":"SELECT 1","steps":[{"title":"Kontrol","sql":"SELECT 1","description":"Elle"}]}));
        let recipients=raw_rpc(owner,"recipients",&json!({"vaultId":vault,"kind":kind})).await?;
        let mut body=crypto::encrypt(&vault,&document,1,&recipients["recipients"])?;
        for(k,v) in json!({"operationId":Uuid::new_v4().to_string(),"vaultId":vault,"recordId":document.id,"kind":kind,"sequence":2+i,"expectedRevision":0,"clientAt":chrono::Utc::now().to_rfc3339(),"aclRevision":recipients["aclRevision"],"deleted":false,"links":[]}).as_object().unwrap(){body[k]=v.clone();}
        assert!(raw_rpc(owner,"apply",&body).await.is_err());
        body["links"]=json!([{"targetId":record.id,"relation":"chain"}]);
        assert!(raw_rpc(owner,"apply",&body).await.is_err());
        body["links"]=json!([{"targetId":record.id,"relation":"member"}]);
        let applied=raw_rpc(owner,"apply",&body).await?;assert_eq!(applied["status"],"applied");
        assert_eq!(raw_rpc(owner,"apply",&body).await?,applied);
        let loaded=raw_rpc(ssh,"sql_document",&json!({"vaultId":vault,"recordId":document.id})).await?;
        assert_eq!(loaded["connectionId"],record.id);
        assert_eq!(crypto::decrypt(&*identity(ssh)?,&vault,&loaded)?,document);
        assert!(raw_rpc(ssh,"sql_document",&json!({"vaultId":vault,"recordId":document.id,"export":true})).await.is_err());
        assert!(raw_rpc(ssh,"sql_authorize",&json!({"vaultId":vault,"recordId":record.id,"permission":"sql_run"})).await.is_err());
        let overview=raw_rpc(owner,"overview",&json!({})).await?;
        raw_rpc(owner,"acl_set",&json!({"vaultId":vault,"recordId":document.id,"userId":ssh.user_id,"permission":"read","effect":"deny","aclRevision":overview["teams"][0]["aclRevision"]})).await?;
        assert!(raw_rpc(ssh,"sql_document",&json!({"vaultId":vault,"recordId":document.id})).await.is_err());
    }
    let recipients=raw_rpc(owner,"recipients",&json!({"vaultId":vault,"recordId":record.id,"kind":record.kind})).await?;
    let mut deletion=crypto::encrypt(&vault,&record,2,&recipients["recipients"])?;
    for(k,v) in json!({"operationId":Uuid::new_v4().to_string(),"vaultId":vault,"recordId":record.id,"kind":record.kind,"sequence":99,"expectedRevision":1,"clientAt":chrono::Utc::now().to_rfc3339(),"aclRevision":recipients["aclRevision"],"deleted":true,"links":[]}).as_object().unwrap(){deletion[k]=v.clone();}
    assert!(raw_rpc(owner,"apply",&deletion).await.is_err());
    // Existing SSH behavior must still distribute its own credentials.
    let host=raw_rpc(owner,"recipients",&json!({"vaultId":vault,"kind":"host"})).await?;
    assert_eq!(host["recipients"].as_array().unwrap().iter().find(|r|r["userId"]==ssh.user_id).unwrap()["secret"],true);
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated TERMTERM_TEAM_PROFILE PostgreSQL TLS lab"]
async fn postgres_encrypted_acl_and_history() -> Result<()> {
    let path = std::env::var("TERMTERM_TEAM_PROFILE")
        .context("Set TERMTERM_TEAM_PROFILE to the isolated lab JSON")?;
    let mut profile: SyncProfile = serde_json::from_slice(&std::fs::read(path)?)?;
    profile.database = "termterm_team_e2e".into();
    let password = "Disposable-native-team-1234";
    let client = sync::connect(&profile).await?;
    let mut accounts = vec![];
    for role in ["owner", "editor", "operator"] {
        let name = format!("{role}_{}", &Uuid::new_v4().simple().to_string()[..12]);
        let (public, private) = crypto::identity(password)?;
        let device = Uuid::new_v4();
        let reply: Value = client
            .query_one(
                "SELECT termterm_team.register_account($1,$2,$3,$4,$5,$6,$7)",
                &[
                    &name,
                    &format!("{name}@example.invalid"),
                    &password,
                    &hex::decode(public)?,
                    &hex::decode(&private)?,
                    &device,
                    &"Rust TLS test",
                ],
            )
            .await?
            .get(0);
        let identity = crypto::unlock_identity(password, &private)?;
        accounts.push(Auth {
            profile: profile.clone(),
            user_id: reply["userId"].as_str().unwrap().into(),
            username: name,
            device_id: device.to_string(),
            access: reply["accessToken"].as_str().unwrap().into(),
            refresh: reply["refreshToken"].as_str().unwrap().into(),
            delivery: reply["deliveryToken"].as_str().unwrap().into(),
            identity: identity.to_vec(),
            expires: now() + 3_600_000,
        });
    }
    let owner = &accounts[0];
    let operator = &accounts[2];
    let team = raw_rpc(
        owner,
        "create_team",
        &json!({"name":"Encrypted integration lab"}),
    )
    .await?["id"]
        .as_str()
        .unwrap()
        .to_string();
    let vault = raw_rpc(
        owner,
        "create_vault",
        &json!({"teamId":team,"name":"TLS test"}),
    )
    .await?["id"]
        .as_str()
        .unwrap()
        .to_string();
    for (account, role) in [(&accounts[1], "editor"), (operator, "operator")] {
        raw_rpc(
            owner,
            "member_set",
            &json!({"teamId":team,"userId":account.user_id,"role":role,"active":true}),
        )
        .await?;
    }
    let dir = tempfile::tempdir()?;
    let mut cache = Cache::open(&dir.path().join("owner.ttteam"), password, true)?;
    cache.snapshot(&vault, &[], now())?;
    let mut record = Record::new(
        "host",
        json!({"label":"Özel İstanbul Sunucusu","address":"127.0.0.1","password":"plaintext-must-not-reach-db","port":22222}),
    );
    let recipients = raw_rpc(owner, "recipients", &json!({"vaultId":vault})).await?;
    let mut encrypted = crypto::encrypt(&vault, &record, 1, &recipients["recipients"])?;
    let operation = Uuid::new_v4().to_string();
    for(k,v)in json!({"operationId":operation,"vaultId":vault,"recordId":record.id,"expectedRevision":0,"sequence":1,"kind":"host","parentId":null,"clientAt":chrono::Utc::now().to_rfc3339(),"aclRevision":recipients["aclRevision"],"deleted":false,"links":[]}).as_object().unwrap(){encrypted[k]=v.clone();}
    let applied = raw_rpc(owner, "apply", &encrypted).await?;
    assert_eq!(applied["revision"], 1);
    assert_eq!(raw_rpc(owner, "apply", &encrypted).await?, applied);
    let raw = raw_rpc(owner, "records", &json!({"vaultId":vault})).await?;
    assert!(!raw.to_string().contains("plaintext-must-not-reach-db"));
    assert!(!raw.to_string().contains("Özel İstanbul"));
    assert_eq!(
        crypto::decrypt(&*identity(owner)?, &vault, &raw["records"][0])?,
        record
    );
    let owner_key = identity(owner)?;
    for permission in ["read", "connect"] {
        let overview = raw_rpc(owner, "overview", &json!({})).await?;
        let mut body = json!({"vaultId":vault,"userId":operator.user_id,"recordId":record.id,"permission":permission,"effect":"allow","aclRevision":overview["teams"][0]["aclRevision"]});
        let preview = raw_rpc(owner, "acl_preview", &body).await?;
        let mut row = raw["records"][0].clone();
        row["id"] = json!(record.id);
        body["rewrap"] = json!(crypto::rewrap(
            &owner_key,
            &vault,
            &row,
            &preview[0]["recipients"]
        )?);
        raw_rpc(owner, "acl_set", &body).await?;
    }
    let rows = raw_rpc(operator, "records", &json!({"vaultId":vault})).await?;
    let decrypted = crypto::decrypt(&*identity(operator)?, &vault, &rows["records"][0])?;
    assert_eq!(decrypted, record);
    assert!(!rows["records"][0]["permissions"]
        .as_array()
        .unwrap()
        .contains(&json!("reveal")));
    assert!(raw_rpc(
        operator,
        "recipients",
        &json!({"vaultId":vault,"recordId":record.id})
    )
    .await
    .is_err());
    let connections = raw_rpc(
        operator,
        "connection_records",
        &json!({"vaultId":vault,"recordId":record.id}),
    )
    .await?;
    assert_eq!(connections.as_array().unwrap().len(), 1);
    // Real TLS shared-terminal protocol: sealed session key, one writer, lease
    // rotation, idempotent frames and revocation. No terminal text in plaintext DB.
    let share = Uuid::new_v4().to_string();
    let lease = Uuid::new_v4().to_string();
    let users = raw_rpc(
        owner,
        "terminal_recipients",
        &json!({"vaultId":vault,"recordId":record.id}),
    )
    .await?;
    let session_key = crate::crypto::random_key();
    let context = format!("termterm-team-share:{share}");
    let envelopes=users["members"].as_array().unwrap().iter().map(|u|Ok(json!({"userId":u["userId"],"envelope":crypto::wrap_key(&session_key,u["publicKey"].as_str().unwrap(),&context)?}))).collect::<Result<Vec<_>>>()?;
    raw_rpc(owner,"terminal_open",&json!({"vaultId":vault,"recordId":record.id,"id":share,"lease":lease,"aclRevision":users["aclRevision"],"envelopes":envelopes})).await?;
    let guest = raw_rpc(
        operator,
        "terminal_join",
        &json!({"vaultId":vault,"id":share}),
    )
    .await?;
    let guest_key = crypto::open_key(
        &*identity(operator)?,
        guest["envelope"].as_str().unwrap(),
        &context,
    )?;
    assert_eq!(*guest_key, *session_key);
    assert!(raw_rpc(
        operator,
        "terminal_control",
        &json!({"vaultId":vault,"id":share,"writer":operator.username,"finish":false})
    )
    .await
    .is_err());
    raw_rpc(
        owner,
        "terminal_control",
        &json!({"vaultId":vault,"id":share,"writer":operator.username,"finish":false}),
    )
    .await?;
    let poll = raw_rpc(
        owner,
        "terminal_poll",
        &json!({"vaultId":vault,"id":share,"cursor":0}),
    )
    .await?;
    let frame = Uuid::new_v4().to_string();
    let new_lease = poll["lease"].as_str().unwrap();
    let aad = format!("termterm-team-terminal-v1:{share}:{new_lease}:{frame}:input");
    let ciphertext = crate::crypto::seal(&guest_key, b"shared test input", aad.as_bytes())?;
    let mut send = json!({"vaultId":vault,"id":share,"frameId":frame,"lease":lease,"kind":"input","payload":hex::encode(ciphertext)});
    assert!(raw_rpc(operator, "terminal_send", &send).await.is_err());
    send["lease"] = json!(new_lease);
    raw_rpc(operator, "terminal_send", &send).await?;
    raw_rpc(operator, "terminal_send", &send).await?;
    let frames = raw_rpc(
        owner,
        "terminal_poll",
        &json!({"vaultId":vault,"id":share,"cursor":0}),
    )
    .await?;
    assert_eq!(frames["frames"].as_array().unwrap().len(), 1);
    assert_eq!(
        &*crate::crypto::unseal(
            &session_key,
            &hex::decode(frames["frames"][0]["payload"].as_str().unwrap())?,
            aad.as_bytes()
        )?,
        b"shared test input"
    );
    raw_rpc(
        owner,
        "member_set",
        &json!({"teamId":team,"userId":operator.user_id,"role":"operator","active":false}),
    )
    .await?;
    assert!(raw_rpc(
        operator,
        "terminal_poll",
        &json!({"vaultId":vault,"id":share,"cursor":0})
    )
    .await
    .is_err());
    assert!(raw_rpc(
        owner,
        "terminal_poll",
        &json!({"vaultId":vault,"id":share,"cursor":0})
    )
    .await
    .is_err());
    assert!(raw_rpc(
        operator,
        "connection_records",
        &json!({"vaultId":vault,"recordId":record.id})
    )
    .await
    .is_err());
    record.data["label"] = json!("Next edit");
    let rows = vec![CachedRecord {
        record: decrypted,
        revision: 1,
        permissions: vec!["read".into(), "connect".into()],
        deleted: false,
    }];
    cache.snapshot(&vault, &rows, now())?;
    let status = cache.info(&vault, "Test", &owner.device_id)?;
    assert!(status.records[0].data.get("password").is_none());
    assert!(cache
        .enqueue(&vault, record, false, now(), None, false)
        .is_err());
    Ok(())
}

#[test]
fn selected_backup_requires_every_dependency_and_its_export_permission() -> Result<()> {
    let make = |id: &str, kind: &str, data: Value| CachedRecord {
        record: Record {
            id: id.into(),
            kind: kind.into(),
            data,
            updated_at: 0,
        },
        revision: 1,
        permissions: vec!["read".into(), "export".into(), "reveal".into()],
        deleted: false,
    };
    let all = vec![
        make(
            "host",
            "host",
            json!({"groupId":"group","chain":["jump"],"credentialId":"key","password":"secret"}),
        ),
        make("group", "group", json!({"label":"Group"})),
        make("jump", "host", json!({"credentialId":"key"})),
        make("key", "credential", json!({"privateKey":"secret-key"})),
        make("unrelated", "host", json!({"label":"Other"})),
    ];
    let result = select_export_records(all.clone(), &["host".into()], true)?;
    assert_eq!(result.len(), 4);
    assert!(!result.iter().any(|r| r.id == "unrelated"));
    let redacted = select_export_records(all.clone(), &["host".into()], false)?;
    assert!(redacted
        .iter()
        .all(|r| r.data.get("privateKey").is_none() && r.data.get("password").is_none()));
    let mut denied = all.clone();
    denied[2].permissions.retain(|p| p != "export");
    assert!(select_export_records(denied, &["host".into()], true).is_err());
    let mut no_reveal = all.clone();
    no_reveal[3].permissions.retain(|p| p != "reveal");
    assert!(select_export_records(no_reveal.clone(), &["host".into()], true).is_err());
    assert!(select_export_records(no_reveal, &["host".into()], false).is_ok());
    let mut missing = all.clone();
    missing.remove(1);
    assert!(select_export_records(missing, &["host".into()], true).is_err());
    assert!(select_export_records(all, &["absent".into()], true).is_err());
    Ok(())
}
