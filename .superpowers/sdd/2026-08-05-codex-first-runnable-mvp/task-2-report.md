# Task 2 implementation report: authoritative backend and persistence

Date: 2026-08-05  
Branch: codex-first-mvp  
Scope: versioned domain restore, in-process SQLite authority, authenticated
Axum/Tokio HTTP and WebSocket backend

## Outcome

Task 2's backend slice is implemented in the requested files. The domain now
serializes a complete schema-v1 continuation snapshot and reconstructs private
aggregate state through typed validation. `SqliteStore` uses an in-process
rusqlite 0.37 bundled connection, WAL/FULL synchronous settings, and
transactions. `AuthoritativeRuntime` persists before broadcasting and retains
event/care replay IDs across restart. `RunningServer` binds only to
`127.0.0.1:0`, serves the authenticated state/event/care/WebSocket routes, and
runs a one-second maintenance task that stops with the server.

No Python subprocess, Python runtime dependency, handwritten HTTP parser, or
handwritten WebSocket implementation remains in the Task 2 backend.

The focused post-review correction pass also aligns the production Task 1 hook
with `/api/v1/events`, consumes the server's length-delimited or chunked HTTP
response without waiting for connection EOF, resubscribes WebSocket lagged
receivers before sending recovery state, and returns typed JSON for known-route
405 responses.

## Red-green record

Tests were added before the corresponding restore/runtime/server APIs existed.

### RED: domain restore

Command:

~~~text
cargo test -p codegotchi-domain --test persistence_restore
~~~

Result: exit 101. The compiler reported the missing
`SnapshotRestoreError`, `PetSimulation::from_snapshot`, snapshot serde
implementation, schema field, and enforcement setter.

### RED: backend HTTP and WebSocket contracts

Commands:

~~~text
cargo test -p codegotchi-cli --test backend_integration
cargo test -p codegotchi-cli --test websocket_integration
~~~

Results: both exited 101. The compiler reported the missing
`AuthoritativeRuntime`, `SqliteStore`, and `RunningServer` exports plus the
not-yet-added Tokio, futures, and WebSocket test dependencies.

## Focused GREEN evidence

~~~text
cargo test --offline -p codegotchi-domain --test persistence_restore
4 passed; 0 failed

cargo test --offline -p codegotchi-cli --test backend_integration
3 passed; 0 failed

cargo test --offline -p codegotchi-cli --test websocket_integration
1 passed; 0 failed
~~~

The domain tests cover complete JSON round-trip including pending poop,
inventory, activity, enforcement, timestamps, and replay IDs; unsupported
versions and invariant violations; duplicate replay after restore; and
deterministic continuation.

The backend tests cover SQLite initialization/reload, corrupt and unsupported
rows, transactional rollback, enforcement persistence, exact new-pet
inventory seeding, no restart reseed, duplicate event/care IDs, authenticated
and unauthenticated HTTP, bounded bodies, typed errors, loopback binding, the
absence of a command route, authenticated WebSocket snapshots, mutation
broadcast, disconnect/reconnect, and authoritative reconnect state.

## Focused post-review correction evidence

The correction pass started from `49ff122` and was limited to the four findings
in `task-2-review.md`. Tests were extended before the corresponding fixes.

### RED

~~~text
cargo test --offline -p codegotchi-cli --test backend_integration
exit 101: missing send_event_to_runtime and maintenance_tick_at

cargo test --offline -p codegotchi-cli --lib lagged_websocket_recovery_discards_retained_stale_snapshots
exit 101: missing next_authoritative_snapshot
~~~

### GREEN

~~~text
cargo test --offline -p codegotchi-cli --test backend_integration
5 passed; 0 failed

cargo test --offline -p codegotchi-cli --lib lagged_websocket_recovery_discards_retained_stale_snapshots
1 passed; 0 failed

cargo test --offline -p codegotchi-cli --test websocket_integration
1 passed; 0 failed
~~~

The hook test launches the production `codegotchi hook` binary with runtime
metadata, observes the translated canonical event in the live runtime and a
fresh SQLite reload, then sends the replay through the same production
transport and asserts the parsed accepted/duplicate response. The response
reader now stops at HTTP framing instead of waiting for EOF.

The lag test publishes 33 snapshots against the capacity-32 channel, verifies
recovery returns the current authoritative snapshot, verifies no retained stale
snapshot follows it, and verifies the next mutation is delivered on the fresh
subscription. The maintenance test covers deterministic unchanged/changed
ticks, persisted state observed with the broadcast snapshot, no extra
broadcast, and bounded server shutdown completion. HTTP tests cover typed 405
JSON for wrong methods on `/api/v1/state` and `/api/v1/events`.

