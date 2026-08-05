# Task 4 — production hook/runtime flow, Strict decisions, and demo controls

Model: `gpt-5.6-luna`; reasoning: `max`.

Base commit: `8cb8cdd`.

Implement only the Task 4 vertical increment. Use TDD. Preserve the accepted
Phase 1/2 boundaries and Task 3 browser contracts; do not redesign them.

## Exact relevant files

- Modify `crates/codegotchi-cli/src/codex_hook.rs`
- Modify `crates/codegotchi-cli/src/classify.rs`
- Modify `crates/codegotchi-cli/src/runtime.rs`
- Modify `crates/codegotchi-cli/src/server.rs`
- Modify `crates/codegotchi-cli/src/cli.rs`
- Modify `crates/codegotchi-cli/src/main.rs` only if exit status plumbing is
  required for the new commands
- Modify `crates/codegotchi-cli/src/protocol.rs`
- Modify `crates/codegotchi-cli/src/lib.rs` only for public integration-test
  seams
- Modify existing focused tests/fixtures only when the wire contract changes
- Create `crates/codegotchi-cli/tests/hook_runtime_flow.rs`
- Create `crates/codegotchi-cli/tests/strict_flow.rs`

Do not modify the web UI, launcher/profile lifecycle, embedded assets, README,
domain policy semantics, or persistence schema. If a demonstrated acceptance
test requires an additional file, keep it narrow and document why in the report.

## Consumed interfaces

- `translate_hook` produces privacy-limited canonical `AgentEvent`s; no prompt,
  source content, raw command, tool output, or transcript may cross into the
  domain/persistence boundary.
- `classify_command` produces structured category/purpose. Only
  `CommandPurpose::SafeDevelopment` is blockable by the existing policy.
- `AuthoritativeRuntime` persists before broadcasting and owns the single
  simulation lock.
- `/api/v1/events` is authenticated and returns `EventIngestResponse` consumed
  by the hook. Hook transport failure, malformed input/response, absent/stale
  metadata, or uncertainty must emit `{}` and exit successfully.
- Active runtime discovery is `CODEGOTCHI_SESSION_FILE` plus
  `RuntimeMetadataV1`; mode/debug commands use it and the same bearer token.
- Task 3 consumes complete snapshots; all accepted mutations must persist and
  broadcast. Be aware that mode/demo-only snapshot changes need observable
  ordering/progress so the browser cannot discard a genuinely newer snapshot.

## Produced interfaces and behavior

- The authenticated event boundary receives a canonical event plus only the
  minimum optional structured permission context needed for a `PreToolUse`
  decision. Do not send raw commands in the request.
- Permission evaluation and accepted event mutation occur under one runtime
  critical section with one persisted/broadcast result. Replays remain
  idempotent. Non-PreTool and uncertain work are evaluated/allowed without
  blocking.
- On a verified Strict denial the hook emits the installed Codex 0.146
  `hookSpecificOutput` deny shape. Exact human text must say why the pet refuses,
  name `feed` or `clean`, say the action must be done in the CodeGotchi UI, and
  tell the user to retry the Codex request afterward.
- Decorative mode never blocks. Only recognized safe local development/test/
  build/edit work may block. `codegotchi`, exit/termination, shell/process
  recovery, Git, infrastructure shutdown, security remediation, diagnostics,
  compound/ambiguous commands, and unknown operations must allow.
- CLI commands: `codegotchi mode decorative|strict`; no implicit Gentle command
  is required. They discover the active runtime, authenticate, persist,
  broadcast, and print a concise result or actionable typed error.
- Guarded commands: `codegotchi debug neglect` and
  `codegotchi debug generate-poop`. They require
  `CODEGOTCHI_ENABLE_DEBUG=1`, accept no arbitrary values, use fixed
  domain-consistent transitions, persist and broadcast, and never expose an
  arbitrary mutation/command-execution endpoint. Generate-poop must leave a
  real authoritative poop removable through normal care; neglect must make a
  real critical state practical for feed/clean recovery.

## Mandatory TDD and acceptance tests

1. RED then GREEN `hook_runtime_flow.rs`: start a real loopback Task 2 server,
   run sanitized installed-schema fixtures through the production hook/runtime
   path, and observe complete snapshots for session active, prompt Thinking,
   Bash searching/testing or generic activity, apply_patch Editing,
   post-tool success and failure feedback, Stop/Waiting, and SessionEnd/idle.
   Assert persisted events contain no raw prompt, command string, source
   contents, tool output, or complete shell output; unknown/future fields allow.
2. RED then GREEN `strict_flow.rs`: create critical persisted state, enable
   Strict, send a recognized safe PreTool fixture, assert exact valid denial
   output and all four explanation elements. Prove decorative, malformed,
   transport failure, unknown, compound/ambiguous, CodeGotchi, Git,
   exit/termination/recovery/diagnostic operations allow.
3. In the same process-level flow, recover through the normal authenticated
   feed or clean API, retry with a new tool-use identity, and prove allow.
4. Prove duplicate hook delivery does not reapply state or change the decision
   unexpectedly.
5. Prove `mode` and both guarded debug commands use active metadata, reject when
   guard/metadata/auth is unavailable, persist and broadcast on success, survive
   SQLite reopen, and expose no arbitrary mutation input.
6. Prove hook stdout is valid Codex JSON only and fail-open paths return success.
7. Run `cargo fmt --all -- --check`,
   `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and
   `cargo test --workspace`.

## Explicit exclusions

- No launcher/process wrapper/browser opening/profile argument work (Task 5).
- No generic coding-agent abstraction, Claude/Cursor/MCP, transcript parsing,
  command rewriting/interception, arbitrary debug values, remote API, UI
  changes, petting, or backlog polish.
- Do not classify broad shell commands as safe merely to demonstrate denial.
- Do not market Strict as a security boundary.

## Report and completion

Write `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-4-report.md`
with RED/GREEN evidence, exact files, exact commands/results, privacy evidence,
and any limitation. Do not edit the plan, ledger, backlog, or review files. Do
not commit; the supervisor will inspect and commit the integrated increment.
