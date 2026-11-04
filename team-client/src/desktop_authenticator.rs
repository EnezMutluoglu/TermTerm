use super::*;
use crate::authenticator::{self, Action, Metadata, Snapshot, Token, View};

pub(super) async fn execute(
    s: &mut Session,
    context: Option<&str>,
    action: Action,
    epoch: u64,
) -> Result<Value> {
    let scope = format!("{}:authenticator:{epoch}", s.client.local_scope()?);
    ensure!(
        context == Some(scope.as_str()) || (context.is_none() && matches!(action, Action::List)),
        "Team session changed; reopen Authenticator"
    );
    let identity = s.identity.as_ref().context("Activate account first")?;
    match action {
        Action::Add { input } => {
            let token = Token::parse(&input)?;
            let metadata = serde_json::to_vec(&token.metadata())?;
            let secret = Zeroizing::new(serde_json::to_vec(&token)?);
            s.client
                .save_private_credential(Uuid::new_v4(), 0, "totp", &metadata, &secret, identity)
                .await?;
        }
        Action::Delete { id, revision } => {
            let id = Uuid::parse_str(&id)?;
            let private = s.client.private_credentials().await?;
            let own = private
                .as_array()
                .context("Invalid credential list")?
                .iter()
                .any(|r| r["id"] == id.to_string() && r["kind"] == "totp");
            if own {
                s.client
                    .request(
                        reqwest::Method::POST,
                        &format!("v1/credentials/private/{id}/remove"),
                        Some(&json!({"expectedRevision":revision})),
                        true,
                    )
                    .await?;
            } else {
                let shared = s.client.shared_credentials().await?;
                ensure!(
                    shared
                        .as_array()
                        .context("Invalid shared token list")?
                        .iter()
                        .any(|r| r["id"] == id.to_string() && r["kind"] == "totp"),
                    "Authenticator token not found"
                );
                s.client.remove_shared(id, revision).await?;
            }
        }
        Action::Reveal { id } => {
            let id = Uuid::parse_str(&id)?;
            let private = s.client.private_credentials().await?;
            let own = private
                .as_array()
                .context("Invalid credential list")?
                .iter()
                .any(|r| r["id"] == id.to_string() && r["kind"] == "totp");
            let credential = if own {
                s.client.open_private_credential(id, identity).await?
            } else {
                s.client
                    .open_shared_credential(id, "reveal", None, identity)
                    .await?
            };
            ensure!(credential.kind == "totp", "Not an Authenticator token");
            let token: Token = serde_json::from_slice(&credential.secret)?;
            return Ok(json!({"uri":token.uri()?.as_str()}));
        }
        Action::List => {}
    }
    let (team, _, _) = s.client.actor()?;
    let private = s.client.private_credentials().await?;
    let shared = s.client.shared_credentials().await?;
    let now = authenticator::now()?;
    let mut entries = vec![];
    let mut tokens = std::collections::HashMap::new();
    for (row, visibility) in private
        .as_array()
        .context("Invalid credential list")?
        .iter()
        .map(|r| (r, "private"))
        .chain(
            shared
                .as_array()
                .context("Invalid shared credential list")?
                .iter()
                .map(|r| (r, "shared")),
        )
    {
        if row["kind"] != "totp" {
            continue;
        }
        let id: Uuid = serde_json::from_value(row["id"].clone())?;
        let revision = row["revision"].as_i64().context("Invalid token revision")?;
        let metadata = (|| -> Result<Metadata> {
            let bytes = keys::decrypt(
                team,
                id,
                revision,
                Purpose::Data,
                row["encryptedData"].as_str().context("Missing metadata")?,
                row["dataEnvelope"]
                    .as_str()
                    .context("Token recovery required")?,
                identity,
            )?;
            let metadata: Metadata = serde_json::from_slice(&bytes)?;
            ensure!((1..=300).contains(&metadata.period), "Invalid token period");
            Ok(metadata)
        })();
        let unavailable = metadata.is_err();
        let metadata = metadata.unwrap_or(Metadata {
            label: "Kurtarma gerekli".into(),
            issuer: "".into(),
            algorithm: "SHA1".into(),
            digits: 6,
            period: 30,
        });
        let permissions: Vec<String> = if visibility == "private" {
            vec!["connect".into(), "reveal".into(), "export".into()]
        } else {
            serde_json::from_value(row["permissions"].clone())?
        };
        if !unavailable && permissions.iter().any(|p| p == "connect") {
            let credential = if visibility == "private" {
                s.client.open_private_credential(id, identity).await?
            } else {
                s.client
                    .open_shared_credential(id, "otp", None, identity)
                    .await?
            };
            ensure!(
                credential.kind == "totp" && credential.revision == revision,
                "Token changed; refresh Authenticator"
            );
            let token: Token =
                serde_json::from_slice(&credential.secret).context("Invalid encrypted token")?;
            ensure!(
                serde_json::to_value(token.metadata())? == serde_json::to_value(&metadata)?,
                "Token metadata does not match"
            );
            token.validate()?;
            tokens.insert(id.to_string(), token);
        }
        entries.push(View {
            id: id.to_string(),
            revision,
            valid_until: (now / metadata.period + 1) * metadata.period,
            metadata,
            code: None,
            visibility: visibility.into(),
            permissions,
            can_remove: visibility == "private" || row["canRemove"] == true,
            can_update: row["canUpdate"] == true,
            unavailable,
        });
    }
    let now = authenticator::now()?;
    for entry in &mut entries {
        entry.valid_until = (now / entry.metadata.period + 1) * entry.metadata.period;
        if let Some(token) = tokens.get(&entry.id) {
            entry.code = Some(token.code(now)?);
        }
    }
    Ok(serde_json::to_value(Snapshot {
        context: scope,
        entries,
        server_time: now,
    })?)
}
