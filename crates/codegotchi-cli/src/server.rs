use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::extract::{
    FromRequest, Request, State, WebSocketUpgrade,
    ws::{Message, WebSocket},
};
use axum::http::HeaderMap;
use axum::http::{StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use codegotchi_domain::{
    AgentEventError, AgentEventKind, CareError, EnforcementMode, WorkDecision, WorkReasonCode,
};
use futures_util::StreamExt;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use crate::assets;
use crate::persistence::PersistenceError;
use crate::protocol::{
    CleanRequest, DebugRequest, ErrorEnvelope, EventIngestRequest, EventIngestResponse,
    FeedRequest, HealthResponse, ModeRequest, NapRequest, SnapshotMutationResponse,
};
use crate::runtime::{AuthoritativeRuntime, MutationReceipt, RuntimeError};

pub const MAX_REQUEST_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("could not bind the loopback server: {0}")]
    Bind(#[source] std::io::Error),
    #[error("server task failed: {0}")]
    Task(#[source] tokio::task::JoinError),
    #[error("server returned an error while shutting down: {0}")]
    Serve(#[source] std::io::Error),
}

#[derive(Clone)]
struct AppState {
    runtime: Arc<AuthoritativeRuntime>,
    bearer_token: Arc<str>,
}

pub struct RunningServer {
    address: SocketAddr,
    bearer_token: Arc<str>,
    shutdown: broadcast::Sender<()>,
    server_task: Option<JoinHandle<Result<(), std::io::Error>>>,
    maintenance_task: Option<JoinHandle<()>>,
}

enum MaintenanceSchedule {
    Interval,
    Trigger(mpsc::UnboundedReceiver<()>),
}

impl RunningServer {
    pub async fn start(
        runtime: Arc<AuthoritativeRuntime>,
        bearer_token: impl Into<String>,
    ) -> Result<Self, ServerError> {
        Self::start_with_schedule(runtime, bearer_token.into(), MaintenanceSchedule::Interval).await
    }

    #[doc(hidden)]
    pub async fn start_with_maintenance_trigger(
        runtime: Arc<AuthoritativeRuntime>,
        bearer_token: impl Into<String>,
        ticks: mpsc::UnboundedReceiver<()>,
    ) -> Result<Self, ServerError> {
        Self::start_with_schedule(
            runtime,
            bearer_token.into(),
            MaintenanceSchedule::Trigger(ticks),
        )
        .await
    }

    async fn start_with_schedule(
        runtime: Arc<AuthoritativeRuntime>,
        bearer_token_value: String,
        maintenance_schedule: MaintenanceSchedule,
    ) -> Result<Self, ServerError> {
        let bearer_token: Arc<str> = Arc::from(bearer_token_value);
        let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
            .await
            .map_err(ServerError::Bind)?;
        let address = listener.local_addr().map_err(ServerError::Bind)?;
        let state = AppState {
            runtime: Arc::clone(&runtime),
            bearer_token: Arc::clone(&bearer_token),
        };
        let app = router(state);
        let (shutdown, _) = broadcast::channel(2);
        let mut server_shutdown = shutdown.subscribe();
        let server_task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = server_shutdown.recv().await;
                })
                .await
        });

        let maintenance_shutdown = shutdown.subscribe();
        let maintenance_task = tokio::spawn(run_maintenance(
            runtime,
            maintenance_shutdown,
            maintenance_schedule,
        ));

        Ok(Self {
            address,
            bearer_token,
            shutdown,
            server_task: Some(server_task),
            maintenance_task: Some(maintenance_task),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    pub fn bearer_token(&self) -> &str {
        &self.bearer_token
    }

    pub async fn shutdown(mut self) -> Result<(), ServerError> {
        let _ = self.shutdown.send(());
        if let Some(task) = self.maintenance_task.take() {
            task.await.map_err(ServerError::Task)?;
        }
        if let Some(task) = self.server_task.take() {
            task.await
                .map_err(ServerError::Task)?
                .map_err(ServerError::Serve)?;
        }
        Ok(())
    }
}

async fn run_maintenance(
    runtime: Arc<AuthoritativeRuntime>,
    mut shutdown: broadcast::Receiver<()>,
    schedule: MaintenanceSchedule,
) {
    match schedule {
        MaintenanceSchedule::Interval => {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.tick().await;
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let _ = runtime.maintenance_tick();
                    }
                    _ = shutdown.recv() => break,
                }
            }
        }
        MaintenanceSchedule::Trigger(mut ticks) => loop {
            tokio::select! {
                tick = ticks.recv() => {
                    if tick.is_none() {
                        break;
                    }
                    let _ = runtime.maintenance_tick();
                }
                _ = shutdown.recv() => break,
            }
        },
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
    }
}

fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/api/v1/state", get(state_handler))
        .route("/api/v1/events", post(events_handler))
        .route("/api/v1/mode", post(mode_handler))
        .route("/api/v1/debug/neglect", post(debug_neglect_handler))
        .route(
            "/api/v1/debug/generate-poop",
            post(debug_generate_poop_handler),
        )
        .route("/api/v1/care/feed", post(feed_handler))
        .route("/api/v1/care/clean", post(clean_handler))
        .route("/api/v1/care/nap", post(nap_handler))
        .route("/api/v1/stream", get(stream_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_bearer,
        ));
    Router::new()
        .route("/api/v1/health", get(health_handler))
        .merge(protected)
        .fallback(static_or_api_not_found_handler)
        .method_not_allowed_fallback(method_not_allowed_handler)
        .with_state(state)
}

async fn require_bearer(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let bearer_authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| constant_time_equal(token.as_bytes(), state.bearer_token.as_bytes()));
    let browser_websocket_authorized = request.uri().path() == "/api/v1/stream"
        && request
            .headers()
            .get_all(header::SEC_WEBSOCKET_PROTOCOL)
            .iter()
            .any(|value| {
                value.to_str().ok().is_some_and(|protocols| {
                    protocols.split(',').any(|protocol| {
                        constant_time_equal(
                            protocol.trim().as_bytes(),
                            state.bearer_token.as_bytes(),
                        )
                    })
                })
            });
    let authorized = bearer_authorized || browser_websocket_authorized;
    if !authorized {
        return error_response(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "a valid bearer token is required",
        );
    }
    next.run(request).await
}

async fn health_handler() -> impl IntoResponse {
    Json(HealthResponse { status: "ok" })
}

async fn state_handler(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.runtime.snapshot())
}

async fn events_handler(
    State(state): State<AppState>,
    BoundedJson(request): BoundedJson<EventIngestRequest>,
) -> Response {
    let classification = (request.event.kind == AgentEventKind::ToolStarted)
        .then(|| {
            request
                .permission
                .as_ref()
                .and_then(crate::protocol::PermissionContext::classification)
        })
        .flatten();
    match state.runtime.ingest_event(&request.event, classification) {
        Ok(receipt) => {
            let mode = receipt.snapshot.enforcement_mode;
            let blocked = receipt.decision.is_blocked();
            Json(EventIngestResponse {
                accepted: true,
                evaluated: true,
                enforcement_mode: Some(enforcement_mode_id(mode).to_owned()),
                strict: mode == EnforcementMode::Strict,
                blocked,
                decision: Some(serde_json::json!(if blocked { "deny" } else { "allow" })),
                reason: denial_reason(receipt.decision),
                duplicate: receipt.duplicate,
            })
            .into_response()
        }
        Err(error) => runtime_error_response(error),
    }
}

async fn mode_handler(
    State(state): State<AppState>,
    BoundedJson(request): BoundedJson<ModeRequest>,
) -> Response {
    match state.runtime.set_enforcement_mode(request.mode) {
        Ok(receipt) => mutation_response(receipt),
        Err(error) => runtime_error_response(error),
    }
}

async fn debug_neglect_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    BoundedJson(_request): BoundedJson<DebugRequest>,
) -> Response {
    if !debug_header_is_present(&headers) {
        return error_response(
            StatusCode::FORBIDDEN,
            "debug_disabled",
            "debug commands require CODEGOTCHI_ENABLE_DEBUG=1",
        );
    }
    match state.runtime.debug_neglect() {
        Ok(receipt) => mutation_response(receipt),
        Err(error) => runtime_error_response(error),
    }
}

async fn debug_generate_poop_handler(
    headers: HeaderMap,
    State(state): State<AppState>,
    BoundedJson(_request): BoundedJson<DebugRequest>,
) -> Response {
    if !debug_header_is_present(&headers) {
        return error_response(
            StatusCode::FORBIDDEN,
            "debug_disabled",
            "debug commands require CODEGOTCHI_ENABLE_DEBUG=1",
        );
    }
    match state.runtime.debug_generate_poop() {
        Ok(receipt) => mutation_response(receipt),
        Err(error) => runtime_error_response(error),
    }
}

async fn feed_handler(
    State(state): State<AppState>,
    BoundedJson(request): BoundedJson<FeedRequest>,
) -> Response {
    match state.runtime.feed(request.action_id, request.food_id) {
        Ok(receipt) => mutation_response(receipt),
        Err(error) => runtime_error_response(error),
    }
}

async fn clean_handler(
    State(state): State<AppState>,
    BoundedJson(request): BoundedJson<CleanRequest>,
) -> Response {
    match state.runtime.clean(request.action_id, request.poop_id) {
        Ok(receipt) => mutation_response(receipt),
        Err(error) => runtime_error_response(error),
    }
}

