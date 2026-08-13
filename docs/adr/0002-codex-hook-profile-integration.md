# ADR 0002: Codex hook/profile integration

Status: Accepted for Task 1; persistent profile lifecycle and Codex 0.147 trust amendment
Date: 2026-08-05

## Decision

Task 1 uses an additive, persistent Codex profile and a short-lived
`codegotchi hook` subprocess. The `codegotchi-cli` package explicitly publishes
the installed binary as `codegotchi`, so the generated command names a binary
provided by `cargo install`. The profile is created under the existing
`CODEX_HOME` as `$CODEX_HOME/<profile>.config.toml`. The complete rendered
hook bytes are hashed with UUID-v5 to derive the stable profile name. The
profile is created with mode `0600`, or reused only when an existing path is a
regular mode-0600 file with byte-identical contents. Altered contents, unsafe
permissions, symlinks, directories, non-files, and unreadable paths are
rejected without overwriting them. New profiles are fully written and synced
through a private uniquely named temporary file, then atomically published
with no-replace semantics in the same directory. The base Codex configuration
is neither copied nor modified. Immediately before launching Codex, the
launcher revalidates the profile and holds a cooperative directory guard
through command construction and child spawn. The child Codex process
receives the generated runtime metadata path in `CODEGOTCHI_SESSION_FILE` and
the profile name through `--profile`.

The guard serializes CodeGotchi's own profile writers only. It cannot protect
against privileged or non-cooperating writers, and it cannot bind Codex to an
open file descriptor because Codex accepts a profile name rather than an fd.

The launcher removes only unique runtime metadata and shuts down its loopback
server on normal exit, spawn/wait failure, or forwarded termination. The
persistent profile remains for approve-once hook trust and later exact reuse;
changed rendered hook bytes receive a new profile identity and leave older
profiles untouched.

### Codex 0.147 managed trust metadata

Codex CLI 0.147 persists normal hook approval by mutating the selected profile.
The observed form is a leading
`approvals_reviewer = "auto_review"` line, the exact pristine CodeGotchi
rendered bytes, then `[hooks.state]` entries. Each entry is keyed by the exact
profile path plus one of `pre_tool_use`, `post_tool_use`, `session_start`,
`session_end`, `user_prompt_submit`, or `stop`, followed by `:0:0`, and contains
only `trusted_hash`. CodeGotchi accepts that extension only when all six keys
are present exactly once, no other table or field is present, every expected
hook definition and command byte is unchanged, and every hash equals Codex's
normalized identity for that event, command, timeout (`600`, or `1` for
`SessionEnd`), and `async = false`. The state-entry order is not significant;
Codex writes it as a map. A valid approved file is preserved byte-for-byte and
is never rewritten. The launcher uses the same validator during
spawn-boundary revalidation. Any altered command, extra config, foreign or
malformed state entry, unsafe permission, symlink, or non-file collision is
still rejected before Codex starts.

The profile enables the installed hooks feature and registers one command
handler for each supported event family:

`SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`,
and `Stop`.

The inline configuration shape verified against the installed CLI is:

```toml
[features]
hooks = true

[[hooks.SessionStart]]

[[hooks.SessionStart.hooks]]
type = "command"
command = "codegotchi hook"
```

The same nested handler shape is used for the other five event families.
The hook reads one bounded JSON object from stdin, retains only structured
metadata, posts a canonical `AgentEvent` to the authenticated loopback
endpoint from `RuntimeMetadataV1`, and emits `{}` unless an authenticated,
successfully evaluated Strict `PreToolUse` response explicitly denies the
action. Transport, parsing, metadata, and translation failures fail open.

The adapter models the official Codex 0.146.0 identity and response fields:
turn-scoped hooks use `turn_id`, tool hooks use `tool_use_id`, and
`PostToolUse` uses `tool_response`. The deterministic event ID includes
`tool_use_id` or `turn_id` when present, plus the event boundary, so replaying
one payload is idempotent while repeated prompt/tool invocations remain
distinct. For ID-less lifecycle hooks it includes the official `SessionStart`
`source` (`startup`, `resume`, `clear`, or `compact`) and the supplied
`SessionEnd` `reason` when present. In Codex 0.146.0 that reason is fixed to
`other`, so an identical lifecycle payload remains the same ID: Codex supplies
no occurrence ID for an identical repeated `SessionEnd` delivery, and
distinguishing that repeat from replay would require state or an invented wire
field. This adapter does neither. The old
`tool_call_id` and `tool_output` spellings are accepted only as deserialization
aliases for compatibility; fixtures use the official names.

