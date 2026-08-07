# CodeGotchi MVP architecture

CodeGotchi is a local, single-launcher product path for Codex. The Rust
process is the authority for pet state; the React application is an
authenticated projection of that state.

```text
codegotchi run -- codex [arguments...]
        |
        +-- SQLite snapshot for the canonical repository identity
        +-- Axum server on 127.0.0.1:<ephemeral-port>
        |      |
        |      +-- embedded production React bundle
        |      +-- authenticated HTTP state/event/care/mode/debug routes
        |      +-- authenticated WebSocket complete snapshots
        |
        +-- additive temporary Codex profile
               |
               +-- Codex hook subprocesses
                       `-- sanitized event -> authenticated Rust runtime
```

There is no daemon, second UI process, remote service, account, telemetry
channel, MCP adapter, or browser-side simulation in this MVP.

## Workspaces and authority

| Component | Responsibility | Authority |
| --- | --- | --- |
| `crates/codegotchi-domain` | Pet needs, activity, progression, poop generation, care validation, permission decisions, and versioned snapshots | Pure domain transitions |
| `crates/codegotchi-cli/src/runtime.rs` | Owns one live simulation, serializes mutations, persists accepted changes, and broadcasts snapshots | Live state authority |
| `crates/codegotchi-cli/src/persistence.rs` | Stores one validated snapshot row in repository-scoped SQLite | Restart state |
| `crates/codegotchi-cli/src/server.rs` | Loopback HTTP/WebSocket boundary, bearer authentication, static embedded assets, and typed errors | Network boundary |
| `crates/codegotchi-cli/src/codex_hook.rs` | Parses one bounded Codex hook payload, discards raw content, classifies it, and submits a canonical event | Codex ingestion boundary |
| `web/src` | Renders snapshots and submits authenticated feed/clean actions | No pet-state authority |

Every accepted event or care action is applied under the runtime lock, saved
before it is broadcast, and assigned an idempotency key. Duplicate event IDs
and care action IDs return success without applying a second transition. A new
WebSocket receives one complete `SimulationSnapshot` first, then complete
authoritative replacements.

## Launch and lifecycle

The only supported launcher shape is:

```text
codegotchi run -- codex [ordinary Codex arguments...]
```

The launcher validates that shape and rejects `-p`, attached `-p...`,
`--profile`, and `--profile=...` so it can inject its own additive profile.
It resolves the real Codex executable, or the test-only
`CODEGOTCHI_REAL_CODEX` override, before creating runtime files. Codex keeps
the caller's working directory, trailing arguments, stdin, stdout, stderr,
and exit status.

Startup creates the state/runtime directories, opens or initializes SQLite,
restores the snapshot, binds the server to loopback only, writes runtime
metadata, creates a unique profile, prints the UI URL, optionally starts a
native browser helper, and then starts Codex. The browser helper is best
effort; a printed URL remains usable if it cannot be opened.

The runtime URL has the form:

```text
http://127.0.0.1:<ephemeral-port>/#token=<high-entropy-token>
```

The token is in the URL fragment, not the HTTP request path or query. The
browser consumes it, removes the visible fragment with `history.replaceState`,
and retains it only in same-tab history state so a reload can reconnect. HTTP
requests use `Authorization: Bearer ...`; the browser WebSocket uses the same
token as its subprotocol. Static assets and health are not state-changing
routes; state, event, care, mode, debug, and stream routes require the bearer
token.

The Codex child inherits `CODEGOTCHI_SESSION_FILE`. Each configured hook runs
the same binary as `codegotchi hook`, so hooks can find the owning runtime
without a daemon or a fixed port. On normal exit, child spawn/wait failure,
and forwarded termination, CodeGotchi removes only the metadata and profile
created by that run. SQLite remains for restart persistence. A later launch
removes only stale, valid CodeGotchi session metadata whose owner is no longer
active; hard-killed profile reclamation and PID-start-identity hardening are
known follow-ups.

## State and persistence

The canonical repository identity is derived from the canonical Git worktree
root, with the canonical current directory as the fallback outside Git. The
snapshot is stored at:

```text
$XDG_STATE_HOME/codegotchi/state.sqlite
```

or, when `XDG_STATE_HOME` is unset:

```text
$HOME/.local/state/codegotchi/state.sqlite
```

The versioned snapshot contains pet identity/name/species, needs, behavior,
activity, outcomes, work and digestion points, timestamps, inventory, pending
poops, enforcement mode, session activity, and event/care replay sets. New
pets receive 50 kibble, 25 treats, and 25 fruit exactly once. Restarts restore
the cared-for state and replay IDs rather than reseeding it.

Short-lived metadata is placed in `$XDG_RUNTIME_DIR/codegotchi/`, falling back
to the CodeGotchi state directory. The directory is mode 0700 and each
`session-<uuid>.json` is mode 0600. A generated profile is placed in
`$CODEX_HOME`, falling back to `$HOME/.codex`, as a mode-0600
`codegotchi-<uuid>.config.toml`. Existing config, hooks, authentication, and
credentials are not copied, overwritten, or edited; Codex's normal profile
layering reads them together with the temporary additive profile.

## Codex hooks, privacy, and Strict mode

The hook bridge accepts the installed Codex lifecycle/tool schema and ignores
unknown fields. It reads a bounded payload, translates only structured facts
needed by the domain, and persists no prompt, source text, full command,
transcript, or complete tool output. Persisted event metadata is limited to
values such as a bounded executable name, coarse category, timing, exit
status, and blocked status. Unknown tools and uncertain command shapes remain
generic work and fail open.

Hook transport, malformed input, missing/stale metadata, authentication, and
server failures serialize as Codex's valid empty allow result:

```json
{}
```

In explicit Strict mode, the blocked tool-call set widens as neglect worsens:
mild neglect (hunger ≥ 70, energy ≤ 30, or cleanliness ≤ 30) blocks safe
development work, moderate neglect (85/15/15) also blocks recovery work, and
severe neglect (95/5/5) blocks every tool call except CodeGotchi control.
The dominant neglected need (hunger, then energy, then cleanliness) supplies
the denial reason and care guidance. A successful `PreToolUse` evaluation
returns Codex's documented `hookSpecificOutput` denial with the care and retry
guidance. CodeGotchi controls always stay allowed, and transport failures
remain fail-open. Strict is a care interaction, not a security boundary or an
OS sandbox.

The CLI mode command is:

```text
codegotchi mode strict
```

It must run with the active runtime's `CODEGOTCHI_SESSION_FILE` environment.
The fixed demo controls are deliberately guarded by the exact
`CODEGOTCHI_ENABLE_DEBUG=1` value:

```text
CODEGOTCHI_ENABLE_DEBUG=1 codegotchi debug neglect
CODEGOTCHI_ENABLE_DEBUG=1 codegotchi debug generate-poop
```

They accept no caller-supplied values and operate only through the active
authenticated runtime. They are for disposable demonstrations, not a general
mutation or command-execution interface.

## Browser and production packaging

`web` builds a Vite production bundle. `node web/scripts/embed-web.mjs` copies
that bundle into `crates/codegotchi-cli/web-dist`; the CLI build script embeds
the bytes at compile time. The installed binary therefore serves `/`, hashed
assets, and SPA fallback without reading the repository, running Vite, needing
Node/pnpm, or starting a second UI server.

The browser displays the complete backend snapshot, connection state, needs,
activity, inventory, and authoritative poops. Food is dragged to the feed
target. Cleanup requires the shovel, a selected poop, and the trash target;
the browser does not send a clean request before that sequence. Care responses
replace client state with the returned authoritative snapshot, and reload or
WebSocket reconnect obtains state again from Rust. Backend errors are shown as
typed code/message alerts.

The production Playwright flow builds and embeds first, starts the Rust fixture
server, and forwards a fixed local test port to that server's embedded assets,
HTTP routes, and WebSocket. It does not use Vite HMR/source files or a
production test-only mutation route.

## Verification boundary and limitations

Rust integration tests cover the compiled launcher, fake Codex, real hooks,
loopback HTTP, WebSocket snapshots, care/debug/mode routes, restart restore,
replay safety, privacy, and cleanup without a browser or paid Codex session.
Vitest covers the client/UI contracts. Production Playwright covers the
embedded room, activity, care/error/cleanup, refresh, and reconnect path when
the host has the pinned Chromium dependencies.

The supervisor owns the final interactive gate: install the binary, run the
exact launcher with the installed Codex, complete Codex's normal `/hooks`
trust review, observe real activity and care behavior, exercise Strict and
the guarded demos, restart, and inspect cleanup. Claude Code, other adapters,
remote access, accounts, telemetry, daemons, command rewriting, petting,
multiple species, and polished art are outside this MVP. Deferred hardening is
listed in [the follow-up backlog](backlog/codex-first-mvp-followups.md).
