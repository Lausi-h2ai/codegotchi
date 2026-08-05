# Codex-First Runnable MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver an installable `codegotchi` binary whose exact `codegotchi run -- codex` command launches real interactive Codex and a persistent, authoritative, usable local pet room.

**Architecture:** One new Rust CLI crate hosts an in-process loopback Axum server, SQLite snapshot persistence, the embedded Vite bundle, the hook bridge, and the real Codex child. The existing domain remains framework-free and gains only a versioned serialization/restore seam; the React client remains a projection and submits authenticated idempotent care actions.

**Tech Stack:** Rust 2024, Tokio, Axum, rusqlite, serde/serde_json, uuid, clap, reqwest/ureq, include_dir, React 19, TypeScript 5.9, Vite 8, Vitest, Testing Library, Playwright, Codex CLI 0.146.0.

## Global Constraints

- The exact user command is `codegotchi run -- codex`; all trailing Codex arguments are preserved.
- Use `gpt-5.6-luna` with reasoning effort `max` for every implementation, investigation, review, and correction subagent.
- Run only one implementation subagent at a time and one scoped reviewer per task.
- Per task: one implementation, one review, at most one focused correction and re-review unless a mandatory test, compile, security/data-loss, or real-integration failure remains.
- Perform exactly one broad final Luna/max review and at most one final focused correction pass.
- The backend is authoritative; browser local/session storage may hold only the ephemeral runtime token and cosmetic client state.
- Bind only to `127.0.0.1`; authenticate every state-changing request; bound payloads; return typed errors.
- Keep default enforcement `Decorative`; Strict is explicit and blocks only classified safe development operations.
- Hook/backend failure is fail-open except after a successful Strict denial evaluation.
- Never persist raw prompts, raw command strings, source contents, transcript contents, or complete tool output.
- Never modify or copy Codex credentials or overwrite existing Codex configuration/profile files.
- Preserve and compose with existing hooks; honor Codex's hook trust flow and never bypass it.
- Keep the normal installed UI self-contained; `pnpm dev` is not part of the user launch path.
- Follow red-green-refactor and record the failing and passing command in each task report.
- Stop adding product scope as soon as every mandatory acceptance criterion passes.

## Current repository findings

- Repository root: `/home/laurent/codegatchi`.
- The checkout initially had no commits on `master`; all 44 foundation files were untracked. They are preserved verbatim in baseline commit `488c9a1` and MVP work runs on `codex-first-mvp`.
- No original specification beyond the supplied milestone prompt, no task reports, no review reports, no `AGENTS.md`, and no persistent progress ledger were present.
- The dated Phase 1-2 design, implementation plan, ADR 0001, architecture, backlog, README, CI, source, and tests are mutually consistent.
- Baseline Rust gates pass: 60 tests, format clean, clippy clean.
- Baseline frontend gates pass through `corepack pnpm`: one Vitest test, lint, format check, and production build.
- Bare `pnpm` is absent from `PATH`; the repository intentionally pins and documents `corepack pnpm@11.20.0`.
- Installed platform is WSL2 Linux with Rust 1.97.1, Node 24.15.0, and Codex CLI 0.146.0.
- `codex --profile --help` correctly errors because `--profile` requires a name; top-level help documents layered `$CODEX_HOME/<name>.config.toml` profiles.
- Current official Codex documentation confirms stable hooks, additive matching hooks, explicit non-managed hook trust, supported lifecycle/tool events, `Bash`/`apply_patch` coverage, and the current denial output shape.

## Gap analysis

### A. Reuse unchanged

- Pure `codegotchi-domain` activity/event vocabulary, deterministic progression, poop thresholds, care validation/idempotency, behavior coordinator, clock ports, and work-permission policy.
- Rust/TypeScript workspace split, Rust lint policy, pinned pnpm/Corepack toolchain, Vite/React foundation, CI shape, and local documentation conventions.
- Backend-authority, privacy, fail-open, deterministic simulation, and dependency-boundary decisions in the Phase 1-2 design and ADR 0001.

### B. Modify for integration