Only canonical labels from the adapter's bounded executable allowlist can
enter `EventMetadata.executable_name`. Unknown one-token commands, paths, and
assignment-only prefixes produce no executable label. Leading shell assignment
prefixes are skipped for classification, so a known command such as
`CODEGOTCHI_SECRET=... cargo test` remains Testing/Development without storing
the assignment or command text.

## Installed facts verified

The installed executable was:

```text
/home/laurent/.nvm/versions/node/v24.15.0/bin/codex
codex-cli 0.146.0
```

`codex features list` reported the `hooks` feature as stable and enabled.
`codex --help` exposed `--profile <CONFIG_PROFILE_V2>` as a layer at
`$CODEX_HOME/<name>.config.toml`, and also exposed
`--dangerously-bypass-hook-trust`. The latter was never passed.

The observed installed payload fields were `session_id`,
`hook_event_name`, `turn_id`, `tool_name`, `tool_use_id`,
`tool_input.command`, `tool_response`, `cwd`, `prompt`, `source`, `reason`,
`stop_hook_active`, and `last_assistant_message`; the adapter accepts unknown
future fields and discards prompt, source, reason, command text, and tool output
from the domain event. Structured fixture payloads exercise
`tool_response.exit_code` and `tool_response.duration_ms`. In the final real
Codex 0.146.0 acceptance, `tool_response` was a model-facing string: both
silent `true` and silent `false` produced the same empty string, so their exit
status is honestly left unknown. For recognized test/build activity, the
adapter can infer an outcome from narrow observable result markers such as a
Cargo test-list count or a leading Cargo error, without retaining the raw
response; unrecognized text stays neutral. Source/reason are used only as
bounded lifecycle ID inputs. Installed tool names observed in the spike were
`Bash` and `apply_patch`. The pinned 0.146 schema defines the four SessionStart
source values and fixes SessionEnd reason to `other`.

