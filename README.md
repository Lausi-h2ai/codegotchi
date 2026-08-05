# CodeGotchi

CodeGotchi is a local coding companion for Codex. Rust owns the pet
simulation, repository-scoped SQLite persistence, authenticated loopback
server, Codex hook bridge, and launcher lifecycle. The React room is only a
projection of that authoritative state.

## Install and launch

From a checkout with the embedded production bundle present:

```sh
cargo install --path crates/codegotchi-cli --locked
codegotchi run -- codex [ordinary Codex arguments...]
```

The launcher starts the runtime before Codex, prints a URL beginning with
`CodeGotchi UI:`, and best-effort opens it with the native browser helper. The
server binds only to `127.0.0.1` on an ephemeral port. Set
`CODEGOTCHI_BROWSER=none` to suppress automatic browser launch, or set it to a
browser-helper executable path. If the helper fails, the printed URL remains
usable.

The token is supplied only in the URL fragment as `#token=...`. The UI removes
the fragment from the visible address bar and keeps the token only in the
current tab's history state for reload. Authenticated HTTP uses a bearer
header; the live WebSocket uses the same token as its subprotocol.

On first use, Codex may pause for its normal `/hooks` trust review. Review and
accept the generated CodeGotchi hook profile there; CodeGotchi does not bypass
or silently approve that trust flow. The profile is additive. Existing Codex
configuration, hooks, authentication, and credentials are preserved and are
not copied or overwritten.

Trailing Codex arguments are preserved in order. CodeGotchi rejects an
explicit `-p`, attached short profile form, `--profile`, or `--profile=...`
because it must inject its own temporary additive profile.

## State, runtime files, and cleanup

SQLite is stored at
`$XDG_STATE_HOME/codegotchi/state.sqlite`, falling back to
`$HOME/.local/state/codegotchi/state.sqlite`. A stable canonical repository
identity gives each Git worktree its own pet. Restarts restore identity, needs,
inventory, activity, poop state, enforcement mode, and event/care replay IDs;
new pets receive 50 kibble, 25 treats, and 25 fruit once.

Short-lived metadata is stored in the mode-0700
`$XDG_RUNTIME_DIR/codegotchi/` directory, falling back to the CodeGotchi state
directory. Its mode-0600 `session-<uuid>.json` contains the loopback URL,
owner PID, repository root, runtime ID, and local bearer token. The unique
mode-0600 additive profile is created in `$CODEX_HOME`, or `$HOME/.codex`, as
`codegotchi-<uuid>.config.toml`.

Normal exit, child spawn/wait failure, and forwarded termination remove only
the metadata and profile owned by that run. SQLite state remains. After an
abnormal launcher death, a later run removes only stale valid CodeGotchi
session metadata whose owner is no longer active; hard-killed temporary
profiles are not yet reclaimed automatically.

## Strict mode and guarded demos

Run these commands from the active Codex environment so
`CODEGOTCHI_SESSION_FILE` identifies the live runtime:

```sh
codegotchi mode strict
```

Strict can deny only recognized safe development actions when hunger or
cleanliness is critical. The denial tells the user to care for the pet in the
UI and retry. Uncertain work, recovery/control work, and hook/transport
failures remain fail-open. Strict is a pet-care interaction, not a security
boundary or operating-system sandbox.

The fixed demonstration controls require the exact `CODEGOTCHI_ENABLE_DEBUG=1`
guard and do not accept arbitrary values:

```sh
CODEGOTCHI_ENABLE_DEBUG=1 codegotchi debug neglect
CODEGOTCHI_ENABLE_DEBUG=1 codegotchi debug generate-poop
```

They operate only against the active authenticated runtime and are intended
for a disposable demo session.

## Privacy and runtime boundary

Hooks accept bounded, forward-compatible Codex JSON but discard prompts,
source content, full commands, transcripts, and complete tool output after
classification. Persisted state contains only the canonical event plus
bounded structured metadata. Hook and backend transport failures produce the
valid empty hook result `{}`. No HTTP route executes a caller-supplied
command. Static assets are embedded in the binary; an installed launch does
not read this checkout, run Vite, require Node/pnpm, or start another UI
process.

## Development and verification

Prerequisites are Rust stable with `cargo`, `rustfmt`, and `clippy`, plus
Node.js 22.22.2 or newer and Corepack. The repository pins pnpm to 11.20.0:

```sh
corepack enable
corepack pnpm install --frozen-lockfile
```

Run the quality gates from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace

corepack pnpm lint
corepack pnpm test
corepack pnpm format:check
corepack pnpm build
node web/scripts/embed-web.mjs
corepack pnpm playwright:test
```

`corepack pnpm playwright:test` builds and embeds the production bundle,
starts a Rust fixture that serves its embedded SPA, and runs the production
browser flow. CI installs the pinned Playwright Chromium package before this
command. On WSL, install the host libraries required by Playwright's browser;
if they are exposed through WSL, the documented retry form is:

```sh
LD_LIBRARY_PATH=/usr/lib/wsl/lib corepack pnpm playwright:test
```

Routine tests use a fake Codex and never invoke a paid or real Codex session.
The final real-Codex/browser acceptance remains a supervisor-owned manual
gate; see [the verification record](docs/verification/codex-first-mvp.md).