async fn nap_handler(
    State(state): State<AppState>,
    BoundedJson(request): BoundedJson<NapRequest>,
) -> Response {
    match state.runtime.nap(request.action_id) {
        Ok(receipt) => mutation_response(receipt),
        Err(error) => runtime_error_response(error),
    }
}

async fn stream_handler(State(state): State<AppState>, websocket: WebSocketUpgrade) -> Response {
    let subscription = match state.runtime.subscribe() {
        Ok(subscription) => subscription,
        Err(error) => return runtime_error_response(error),
    };
    let runtime = Arc::clone(&state.runtime);
    websocket
        .protocols([state.bearer_token.to_string()])
        .on_upgrade(move |socket| websocket_session(socket, subscription, runtime))
        .into_response()
}

async fn websocket_session(
    mut socket: WebSocket,
    (initial, mut snapshots): (
        codegotchi_domain::SimulationSnapshot,
        broadcast::Receiver<codegotchi_domain::SimulationSnapshot>,
    ),
    runtime: Arc<AuthoritativeRuntime>,
) {
    if send_snapshot(&mut socket, initial).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            message = next_authoritative_snapshot(&runtime, &mut snapshots) => {
                match message {
                    Some(snapshot) => {
                        if send_snapshot(&mut socket, snapshot).await.is_err() {
                            return;
                        }
                    }
                    None => return,
                }
            }
            incoming = socket.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => return,
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            return;
                        }
                    }
                    Some(Ok(Message::Pong(_))) | Some(Ok(Message::Text(_))) | Some(Ok(Message::Binary(_))) => {}
                    Some(Err(_)) => return,
                }
            }
        }
    }
}

async fn next_authoritative_snapshot(
    runtime: &Arc<AuthoritativeRuntime>,
    snapshots: &mut broadcast::Receiver<codegotchi_domain::SimulationSnapshot>,
) -> Option<codegotchi_domain::SimulationSnapshot> {
    match snapshots.recv().await {
        Ok(snapshot) => Some(snapshot),
        Err(broadcast::error::RecvError::Lagged(_)) => {
            let (snapshot, fresh_receiver) = runtime.subscribe().ok()?;
            *snapshots = fresh_receiver;
            Some(snapshot)
        }
        Err(broadcast::error::RecvError::Closed) => None,
    }
}

async fn send_snapshot(
    socket: &mut WebSocket,
    snapshot: codegotchi_domain::SimulationSnapshot,
) -> Result<(), axum::Error> {
    let payload = serde_json::to_string(&snapshot).map_err(axum::Error::new)?;
    socket.send(Message::Text(payload.into())).await
}

fn mutation_response(receipt: MutationReceipt) -> Response {
    Json(SnapshotMutationResponse {
        snapshot: receipt.snapshot,
        duplicate: receipt.duplicate,
    })
    .into_response()
}

fn runtime_error_response(error: RuntimeError) -> Response {
    match error {
        RuntimeError::Care(error) => {
            let (code, status) = match error {
                CareError::UnknownFood(_) => ("unknown_food", StatusCode::UNPROCESSABLE_ENTITY),
                CareError::OutOfStock(_) => ("out_of_stock", StatusCode::UNPROCESSABLE_ENTITY),
                CareError::MissingPoop(_) => ("missing_poop", StatusCode::UNPROCESSABLE_ENTITY),
                CareError::InsufficientDuration => {
                    ("insufficient_duration", StatusCode::UNPROCESSABLE_ENTITY)
                }
                CareError::NonFinitePointerDistance => (
                    "non_finite_pointer_distance",
                    StatusCode::UNPROCESSABLE_ENTITY,
                ),
                CareError::InsufficientDistance => {
                    ("insufficient_distance", StatusCode::UNPROCESSABLE_ENTITY)
                }
                CareError::UnsupportedCondition => {
                    ("unsupported_condition", StatusCode::UNPROCESSABLE_ENTITY)
                }
            };
            error_response(status, code, error.to_string())
        }
        RuntimeError::Event(AgentEventError::UnsupportedSchemaVersion(version)) => error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_event_schema",
            format!("unsupported agent event schema version {version}"),
        ),
        RuntimeError::Persistence(error) => persistence_error_response(error),
        RuntimeError::Restore(error) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid_snapshot",
            error.to_string(),
        ),
        RuntimeError::LockPoisoned => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "runtime_unavailable",
            "the authoritative runtime lock is unavailable",
        ),
    }
}

