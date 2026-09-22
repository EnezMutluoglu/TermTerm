use crate::{connections, state::Shared};
use anyhow::{ensure, Result};
use russh::keys::{
    agent::client::{AgentClient, AgentStream},
    HashAlg,
};
use serde_json::{json, Value};
use tauri::AppHandle;

pub async fn agent(kind: &str) -> Result<AgentClient<Box<dyn AgentStream + Send + Unpin>>> {
    #[cfg(windows)]
    {
        match kind {
            "openssh" => Ok(
                AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent")
                    .await?
                    .dynamic(),
            ),
            "pageant" => Ok(AgentClient::connect_pageant().await?.dynamic()),
            _ => anyhow::bail!("Choose Windows OpenSSH agent or Pageant"),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = kind;
        Ok(AgentClient::connect_env().await?.dynamic())
    }
}
pub async fn identities(kind: &str) -> Result<Vec<Value>> {
    let mut agent = agent(kind).await?;
    Ok(agent.request_identities().await?.into_iter().map(|identity| {
        let public=identity.public_key();
        json!({"agent":kind,"agentKey":public.fingerprint(HashAlg::Sha256).to_string(),"publicKey":public.to_openssh().unwrap_or_default(),"label":identity.comment(),"portable":false})
    }).collect())
}
pub fn known_address_matches(saved: &str, address: &str) -> bool {
    if !saved.starts_with("|1|") {
        return saved.eq_ignore_ascii_case(address);
    }
    use base64::{engine::general_purpose::STANDARD, Engine};
    use hmac::{Hmac, Mac};
    let parts: Vec<_> = saved.split('|').collect();
    if parts.len() != 4 {
        return false;
    }
    let (Ok(salt), Ok(expected)) = (STANDARD.decode(parts[2]), STANDARD.decode(parts[3])) else {
        return false;
    };
    if salt.len() != 20 || expected.len() != 20 {
        return false;
    }
    let Ok(mut mac) = Hmac::<sha1::Sha1>::new_from_slice(&salt) else {
        return false;
    };
    mac.update(address.to_ascii_lowercase().as_bytes());
    mac.verify_slice(&expected).is_ok()
}

pub async fn install(app: &AppHandle, state: &Shared, host_id: &str, public: &str) -> Result<()> {
    let key = russh::keys::PublicKey::from_openssh(public)?;
    // Re-encode the parsed key without comments. Only SSH key base64 enters this fixed command.
    let canonical = key.to_openssh()?;
    ensure!(
        !canonical.contains(['\'', '\n', '\r']),
        "Invalid public key"
    );
    let ssh =
        connections::connect_ssh(app, state, &uuid::Uuid::new_v4().to_string(), host_id, None)
            .await?;
    let mut channel = ssh.handle.channel_open_session().await?;
    let command=format!("umask 077; mkdir -p \"$HOME/.ssh\" && touch \"$HOME/.ssh/authorized_keys\" && chmod 700 \"$HOME/.ssh\" && chmod 600 \"$HOME/.ssh/authorized_keys\" && (grep -qxF '{}' \"$HOME/.ssh/authorized_keys\" || printf '%s\\n' '{}' >> \"$HOME/.ssh/authorized_keys\")",canonical,canonical);
    channel.exec(true, command).await?;
    let status = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let mut status = None;
        while let Some(msg) = channel.wait().await {
            if let russh::ChannelMsg::ExitStatus { exit_status } = msg {
                status = Some(exit_status);
            }
        }
        status
    })
    .await?;
    ensure!(
        status == Some(0),
        "Public key installation failed; destination must provide a POSIX shell"
    );
    Ok(())
}
