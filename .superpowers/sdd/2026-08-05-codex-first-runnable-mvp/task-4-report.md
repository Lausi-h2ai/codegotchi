# Task 4 report — production hook/runtime flow, Strict decisions, and demo controls

Date: 2026-08-05
Task brief base: `8cb8cdd` (working tree continuation from the existing Task 4
session)
Scope: Task 4 only. No commit was created.

## Implementation

- Added the production hook-to-runtime permission context: only structured
  category/purpose values accompany a `PreToolUse` event; prompts, source,
  commands, tool output, and transcript values remain adapter-local.
- Added conservative command classification. Only recognized local
  development/test/build/edit operations can be blockable; CodeGotchi,
  termination, shell/process recovery, Git, infrastructure, security,
  diagnostics, compound/ambiguous, unknown, and uncertain operations fail
  open.
- Added one locked authoritative ingest path that evaluates permission,
  applies the canonical event, persists, and broadcasts. Duplicate event IDs
  remain no-ops and return the current decision without a second mutation.
- Added authenticated `mode decorative|strict` and fixed, guarded
  `debug neglect` / `debug generate-poop` commands using active runtime
  metadata and the existing bearer token.
- Added fixed domain transitions for neglect and poop generation. Generated
  poop is authoritative and was removed through the normal authenticated
  clean route.
- Preserved the Task 3 event/snapshot wire boundaries and complete snapshot
  broadcasts. Mode changes advance the authoritative timestamp by one
  millisecond so equal-time browser snapshots still have observable progress.
- Restored the production hook transport timeout to 250 ms and kept the
  existing response framing. Blocking child-process helpers in both new
  process-level tests run through `tokio::task::spawn_blocking`.

## TDD evidence

### RED

The new acceptance tests were first run before the Task 4 implementation was
complete. The hook flow failed waiting for an authoritative snapshot and the
Strict flow failed because the mode/debug production routes did not yet exist.
After the first implementation pass, both tests still failed with the
environment-level `Resource temporarily unavailable` symptom: synchronous
child process waits occupied the only current-thread Tokio executor while the
Axum server was waiting for that executor to process the request.

### GREEN

The process helpers were moved into `spawn_blocking`, temporary diagnostics
and the unused tick sender were removed, and the fast 250 ms hook timeout was
restored. The focused acceptance command then passed:

```text
cargo test --offline -p codegotchi-cli --test hook_runtime_flow --test strict_flow
PASS — hook_runtime_flow: 1 passed; strict_flow: 1 passed
```

The focused flows cover:

- SessionStart/End, prompt Thinking, Bash searching/testing/generic activity,
  apply_patch Editing, post-tool success/failure outcomes, Stop/Waiting, and
  complete idle state after SessionEnd.
- Exact Codex denial JSON, including the refusal explanation, critical need,
  `feed`, CodeGotchi UI, and retry-after-care text.
- Decorative allow, malformed input, malformed response, transport failure,
  invalid metadata, unknown/future work, compound commands, CodeGotchi,
  Git, exit/termination, process recovery, and diagnostics.
- Duplicate denial delivery, normal authenticated feed recovery, a new
  tool-use identity retry, and normal authenticated poop cleaning.
- Missing/stale/unauthorized metadata, missing debug guard, arbitrary CLI
  arguments, unknown JSON fields on mode/debug requests, persistence reopen,
  and one persisted/broadcast snapshot per accepted mutation.

## Privacy and boundary evidence

`hook_runtime_flow.rs` serializes the reopened SQLite snapshot and asserts that
it contains none of these fixture values:

```text
never-persist-this-prompt
secret-source-content
sensitive-tool-output
cargo test -p secret-project
do-not-persist-search-source
```

The event boundary contains only the canonical `AgentEvent` plus optional
structured `category` and `purpose`. The persisted store contains the complete
simulation snapshot and replay IDs, not the request envelope or raw hook
payload. Unknown/future hook fields are ignored by the tolerant input adapter.
Hook stdout assertions parse the complete output as JSON and require `{}` on
every fail-open path; the verified denial is the installed Codex
`hookSpecificOutput` shape.

## Exact changed files

Modified:

- `crates/codegotchi-cli/src/classify.rs`
- `crates/codegotchi-cli/src/cli.rs`
- `crates/codegotchi-cli/src/codex_hook.rs`
- `crates/codegotchi-cli/src/lib.rs`
- `crates/codegotchi-cli/src/protocol.rs`
- `crates/codegotchi-cli/src/runtime.rs`
- `crates/codegotchi-cli/src/server.rs`

Created:

- `crates/codegotchi-cli/tests/hook_runtime_flow.rs`
- `crates/codegotchi-cli/tests/strict_flow.rs`
- `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-4-report.md`
- `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-4-correction-report.md`

No web UI, launcher/profile lifecycle, embedded asset, README, domain source,
persistence schema, plan, ledger, backlog, or review file was changed.
`main.rs` required no change because its existing nonzero CLI error plumbing
already covers the new commands.

## Required gates

| Gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace` | PASS — 88 executed tests passed; 1 pre-existing manual Codex 0.146 test ignored |
| `git diff --check` | PASS |

The first chained workspace-gate attempt encountered a transient sandbox
`Operation not permitted` while several existing loopback integration tests
bound concurrently. The exact standalone `cargo test --workspace` rerun passed
without code changes.

## Limitation

Strict is intentionally a fail-open product policy/demo flow, not a security
boundary. Runtime liveness discovery uses the existing metadata plus owner PID
check, and all hook transport, metadata, parsing, and uncertainty failures
remain successful `{}` allows as required.

## Focused correction

The independent review identified two MVP blockers; both were corrected without
touching the web UI, launcher, domain policy, persistence schema, or the
review's optional debug-guard backlog item.

- Stable lifecycle/turn/tool identities now use only runtime ID, session ID,
  hook/event kind, and the supplied stable identity in the deterministic event
  ID. The existing privacy-safe metadata fallback remains unchanged when no
  stable identity is available. PreToolUse and PostToolUse remain distinct.
- `apply_patch` is SafeDevelopment only when the adapter has a non-empty,
  non-whitespace command string from an object-shaped `tool_input`. Missing,
  non-object, non-string, and empty/whitespace inputs classify as
  Unknown/Uncertain and therefore allow under Strict. The verified installed
  apply_patch fixture remains Editing/SafeDevelopment.

TDD correction evidence:

- RED: the new PostToolUse replay assertion observed a second broadcast with
  changed exit status/duration; the incomplete apply_patch cases returned the
  exact Strict denial instead of `{}`.
- GREEN: the corrected process-level flows pass. The replay case asserts both
  unchanged authoritative snapshot and no broadcast; the Strict flow covers
  missing input, non-object input, non-string input, non-string command,
  empty command, and whitespace-only command.

Fresh correction verification:

```text
cargo test --offline -p codegotchi-cli --test hook_runtime_flow -- --nocapture
PASS — 1 passed
cargo test --offline -p codegotchi-cli --test strict_flow -- --nocapture
PASS — 1 passed
cargo fmt --all -- --check
PASS
cargo clippy --workspace --all-targets --all-features -- -D warnings
PASS
cargo test --workspace
PASS — 88 executed tests passed; 1 pre-existing manual Codex test ignored
```

The combined two-test command was also attempted; this sandbox intermittently
rejects concurrent loopback binds with `Operation not permitted`, so the two
focused binaries were rerun and verified independently. The workspace gate
passed as a standalone command.
