# Task 6 — vertical acceptance harness and verification evidence

## Objective

Prove the already-integrated Tasks 1–5 as one product path and close only
demonstrated cross-task defects. Produce the automated and documentary base
for the supervisor's final real-Codex/browser acceptance.

## Primary files

- Create `crates/codegotchi-cli/tests/full_vertical_flow.rs`.
- Modify `web/e2e/fixture.mjs`, `web/e2e/mvp.spec.ts`, and
  `web/playwright.config.ts` only as required to exercise the compiled
  production bundle rather than a Vite-served source build.
- Modify root/web package scripts only if needed for an explicit production
  Playwright command.
- Modify `.github/workflows/ci.yml` to install the pinned browser and run the
  essential production Playwright flow.
- Rewrite stale `docs/architecture.md` to describe the actual MVP.
- Update `README.md` only for exact final install/runtime/trust/demo/privacy
  behavior missing or inaccurate after Tasks 1–5.
- Create `docs/verification/codex-first-mvp.md` with every known environment,
  command, and automated result populated; mark genuinely pending supervisor
  manual observations as pending without claiming success.
- Update `docs/backlog/codex-first-mvp-followups.md` only for genuine new
  Backlog findings.
- Modify load-bearing Tasks 1–5 code only when a new vertical test is first
  observed failing for that concrete integration defect.
- Report to
  `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-6-report.md`.

## Consumed interfaces

- Exact launcher: `codegotchi run -- codex [arguments...]`, fake executable
  override `CODEGOTCHI_REAL_CODEX`, printed `CodeGotchi UI:` URL, inherited
  stdio, embedded assets, runtime metadata/profile lifecycle, repository-scoped
  SQLite restart persistence.
- Hook bridge: `CODEGOTCHI_SESSION_FILE`, installed-schema fixtures under
  `crates/codegotchi-cli/tests/fixtures/hooks`, valid fail-open/deny stdout.
- Backend: authenticated HTTP state/event/care/mode/debug routes and
  `/api/v1/stream` complete snapshots.
- UI: URL-fragment token, connection/activity projection, food-to-feed-target
  drag/drop, shovel→poop→trash cleanup, refresh/reconnect from authoritative
  snapshots.
- Strict/demo: `codegotchi mode strict`, guarded
  `CODEGOTCHI_ENABLE_DEBUG=1 codegotchi debug neglect|generate-poop`.

## Required test-first increments

1. Add one process-level restart flow using the real compiled CodeGotchi
   binary plus fake Codex. While the launcher is alive, discover its printed
   URL/runtime metadata, send real hook subprocess fixtures, consume a real
   WebSocket snapshot, perform authenticated feed, guarded poop generation and
   normal clean, verify a duplicate event/care ID does not reapply, exit, and
   relaunch in the same repository/state home. Assert the same pet identity and
   cared-for persisted state, with owned runtime/profile files removed between
   runs.
2. In the same test file add a process-level Strict flow: enable Strict,
   neglect, prove a recognized safe PreToolUse returns the exact installed
   denial shape and all required care/retry guidance, care using normal
   authenticated endpoints, retry with a fresh tool-use ID and prove allow,
   then stop the server and prove hook transport failure is `{}` fail-open.
3. Run both new tests red against current Tasks 1–5 before any production fix.
   If existing behavior already satisfies a slice, keep the test as integration
   proof; do not manufacture a code change.
4. Add production-bundle Playwright coverage. Build/embed first, launch a real
   CodeGotchi fixture server that serves its embedded SPA, and exercise initial
   room, authoritative activity transitions, feed/invalid drop, shovel cleanup,
   refresh persistence, disconnect/recovery, and backend error presentation.
   No test-only mutation endpoint may exist in normal production builds.
5. Add the production Playwright command to CI using the pinned package manager
   and Playwright browser installation. Keep existing lint/test/format/build
   gates.

## Mandatory acceptance checks

- New Rust vertical tests pass deterministically without invoking paid/real
  Codex or opening a browser.
- WebSocket assertion observes the initial complete snapshot and a later
  authoritative change.
- Restart proof compares concrete persisted identity, needs/inventory/poop,
  enforcement mode, and replay/idempotency state.
- Strict proof is deny → UI-equivalent normal care → fresh retry allow → server
  unavailable fail-open.
- No raw prompt, source content, full command, or complete output appears in
  serialized persisted state or test logs.
- Browser flow runs against production embedded bytes, not Vite HMR/source.
- Documentation matches actual paths, token handling, loopback-only bind,
  trust, cleanup, config preservation, guarded debug controls, and limitations.
- `docs/verification/codex-first-mvp.md` includes OS/WSL, Rust, Node, pnpm,
  Codex version, install/launch/gate commands, exact automated counts/results,
  and explicit placeholders only for the supervisor's pending real interactive
  observations.

## Required commands

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace`
- `corepack pnpm lint`
- `corepack pnpm test`
- `corepack pnpm format:check`
- `corepack pnpm build`
- `node web/scripts/embed-web.mjs`
- `corepack pnpm playwright:test` (with the documented WSL library path if
  required in this environment)

## Explicit exclusions

- Do not run a real/paid Codex session; the supervisor owns that final gate.
- Do not add another agent adapter, daemon, MCP, generic SDK, command rewriting,
  remote access, accounts, telemetry, or polished art.
- Do not redesign the domain, backend, hook, launcher, or UI.
- Do not add petting, optional animations, abstractions, or backlog hardening.
- Do not weaken authentication/debug guards or create production test-only
  fixture routes.
- Do not commit; hand the completed worktree and report back to the supervisor.