- Add serde representations and a validated restore constructor to the existing domain snapshot/value types because restart persistence cannot otherwise restore private aggregate state.
- Replace the static React room with a backend-fed projection and care interactions because the current placeholder cannot satisfy the demonstrated browser integration path.
- Extend workspace manifests, root scripts, CI, architecture, README, and backlog only for the new CLI crate, browser tests, embedded bundle, installation, and verified MVP operation.

### C. Missing MVP work

- Installed-schema Codex hook fixtures, privacy-preserving translation, command classification, strict denial response, temporary layered profile, and real integration spike evidence.
- Loopback HTTP/WebSocket server, bearer authentication, bounded JSON ingestion, typed errors, SQLite persistence, authoritative snapshots, restart recovery, and demo controls.
- Functional food drag/drop, shovel/poop/trash workflow, activity presentation, reconnect handling, error UI, and Playwright coverage.
- Runtime metadata discovery, stale-session handling, browser launch, embedded production assets, transparent Codex process wrapper, signal/exit propagation, and cleanup.
- End-to-end integration harness, real interactive Codex smoke test, installation proof, strict refusal/recovery proof, and final verification report.

## File map and cross-task interfaces

- `crates/codegotchi-cli/src/protocol.rs` owns HTTP/WebSocket DTOs shared by hook, server, and tests.
- `crates/codegotchi-cli/src/codex_hook.rs` owns Codex-only input schemas and translation into `codegotchi_domain::AgentEvent`.
- `crates/codegotchi-cli/src/classify.rs` owns privacy-limited command/tool classification and strict blockability.
- `crates/codegotchi-cli/src/runtime_metadata.rs` owns active-session discovery and mode-0600 metadata.
- `crates/codegotchi-cli/src/codex_profile.rs` owns unique additive profile creation and exact cleanup.
- `crates/codegotchi-cli/src/persistence.rs` owns SQLite schema/versioning and snapshot transactions.
- `crates/codegotchi-cli/src/runtime.rs` owns the mutex-protected simulation, settings, mutation/persistence ordering, and broadcast snapshots.
- `crates/codegotchi-cli/src/server.rs` owns loopback bind, authenticated routes, bounded bodies, static assets, and graceful shutdown.
- `crates/codegotchi-cli/src/launcher.rs` owns Codex resolution, argument conflict detection, browser launch, inherited stdio, child lifetime, and cleanup.
- `web/src/protocol.ts`, `web/src/client.ts`, and `web/src/App.tsx` consume the backend DTO contract; no domain rules live in TypeScript.
- `crates/codegotchi-cli/web-dist/` is generated from `web/dist/` by `web/scripts/embed-web.mjs` and embedded by the Rust binary.

---

### Task 1: Installed Codex integration spike and production hook seam

**Files:**
- Create: `crates/codegotchi-cli/Cargo.toml`
- Create: `crates/codegotchi-cli/src/main.rs`
- Create: `crates/codegotchi-cli/src/cli.rs`
- Create: `crates/codegotchi-cli/src/protocol.rs`
- Create: `crates/codegotchi-cli/src/runtime_metadata.rs`
- Create: `crates/codegotchi-cli/src/codex_profile.rs`
- Create: `crates/codegotchi-cli/src/classify.rs`
- Create: `crates/codegotchi-cli/src/codex_hook.rs`
- Create: `crates/codegotchi-cli/tests/hook_fixtures.rs`
- Create: `crates/codegotchi-cli/tests/profile_lifecycle.rs`
- Create: `crates/codegotchi-cli/tests/fixtures/hooks/*.json`
- Create: `docs/adr/0002-codex-hook-profile-integration.md`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: `AgentEvent`, `ActivityKind`, `AgentEventKind`, `EventMetadata`, and permission types from `codegotchi-domain`.
- Produces: `RuntimeMetadataV1`, `HookInput`, `HookOutput`, `EventIngestRequest`, `EventIngestResponse`, `translate_hook`, `classify_command`, `TemporaryCodexProfile`, and a working `codegotchi hook` command.
- `RuntimeMetadataV1` fields are `schema_version`, runtime UUID, repository root, loopback base URL, bearer token, and owning PID; JSON uses camelCase.
- `HookOutput::allow()` serializes as `{}`; denial serializes only the documented `hookSpecificOutput` shape.