## Repository gates

Commands and results:

~~~text
cargo fmt --all -- --check
exit 0

cargo clippy --offline --workspace --all-targets --all-features -- -D warnings
exit 0

cargo test --offline --workspace
exit 0
~~~

The workspace test run passed the 1 CLI unit test, 5 backend integration
tests, 11 hook fixture tests, 1 non-ignored installed-Codex test, 3 profile
tests, 1 WebSocket integration test, 18 domain unit tests, 13 care-flow tests,
2 event replay tests, 5 permission-matrix tests, 4 restore tests, 3 Task 1
contract tests, 8 Task 2 contract tests, and 11 Task 2 correction tests.

`--offline` was required because crates.io DNS was unavailable. The requested
cached versions resolved as Axum 0.8.9, rusqlite 0.37.0 with bundled SQLite,
Tokio 1.53.1, futures-util 0.3.33, and direct test dependency
tokio-tungstenite 0.27.0.

## Interface notes for Task 3

- `GET /api/v1/health` is unauthenticated and returns `{"status":"ok"}`.
- State reads, event/care mutations, and `/api/v1/stream` require
  `Authorization: Bearer <token>`.
- `GET /api/v1/state` returns the complete camelCase `SimulationSnapshot`.
- `POST /api/v1/events` consumes the Task 1 `EventIngestRequest` and returns
  the tolerant event response with `accepted`, `evaluated`, `duplicate`, and
  enforcement-mode fields.
- Feed JSON is `{ "actionId": "...", "foodId": "kibble" }`; clean JSON is
  `{ "actionId": "...", "poopId": "..." }`. Successful care responses are
  the complete authoritative snapshot with a `duplicate` field.
- WebSocket connection sends one complete snapshot immediately and every
  persisted accepted mutation thereafter. Reconnect starts with the current
  authoritative snapshot; browser state need not be replayed locally.
- Snapshot fields include needs, behavior/activity, outcomes, poops,
  inventory, session activity, event/care replay IDs, enforcement mode, and
  logical update timestamps.

## Interface notes for Task 4

`AuthoritativeRuntime::enforcement_mode` and
`set_enforcement_mode` persist the domain enforcement setting. Event ingestion
currently records and accepts canonical events with `evaluated: false`; Task 4
can add structured classification and `WorkPermissionPolicy` evaluation at
that authenticated mutation boundary without changing persistence or the
WebSocket contract. No command-execution route was added.

## Changed files

- `Cargo.toml`, `Cargo.lock`
- `crates/codegotchi-cli/Cargo.toml`
- `crates/codegotchi-cli/src/codex_hook.rs`, `lib.rs`, `protocol.rs`,
  `persistence.rs`, `runtime.rs`, `server.rs`
- `crates/codegotchi-cli/tests/backend_integration.rs`,
  `websocket_integration.rs`
- `crates/codegotchi-domain/src/lib.rs`, `permission.rs`, `pet.rs`,
  `progression.rs`
- `crates/codegotchi-domain/tests/persistence_restore.rs`

Correction-pass files:

- `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-2-report.md`
- `crates/codegotchi-cli/src/codex_hook.rs`, `lib.rs`, `runtime.rs`,
  `server.rs`
- `crates/codegotchi-cli/tests/backend_integration.rs`
- `crates/codegotchi-domain/src/progression.rs`

## Commit and worktree status

The correction commit was attempted after the final gates, but Git metadata is
read-only in this execution sandbox:

~~~text
git add .superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-2-report.md crates/codegotchi-cli/src/codex_hook.rs crates/codegotchi-cli/src/lib.rs crates/codegotchi-cli/src/runtime.rs crates/codegotchi-cli/src/server.rs crates/codegotchi-cli/tests/backend_integration.rs crates/codegotchi-domain/src/progression.rs
fatal: Unable to create '/home/laurent/codegatchi/.git/index.lock': Read-only file system
~~~

No stale `.git/index.lock` exists. No correction commit hash can honestly be
reported, and the seven changed files listed above remain unstaged in the
worktree for the supervisor to commit when Git metadata write access is
restored.

## Deferred findings

No Task 2 backend requirement is deferred. CLI runtime startup/launcher,
frontend consumption, and strict decision/debug controls remain intentionally
owned by Tasks 3–5 and are not implemented here.
