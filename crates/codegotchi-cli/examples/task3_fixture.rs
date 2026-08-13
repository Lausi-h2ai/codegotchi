use std::io::Write;
use std::path::PathBuf;

use chrono::{Duration, Utc};
use codegotchi_cli::{AuthoritativeRuntime, RunningServer, SqliteStore};
use codegotchi_domain::{
    ActivityKind, AgentEvent, AgentEventKind, DefaultNeedProgressionStrategy, EnforcementMode,
    EventMetadata, EventSource, FoodInventory, Pet, PetBehavior, PetDemand, PetDemandKind,
    PetNeeds, PetSimulation, PetSpecies, Poop, SimulationSnapshot, SystemClock,
};
use uuid::Uuid;

const TOKEN: &str = "task3-playwright-token";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("CODEGOTCHI_PLAYWRIGHT_MODE").ok())
        .unwrap_or_else(|| "default".to_owned());
    let mode = FixtureMode::parse(&mode)?;
    let database = fixture_database_path();
    let _ = std::fs::remove_file(&database);
    let now = Utc::now();
    let runtime =
        AuthoritativeRuntime::new(SqliteStore::open(&database)?, fixture_snapshot(mode, now))?;

    if mode == FixtureMode::Default {
        seed_fixture(&runtime)?;
    }
    let server = RunningServer::start_with_debug(runtime, TOKEN).await?;
    println!(
        "TASK3_FIXTURE_READY {}",
        serde_json::json!({
            "baseUrl": server.base_url(),
            "token": TOKEN,
            "mode": mode.as_str(),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FixtureMode {
    Default,
    Affection,
    Snack,
    Poop,
    SnackPoop,
    StrictHappiness,
    Overdue,
}

impl FixtureMode {
    fn parse(value: &str) -> Result<Self, std::io::Error> {
        match value {
            "default" => Ok(Self::Default),
            "affection" => Ok(Self::Affection),
            "snack" => Ok(Self::Snack),
            "poop" => Ok(Self::Poop),
            "snack-poop" => Ok(Self::SnackPoop),
            "strict-happiness" => Ok(Self::StrictHappiness),
            "overdue" => Ok(Self::Overdue),
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("unsupported Playwright fixture mode `{other}`"),
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Affection => "affection",
            Self::Snack => "snack",
            Self::Poop => "poop",
            Self::SnackPoop => "snack-poop",
            Self::StrictHappiness => "strict-happiness",
            Self::Overdue => "overdue",
        }
    }
}

fn fixture_snapshot(mode: FixtureMode, now: chrono::DateTime<Utc>) -> SimulationSnapshot {
    let pet = Pet::with_inventory(
        Uuid::from_u128(1),
        "Mochi",
        PetSpecies::Cat,
        now,
        FoodInventory::starter(),
    );
    let mut snapshot =
        PetSimulation::new(pet, SystemClock, DefaultNeedProgressionStrategy).snapshot();
    snapshot.next_incident_at = Some(now + Duration::minutes(5));

    match mode {
        FixtureMode::Affection => {
            snapshot.needs = PetNeeds::new(0.0, 100.0, 80.0, 100.0);
            snapshot.pending_demands.push(PetDemand::new(
                Uuid::from_u128(0xaffec7),
                PetDemandKind::Affection,
                now,
            ));
        }
        FixtureMode::Snack => {
            snapshot.pending_demands.push(PetDemand::new(
                Uuid::from_u128(0x5ac9),
                PetDemandKind::Snack,
                now,
            ));
        }
        FixtureMode::Poop => {
            snapshot
                .pending_poops
                .push(Poop::new(Uuid::from_u128(0x700f), now));
            snapshot.poop_sequence = 1;
        }
        FixtureMode::SnackPoop => {
            snapshot.pending_demands.push(PetDemand::new(
                Uuid::from_u128(0x5ac9),
                PetDemandKind::Snack,
                now,
            ));
            snapshot
                .pending_poops
                .push(Poop::new(Uuid::from_u128(0x700f), now));
            snapshot.poop_sequence = 1;
        }
        FixtureMode::StrictHappiness => {
            snapshot.needs = PetNeeds::new(20.0, 80.0, 5.0, 80.0);
            snapshot.behavior = PetBehavior::CriticalNeed;
            snapshot.enforcement_mode = EnforcementMode::Strict;
        }
        FixtureMode::Overdue => {
            let past = now - Duration::minutes(20);
            snapshot.last_updated_at = past;
            snapshot.last_activity_at = Some(past);
            snapshot.needs = PetNeeds::new(20.0, 80.0, 80.0, 80.0);
            snapshot.behavior = PetBehavior::Wandering;
            snapshot.next_incident_at = Some(past - Duration::minutes(1));
        }
        FixtureMode::Default => {}
    }

    snapshot
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
