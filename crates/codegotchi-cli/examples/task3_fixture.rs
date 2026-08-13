use std::io::Write;
use std::path::PathBuf;

use chrono::Utc;
use codegotchi_cli::{AuthoritativeRuntime, RunningServer, SqliteStore};
use codegotchi_domain::{
    ActivityKind, AgentEvent, AgentEventKind, EventMetadata, EventSource, Pet, PetSpecies,
};
use uuid::Uuid;

const TOKEN: &str = "task3-playwright-token";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database = fixture_database_path();
    let _ = std::fs::remove_file(&database);
    let now = Utc::now();
    let runtime = AuthoritativeRuntime::new(
        SqliteStore::open(&database)?,
        Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, now),
    )?;

    seed_fixture(&runtime)?;
    let server = RunningServer::start_with_debug(runtime, TOKEN).await?;
    println!(
        "TASK3_FIXTURE_READY {}",
        serde_json::json!({
            "baseUrl": server.base_url(),
            "token": TOKEN,
        })
    );
    std::io::stdout().flush()?;

    std::future::pending::<()>().await;
    #[allow(unreachable_code)]
    {
        server.shutdown().await?;
        Ok(())
    }
}

fn fixture_database_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "codegotchi-task3-playwright-{}.sqlite",
        std::process::id()
    ))
}

fn seed_fixture(
    runtime: &std::sync::Arc<AuthoritativeRuntime>,
) -> Result<(), codegotchi_cli::RuntimeError> {
    let session_id = Uuid::from_u128(7);
    let now = Utc::now();
    runtime.apply_event(&AgentEvent::new(
        Uuid::from_u128(1),
        session_id,
        "playwright-fixture",
        EventSource::Codex,
        AgentEventKind::SessionStarted,
        None,
        now,
        EventMetadata::default(),
    ))?;

    for index in 0..20_u128 {
        runtime.apply_event(&AgentEvent::new(
            Uuid::from_u128(100 + index),
            session_id,
            "playwright-fixture",
            EventSource::Codex,
            AgentEventKind::CommandStarted,
            Some(ActivityKind::Testing),
            now,
            EventMetadata::default(),
        ))?;
    }

    // Exhaust kibble through the normal care API so a real browser action can
    // exercise the backend's typed out-of-stock error without a fixture route.
    for index in 0..50_u128 {
        runtime.feed(Uuid::from_u128(1_000 + index), "kibble")?;
    }
    Ok(())
}