The tested denial response is exactly:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"..."}}
```

These facts agree with the official Codex hooks documentation:
<https://developers.openai.com/codex/hooks/> and the pinned 0.146.0 hook
schema at
<https://github.com/openai/codex/blob/rust-v0.146.0/codex-rs/hooks/src/schema.rs>
(accessed 2026-08-05).

## Trust and real-spike evidence

The first throwaway run from an untrusted temporary repository produced no
hook callbacks. The real run then used Codex's trust flow. Codex displayed:

```text
You are in /tmp/.../scratch. Do you trust contents of this directory?
1. Yes, continue
```

After that, Codex displayed:

```text
Hooks need review
6 hooks are new or changed.
Hooks can run outside the sandbox after you trust them.
1. Review hooks
2. Trust all and continue
3. Continue without trusting (hooks won't run)
```

The spike selected `Yes, continue` and then `Trust all and continue`. It did
not use hook-trust bypass. The sanitized capture summary was:

```text
codex_status=exit status: 0
hook_events=session_started:idle:-:-,turn_started:thinking:-:-,tool_started:thinking:printf:shell,tool_completed:thinking:printf:shell,tool_started:editing:apply_patch:development,tool_completed:editing:apply_patch:development,turn_completed:waiting:-:-,session_ended:idle:-:-,session_started:idle:-:-,turn_started:thinking:-:-,tool_started:thinking:printf:shell,tool_completed:thinking:printf:shell,tool_started:editing:apply_patch:development,tool_completed:editing:apply_patch:development,tool_started:testing:cargo:development,turn_completed:waiting:-:-,session_ended:idle:-:-,session_ended:idle:-:-
patch_marker_created=true
```

The final `tool_started:testing:cargo:development` has no corresponding
`tool_completed` event: the loopback receiver returned the strict denial for
that safe-development `PreToolUse`. The other Bash and `apply_patch` actions
produced both pre and post events. The capture retained only the sanitized
summary; prompts, source, complete commands, and tool output were not written
to the repository or generated runtime metadata.

## Coexistence and cleanup observations

The committed ignored test at
`crates/codegotchi-cli/tests/installed_codex.rs` now supplies the missing
coexistence gate. It creates a disposable `CODEX_HOME` with a real
pre-existing `SessionStart` command hook. That hook consumes stdin with
`cat >/dev/null` and writes only a fixed marker, so it co-executes without
persisting the Codex payload. The test uses the generated `codegotchi` binary
through `PATH`, an authenticated `127.0.0.1` capture receiver, and a temporary
mode-0600 runtime file. The receiver returns the strict denial for canonical
`cargo` work and the test asserts the exact denial JSON from the generated
binary as well as a denial during the real Codex run. It parses the
authenticated POST as `EventIngestRequest` and counts/denies only
`AgentEventKind::ToolStarted` events whose canonical executable label is
`cargo`; a routine test covers the predicate. The real run prepends a
disposable fake `cargo` to `PATH`; its sentinel must remain absent after the
denial, proving the denied command did not execute.

The test never uses `--dangerously-bypass-hook-trust`; it inherits the
operator's terminal so the normal repository trust and “Hooks need review”
prompts are answered manually. It uses a temporary Codex home and an API key
environment variable, never the user's base config or credentials. On
success it verifies the disposable base config checksum, confirms the
persistent profile remains byte-identical, removes the runtime file by exact
path, removes the marker/hook, and removes the exact temporary root.

The persistent profile remains until its disposable Codex home is removed with
the test root. Runtime metadata, temporary receiver state, and the temporary
repository are removed. Cleanup uses exact generated paths only; no user
config, credentials, or unrelated files are removed.

## Focused correction evidence

The hook/runtime correction was TDD'd against the review blockers. The
lifecycle RED run showed the new start-source distinction assertion failing
because all ID-less `SessionStart` payloads still produced
`71c74d99-b7d4-54b4-9c1b-cf5f5cd99a0c`. The receiver RED run failed to compile
with `EventIngestRequest: serde::Deserialize<'de>` not implemented. After the
implementation, the focused GREEN commands passed:

```text
cargo test -p codegotchi-cli --test hook_fixtures
test result: ok. 12 passed; 0 failed; 0 ignored

cargo test -p codegotchi-cli --test installed_codex
test result: ok. 1 passed; 0 failed; 1 ignored

cargo test -p codegotchi-cli --test profile_lifecycle
test result: ok. 11 passed; 0 failed; 0 ignored
```

The automated installed-Codex test currently reports one routine test passed
and one intentionally ignored manual gate. On 2026-08-13, the ignored gate was
run successfully against authenticated `codex-cli 0.147.0` by copying the
private auth file and already-approved persistent profile into a disposable
Codex home. The real client created the requested marker, attempted exactly
`cargo --version`, and received the expected strict `PreToolUse` denial before
the command executed. The gate also verified that the source auth/profile
bytes and source-home entries were unchanged. It remains intentionally manual
because it requires authenticated Codex access and normal hook trust. Its exact
command is:

```text
CODEGOTCHI_AUTH_FILE=/home/laurent/.codex/auth.json cargo test -p codegotchi-cli --test installed_codex installed_codex_real_trust_and_coexistence_gate -- --ignored --nocapture
```

## Consequences and follow-up

This gives later tasks a tested event and permission-response seam without
introducing a backend or persistence implementation into Task 1. The runtime
derives a stable profile identity from exact rendered hook bytes, safely
reuses only verified mode-0600 files, writes and removes the mode-0600 metadata
file, binds the receiver only to loopback, and passes the user's normal Codex
arguments without allowing a conflicting user profile override.

The exact manual command is:

```text
OPENAI_API_KEY="$OPENAI_API_KEY" cargo test -p codegotchi-cli --test installed_codex -- --ignored --nocapture
```

Alternatively, set `CODEGOTCHI_CODEX_BIN` to a specific installed `codex`
executable and use `CODEX_API_KEY` instead of `OPENAI_API_KEY`. The ignored
test is not part of routine test execution.
