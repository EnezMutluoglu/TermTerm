//! Disposable second-client helper for native integration tests. Not bundled.
//! Private input arrives through stdin; only an operation receipt goes to stdout.
use serde::Deserialize;
use std::io::Read;
use termterm_team_client::{
    resources::ResourceContent,
    transport::{ServiceProfile, TeamClient},
};
use uuid::Uuid;
use zeroize::Zeroizing;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    profile: ServiceProfile,
    username: String,
    password: String,
    vault_id: Uuid,
    id: Uuid,
    expected_revision: i64,
    content: ResourceContent,
}
#[tokio::main]
async fn main() {
    if run().await.is_err() {
        eprintln!("Disposable test writer failed");
        std::process::exit(1);
    }
}
async fn run() -> anyhow::Result<()> {
    let mut bytes = Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(4 * 1024 * 1024)
        .read_to_end(&mut bytes)?;
    let mut input: Input = serde_json::from_slice(&bytes)?;
    let password = Zeroizing::new(std::mem::take(&mut input.password));
    let mut client = TeamClient::new(input.profile)?;
    client
        .login(
            &input.username,
            &password,
            Uuid::new_v4(),
            "Disposable concurrent test writer",
        )
        .await?;
    let timestamp =
        chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now()).to_rfc3339();
    let plan = client
        .prepare_resource(
            input.vault_id,
            input.id,
            input.expected_revision,
            1,
            &timestamp,
            &input.content,
        )
        .await?;
    let receipt = client.save_resource(&plan).await?;
    client.logout().await?;
    println!("{}", serde_json::to_string(&receipt)?);
    Ok(())
}
