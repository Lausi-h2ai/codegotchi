# Task 4 focused correction report

Date: 2026-08-05
Scope: the two MVP-blocking findings in `task-4-review.md` only. No commit was
created.

## Findings corrected

### Stable replay identity

The prior deterministic ID included mutable executable/category/activity,
exit-status, duration, blocked state, repository, and URL fields even when a
Codex lifecycle, turn, or tool identity was available. A second delivery of a
tool with the same `tool_use_id` but changed post-tool status/duration could
therefore become a new event.

The stable branch now derives the UUIDv5 input from exactly:

```text
runtime_id | session_id | hook_event_name | AgentEventKind | stable_identity
```

The existing metadata-based `codegotchi-hook-v2` fallback remains used only
when no stable identity exists. Including both hook name and event kind keeps
PreToolUse and PostToolUse IDs distinct.

`hook_runtime_flow.rs` sends the installed Bash PostToolUse event once, then
sends the same `tool_use_id` with changed exit status, duration, and structured
stderr. It asserts the production hook allows, the complete authoritative
snapshot is unchanged, and no second broadcast arrives.

### Incomplete apply_patch fail-open

The adapter now treats `apply_patch` as blockable only when its extracted
installed-schema `command` string is non-empty after trimming. Invalid minimum
inputs become Unknown/Uncertain for policy and UnknownWork for activity:

- missing `tool_input`;
- non-object array input;
- non-string scalar input;
- object input with a non-string `command`;
- empty command;
- whitespace-only command.

The existing installed `apply_patch_pre.json` fixture remains covered by the
original flow and still produces Editing/SafeDevelopment behavior.

`strict_flow.rs` exercises all six incomplete valid-JSON payloads while the
persisted pet is critical and Strict is enabled. Every hook response is valid
Codex JSON `{}` and every accepted event still produces its complete snapshot.

## TDD evidence

The correction tests were added before the production changes.

RED runs:

```text
cargo test --offline -p codegotchi-cli --test hook_runtime_flow -- --nocapture
FAIL — duplicate delivery broadcast a snapshot

cargo test --offline -p codegotchi-cli --test strict_flow -- --nocapture
FAIL — incomplete apply_patch returned the Strict denial instead of `{}`
```

The first combined invocation also encountered the known sandbox concurrent
loopback-bind restriction; isolated runs exposed the intended behavioral
failures above.

GREEN runs:

```text
cargo test --offline -p codegotchi-cli --test hook_runtime_flow -- --nocapture
PASS — 1 passed

cargo test --offline -p codegotchi-cli --test strict_flow -- --nocapture
PASS — 1 passed
```

## Required gates

```text
cargo fmt --all -- --check
PASS

cargo clippy --workspace --all-targets --all-features -- -D warnings
PASS

cargo test --workspace
PASS — 88 executed tests passed; 1 pre-existing manual Codex 0.146 test ignored
```

The 250 ms hook timeout and privacy boundary are unchanged. No domain policy
semantics, persistence schema, web UI, launcher, or unrelated code was
modified.

## Changed files for this correction

Modified:

- `crates/codegotchi-cli/src/classify.rs`
- `crates/codegotchi-cli/src/codex_hook.rs`
- `crates/codegotchi-cli/tests/hook_runtime_flow.rs`
- `crates/codegotchi-cli/tests/strict_flow.rs`

Updated documentation:

- `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-4-report.md`
- `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-4-correction-report.md`