- [x] Write sanitized exact-schema fixtures for SessionStart, SessionEnd, UserPromptSubmit, Bash pre/post success/post failure, apply_patch pre/post, Stop, unknown tool, future fields, and malformed JSON.
- [x] Write fixture tests proving deterministic event IDs, exact activity/kind mapping, command categories, prompt/source/output discard, unknown-tool generic work, malformed-input fail-open output, and Strict denial serialization.
- [x] Run `cargo test -p codegotchi-cli --test hook_fixtures` and record the expected missing-module/API failures.
- [x] Implement the bounded stdin reader, tolerant Codex-only deserialization, privacy-limited translator, low-timeout loopback POST, and stdout discipline. No backend server is implemented in this task.
- [x] Write profile lifecycle tests proving unique-name conflict refusal, base config preservation by checksum, mode-0600 creation, additive inline hooks for all six event families, inherited `CODEGOTCHI_SESSION_FILE`, and exact-file cleanup.
- [x] Run the profile tests red, implement the smallest profile/runtime metadata seams, and run them green.
- [x] Use a temporary profile, a loopback capture receiver, and the installed Codex CLI to prove SessionStart, prompt, Bash, apply_patch, Stop, and SessionEnd input plus one safe PreTool denial. Do not continue to Task 2 until this succeeds; do not bypass hook trust.
- [x] Record exact installed fields, trust interaction, existing-hook coexistence observation, cleanup, and the chosen production mechanism in ADR 0002.
- [x] Run Rust format/clippy/workspace tests and commit the task. Write the report at the plan workspace's `task-1-report.md`.

### Task 2: Versioned domain restore, SQLite authority, HTTP, and WebSocket

**Files:**
- Modify: `crates/codegotchi-domain/src/pet.rs`
- Modify: `crates/codegotchi-domain/src/progression.rs`
- Modify: `crates/codegotchi-domain/src/permission.rs`
- Test: `crates/codegotchi-domain/tests/persistence_restore.rs`
- Create: `crates/codegotchi-cli/src/persistence.rs`
- Create: `crates/codegotchi-cli/src/runtime.rs`
- Create: `crates/codegotchi-cli/src/server.rs`
- Create: `crates/codegotchi-cli/tests/backend_integration.rs`
- Create: `crates/codegotchi-cli/tests/websocket_integration.rs`
- Modify: `crates/codegotchi-cli/src/main.rs`
- Modify: `crates/codegotchi-cli/src/protocol.rs`
- Modify: `crates/codegotchi-cli/Cargo.toml`

**Interfaces:**
- Consumes: Task 1 protocol DTOs and existing `PetSimulation` transitions.
- Produces: serde `SimulationSnapshot`, `PetSimulation::from_snapshot`, `SqliteStore::{open,load_or_initialize,save}`, `AuthoritativeRuntime`, `RunningServer`, and routes `/api/v1/health`, `/api/v1/state`, `/api/v1/events`, `/api/v1/care/feed`, `/api/v1/care/clean`, `/api/v1/stream`.
- Every accepted mutation persists before broadcast. Duplicate event/action IDs return success without a second transition.
- New pets start with a literal practical inventory of 50 kibble, 25 treats, and 25 fruit; restored pets never receive a second seed.

- [x] Write domain restore tests proving complete snapshot round-trip, version rejection, replay-ID survival, deterministic continuation, and unchanged Phase 2 behavior.
- [x] Run the restore test red, add only serde/restore seams, then run it green and run all domain tests.
- [x] Write real temporary-SQLite tests for initialization, reload, corrupt/unsupported snapshot errors, atomic save, enforcement persistence, and restart idempotency.
- [x] Run the persistence tests red, implement schema version 1 and transactional snapshot storage, then run them green.
- [x] Write loopback integration tests for health/state, missing/wrong bearer rejection, bounded body rejection, valid/duplicate event ingestion, valid/invalid/duplicate feeding and cleaning, typed error envelopes, and no command execution route.
- [x] Implement the authoritative runtime and Axum routes with `127.0.0.1:0`, then run the HTTP tests green.
- [x] Write and run WebSocket tests for authenticated initial full snapshot, mutation broadcast, disconnect/reconnect, and the authoritative reconnect snapshot.
- [x] Add a one-second maintenance tick that advances/persists/broadcasts only when the snapshot changes and shuts down with the server.
- [x] Run Rust format/clippy/workspace tests and commit the task. Write the report at the plan workspace's `task-2-report.md`.

