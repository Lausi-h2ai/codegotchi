# CodeGotchi

CodeGotchi is a local coding companion for Codex. Rust owns the pet
simulation, persistence, authenticated loopback server, Codex hook bridge, and
process lifecycle; the React client is a projection of that authoritative
state.

## Install and launch

Install the self-contained binary with Rust and run the exact launcher command:

```sh
cargo install --path crates/codegotchi-cli
codegotchi run -- codex [ordinary Codex arguments...]
```

CodeGotchi starts the runtime before Codex, prints a local `CodeGotchi UI:` URL,
and attempts to open it with the native browser helper. The token is present
only in the URL fragment; the UI removes it from the visible address bar and
uses same-origin authenticated HTTP/WebSocket requests. Set
`CODEGOTCHI_BROWSER=none` to suppress browser spawning in tests or headless
sessions.

On the first run, Codex may pause for its normal `/hooks` trust review. Review
and accept the generated CodeGotchi hook profile in Codex; CodeGotchi does not
bypass that trust flow. Existing Codex configuration, authentication, and
other credentials are layered normally and are not copied or modified.

Trailing Codex arguments are preserved in order. CodeGotchi rejects an
explicit `-p`, attached short profile form, `--profile`, or `--profile=...`
because it must inject its own additive temporary profile.

## State, runtime files, and cleanup

The SQLite snapshot is stored at
`$XDG_STATE_HOME/codegotchi/state.sqlite`, falling back to
`$HOME/.local/state/codegotchi/state.sqlite`. A stable canonical repository
identity gives each worktree its own persisted pet while repeated launches
reload the same pet, needs, inventory, care state, poop state, and replay IDs.

Short-lived runtime metadata is stored in the mode-0700
`$XDG_RUNTIME_DIR/codegotchi/` directory, falling back to the CodeGotchi state
directory. Its `session-<uuid>.json` file is mode 0600 and contains the
loopback URL, owner PID, repository root, runtime ID, and a high-entropy local
token. The unique additive profile is created in `$CODEX_HOME`, or
`$HOME/.codex`, as `codegotchi-<uuid>.config.toml`.

On normal exit, child spawn/wait errors, and forwarded termination signals,
CodeGotchi removes only the metadata and profile created for that run. SQLite
state remains. If the launcher is killed abnormally, the next launch removes
only stale, valid CodeGotchi session metadata whose owner is no longer active;
unrelated files and active sessions are retained.

## Strict mode and guarded demos

The default mode is Decorative. From the active Codex environment, use:

```sh
codegotchi mode strict
```

Strict can deny only the Task 4 policy's recognized safe development actions
when the pet has a critical need. Uncertain work and hook/transport failures
remain fail-open. Strict is a pet-care interaction, not a security boundary or
an operating-system sandbox; it does not prevent independent processes or
deliberate bypasses.

The fixed demonstration controls require `CODEGOTCHI_ENABLE_DEBUG=1` and do
not accept arbitrary values:

```sh
codegotchi debug neglect
codegotchi debug generate-poop
```

They operate only against the active runtime and are intended for a disposable
demo session.

## Development

Prerequisites are Rust stable with `cargo`, `rustfmt`, and `clippy`, plus
Node.js 22.22.2 or newer on the Node 22 line and Corepack. The repository pins
pnpm to 11.20.0:

```sh
corepack enable
corepack pnpm install --frozen-lockfile
```

The frontend production bundle is built from `web` and copied into
`crates/codegotchi-cli/web-dist` with the existing embed script. The CLI build
script generates a compile-time table and uses `include_bytes!`; an installed
binary does not read the repository, invoke Vite, require pnpm, or start a
second UI process.

Run the current gates from the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace

corepack pnpm test
corepack pnpm lint
corepack pnpm format:check
corepack pnpm build
corepack pnpm playwright:test
```

Routine tests use a fake Codex executable and do not invoke a real or paid
Codex session. The real installed command remains the final interactive check:
run `codegotchi run -- codex`, complete the normal hook trust review, confirm
the printed/opened embedded UI, exercise care and Strict behavior, restart to
verify persistence, and confirm temporary-file cleanup.
