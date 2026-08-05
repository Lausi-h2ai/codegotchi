# Task 2 brief — authoritative backend and persistence

You are the sole primary implementer for Task 2 of the Codex-first runnable MVP. Work only in `/home/laurent/codegatchi` on branch `codex-first-mvp`, starting from commit `8667af0`. Read the task brief, the Task 2 section of `docs/superpowers/plans/2026-08-05-codex-first-runnable-mvp.md`, the existing domain public API/tests, and Task 1 protocol DTOs. Do not reread unrelated project history.

Use strict test-driven development: add focused failing tests, capture the RED evidence in the report, implement the smallest solution, then run focused and workspace gates. Preserve Phase 1–2 behavior and avoid unrelated refactoring.

## Exact files

- Modify `crates/codegotchi-domain/src/pet.rs`
- Modify `crates/codegotchi-domain/src/progression.rs`
- Modify `crates/codegotchi-domain/src/permission.rs`
- Add `crates/codegotchi-domain/tests/persistence_restore.rs`
- Add `crates/codegotchi-cli/src/persistence.rs`
- Add `crates/codegotchi-cli/src/runtime.rs`
- Add `crates/codegotchi-cli/src/server.rs`
- Add `crates/codegotchi-cli/tests/backend_integration.rs`
- Add `crates/codegotchi-cli/tests/websocket_integration.rs`
- Modify `crates/codegotchi-cli/src/lib.rs`, `main.rs`, `protocol.rs`, and `Cargo.toml` only as needed to expose this slice
- Write the implementation report to `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-2-report.md`

If Cargo.lock or workspace manifests must change because of dependencies, include only those necessary changes. Do not edit the frontend, launcher/profile generation, hook translation/classification, packaging, or unrelated docs.

## Consumed and produced interfaces

Consume Task 1 `EventIngestRequest`, canonical `AgentEvent`, strict-decision DTOs, and the existing `PetSimulation` transitions. Produce:

- a versioned serde `SimulationSnapshot` containing all deterministic continuation state, including replay IDs, needs, behavior/activity, poop, inventory, enforcement, and last-update time;
- `PetSimulation::from_snapshot` with typed rejection of unsupported/corrupt/invariant-violating data;
- `SqliteStore::{open, load_or_initialize, save}` with schema version 1 and transactional authority;
- `AuthoritativeRuntime` and `RunningServer` bound only to `127.0.0.1:0`;
- `GET /api/v1/health`, authenticated `GET /api/v1/state`, authenticated `POST /api/v1/events`, `POST /api/v1/care/feed`, `POST /api/v1/care/clean`, and authenticated `WS /api/v1/stream`;
- typed JSON error envelopes and bounded request bodies;
- complete snapshot on WebSocket connection and after authoritative mutations.

Every accepted mutation must persist before broadcast. Duplicate canonical event IDs and care action IDs must return success without applying a second transition. New pets receive exactly 50 kibble, 25 treats, and 25 fruit; restored pets are never reseeded.

Use bearer authentication for all state reads, mutations, and WebSocket access. The later browser will receive the token from the URL fragment. There must be no command-execution route. Bind loopback only. Keep raw prompts, source contents, shell commands, and complete outputs out of persistence.

Add a one-second maintenance tick that advances, persists, and broadcasts only when the snapshot changes and shuts down with the server.

## Mandatory acceptance tests

- Domain restore: complete round trip, snapshot-version rejection, replay-ID survival, deterministic continuation, and all existing Phase 2 tests unchanged.
- SQLite: initialization/reload, corrupt or unsupported snapshot typed errors, transactional/atomic save behavior, enforcement persistence, no inventory reseed, and restart idempotency.
- HTTP: loopback health; missing/wrong bearer rejected; bounded-body rejection; full state snapshot; valid and duplicate event ingestion; valid/invalid/duplicate feed and clean; typed errors; no arbitrary command route.
- WebSocket: authenticated initial snapshot, mutation broadcast, disconnect/reconnect, authoritative reconnect snapshot.
- Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace`.

Commit the implementation and report in intentional commits, leave the worktree clean, and end with commit hashes, exact tests/results, changed files, interface notes for Task 3/4, and any genuinely deferred findings. Do not implement backlog polish.