### Task 3: Functional pet room, authenticated care, reconnect, and Playwright

**Files:**
- Create: `web/src/protocol.ts`
- Create: `web/src/client.ts`
- Create: `web/src/useCodeGotchi.ts`
- Modify: `web/src/App.tsx`
- Modify: `web/src/App.css`
- Modify: `web/src/App.test.tsx`
- Create: `web/src/client.test.ts`
- Create: `web/e2e/mvp.spec.ts`
- Create: `web/playwright.config.ts`
- Create: `web/scripts/embed-web.mjs`
- Modify: `web/package.json`
- Modify: `package.json`
- Modify: `pnpm-lock.yaml`

**Interfaces:**
- Consumes: Task 2 JSON snapshot/error/care contracts and token from `location.hash`.
- Produces: accessible room projection, mandatory activity labels/classes, authenticated drag/drop feed, shovel-poop-trash clean workflow, reconnecting WebSocket, backend error banner, and an embed script that replaces only `crates/codegotchi-cli/web-dist/`.
- The client generates UUID care IDs once per user action and never mutates displayed needs/poops optimistically.

- [x] Rewrite the component tests first for initial/loading/disconnected states, all mandatory activity labels, authoritative snapshot render, and backend-error presentation; run them and record expected failures.
- [x] Add client tests for fragment token extraction/removal, authenticated care headers, invalid drop no request, WebSocket reconnect, and replacement by a complete reconnect snapshot.
- [x] Implement typed client/state hooks and the minimum real UI needed for tests; no browser-side need or poop calculation.
- [x] Add drag data for food and shovel, valid feed target handling, poop selection/application, trash confirmation, keyboard-accessible equivalents, and visible success feedback.
- [x] Add Playwright tests against a real Task 2 server fixture for room load, food drop persistence after reload, invalid drop, poop clean persistence after reload, disconnect/recovery, and backend error display.
- [x] Run `corepack pnpm test`, `corepack pnpm lint`, `corepack pnpm format:check`, and the focused Playwright suite.
- [x] Build Vite and run `node web/scripts/embed-web.mjs`; verify the copied bundle contains no development-server dependency.
- [x] Commit the task. Write the report at the plan workspace's `task-3-report.md`.

### Task 4: Hook-to-runtime activity mapping, strict decisions, and demo controls

**Files:**
- Modify: `crates/codegotchi-cli/src/codex_hook.rs`
- Modify: `crates/codegotchi-cli/src/classify.rs`
- Modify: `crates/codegotchi-cli/src/runtime.rs`
- Modify: `crates/codegotchi-cli/src/server.rs`
- Modify: `crates/codegotchi-cli/src/cli.rs`
- Modify: `crates/codegotchi-cli/src/main.rs`
- Modify: `crates/codegotchi-cli/src/protocol.rs`
- Create: `crates/codegotchi-cli/tests/hook_runtime_flow.rs`
- Create: `crates/codegotchi-cli/tests/strict_flow.rs`

**Interfaces:**
- Consumes: Task 1 hook translation and Task 2 authenticated event endpoint.
- Produces: event-ingest Strict decision, documented refusal text, `codegotchi mode decorative|strict`, and guarded `codegotchi debug neglect|generate-poop` commands against the active runtime.
- Blockable commands are only recognized test/build/development commands; CodeGotchi, termination, shell/process recovery, Git, infrastructure shutdown, security remediation, diagnostics, and uncertain operations always allow.

