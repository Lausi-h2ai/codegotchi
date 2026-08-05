# Task 3 independent review

Scope: `c0189e4..a4c00e6`. I read the Task 3 brief and report, the applicable
MVP plan constraints, the complete committed diff, the changed Rust/browser
interfaces, the Playwright fixture, and the generated bundle. I did not edit
production code, tests, plans, ledgers, or backlog files.

## Verification

- `git diff --check c0189e4..a4c00e6` — PASS.
- `corepack pnpm test` — PASS, 2 files and 23 tests.
- `corepack pnpm lint` — PASS.
- `corepack pnpm format:check` — PASS.
- `corepack pnpm build` — PASS.
- `cargo fmt --all -- --check` — PASS.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS.
- `cargo test --workspace` — PASS; all executed tests passed and the one
  pre-existing manual installed-Codex test remained ignored.
- The committed `web-dist` files are byte-identical to the current `web/dist`
  build (`cmp` passed), and the copied bundle contains none of
  `@vite/client`, `import.meta.hot`, `vite/dev`, or `__vite`. I did not run the
  embed script because it overwrites tracked generated output.
- The submitted Task 3 report records six Playwright tests passing with its
  scoped Chromium loader workaround. I could not independently reproduce that
  run in this review sandbox: the unscoped command stopped at the Chromium
  `libnspr4.so` loader failure, and the scoped command reached the real Rust
  fixture but its child server exited with
  `Bind(Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" })`
  before the browser tests. This is an environment limitation, not counted as
  a product finding; the in-process Rust WebSocket integration test passed.

The browser-auth boundary is otherwise correctly wired for the syntax-safe
fixture token: `web/src/client.ts:172` sends it as the WebSocket subprotocol,
`crates/codegotchi-cli/src/server.rs:207-237` accepts either bearer auth or the
matching protocol only for `/api/v1/stream`, and
`crates/codegotchi-cli/src/server.rs:297-300` negotiates the offered protocol.
The Rust integration test covers both legacy bearer and browser-subprotocol
connections. The fixture starts `cargo run --package codegotchi-cli
--example task3_fixture` and Vite proxies both `/api` HTTP and WebSocket
traffic to the real server (`web/e2e/fixture.mjs:17-49`); it does not expose a
browser mutation or command-execution route.

## MVP-blocking

### 1. Direct poop-to-trash drag bypasses the required shovel step

`web/src/App.tsx:78-93` accepts either the locally selected
`cleaningPoopId` or a raw `poop:<id>` drag payload. It only checks that the ID
is currently in `snapshot.pendingPoops`, then calls `disposePoop`, which sends
the authenticated clean action through `web/src/useCodeGotchi.ts:78-90`.
Therefore a user can drag an authoritative poop directly to Trash and remove
it without ever arming/applying the shovel. The required state machine is
shovel → poop → trash, and the direct path is a real persisted mutation, not
just a presentation issue.

The submitted component test (`web/src/App.test.tsx:197-219`) and Playwright
test (`web/e2e/mvp.spec.ts:70-87`) cover only the valid click sequence, so this
invalid disposal is currently untested.

### 2. Concurrent HTTP and WebSocket snapshots can regress the authoritative projection

`web/src/client.ts:106-112` starts the initial `GET /api/v1/state` and the
WebSocket concurrently. The HTTP callback unconditionally forwards its
snapshot at `web/src/client.ts:146-154`; every stream snapshot independently
forwards another value at `web/src/client.ts:189-197`. Care responses are also
forwarded directly by `web/src/useCodeGotchi.ts:68-70` and `83-86`. There is no
revision/request ordering guard using the snapshot's `lastUpdatedAt`, nor any
sequencing that prevents an older request from becoming the last writer.

For example, the state request can capture S0, the stream can deliver S1
after a feed/clean mutation, and the delayed state response can then replace
the UI with S0. Two care requests can produce the same regression if their
responses arrive out of order. Persistence remains backend-authoritative, but
the visible needs, inventory, and poops become stale until another snapshot,
which violates the required authoritative snapshot/reconnect projection.
The current client test exercises only ordered fake messages
(`web/src/client.test.ts:129-174`), and the Playwright suite does not create
this ordering.

## Backlog

- **Define a syntax-safe token contract.** `web/src/client.ts:59-73` parses a
  URLSearchParams token and `:172` passes the raw string as a WebSocket
  subprotocol, while `RuntimeMetadataV1.bearer_token` is an unconstrained
  `String` and the server parses comma-separated protocol values at
  `crates/codegotchi-cli/src/server.rs:214-228`. Constrain production tokens
  to UUID/base64url-safe syntax (or validate/encode them) and test URL and
  RFC6455 subprotocol edge cases.
- **Make the real-backend fixture repeatable after abrupt shutdown.**
  `crates/codegotchi-cli/examples/task3_fixture.rs:15-16` removes only the
  main SQLite file, its path is based only on the process ID at `:48-52`, and
  `web/e2e/fixture.mjs:60-61` terminates the child with SIGTERM. Use a unique
  path and clean the `-wal`/`-shm` sidecars or shut the server down gracefully.
- **Strengthen Playwright reconnect evidence.**
  `web/e2e/mvp.spec.ts:89-107` proves status recovery but does not change the
  backend while the stream is disconnected or assert a changed snapshot after
  reconnect. Add that assertion when extending the browser suite.

## Verdict

CORRECT MVP-BLOCKERS
