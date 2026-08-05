# SDD ledger — plan: docs/superpowers/plans/2026-08-05-codex-first-runnable-mvp.md

Plan commit: 60972be
Foundation baseline: 488c9a1
Branch: codex-first-mvp
Started: 2026-08-05 Europe/Berlin

Repository reconstruction:
- No commits, task reports, review reports, or prior ledger existed on arrival.
- Existing Phase 1-2 tree preserved in 488c9a1.
- Baseline: cargo fmt PASS; cargo clippy PASS; cargo test PASS (60); corepack pnpm lint PASS; test PASS (1); format PASS; build PASS.
- Installed Codex: codex-cli 0.146.0; hooks stable; profile layering and exact current hook schemas verified from installed help/current official manual.

Task 1: complete; supervisor approved after bounded correction loop
- Implementer: Codex CLI session 019fd0f7-5500-7c12-b934-ac784d4f1d99, model gpt-5.6-luna, reasoning max
- Base: 60972be
- Result: d9e9d93, e93e7eb, 568ebfb
- Tests: cargo fmt PASS; strict workspace clippy PASS; cargo test --workspace PASS (69)
- Real spike: Codex 0.146.0 PASS; trusted six hooks; Bash/apply_patch lifecycle observed; safe Cargo PreToolUse denied; config checksum preserved; generated files cleaned
- Report: task-1-report.md
- Review: NEEDS FIXES — 4 MVP-blocking, 0 Backlog (task-1-review.md)
- Correction: first dispatch rejected at usage gate; free earned reset redeemed; completed by Codex CLI session 019fd12b-9ce3-7850-af1e-3028b13b63d4, model gpt-5.6-luna, reasoning max
- Correction commits: 8a1a0be (implementation/tests), 8b0fe7a (ADR/report)
- Correction tests: focused hook/profile PASS; ignored installed-Codex gate compiles; cargo fmt PASS; strict workspace clippy PASS; cargo test --workspace PASS
- Focused re-review: NOT APPROVED — 2 MVP-blocking, 0 Backlog (task-1-rereview.md); second correction permitted because accepted idempotency/evidence criteria still fail
- Second correction: resume of ephemeral session 019fd12b-9ce3-7850-af1e-3028b13b63d4 failed (`no rollout found`); narrowly scoped replacement session 019fd143-df18-7fd3-96b7-e2e69ea68eb7, model gpt-5.6-luna, reasoning max
- Second correction commit: 97f2257
- Second correction tests: focused hook/profile PASS; routine installed-Codex predicate PASS with manual paid/authenticated gate ignored; cargo fmt PASS; strict workspace clippy PASS; cargo test --workspace PASS
- Supervisor adjudication: APPROVED; denial/non-execution and lifecycle-source blockers resolved. Identical ID-less SessionEnd occurrence distinction deferred to backlog because Codex 0.146 exposes no discriminator.
- Deferred findings: docs/backlog/codex-first-mvp-followups.md
Task 2: complete; supervisor approved after two bounded correction rounds
- Implementer: Codex CLI session 019fd151-dd8d-75d1-b660-5cae2b334267, model gpt-5.6-luna, reasoning max
- Base: 8667af0
- Brief commit: df45d7a
- Implementation commit: 49ff122
- Tests: cargo fmt PASS; strict workspace clippy PASS; cargo test --workspace PASS
- Review: NEEDS FIXES — 4 MVP-blocking, 0 Backlog; reviewer session 019fd16c-8286-7c81-b5d2-4c0f2f7c820a, model gpt-5.6-luna, reasoning max
- First correction: original implementer resumed with Luna/max; commit 4e09469; hook/live-server, WebSocket lag ordering, typed 405, and deterministic maintenance/runtime coverage fixed
- Focused re-review: 3 findings resolved; scheduled `RunningServer` maintenance task remained unexercised
- Second correction: original implementer resumed with Luna/max; shared interval/trigger maintenance runner plus production-server scheduler test
- Supervisor adjudication: APPROVED; focused scheduler test, formatting, strict clippy, persistence/broadcast, bounded shutdown, and receiver-drop proof PASS
- Deferred findings: none
Task 3: complete; accepted after one focused correction
- Implementer: Codex CLI session 019fd194-480e-72b0-9ea8-84790a137b70, model gpt-5.6-luna, reasoning max
- Base: e0b7fcf; brief commit: c0189e4; implementation commit: a4c00e6
- Initial tests: frontend 23 PASS; lint/format/build PASS; Rust fmt/strict clippy/workspace tests PASS; Playwright 6/6 PASS in supervisor WSL environment
- Review: CORRECT MVP-BLOCKERS — 2 MVP-blocking, 3 Backlog; reviewer session 019fd1b2-5cc8-7291-80ba-05f642a28831, model gpt-5.6-luna, reasoning max; review commit d1e24f4
- Correction: original implementer resumed; direct poop-to-trash bypass removed and all HTTP/WebSocket/care snapshots routed through a monotonic complete-snapshot gate; commit 6a5a17a
- Correction tests: frontend 29 PASS; lint/format/build PASS; embedded bundle byte-identical and free of Vite development markers; Rust fmt/strict clippy/workspace tests PASS; Playwright 7/7 PASS against real Task 2 server
- Focused re-review: ACCEPTED; both original blockers resolved; no residual finding
- Deferred findings: syntax-safe WebSocket subprotocol token contract, graceful fixture sidecar cleanup, stronger disconnected-state Playwright mutation proof; recorded in docs/backlog/codex-first-mvp-followups.md
Task 4: complete; accepted after one focused correction
- Implementer: Codex CLI session 019fd1d3-e431-7b81-b5d8-ddbbee200bc4, model gpt-5.6-luna, reasoning max
- Base: 8cb8cdd; brief commit: a60b29f; implementation commit: 34a6516
- Initial tests: hook/runtime and Strict process flows PASS; cargo fmt PASS; strict workspace clippy PASS; cargo test --workspace PASS (88 executed, 1 manual installed-Codex test ignored); frontend lint/test/build PASS; Playwright 7/7 PASS
- Review: CORRECTION REQUIRED — 2 MVP-blocking, 1 Backlog; reviewer session 019fd1fc-9a71-7ce2-a5ec-737cd8289d0f, model gpt-5.6-luna, reasoning max; review commit 094ecba
- Correction: original implementer resumed with Luna/max; stable hook replay IDs no longer depend on mutable response metadata and incomplete apply_patch payloads now remain uncertain/fail-open; correction commit bc046cc
- Correction tests: focused hook/runtime and Strict flows PASS; verified installed apply_patch fixture remains Editing/SafeDevelopment; cargo fmt PASS; strict workspace clippy PASS; cargo test --workspace PASS (88 executed, 1 ignored)
- Focused re-review: ACCEPT; both original blockers resolved with no residual finding
- Deferred finding: bind debug enablement to runtime startup configuration or a dedicated session capability; recorded in docs/backlog/codex-first-mvp-followups.md
Task 5: complete; supervisor accepted after two bounded correction rounds and personal adjudication
- Implementer: Codex CLI session 019fd217-8258-70e2-8916-d4cf528bb315, model gpt-5.6-luna, reasoning max
- Base: 885977f; brief commit: f293795; implementation commit: 2fef54a
- Initial tests: process wrapper 10 PASS; static/install 2 PASS; Rust fmt/strict clippy/workspace tests PASS; frontend lint/test/format/build PASS; embedded bundle byte-identical
- Review: CORRECTION REQUIRED — 7 MVP-blocking, 2 Backlog; reviewer session 019fd23d-ef0e-7d41-b887-9ebe343e9a6e, model gpt-5.6-luna, reasoning max; review/backlog commit 6d453db
- First correction: original implementer resumed with Luna/max; setup-time handlers, browser warning, owned stale cleanup, atomic metadata, installed HTTP asset fetch, restart persistence, and shared-terminal duplicate-signal coverage fixed; commit f2210e5
- Focused re-review: six findings resolved; one direct-wrapper signal defect remained; record commit d243c1a
- Second correction investigation: original implementer resumed with Luna/max and proved direct-PID versus negative-process-group signals have identical Linux siginfo; the same-group source distinction is impossible
- Supervisor adjudication and implementation: standard POSIX child process-group/foreground terminal handoff using safe nix APIs, exact signal-mask restoration, direct and terminal-group one-delivery fixtures, and PTY ownership restoration mutation proof; commit b8bb665
- Final focused evidence: process wrapper 16 PASS; cargo fmt PASS; strict workspace clippy PASS
- Deferred findings: PID reuse hardening and best-effort cleanup after hard-kill; recorded in docs/backlog/codex-first-mvp-followups.md
Task 6: pending
Final review: pending
Real acceptance: pending
