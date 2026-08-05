# Task 1 implementation report: mandatory Codex integration gate

Date: 2026-08-05  
Branch: codex-first-mvp  
Scope: installed Codex hook/profile/runtime metadata seam only

## Outcome

Task 1 is implemented. The new codegotchi-cli crate provides the working
codegotchi hook command, bounded tolerant hook input handling, privacy-limited
domain-event translation, deterministic event IDs, authenticated low-timeout
loopback delivery, Strict denial serialization, versioned runtime metadata,
and an additive mode-0600 temporary Codex profile. No backend, server,
persistence, UI, or launcher code was added.

The installed Codex spike succeeded through the real trust flow. It observed
the required lifecycle, prompt, Bash, apply_patch, Stop, and SessionEnd
hooks, and a strict safe-development PreToolUse denial. The existing user
Codex config checksum was unchanged.

## Red-green-refactor record

Tests were written against the requested public crate boundary before the
implementation modules existed.

### RED: hook fixtures

Command:

~~~text
cargo test -p codegotchi-cli --test hook_fixtures
~~~

Captured result: exit 101. The relevant compiler output was:

~~~text
Compiling codegotchi-cli v0.1.0 (/home/laurent/codegatchi/crates/codegotchi-cli)
error[E0432]: unresolved import codegotchi_cli
 --> crates/codegotchi-cli/tests/hook_fixtures.rs:5:5
  |