- [x] Write an end-to-end hook/runtime test that posts every fixture and observes Session active, Thinking, Bash category activity, Editing, success/failure, Waiting, and Session end/idle snapshots without persisted raw fields.
- [x] Write the strict flow red: critical snapshot plus safe PreTool is denied with all four required explanation elements; decorative, fail-open transport, unknown/recovery/CodeGotchi operations allow; feed/clean through normal API then retry allows.
- [x] Implement atomic event application plus permission evaluation and exact Codex output selection, keeping transport failure fail-open.
- [x] Write guarded demo tests proving neglect creates critical authoritative persisted state and generate-poop uses canonical work plus normal domain feed/care transitions before broadcasting.
- [x] Implement mode/debug commands using active runtime discovery; require `CODEGOTCHI_ENABLE_DEBUG=1` for debug mutation and never expose arbitrary values or command execution.
- [x] Run Rust format/clippy/workspace tests and commit the task. Write the report at the plan workspace's `task-4-report.md`.

### Task 5: Transparent `run -- codex`, embedded packaging, browser launch, and cleanup

**Files:**
- Create: `crates/codegotchi-cli/src/launcher.rs`
- Create: `crates/codegotchi-cli/src/assets.rs`
- Modify: `crates/codegotchi-cli/src/server.rs`
- Modify: `crates/codegotchi-cli/src/cli.rs`
- Modify: `crates/codegotchi-cli/src/main.rs`
- Modify: `crates/codegotchi-cli/Cargo.toml`
- Create: `crates/codegotchi-cli/tests/process_wrapper.rs`
- Create: `crates/codegotchi-cli/tests/static_assets.rs`
- Create: `crates/codegotchi-cli/tests/fixtures/fake-codex.sh`
- Create: `crates/codegotchi-cli/web-dist/**`
- Modify: `.gitignore`
- Modify: `README.md`

**Interfaces:**
- Consumes: Tasks 1-4 profile, metadata, server, persistence, hook, UI bundle, and runtime services.
- Produces: exact `codegotchi run -- codex [arguments...]`, `CODEGOTCHI_REAL_CODEX` test override, real executable recursion guard, automatic browser launch or printed URL, embedded SPA assets, inherited terminal streams, exit-status preservation, and scope-exact cleanup.
- Explicit `-p`, `--profile`, or `--profile=...` in trailing Codex arguments returns a typed actionable conflict before creating runtime files.

- [ ] Write fake-agent process tests first for exact argument order, stdout/stderr visibility, exit code, Ctrl+C/termination forwarding, no PTY color stripping, runtime/profile environment, and normal cleanup.
- [ ] Write failure-path tests for missing real Codex, recursive resolution, profile conflict, child spawn failure, browser-open failure, stale runtime metadata, abnormal stale-file recovery, and preservation checksums for existing config/credentials.
- [ ] Implement direct inherited-stdio `Command` launch and only add PTY code if a real smoke test demonstrates a terminal defect.
- [ ] Serve embedded `web-dist` with correct MIME types, SPA fallback, and no filesystem dependency; test the installed binary from a directory outside the repository.
- [ ] Build the frontend, refresh the committed embedded bundle, run `cargo install --path crates/codegotchi-cli --root <temporary-root>`, and prove that installed `codegotchi run -- <fake-codex>` needs no second terminal or Vite process.
- [ ] Update README with exact Corepack development gates, one-command CLI installation, launch, first-run `/hooks` trust, Strict mode, debug demo, state locations, and cleanup behavior.
- [ ] Run Rust and frontend gates and commit the task. Write the report at the plan workspace's `task-5-report.md`.

### Task 6: Vertical acceptance harness, regression closure, and verification evidence

**Files:**
- Create: `crates/codegotchi-cli/tests/full_vertical_flow.rs`
- Modify: `web/e2e/mvp.spec.ts`
- Modify: `.github/workflows/ci.yml`
- Modify: `docs/architecture.md`
- Create: `docs/backlog/codex-first-mvp-followups.md`
- Create: `docs/verification/codex-first-mvp.md`
- Modify: `README.md`
- Modify: any load-bearing file from Tasks 1-5 only when a failing vertical test demonstrates the need

