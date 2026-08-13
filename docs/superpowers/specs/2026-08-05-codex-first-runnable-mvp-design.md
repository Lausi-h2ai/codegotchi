# CodeGotchi Codex-First Runnable MVP Design

## Status and authority

This design turns the 2026-08-05 Codex-first MVP requirements into a narrow
vertical slice. The user-provided milestone specification is the product
authority. The repository's Phase 1-2 design remains authoritative for the
domain boundary and deterministic simulation.

The repository arrived without Git history, task reports, review reports, or a
progress ledger. Its complete Phase 1-2 tree was preserved in baseline commit
`488c9a1` before MVP work began. All Rust gates and all pinned Corepack pnpm
gates passed against that baseline.

## Approaches considered

### In-process Rust runtime with SQLite snapshot and embedded React bundle — selected

Add one `codegotchi-cli` crate. Its `run` command owns the authoritative domain
simulation, SQLite connection, loopback Axum server, WebSocket broadcaster,
runtime metadata, persistent content-addressed Codex profile, browser launch, and child Codex
process. Its `hook` command is a short-lived client that translates one Codex
hook payload and posts a privacy-limited canonical event. The Vite production
build is copied into the CLI crate and embedded in the installed binary.

This is the smallest architecture that supports one-command installation,
restart persistence, hook subprocess discovery, real browser care actions, and
transparent Codex execution without a permanent daemon.

### Separate Rust daemon and thin CLI

A background daemon would separate lifetimes cleanly and make concurrent agent
sessions easier later. It adds service discovery, daemon ownership, upgrades,
stale-process management, and a second installed executable before the MVP
needs them. It is deferred.

### TypeScript backend beside the Rust domain

A Node backend would shorten frontend integration but would either duplicate
the Phase 2 rules or require a Rust bridge. It would also make `cargo install`
insufficient. It is rejected for this milestone.

## Runtime architecture

`codegotchi run -- codex [arguments...]` resolves the repository and state
paths, opens or initializes SQLite, restores a versioned domain snapshot,
starts an Axum server on an ephemeral `127.0.0.1` port, writes a mode-0600
runtime metadata file with a random bearer token, renders the complete hook
configuration and ensures its stable UUID-v5 content-addressed layered Codex
profile under the existing `CODEX_HOME`, opens the embedded UI with the token
in a URL fragment, and runs the real Codex executable with inherited terminal
streams.

The child inherits `CODEGOTCHI_SESSION_FILE`. Every configured hook invokes the
same installed binary as `codegotchi hook`. The hook reads at most one bounded
JSON object from stdin, accepts unknown fields, discards prompt/source/output
content after classification, produces a deterministic canonical event ID,
and posts it to the active loopback server. Communication errors return an
empty valid JSON object and never block. Only an authenticated, successfully
evaluated Strict-mode `PreToolUse` denial emits Codex's current
`hookSpecificOutput.permissionDecision = "deny"` shape.

The server advances and mutates `PetSimulation`; the browser only displays
snapshots and submits authenticated idempotent care actions. Each accepted
event or care action is persisted in a transaction before a replacement state
snapshot is broadcast. A fresh WebSocket receives a complete snapshot.

## Persistence

SQLite stores one versioned JSON snapshot per repository identity plus its
enforcement mode. The domain gains only the serialization and validated
restore seam demonstrated necessary by this integration. It does not gain SQL,
HTTP, filesystem, or Codex concepts. Event and care replay sets remain in the
snapshot, so idempotency survives restart.

Default inventory is seeded only for a newly created pet. Existing inventory,
poops, needs, timestamps, work/digestion points, session state, outcome state,
and replay IDs are restored exactly.

## Browser interaction

The existing React room becomes a functional projection. It displays the pet,
desk, food, feed target, shovel, authoritative poop objects, trash, connection
state, needs, enforcement mode, and a readable activity label. CSS classes and
labels distinguish every mandatory presentation state.

Food uses HTML drag and drop onto the pet/feed target. Cleaning is a bounded
three-step state machine: select/drag shovel, apply it to one poop, then dispose
at trash. The clean request is not sent before disposal. Successful responses
come back as authoritative snapshots and visibly trigger eating/cleaning
feedback. Reconnection discards stale browser state and accepts the next
complete backend snapshot.

## Safety, privacy, and errors

The listener binds only to `127.0.0.1`. All POST and WebSocket routes require
the runtime token. Bodies are bounded before JSON parsing. Errors use a stable
JSON envelope with a machine code and human message. No endpoint executes a
command supplied over HTTP.

Runtime metadata and persistent profile files use mode 0600. Metadata contains
no prompt or source content. Existing Codex config and credentials are read by
Codex through normal layering and are never copied or modified. A conflicting
user `--profile`/`-p` argument fails before runtime startup. An unchanged
profile is reused only when its regular-file mode and bytes match exactly;
altered, unsafe, symlink, directory, non-file, or unreadable collisions are
rejected without overwrite. Cleanup removes only unique runtime metadata and
the loopback server for the current run; persistent profiles remain for
approve-once hook trust, and stale metadata is ignored when its PID is dead.

Raw prompts, complete commands, source text, tool output, and transcript data
are never persisted. Bash classification retains only executable name,
category, timing, result, and blocked status. Unknown tools and unknown command
shapes fail open as generic work.

## Codex integration facts verified before implementation

Installed Codex is `codex-cli 0.146.0`. Its help exposes
`--profile <CONFIG_PROFILE_V2>` as a layer from
`$CODEX_HOME/<name>.config.toml` over base user config, and exposes the stable
`hooks` feature. Current official Codex documentation says matching hooks from
multiple layers all run, non-managed hooks require explicit review/trust, and
the supported events include `SessionStart`, `SessionEnd`,
`UserPromptSubmit`, `PreToolUse`, `PostToolUse`, and `Stop`.

For this installed generation, shell and unified-exec tools report as `Bash`;
file edits report as `apply_patch`; both expose the command under
`tool_input.command`. A supported PreTool denial uses:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "..."
  }
}
```

The implementation must still capture sanitized real fixtures and verify these
facts against the installed CLI before backend/UI work continues.

## Testing strategy

Tests follow red-green-refactor. Rust integration tests exercise real SQLite,
real loopback HTTP, real WebSocket clients, a fake agent process, and actual
cleanup. Hook tests use sanitized installed-schema fixtures and a local test
receiver. Vitest exercises the real React UI contracts. Playwright runs the
essential room/care/reconnect flow against the production server and embedded
bundle. One final interactive Codex session supplies real acceptance evidence;
routine tests never invoke a paid Codex session.

## Explicit deferrals

Claude Code, Cursor, MCP, generic agent abstractions, background daemons,
event sourcing, remote access, cloud state, full shell interception, command
rewriting, sophisticated inventory, polished art, advanced animation,
multiple pets/species, petting, and unsupported hosted-tool visibility remain
out of scope.

## Definition of done

The milestone is done only when the exact installed command
`codegotchi run -- codex` has personally launched Codex interactively and all
mandatory activity, care, persistence, strict refusal/recovery, cleanup,
privacy, packaging, Rust, frontend, and Playwright checks are recorded in
`docs/verification/codex-first-mvp.md`.
