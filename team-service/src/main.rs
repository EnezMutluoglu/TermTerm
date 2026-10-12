use anyhow::{Context, Result};
use std::path::PathBuf;
use termterm_team_service::{router, App, Config};
#[tokio::main]
async fn main() -> Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .context("Usage: termterm-team-service /path/to/service.json")?;
    let config = Config::read(&path)?;
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    println!(
        "TermTerm Team Service listening on {} (HTTPS proxy required)",
        config.listen
    );
    let app = App::new(config);
    let maintenance = app.clone();
    let worker = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            maintenance.retention_tick().await;
        }
    });
    let result = axum::serve(listener, router(app))
        .with_graceful_shutdown(shutdown())
        .await;
    worker.abort();
    let _ = worker.await;
    result?;
    Ok(())
}
async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut term) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _=tokio::signal::ctrl_c()=>{}, _=term.recv()=>{} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
