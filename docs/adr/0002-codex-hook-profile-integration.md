# ADR 0002: Codex hook/profile integration

Status: Accepted for Task 1  
Date: 2026-08-05

## Decision

Task 1 uses an additive, short-lived Codex profile and a short-lived
`codegotchi hook` subprocess. The profile is created under the existing
`CODEX_HOME` as `$CODEX_HOME/<profile>.config.toml`, is opened with mode
`0600` and create-new semantics, and is removed only by the owner that created
it. The base Codex configuration is neither copied nor modified. The child
Codex process receives the generated runtime metadata path in
`CODEGOTCHI_SESSION_FILE` and the profile name through `--profile`.

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
`hook_event_name`, `tool_name`, `tool_input.command`, `tool_output.exit_code`,
`tool_output.duration_ms`, `cwd`, `prompt`, `source`,
`stop_hook_active`, and `last_assistant_message`; the adapter accepts unknown
future fields and discards prompt, source, command text, and tool output before
constructing the domain event. Installed tool names observed in the spike
were `Bash` and `apply_patch`.

The tested denial response is exactly:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"..."}}
```

These facts agree with the official Codex hooks documentation:
<https://developers.openai.com/codex/hooks/> (accessed 2026-08-05).

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

The actual user config at `/home/laurent/.codex/config.toml` had no existing
`[hooks]` block in this environment. Its SHA-256 remained exactly
`d1495adcbc6fab6465db015d92f4c5c7f126cc1a0e558537699973ec1f401833` before
and after the spike. The profile lifecycle test also used a base config with
an existing user `SessionStart` hook and verified checksum preservation while
the six profile hook families were added in a separate profile layer. Direct
execution with a real pre-existing user hook was not exercised because none
was present.

The throwaway profile, runtime metadata, temporary receiver state, and
temporary repository were removed. Cleanup used exact generated paths only;
no user config, credentials, or unrelated files were removed.

## Consequences and follow-up

This gives later tasks a tested event and permission-response seam without
introducing a backend or persistence implementation into Task 1. The future
runtime must generate a unique profile name, write and remove the mode-0600
metadata file, bind the receiver only to loopback, and pass the user's normal
Codex arguments without allowing a conflicting user profile override. Real
existing-hook coexistence remains a follow-up acceptance check when the
launcher exists.
