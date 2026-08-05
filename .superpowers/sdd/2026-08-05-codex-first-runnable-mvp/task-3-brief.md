# Task 3 — functional pet room, authenticated care, reconnect, Playwright

Model: `gpt-5.6-luna`; reasoning: `max`.

Base commit: `e0b7fcf`.

Implement the browser-facing vertical slice against the accepted Task 2
authoritative backend. Use TDD and keep the room functional rather than
polished.

## Relevant files

- Create `web/src/protocol.ts`, `web/src/client.ts`, `web/src/useCodeGotchi.ts`
- Modify `web/src/App.tsx`, `web/src/App.css`, `web/src/App.test.tsx`
- Create `web/src/client.test.ts`
- Create `web/e2e/mvp.spec.ts`, `web/playwright.config.ts`
- Create `web/scripts/embed-web.mjs`
- Modify `web/package.json`, root `package.json`, `pnpm-lock.yaml`
- Narrowly modify `crates/codegotchi-cli/src/server.rs` and its tests only as
  required for browser-compatible authenticated WebSocket setup
- You may add a development-only Rust example/fixture that starts the real
  Task 2 server and serves/proxies the built UI for Playwright. It must not add
  test-only mutation endpoints to the installed production binary.
- Generate `crates/codegotchi-cli/web-dist/` from `web/dist/`; never edit
  generated files manually.

## Consumed interfaces

- Fragment launch form: `http://127.0.0.1:<port>/#token=<token>`. Extract the
  token once and immediately remove the fragment with `history.replaceState`.
- Authenticated HTTP: bearer header for GET state and POST care.
- `POST /api/v1/care/feed`: `{actionId, foodId}`.
- `POST /api/v1/care/clean`: `{actionId, poopId}`.
- WebSocket `/api/v1/stream`: complete snapshot on connect and every accepted
  mutation. Browser APIs cannot set `Authorization`; add the narrowest safe
  browser auth mechanism, preferably a WebSocket subprotocol carrying the
  random token rather than a URL query parameter. Preserve existing bearer
  clients and loopback-only binding.
- `SimulationSnapshot` is camelCase and authoritative. Do not persist it to
  localStorage and do not calculate needs, inventory, or poop state locally.

## Required increment

- Render room, pet, desk/work area, food/inventory, feed target, shovel,
  authoritative poops, trash target, connection status, needs, and errors.
- Visibly distinguish idle, wandering/walking, sleeping, thinking,
  reading/searching, typing/editing, testing/building, generic work,
  celebrating, upset, eating, and refusing. Labels/icons/CSS poses are enough.
- Food drag/drop onto the pet/feed target sends one UUID care action; invalid
  drops send nothing. Display only returned/streamed authoritative changes.
- Cleaning is a simple shovel -> poop -> trash interaction. Send clean only at
  valid disposal, then render authoritative removal/cleanliness. Include
  keyboard-accessible equivalents.
- Reconnect WebSocket with a bounded retry and replace state from the complete
  reconnect snapshot. Show disconnected/reconnecting state and backend errors.
- Generate UUIDs once per user action so retry/replay remains idempotent.

## Mandatory tests

Write failing tests before implementation and record RED/GREEN evidence.

- Vitest: loading/disconnected/error states; complete snapshot; all mandatory
  activity presentations; token extraction/removal; bearer care headers;
  invalid drop; feed; shovel-poop-trash clean; reconnect and state replacement.
- Playwright using a real Task 2 server fixture: room load; valid feed and
  persisted result after reload; invalid feed drop; poop clean and persisted
  result after reload; disconnect/recovery; backend error presentation.
- The fixture may seed state through existing domain/persistence APIs, but it
  must not make browser state authoritative or expose arbitrary command
  execution.
- Ensure Playwright and a Chromium browser are installed for the focused suite.

Run:

```bash
corepack pnpm test
corepack pnpm lint
corepack pnpm format:check
corepack pnpm build
corepack pnpm playwright:test
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
node web/scripts/embed-web.mjs
```

Verify the copied bundle has no Vite/dev-server dependency. Write the report
to `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-3-report.md` with
changed files and exact results. Do not commit; the supervisor will review and
commit shared-worktree changes.

## Exclusions

No launcher/profile/runtime-metadata lifecycle (Task 5), strict decisions or
debug controls (Task 4), petting, polished art, browser simulation, arbitrary
fixture mutation routes, or unrelated foundation refactors.
