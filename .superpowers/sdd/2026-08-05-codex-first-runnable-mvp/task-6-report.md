# Task 6 report — vertical acceptance harness and verification evidence

Date: 2026-08-05

## Result

Task 6 adds the compiled-binary vertical acceptance harness, switches the
browser fixture to the embedded production bundle, adds the CI production
Playwright command, and updates the architecture, README, verification
record, and this report. No load-bearing Tasks 1–5 code required a correction:
the new process flows passed against the integrated behavior.

## Test-first evidence

The new `full_vertical_flow.rs` was added before any production correction and
run with:

```text
cargo test -p codegotchi-cli --test full_vertical_flow -- --nocapture
```

The first red invocation was a harness-only compile failure (a local helper
variable shadow and unused import). Those test-file issues were corrected; a
second run passed both flows. No runtime/server/domain/launcher change was
made in response to the red run.

The focused green result is:

```text
running 2 tests
test strict_flow_denies_cares_retries_and_fails_open_when_server_stops ... ok
test launcher_vertical_flow_persists_and_replays_across_restart ... ok
test result: ok. 2 passed; 0 failed
```

The restart flow uses the compiled `CARGO_BIN_EXE_codegotchi`, the repository
fake Codex, an isolated HOME/CODEX_HOME/XDG state/runtime home, real hook
subprocess fixtures, including prompt, source/patch, command, and complete
tool-output cases, authenticated HTTP, and `tokio-tungstenite`. It checks
the printed URL/runtime metadata match, a complete initial WebSocket snapshot,
a later authoritative event snapshot, feed/invalid feed, guarded debug poop,
normal clean, complete-snapshot duplicate event/care no-op behavior, and
HTTP/SQLite privacy checks after those sensitive fixtures pass through the
launched hook process. It also checks runtime/profile cleanup and a concrete
restart projection of identity, needs, inventory, poop, enforcement mode,
work/digestion points, and replay sets.

The Strict flow checks the exact installed denial JSON and both care/retry
guidance clauses, a retry ID that is distinct and independently absent-before/
present-after recorded, normal authenticated feed recovery, and `{}` fail-open
after the launcher/server has stopped.

## Production browser harness

`web/e2e/fixture.mjs` no longer creates a Vite server. It starts the real Rust
`task3_fixture`, which serves `web-dist` through the same embedded asset table
as the CLI, then forwards fixed-port HTTP and WebSocket traffic to it. The
Playwright spec asserts no `/src/` or `/@vite` script, sends one ordinary
authenticated event to observe an authoritative activity transition, covers
feed/invalid drop, shovel cleanup, refresh persistence, backend error
presentation, and a real stream disconnect/reconnect. The reconnect test uses
Playwright's test-only `routeWebSocket` facility to pass the socket through to
the fixture, close the browser-side connection, and require a fresh connection
before accepting the replacement snapshot. It does not add a production test
mutation route or use the development disconnect seam.

The root and web package scripts now expose `playwright:test:production`;
that command builds and embeds before Playwright. CI verifies the pinned pnpm
version, installs Chromium with `--with-deps`, and runs the production command
after retaining all existing Rust/web gates.

## Gate results

- `cargo fmt --all -- --check`: PASS.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  PASS.
- `cargo test --workspace`: PASS — 110 passed, 0 failed, 1 intentionally
  ignored manual installed-Codex test; doc-tests had 0 tests.
- `corepack pnpm lint`: PASS.
- `corepack pnpm test`: PASS — 3 files, 29 tests.
- `corepack pnpm format:check`: PASS.
- `corepack pnpm build`: PASS.
- `node web/scripts/embed-web.mjs`: PASS.
- `LD_LIBRARY_PATH=/tmp/codegotchi-playwright-libs.uo9gm9/extracted/usr/lib/x86_64-linux-gnu corepack pnpm playwright:test`:
  PASS — 7 passed, 0 failed in 5.1 seconds against the embedded production
  bundle, including deterministic WebSocket disconnect/reconnect.

Environment details and pending manual observations are in
`docs/verification/codex-first-mvp.md`. No real or paid Codex session was run,
and no commit was made.
