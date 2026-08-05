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

The workspace test run passed the 3 backend integration tests, 11 hook
fixture tests, 1 non-ignored installed-Codex test, 3 profile tests, 1
WebSocket integration test, 18 domain unit tests, 13 care-flow tests, 2 event
replay tests, 5 permission-matrix tests, 4 restore tests, 3 Task 1 contract
tests, 8 Task 2 contract tests, and 11 Task 2 correction tests. One earlier
parallel workspace invocation hit a transient loopback bind `EPERM`; the
focused HTTP test passed immediately in isolation and the complete workspace
command was rerun successfully.

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
- `crates/codegotchi-cli/src/lib.rs`, `protocol.rs`, `persistence.rs`,
  `runtime.rs`, `server.rs`
- `crates/codegotchi-cli/tests/backend_integration.rs`,
  `websocket_integration.rs`
- `crates/codegotchi-domain/src/lib.rs`, `permission.rs`, `pet.rs`,
  `progression.rs`
- `crates/codegotchi-domain/tests/persistence_restore.rs`

## Commit and worktree status

The implementation and report are ready, but the execution sandbox mounts
`.git` read-only. `git add ...` fails before creating an index lock with:

~~~text
fatal: Unable to create '/home/laurent/codegatchi/.git/index.lock': Read-only file system
~~~

No stale `index.lock` exists. Consequently no commit hash can honestly be
reported and the worktree cannot be made clean from this session. The source
worktree changes are preserved for the supervisor to commit once Git metadata
write access is restored.

## Deferred findings

No Task 2 backend requirement is deferred. CLI runtime startup/launcher,
frontend consumption, and strict decision/debug controls remain intentionally
owned by Tasks 3–5 and are not implemented here.