5 | use codegotchi_cli::{
  |     ^^^^^^^^^^^^^^ use of unresolved module or unlinked crate codegotchi_cli
  |
  = help: if you wanted to use a crate named codegotchi_cli, use cargo add codegotchi_cli to add it to your Cargo.toml

For more information about this error, try rustc --explain E0432.
error: could not compile codegotchi-cli (test "hook_fixtures") due to 1 previous error
~~~

The first run also recorded Cargo adding the test-only sha2 dependency
tree to the lockfile. The complete captured log is
/tmp/codegotchi-task-1-red-hook.log from the working session.

### RED: profile lifecycle

Command:

~~~text
cargo test -p codegotchi-cli --test profile_lifecycle
~~~

Captured result: exit 101. The relevant compiler output was:

~~~text
Compiling codegotchi-cli v0.1.0 (/home/laurent/codegatchi/crates/codegotchi-cli)
error[E0432]: unresolved import codegotchi_cli
 --> crates/codegotchi-cli/tests/profile_lifecycle.rs:5:5
  |
5 | use codegotchi_cli::TemporaryCodexProfile;
  |     ^^^^^^^^^^^^^^ use of unresolved module or unlinked crate codegotchi_cli
  |
  = help: if you wanted to use a crate named codegotchi_cli, use cargo add codegotchi_cli to add it to your Cargo.toml

For more information about this error, try rustc --explain E0432.
error: could not compile codegotchi-cli (test "profile_lifecycle") due to 1 previous error
~~~

The complete captured log is /tmp/codegotchi-task-1-red-profile.log from
the working session.

### GREEN: focused tests

Command:

~~~text
cargo test -p codegotchi-cli --test hook_fixtures
~~~

Output summary:

~~~text
running 7 tests
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
~~~

Command:

~~~text
cargo test -p codegotchi-cli --test profile_lifecycle
~~~

Output summary:

~~~text
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
~~~

The focused tests cover exact installed-schema fixtures for SessionStart,
SessionEnd, UserPromptSubmit, Bash pre/post success/post failure,
apply_patch pre/post, Stop, unknown tools, future fields, and malformed
JSON. They assert deterministic IDs, exact activity/kind mapping, structured
command categories, privacy discard, unknown-work behavior, fail-open output,
and the exact Strict denial JSON shape. Profile tests assert conflict refusal,
base checksum preservation, mode 0600, all six nested event families,
CODEGOTCHI_SESSION_FILE, and exact-file cleanup.

### REFACTOR and final gates

The final bounded metadata reader was changed from unbuffered byte iteration to
Read::read_to_end over a capped reader after Clippy identified the former as
inefficient. Dead test-only helpers and an unused transport helper were also
removed.

Commands and results:

~~~text
cargo fmt --all -- --check
exit 0

cargo clippy --workspace --all-targets --all-features -- -D warnings
Finished dev profile [unoptimized + debuginfo] target(s) in 0.45s
exit 0

cargo test --workspace --all-targets
test result: ok. 7 passed; 0 failed   (codegotchi-cli hook fixtures)
test result: ok. 2 passed; 0 failed   (codegotchi-cli profile lifecycle)
test result: ok. 18 passed; 0 failed  (codegotchi-domain unit tests)
test result: ok. 13 passed; 0 failed  (care flow)
test result: ok. 2 passed; 0 failed   (event replay)
test result: ok. 5 passed; 0 failed   (permission matrix)
test result: ok. 3 passed; 0 failed   (Task 1 contract)
test result: ok. 8 passed; 0 failed   (Task 2 contract)
test result: ok. 11 passed; 0 failed  (Task 2 corrections)
exit 0
~~~

cargo fmt --all, cargo fmt --all -- --check, cargo clippy ..., and
git diff --check were also run after the final source changes.

## Exact installed Codex experiment evidence

Installed executable and version:

~~~text
/home/laurent/.nvm/versions/node/v24.15.0/bin/codex
codex-cli 0.146.0
~~~

Installed facts recorded before implementation and confirmed by the spike:

- codex features list reported hooks stable true.
- codex --help exposed --profile <CONFIG_PROFILE_V2> as the
  $CODEX_HOME/<name>.config.toml profile layer.
- codex --help exposed --dangerously-bypass-hook-trust; this flag was
  never passed.
- The accepted profile shape used [features] hooks = true, one
  [[hooks.<Event>]] entry per event, and one nested
  [[hooks.<Event>.hooks]] command handler with type = "command".
- Real installed tool names were Bash and apply_patch; both supplied the
  command under tool_input.command.
- The denial output used only the documented
  hookSpecificOutput.hookEventName, permissionDecision, and
  permissionDecisionReason fields.

The production hook command was run with a temporary profile in the existing
/home/laurent/.codex directory, a temporary mode-0600 runtime metadata file,
a loopback capture receiver, and a throwaway git repository. The first
untrusted run produced no callbacks. The real run then used Codex's trust
prompts exactly as follows:

~~~text
You are in /tmp/.../scratch. Do you trust contents of this directory?
1. Yes, continue
~~~

The selection was 1. Yes, continue. Codex next displayed:

~~~text
Hooks need review
6 hooks are new or changed.
Hooks can run outside the sandbox after you trust them.
1. Review hooks
2. Trust all and continue
3. Continue without trusting (hooks won't run)
~~~

The selection was 2. Trust all and continue. The bypass flag was not used.

Sanitized receiver/harness output:

~~~text
codex_status=exit status: 0
hook_events=session_started:idle:-:-,turn_started:thinking:-:-,tool_started:thinking:printf:shell,tool_completed:thinking:printf:shell,tool_started:editing:apply_patch:development,tool_completed:editing:apply_patch:development,turn_completed:waiting:-:-,session_ended:idle:-:-,session_started:idle:-:-,turn_started:thinking:-:-,tool_started:thinking:printf:shell,tool_completed:thinking:printf:shell,tool_started:editing:apply_patch:development,tool_completed:editing:apply_patch:development,tool_started:testing:cargo:development,turn_completed:waiting:-:-,session_ended:idle:-:-,session_ended:idle:-:-
patch_marker_created=true
~~~

The captured sequence contains two SessionStart boundaries; the labels are
reported verbatim without inferring which process phase produced each one.
The Bash and apply_patch actions emitted both
PreToolUse/PostToolUse pairs. The final safe-development
tool_started:testing:cargo:development has no tool_completed event: its
Strict PreToolUse was denied by the loopback response. The session then
completed with Stop/SessionEnd, and the apply-patch marker was present.
Only the summarized labels above were retained; raw prompts, source,
complete command text, and tool output were not persisted.

The base user config checksum was captured before and after the spike:

~~~text
before=d1495adcbc6fab6465db015d92f4c5c7f126cc1a0e558537699973ec1f401833
after =d1495adcbc6fab6465db015d92f4c5c7f126cc1a0e558537699973ec1f401833
base_config_checksum_preserved=0
~~~

The actual base config had no [hooks] block, so real execution with an
existing user hook was not possible in this environment. The profile test
used a synthetic existing SessionStart hook and verified that its file
checksum stayed unchanged while the CodeGotchi profile contained all six
event families.

Cleanup verified that the generated profile files under
/home/laurent/.codex/codegotchi-task-1-spike-*, generated runtime metadata,
temporary receiver data, and throwaway repository were removed. No user
configuration, credentials, or unrelated files were removed. The only
remaining /tmp files from the working session are the red-phase logs named
above; they are outside the repository and contain compiler diagnostics only.

## Implementation files

Modified:

- Cargo.toml: added crates/codegotchi-cli to the workspace.
- Cargo.lock: recorded the CLI and test dependency graph.

Added under crates/codegotchi-cli:

- Cargo.toml: crate metadata and narrow domain/serde dependencies.
- src/main.rs, src/cli.rs: stdout-disciplined codegotchi hook entrypoint.
- src/protocol.rs: tolerant hook input, runtime metadata, event envelope,
  response envelope, and exact hook output serialization.
- src/runtime_metadata.rs: bounded mode-0600 metadata write/read/remove.
- src/codex_profile.rs: additive profile rendering, child environment,
  conflict refusal, and owned-file cleanup.
- src/classify.rs: privacy-preserving command classification and activity
  mapping.
- src/codex_hook.rs: translation, deterministic IDs, loopback-only HTTP,
  timeout/bounds, and fail-open behavior.
- src/lib.rs: public test and future-runtime seam.
- tests/hook_fixtures.rs and tests/profile_lifecycle.rs.
- tests/fixtures/hooks/*.json: sanitized installed-schema fixtures.

Added documentation:

- docs/adr/0002-codex-hook-profile-integration.md.
- This report at
  .superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-1-report.md.

## Self-review

- [x] Only AgentEvent, ActivityKind, AgentEventKind, EventMetadata, and
  structured permission classification types cross from the domain.
- [x] Runtime metadata has schema version 1, runtime UUID, repository root,
  loopback URL, bearer token, and owning PID, serialized as camelCase.
- [x] Hook output allow is exactly {}; denial is only the documented
  hookSpecificOutput shape.
- [x] Unknown fields and unknown tools fail open; unknown tool activity is
  generic UnknownWork.
- [x] Raw prompt, source, command, patch, and tool output values are excluded
  from AgentEvent and EventIngestRequest.
- [x] Hook stdin and HTTP response bodies are bounded; loopback HTTP has a
  250ms connect/read/write timeout.
- [x] Existing Codex config is not copied or modified; profile creation uses
  create-new mode 0600 and refuses collisions.
- [x] Hook trust was exercised through Codex's actual review flow; no trust
  bypass was used.
- [x] Cleanup targets only generated profile/metadata/spike paths.
- [x] No backend/server/persistence/UI/launcher work was added.
- [x] Rust format, strict Clippy, workspace tests, focused tests, and diff
  whitespace checks pass.

## Concerns

1. The actual user config had no hook block, so direct co-execution with a
   real pre-existing user hook was not observable. Additive coexistence is
   covered structurally by the profile test and base checksum preservation;
   the launcher should repeat the check on a machine with a real user hook.
2. The loopback client is intentionally a small HTTP/1.1 implementation for
   this hook seam. It accepts only http://127.0.0.1:<port> or
   http://localhost:<port> and expects a bounded non-chunked JSON response.
   A future backend must preserve that contract or replace the client as one
   coordinated change.

## Commits

- d9e9d933c36bada17acfec64a4d5f6f609e9a65d — feat: add Codex hook integration gate
  (implementation, fixtures, tests, and ADR).
- The documentation commit containing this report follows the implementation
  commit; its hash is returned in the final handoff because a report cannot
  embed its own commit hash without changing that hash.

## Correction round: RED/GREEN evidence

Date: 2026-08-05
Reviewed base: `568ebfb`
Implementation commit: `8a1a0be` (`fix: close Task 1 Codex hook review blockers`)

### RED

The new correction assertions were run before their implementation:

~~~text
cargo test -p codegotchi-cli --test hook_fixtures
error[E0609]: no field `tool_response` on type `HookInput`
~~

~~~text
cargo test -p codegotchi-cli --test profile_lifecycle
test installed_binary_target_is_codegotchi_and_runs_the_generated_hook_command ... FAILED
the crate must install an explicit codegotchi binary
~~

These failures demonstrate the missing official response field and missing
installed binary target rather than silently accepting the old behavior.

### GREEN

After the correction implementation:

~~~text
cargo test -p codegotchi-cli --test hook_fixtures
test result: ok. 9 passed; 0 failed; 0 ignored

cargo test -p codegotchi-cli --test profile_lifecycle
test result: ok. 3 passed; 0 failed; 0 ignored

cargo test -p codegotchi-cli --test installed_codex --no-run
Finished `test` profile

cargo fmt --all -- --check
exit 0

cargo clippy --workspace --all-targets --all-features -- -D warnings
Finished `dev` profile
~~~

The focused tests now use official `turn_id`, `tool_use_id`, and
`tool_response` fixtures; prove replay idempotence and distinct repeated
prompt/tool IDs; prove canonical executable labels do not retain one-token
secrets, paths, or assignments; preserve known-command classification after an
assignment prefix; and prove the generated `codegotchi hook` binary runs.

The committed ignored manual gate is in
`crates/codegotchi-cli/tests/installed_codex.rs`. It uses the generated binary,
normal Codex 0.146.0 trust prompts without a bypass, a bearer-authenticated
loopback receiver, a strict denial assertion, a temporary base config with a
disposable pre-existing hook that consumes stdin via `cat >/dev/null`, and
exact cleanup under a temporary `CODEX_HOME`. It is intentionally not run by
routine tests. The exact command is documented in ADR 0002:

~~~text
OPENAI_API_KEY="$OPENAI_API_KEY" cargo test -p codegotchi-cli --test installed_codex -- --ignored --nocapture
~~~

### Exact implementation files

Commit `8a1a0be` contains the correction in:

- `crates/codegotchi-cli/Cargo.toml`
- `crates/codegotchi-cli/src/classify.rs`
- `crates/codegotchi-cli/src/codex_hook.rs`
- `crates/codegotchi-cli/src/protocol.rs`
- `crates/codegotchi-cli/tests/hook_fixtures.rs`
- `crates/codegotchi-cli/tests/profile_lifecycle.rs`
- `crates/codegotchi-cli/tests/installed_codex.rs`
- `crates/codegotchi-cli/tests/fixtures/hooks/apply_patch_post.json`
- `crates/codegotchi-cli/tests/fixtures/hooks/apply_patch_pre.json`
- `crates/codegotchi-cli/tests/fixtures/hooks/bash_post_failure.json`
- `crates/codegotchi-cli/tests/fixtures/hooks/bash_post_success.json`
- `crates/codegotchi-cli/tests/fixtures/hooks/bash_pre.json`
- `crates/codegotchi-cli/tests/fixtures/hooks/bash_pre_repeat.json`
- `crates/codegotchi-cli/tests/fixtures/hooks/session_end_repeat.json`
- `crates/codegotchi-cli/tests/fixtures/hooks/session_start_clear.json`
- `crates/codegotchi-cli/tests/fixtures/hooks/session_start_compact.json`
- `crates/codegotchi-cli/tests/fixtures/hooks/session_start_resume.json`
- `crates/codegotchi-cli/tests/fixtures/hooks/stop.json`
- `crates/codegotchi-cli/tests/fixtures/hooks/user_prompt_submit.json`
- `crates/codegotchi-cli/tests/fixtures/hooks/user_prompt_submit_future_fields.json`
- `crates/codegotchi-cli/tests/fixtures/hooks/user_prompt_submit_repeat.json`

## Final focused correction: RED/GREEN evidence

Date: 2026-08-05
Scope: only the two blockers from task-1-rereview.md; no backend, UI, or launcher changes

### RED

The lifecycle tests were added before the identity change. The relevant
failure from:

~~~
cargo test -p codegotchi-cli --test hook_fixtures
~~~

was:

~~~
test lifecycle_identity_uses_official_sources_and_preserves_exact_replay ... FAILED
assertion left != right failed
left: 71c74d99-b7d4-54b4-9c1b-cf5f5cd99a0c
right: 71c74d99-b7d4-54b4-9c1b-cf5f5cd99a0c
~~~

The typed receiver test was added before the ingest envelope became
deserializable. The relevant compiler failure from:

~~~
cargo test -p codegotchi-cli --test installed_codex
error[E0277]: the trait bound EventIngestRequest: serde::Deserialize is not satisfied
~~~

These RED results demonstrate both remaining defects: ID-less start sources
collapsed, and the gate could not parse its authenticated request as the
declared event envelope.

### GREEN

After the narrow implementation and cargo fmt --all, the focused suites
passed:

~~~
cargo test -p codegotchi-cli --test hook_fixtures
test result: ok. 11 passed; 0 failed; 0 ignored

cargo test -p codegotchi-cli --test installed_codex
test result: ok. 1 passed; 0 failed; 1 ignored

cargo test -p codegotchi-cli --test profile_lifecycle
test result: ok. 3 passed; 0 failed; 0 ignored
~~~

The lifecycle adapter now uses SessionStart.source and SessionEnd.reason as
bounded identity inputs. Exact replay remains stable; startup, resume, clear,
and compact starts produce distinct IDs. Codex 0.146.0 fixes SessionEnd.reason
to other and supplies no occurrence ID, so an identical repeated ID-less
SessionEnd payload remains the same ID. Distinguishing identical delivery
from replay would require persisted state or an invented wire field, so
neither was added. This is the remaining schema boundary and is intentionally
documented in ADR 0002.

The ignored receiver gate now deserializes authenticated JSON as
EventIngestRequest and counts/denies only canonical
AgentEventKind::ToolStarted events with EventMetadata.executable_name ==
cargo. A routine non-ignored test covers the predicate. The manual gate
prepends a disposable fake cargo to PATH, writes a sentinel only if it is
executed, and asserts the sentinel is absent after the real strict denial. It
continues to use the generated codegotchi binary, bearer-authenticated
loopback transport, a disposable coexisting hook, normal trust prompts without
--dangerously-bypass-hook-trust, and exact cleanup.

The real ignored gate could not be completed in this environment because no
OPENAI_API_KEY or CODEX_API_KEY was available. It therefore stops before
creating its temporary home with:

~~~
run the manual gate with OPENAI_API_KEY or CODEX_API_KEY set
~~~

This is a paid/authenticated manual-test limitation, not a routine-test
failure. The exact command remains:

~~~
OPENAI_API_KEY="$OPENAI_API_KEY" cargo test -p codegotchi-cli --test installed_codex -- --ignored --nocapture
~~~

### Final verification gates

~~~text
cargo fmt --all -- --check
exit 0

cargo clippy --workspace --all-targets --all-features -- -D warnings
Finished `dev` profile

cargo test --workspace
codegotchi-cli: 11 passed; 0 failed; 1 ignored
codegotchi-cli profile lifecycle: 3 passed; 0 failed
codegotchi-domain unit tests: 18 passed; 0 failed
care flow: 13 passed; 0 failed
event replay: 2 passed; 0 failed
permission matrix: 5 passed; 0 failed
Task 1 contract: 3 passed; 0 failed
Task 2 contract: 8 passed; 0 failed
Task 2 corrections: 11 passed; 0 failed
exit 0
~~~
