# Task 2 focused re-review

Scope: the four `MVP-blocking` findings in `task-2-review.md`, correction
commit `49ff122..4e09469`, and only the directly relevant hook, runtime,
server, protocol, and test paths. No source code or Git state was modified.

Focused verification:

- `cargo test --offline -p codegotchi-cli --test backend_integration` — 5 passed.
- `cargo test --offline -p codegotchi-cli --lib lagged_websocket_recovery_discards_retained_stale_snapshots` — 1 passed.
- `cargo test --offline -p codegotchi-cli --test websocket_integration` — 1 passed.

## Original-finding verification

1. **Task 1 hook event endpoint — resolved.** `EVENT_INGEST_PATH` is now
   `/api/v1/events` (`crates/codegotchi-cli/src/codex_hook.rs:19,274`), which
   matches the server route (`crates/codegotchi-cli/src/server.rs:139`). The
   production `codegotchi hook` binary is exercised against a live
   `RunningServer` by
   `backend_integration.rs:127-175`; the test observes the event in runtime
   memory and after SQLite reload, and verifies the replay response.

2. **WebSocket lag recovery ordering — resolved.** On `Lagged`,
   `next_authoritative_snapshot` obtains the current snapshot and a fresh
   receiver while the runtime simulation lock is held, replacing the receiver
   before continuing (`crates/codegotchi-cli/src/server.rs:273-285`,
   `crates/codegotchi-cli/src/runtime.rs:173-180`). The correction unit test
   publishes 33 snapshots against the capacity-32 channel, verifies that no
   retained stale snapshot follows recovery, and verifies the next mutation
   is delivered (`server.rs:441-488`).

3. **Maintenance-tick acceptance lifecycle — not fully resolved.** The new
   `maintenance_tick_at` API and integration test cover unchanged/changed
   runtime ticks, persistence, broadcast output, no extra broadcast, and a
   bounded server shutdown (`crates/codegotchi-cli/src/runtime.rs:158-170`,
   `backend_integration.rs:482-512`). However, that test calls the runtime
   method directly and then starts and immediately shuts down `RunningServer`;
   it never waits for or observes the one-second maintenance task in
   `server.rs:81-93`. A wiring regression that removed or bypassed the
   server-scheduled tick would still pass. The mandatory server maintenance
   acceptance behavior therefore remains unverified.

4. **Typed JSON for known-route method errors — resolved.** The router now
   installs `method_not_allowed_handler`, returning the standard
   `ErrorEnvelope` with status 405 (`crates/codegotchi-cli/src/server.rs:147-152,
   360-366`). The focused HTTP test verifies typed 405 responses for wrong
   methods on both `/api/v1/state` and `/api/v1/events`
   (`backend_integration.rs:362-373`).

## MVP-blocking

### Maintenance task is not exercised through the running server

The correction test validates `AuthoritativeRuntime::maintenance_tick_at`,
but does not validate that `RunningServer` invokes the tick on its one-second
interval before shutdown. This is a test-validity/acceptance failure for the
required maintenance lifecycle, not optional hardening.

## Backlog

No remaining Backlog findings.

NEEDS FIXES — 1 MVP-blocking, 0 Backlog.
