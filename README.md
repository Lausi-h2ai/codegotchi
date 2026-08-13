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

Codex may pause for its normal `/hooks` trust review. Choose the review flow,
inspect the generated CodeGotchi command hooks, and select **Trust all and
continue**; CodeGotchi does not bypass or silently approve that trust flow.
The profile is additive. Existing Codex configuration, hooks, authentication,
and credentials are preserved and are not copied or overwritten. The rendered
hook bytes determine a stable mode-0600 profile name, so approving the
unchanged hooks once is enough for later launches to reuse that exact profile.
Codex asks for review again only when the rendered hook configuration changes
(for example, when the installed CodeGotchi executable path changes).

Codex CLI 0.147 may persist that approval by adding its managed
`approvals_reviewer = "auto_review"` setting and `[hooks.state]` trusted-hash
entries to the profile. CodeGotchi accepts that one observed Codex-managed
extension only when every generated hook block and command remains byte-for-byte
unchanged, all six state keys identify this exact profile and handler indexes,
and each hash matches Codex's normalized command-hook identity. Extra config,
foreign or malformed trust entries, altered commands, and unsafe paths remain
rejected without rewriting the file.

Trailing Codex arguments are preserved in order. CodeGotchi rejects an
explicit `-p`, attached short profile form, `--profile`, or `--profile=...`
because it must inject its own persistent additive profile.

## State, runtime files, and cleanup

SQLite is stored at
`$XDG_STATE_HOME/codegotchi/state.sqlite`, falling back to
`$HOME/.local/state/codegotchi/state.sqlite`. A stable canonical repository
identity gives each Git worktree its own pet. Restarts restore identity, needs,
inventory, activity, pending care demands, poop state, the exact next
attention-incident deadline, enforcement mode, and event/care replay IDs.
Elapsed wall-clock time is applied when the runtime resumes, so closing the
browser or stopping CodeGotchi does not freeze the pet. A single long catch-up
creates at most five missed incident objects, while needs still progress
across the complete absence. New pets receive 50 kibble, 25 treats, and 25
fruit once.

Short-lived metadata is stored in the mode-0700
`$XDG_RUNTIME_DIR/codegotchi/` directory, falling back to the CodeGotchi state
directory. Its mode-0600 `session-<uuid>.json` contains the loopback URL,
owner PID, repository root, runtime ID, and local bearer token. The
content-addressed mode-0600 additive profile is created or reused in
`$CODEX_HOME`, or `$HOME/.codex`, as `codegotchi-<uuid>.config.toml`. Profiles
are persistent: unchanged hook bytes reuse the existing file, while changed
bytes receive a new profile identity. Existing profile collisions, altered
contents, unsafe permissions, symlinks, and non-files are rejected without
overwriting the path. An approved Codex 0.147 profile is the only accepted
content extension; it is preserved in place and is never rewritten by
CodeGotchi.

Normal exit, child spawn/wait failure, and forwarded termination remove only
the unique runtime metadata owned by that run and shut down its loopback
server. SQLite state and persistent profiles remain. After an abnormal
launcher death, a later run removes only stale valid CodeGotchi session
metadata whose owner is no longer active; persistent profiles are never
silently reclaimed.

## Strict mode and guarded demos

Run these commands from the active Codex environment so
`CODEGOTCHI_SESSION_FILE` identifies the live runtime:

```sh
codegotchi mode strict
```

Strict mode escalates as the pet's needs worsen. Mild neglect (hunger ≥ 70,
energy ≤ 30, cleanliness ≤ 30, or happiness ≤ 30) blocks safe development
work; moderate neglect (85/15/15/15) also blocks recovery work; and severe
neglect (95/5/5/5) blocks every tool call except CodeGotchi control. Hunger,
energy, cleanliness, and happiness each drive the escalation, and the denial
tells the user what to care for in the UI before retrying. Uncertain work and
hook/transport failures remain fail-open until severe neglect. Strict is a
pet-care interaction, not a security boundary or operating-system sandbox.

Needs progress continuously in real wall-clock time: hunger rises by 25 points
per hour and energy falls by 50 points per hour outside the hammock's recovery
window, regardless of whether Codex is active, idle, waiting, or blocked.
Every deterministic randomized 3–5 minutes, the authoritative Rust simulation
creates one affection request, snack request, or floor poop. Each unresolved
item adds 240 need points per hour of pressure to happiness, hunger, or
cleanliness respectively, and multiple items stack. A long absence can
therefore restore directly into severe strict-mode refusal without generating
an unbounded room full of objects.

The room displays affection and snack requests from the authoritative
snapshot. Drag kibble, a treat, or fruit to the pet to satisfy one oldest snack
request; energy drinks do not satisfy snack requests. To satisfy one oldest
affection request, pet CodeGotchi for at least 1,500 ms while moving the
pointer at least 120 px along its path. The browser only measures and submits
the gesture: the backend validates it and the demand remains visible until an
authoritative response or WebSocket snapshot removes it.

The fixed demonstration controls require the exact `CODEGOTCHI_ENABLE_DEBUG=1`
guard and do not accept arbitrary values:

```sh
CODEGOTCHI_ENABLE_DEBUG=1 codegotchi debug neglect
CODEGOTCHI_ENABLE_DEBUG=1 codegotchi debug restock
CODEGOTCHI_ENABLE_DEBUG=1 codegotchi debug generate-poop
```

They operate only against the active authenticated runtime and are intended
for a disposable demo session. Starting the runtime itself with
`CODEGOTCHI_ENABLE_DEBUG=1 codegotchi run -- codex` additionally enables the
debug room: the pet room shows a "Restock pantry" button that restores the
starter food (50 kibble, 25 treats, 25 fruit, 10 energy drinks) without
restarting the session.

`debug neglect` makes hunger and energy critical at the current wall clock so
the refusal tiers and the hammock-nap/energy-drink care loop can be
demonstrated immediately. It deliberately does not jump the simulation
timeline far into the future; an earlier version advanced 100 hours, which
froze naps and need progression until the wall clock caught up.

`debug restock` restores the starter pantry at the current wall clock. Food is
otherwise a finite consumable: once the pantry is empty, feeding returns
`out_of_stock` and nothing in the simulation ever replenishes it, so the
guarded restock keeps disposable demos from soft-locking on an empty fridge.

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
The completed supervisor-owned real Codex/browser acceptance is recorded in
[the verification record](docs/verification/codex-first-mvp.md).