**Interfaces:**
- Consumes: the complete Tasks 1-5 public behavior.
- Produces: one automated restart flow and the evidence template used for the supervisor's personal real-Codex/browser acceptance.

- [ ] Write a full process-level test using the installed-style binary and fake Codex: start runtime, ingest hook activity, connect WebSocket, feed, generate/clean poop, stop, restart, and assert the persisted cared-for state plus replay idempotency.
- [ ] Add a strict process-level test for neglect, safe denial, authenticated UI-equivalent care, retry allow, and fail-open after server shutdown.
- [ ] Run the vertical tests red against the integrated Tasks 1-5 result; fix only demonstrated cross-task defects and run them green.
- [ ] Add Playwright production-bundle coverage to root scripts and CI while keeping fixture mutation guarded from normal production use.
- [ ] Update architecture and README to match the final executable, data flow, persistence, privacy, install, launch, trust, Strict, demo, and limitation behavior.
- [ ] Create the verification report with environment and command fields populated from current evidence; leave no success claim unverified.
- [ ] Run all focused and repository-wide automated gates and commit the task. Write the report at the plan workspace's `task-6-report.md`.

## Manual verification sequence

- [ ] Install with `cargo install --path crates/codegotchi-cli --force` and resolve `codegotchi` from the installed location.
- [ ] Run `codegotchi run -- codex`, complete Codex's `/hooks` trust review without bypass flags, and confirm the browser opens from the embedded bundle.
- [ ] Submit: `Inspect Cargo.toml, tell me the workspace package names, and run one harmless metadata or test-listing command. Do not modify files.`
- [ ] Observe authoritative session active, thinking, Bash/unified-exec category, command activity, result feedback, waiting, and idle/end transitions in the UI.
- [ ] In a disposable tracked fixture, ask Codex to make one harmless `apply_patch` edit, observe editing plus completion, then restore/remove only that fixture.
- [ ] Use food drag/drop and record hunger before/after plus the same value after refresh.
- [ ] Enable guarded demo mode, generate poop, complete shovel-poop-trash disposal, and record removal/cleanliness plus refresh persistence.
- [ ] Exit Codex/CodeGotchi, launch the exact command again, and record the same pet identity, needs, inventory, and poop state.
- [ ] Enable Strict, trigger neglect, have Codex attempt a safe test-listing command, record denial text, care through the UI, retry, and record allowance.
- [ ] Verify Codex exit status, Ctrl+C behavior, runtime/profile removal, unchanged base config/credential checksums, loopback bind, and absence of sensitive persisted fields.

## Complete verification suite

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `corepack pnpm lint`
- [ ] `corepack pnpm test`
- [ ] `corepack pnpm format:check`
- [ ] `corepack pnpm build`
- [ ] `corepack pnpm playwright:test`
- [ ] Local `cargo install --path crates/codegotchi-cli --force`
- [ ] Real interactive `codegotchi run -- codex`

## Explicit deferred requirements

- Claude Code, Cursor, MCP, generic agent SDKs, hosted-tool completeness, full command interception/rewriting, PATH shims, output compression, permanent daemon operation, remote/cloud access, telemetry, accounts, multiplayer/team pets, species/evolution/breeding, marketplace/payments, voice/dialogue, mobile/native non-WSL Windows/macOS packaging, installer GUI, auto-update, sophisticated inventory, complex physics, polished art, deliberate-bypass prevention, and petting.
- Optional architecture polish and speculative edge cases raised by reviewers go only to `docs/backlog/codex-first-mvp-followups.md`.

## Exact definition of done

- [ ] Every mandatory acceptance criterion in the user milestone has direct automated or recorded manual evidence.
- [ ] The final broad Luna/max review has no unresolved MVP-blocking finding after its single permitted correction pass.
- [ ] All quality gates and local installation pass from fresh commands.
- [ ] The supervisor personally ran the installed `codegotchi run -- codex` command with real Codex, real trusted hooks, the embedded browser UI, feeding, cleaning, restart persistence, Strict refusal/recovery, and cleanup.
- [ ] `docs/verification/codex-first-mvp.md` contains exact results and limitations without “should work” language.
- [ ] No backlog item was implemented after the mandatory path passed.
