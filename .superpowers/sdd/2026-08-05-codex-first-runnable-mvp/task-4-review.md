# Independent Task 4 review

Scope: the Task 4 brief/report, commit diff `8cb8cdd..34a6516`, the listed
domain interfaces, and the four listed CLI tests. No production or test file
was modified for this review.

## Verification

- `cargo test --offline -p codegotchi-cli --test hook_fixtures --test hook_runtime_flow --test strict_flow --test backend_integration` — PASS (18 tests).
- `cargo test --offline -p codegotchi-cli --lib lagged_websocket_recovery_discards_retained_stale_snapshots` — PASS.
- `cargo fmt --all -- --check` — PASS.
- `cargo clippy --offline -p codegotchi-cli --all-targets --all-features -- -D warnings` — PASS.
- `git diff --check 8cb8cdd..34a6516` — PASS.

## MVP-blocking

### 1. Stable tool identity is not the replay key

Evidence: `crates/codegotchi-cli/src/protocol.rs:89-100` defines
`tool_use_id` as the stable tool-hook identity, but
`crates/codegotchi-cli/src/codex_hook.rs:228-253` also hashes executable,
category, exit status, duration, blocked state, repository, and URL into the
event ID. Exit status and duration come from the mutable hook response at
`crates/codegotchi-cli/src/codex_hook.rs:64-80`.

Impact: a duplicate delivery with the same Codex `tool_use_id` but a changed
or newly-populated duration/exit-status field gets a new event ID. The runtime
then applies it again and persists/broadcasts another mutation, so replay
idempotency and the required duplicate-delivery guarantee are not general.

Smallest correction: when a stable lifecycle/turn/tool identity exists, derive
the event ID from runtime ID, session ID, event kind, and that identity only;
retain the current metadata-based fallback only for events without an identity.
Add one focused regression using the same `tool_use_id` with changed structured
post-tool fields and assert no second mutation/broadcast.

### 2. An incomplete `apply_patch` PreToolUse can fail closed into a denial

Evidence: `crates/codegotchi-cli/src/classify.rs:11-16` unconditionally marks
`apply_patch` as `SafeDevelopment`. `crates/codegotchi-cli/src/protocol.rs:57-63`
returns no command for missing/non-object tool input, while
`crates/codegotchi-cli/src/codex_hook.rs:125-133` still supplies a classification
for every `PreToolUse` payload.

Impact: with a critical pet and Strict enabled, valid JSON such as a
`PreToolUse` carrying `tool_name: "apply_patch"` but missing or malformed
`tool_input` can reach the runtime as blockable work and emit a deny response.
That is uncertainty/malformed-hook handling taking the blocking path, contrary
to the required fail-open behavior. The focused malformed test only covers
invalid JSON at `crates/codegotchi-cli/tests/strict_flow.rs:407-414`.

Smallest correction: make an `apply_patch` classification uncertain unless the
minimum recognized edit input is present (or reject the semantically malformed
payload before permission evaluation), and add the missing-input regression to
the Strict flow.

## Backlog

### 1. The HTTP debug guard is an attested header, not the environment guard

Evidence: the CLI checks `CODEGOTCHI_ENABLE_DEBUG=1` at
`crates/codegotchi-cli/src/cli.rs:90-93`, but the transport merely adds
`X-CodeGotchi-Debug: 1` at `crates/codegotchi-cli/src/codex_hook.rs:351-355` and
the server authorizes solely on that header at
`crates/codegotchi-cli/src/server.rs:299-312` and `528-533`.

Impact: a caller that already has the bearer token can invoke either fixed
debug route directly with that header even when the CLI environment variable
was not set. The CLI commands themselves satisfy the guard and the routes are
fixed/authenticated, so this is optional hardening rather than an MVP blocker.

Smallest correction: bind debug enablement to runtime startup configuration (or
another authenticated session capability) and require it in addition to the
CLI/header check.

## Other acceptance sections

- Exact Codex output and ordinary transport fail-open: None. The focused flows
  verify `{}` on malformed, transport, stale-metadata, and malformed-response
  paths, and verify the exact `hookSpecificOutput` denial shape.
- Privacy-limited translation and persistence: None. The event envelope carries
  only the canonical event plus structured permission strings, and the reopened
  snapshot checks the fixture prompt, source, command, output, and search values.
- Required Strict exemptions: None beyond the incomplete-input issue above.
- Atomic save-before-broadcast, rollback, care recovery, and Task 3 snapshot
  ordering: None found in the bounded review. The runtime holds one simulation
  lock, restores on save failure, saves before broadcasting, and mode/demo-only
  changes advance observable snapshot progress.
- Focused-test scope: The tests are meaningful process-level coverage, but do
  not cover the two MVP-blocking regressions identified above.

## Verdict

CORRECTION REQUIRED