fn persistence_error_response(error: PersistenceError) -> Response {
    let status = match error {
        PersistenceError::UnsupportedSchemaVersion(_)
        | PersistenceError::CorruptSnapshot(_)
        | PersistenceError::InvalidSnapshot(_) => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    error_response(status, "persistence_error", error.to_string())
}

async fn static_or_api_not_found_handler(uri: Uri) -> Response {
    if uri.path() == "/api/v1" || uri.path().starts_with("/api/v1/") {
        return error_response(StatusCode::NOT_FOUND, "not_found", "route not found");
    }

    let asset = if uri.path() == "/" {
        assets::index()
    } else {
        assets::find(uri.path()).unwrap_or_else(assets::index)
    };
    let mut response = Response::new(Body::from(asset.bytes.to_vec()));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static(asset.content_type),
    );
    response
}

async fn method_not_allowed_handler() -> Response {
    error_response(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "method not allowed for this route",
    )
}

fn error_response(status: StatusCode, code: &'static str, message: impl Into<String>) -> Response {
    (status, Json(ErrorEnvelope::new(code, message))).into_response()
}

fn enforcement_mode_id(mode: EnforcementMode) -> &'static str {
    match mode {
        EnforcementMode::Decorative => "decorative",
        EnforcementMode::Gentle => "gentle",
        EnforcementMode::Strict => "strict",
    }
}

fn denial_reason(decision: WorkDecision) -> Option<String> {
    let WorkDecision::Blocked { reason_code, .. } = decision else {
        return None;
    };
    let reason = match reason_code {
        WorkReasonCode::CriticalHunger => {
            "The pet refuses this action because its hunger is critical. Feed the pet in the CodeGotchi UI, then retry the Codex request afterward."
        }
        WorkReasonCode::CriticalEnergy => {
            "The pet refuses this action because it is too exhausted to keep working. Give the pet time to rest, then retry the Codex request afterward."
        }
        WorkReasonCode::CriticalCleanliness => {
            "The pet refuses this action because its cleanliness is critical. Clean the pet in the CodeGotchi UI, then retry the Codex request afterward."
        }
    };
    Some(reason.to_owned())
}

fn debug_header_is_present(headers: &HeaderMap) -> bool {
    headers
        .get("x-codegotchi-debug")
        .and_then(|value| value.to_str().ok())
        == Some("1")
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= usize::from(left.get(index).copied().unwrap_or_default())
            ^ usize::from(right.get(index).copied().unwrap_or_default());
    }
    difference == 0
}

struct BoundedJson<T>(T);

impl<S, T> FromRequest<S> for BoundedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = Response;

    async fn from_request(request: Request, _state: &S) -> Result<Self, Self::Rejection> {
        if request
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|length| length > MAX_REQUEST_BODY_BYTES)
        {
            return Err(error_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "body_too_large",
                "request body exceeds the 64 KiB limit",
            ));
        }
        let body = to_bytes(request.into_body(), MAX_REQUEST_BODY_BYTES)
            .await
            .map_err(|_| {
                error_response(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "body_too_large",
                    "request body exceeds the 64 KiB limit",
                )
            })?;
        serde_json::from_slice(&body)
            .map(BoundedJson)
            .map_err(|error| {
                error_response(StatusCode::BAD_REQUEST, "invalid_json", error.to_string())
            })
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use codegotchi_domain::{
        AgentEvent, AgentEventKind, EventMetadata, EventSource, Pet, PetSpecies,
    };
    use tokio::sync::broadcast::error::TryRecvError;
    use uuid::Uuid;

    use super::{AuthoritativeRuntime, next_authoritative_snapshot};
    use crate::persistence::SqliteStore;

    #[tokio::test]
    async fn lagged_websocket_recovery_discards_retained_stale_snapshots() {
        let start = Utc.with_ymd_and_hms(2026, 8, 5, 12, 0, 0).unwrap();
        let runtime = AuthoritativeRuntime::new(
            SqliteStore::open(":memory:").unwrap(),
            Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, start),
        )
        .unwrap();
        let (_, mut snapshots) = runtime.subscribe().unwrap();

        for id in 1..=33 {
            let event = AgentEvent::new(
                Uuid::from_u128(id),
                Uuid::from_u128(7),
                "repo",
                EventSource::Codex,
                AgentEventKind::TurnStarted,
                None,
                start,
                EventMetadata::default(),
            );
            runtime.apply_event(&event).unwrap();
        }

        let recovered = next_authoritative_snapshot(&runtime, &mut snapshots)
            .await
            .unwrap();
        assert_eq!(recovered, runtime.snapshot());
        assert!(matches!(snapshots.try_recv(), Err(TryRecvError::Empty)));

        let next_event = AgentEvent::new(
            Uuid::from_u128(34),
            Uuid::from_u128(7),
            "repo",
            EventSource::Codex,
            AgentEventKind::TurnStarted,
            None,
            start,
            EventMetadata::default(),
        );
        let next = runtime.apply_event(&next_event).unwrap().snapshot;
        assert_eq!(
            next_authoritative_snapshot(&runtime, &mut snapshots)
                .await
                .unwrap(),
            next
        );
    }
}
