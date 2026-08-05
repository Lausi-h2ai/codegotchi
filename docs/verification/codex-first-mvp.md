# Codex-first runnable MVP verification

Date of this record: 2026-08-05. Routine automation used only fake Codex
processes. The final sections separately record the supervisor-owned real
installed Codex/browser acceptance.

## Environment

| Item | Observed value |
| --- | --- |
| OS | Ubuntu 24.04.4 LTS (`VERSION_ID=24.04`) under WSL2 |
| Kernel/architecture | Linux 6.6.87.2-microsoft-standard-WSL2, x86_64 |
| WSL indicators | `WSL_DISTRO_NAME=Ubuntu-24.04-bonsai-vllm`, `WSL2_GUI_APPS_ENABLED=1` |
| Time zone/date | Europe/Berlin, 2026-08-05; observed `date -Is`: `2026-08-05T17:40:31+02:00` |
| Rust | `rustc 1.97.1 (8bab26f4f 2026-07-14)` |
| Cargo | `cargo 1.97.1 (c980f4866 2026-06-30)` |
| Node | `v24.15.0` |
| Corepack | `0.34.6` |
| pnpm | `11.20.0` (repository-pinned) |
| Codex CLI | `codex-cli 0.146.0` |
| Playwright | `1.62.1` |
| Workspace | `/home/laurent/codegatchi` |

The pinned Playwright Chromium run used the supervisor-provided extracted
runtime libraries with:

```sh
LD_LIBRARY_PATH=/tmp/codegotchi-playwright-libs.uo9gm9/extracted/usr/lib/x86_64-linux-gnu corepack pnpm playwright:test
```

CI installs Chromium and its host dependencies with Playwright's pinned
`--with-deps` command.

## Exact install and runtime commands

The intended checkout installation and launch are:

```sh
cargo install --path crates/codegotchi-cli --locked
codegotchi run -- codex [ordinary Codex arguments...]
```

The launcher prints `CodeGotchi UI: http://127.0.0.1:<port>/#token=...` and
uses an ephemeral loopback port. `CODEGOTCHI_BROWSER=none` suppresses the
best-effort browser helper. The normal Codex `/hooks` trust review remains
operator-controlled.

Development setup and all required repository gates are:

```sh
corepack enable
corepack pnpm install --frozen-lockfile

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

The explicit CI production command is:

```sh
corepack pnpm --filter @codegotchi/web exec playwright install --with-deps chromium
corepack pnpm playwright:test:production
```

`playwright:test` and `playwright:test:production` build/embed before starting
the fixture. The fixture starts the Rust `task3_fixture`, forwards its
embedded static assets, authenticated HTTP, and WebSocket through the fixed
Playwright port, and does not start Vite or add a production mutation route.

## Automated results

| Command | Result |
| --- | --- |
| `cargo test -p codegotchi-cli --test full_vertical_flow -- --nocapture` | PASS — 2 passed, 0 failed |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace` | PASS — 111 passed, 0 failed, 1 intentionally ignored manual installed-Codex test; doc-tests 0 |
| `corepack pnpm lint` | PASS |
| `corepack pnpm test` | PASS — 3 files, 30 tests |
| `corepack pnpm format:check` | PASS |
| `corepack pnpm build` | PASS — Vite production bundle built |
| `node web/scripts/embed-web.mjs` | PASS — `web/dist` copied to `crates/codegotchi-cli/web-dist` |
| `LD_LIBRARY_PATH=/tmp/codegotchi-playwright-libs.uo9gm9/extracted/usr/lib/x86_64-linux-gnu corepack pnpm playwright:test` | PASS — production embedded-bundle flow, 7 passed, 0 failed |

The new Rust tests were run before any load-bearing production correction.
The first focused invocation was red only because the newly added harness had
a local helper-name shadowing compile error and an unused import. After those
test-only corrections, the current Tasks 1–5 behavior passed both flows; no
cross-task production defect was observed and no Task 1–5 load-bearing code
was changed for Task 6.

### Rust vertical evidence

`full_vertical_flow.rs` launches the compiled `codegotchi` binary with the
repository fake Codex and `CODEGOTCHI_BROWSER=none`. It discovers and checks
the printed fragment URL against mode-0600 runtime metadata, then:

- consumes a complete authenticated WebSocket snapshot and a later
  authoritative snapshot after a real `SessionStart` hook subprocess, then
  sends installed prompt, source/patch, command, and complete-output fixtures
  through the launched hook process;
- invokes the same installed-schema hook fixture twice and confirms the
  duplicate event is a no-op, including an authenticated duplicate event
  response;
- performs authenticated feed, rejects invalid food, proves duplicate care is
  a complete authoritative snapshot no-op for both feed and clean, runs the
  guarded CLI poop demo, and cleans the resulting poop with the normal care
  endpoint;
- checks concrete persisted pet identity, needs, inventory, poop sequence and
  pending-poop state, enforcement mode, work/digestion points, and event/care
  replay sets after relaunch in the same repository/state home;
- checks owned session/profile files are removed while SQLite remains; and
- checks HTTP and SQLite serialized state immediately after those launched
  sensitive fixtures contain none of their prompt, source content, complete
  command, or complete output values.

The Strict process flow enables Strict, applies fixed debug neglect, verifies
the exact installed denial JSON and care/retry guidance, proves the retry ID is
distinct and absent before/present after independent recording, feeds through
the normal authenticated care route, and proves that a valid metadata file
pointing at the stopped server returns `{}` fail-open.

Existing workspace tests additionally cover all Task 1–5 hook, domain,
persistence, HTTP, WebSocket, UI, launcher, cleanup, and embedded-asset
contracts. The one ignored test is the intentionally manual authenticated
Codex 0.146.0 trust/coexistence gate.

## Privacy, trust, and cleanup observations automated here

- Hook output is `{}` on allow/no-op and on transport failure; only a
  successful authenticated Strict denial emits the documented
  `hookSpecificOutput` shape.
- Hook payloads are bounded and raw prompts, source contents, full commands,
  transcripts, and complete output are not present in the persisted snapshot
  or the vertical-test assertions/logging.
- The loopback server binds to `127.0.0.1`; protected state-changing routes
  require the bearer token. Debug mutations additionally require the fixed
  CLI guard and debug header.
- The temporary profile is additive and owned-file cleanup is checked without
  touching a base config or credential file. Codex's normal trust review is
  not automated or bypassed.
- The UI token is fragment-only at launch, removed from the visible URL, and
  retained only for same-tab reload in history state.

## Real installed Codex and browser acceptance

The supervisor installed and resolved the real binaries:

```text
cargo install --path crates/codegotchi-cli --locked --force
/home/laurent/.cargo/bin/codegotchi
/home/laurent/.nvm/versions/node/v24.15.0/bin/codex
codex-cli 0.146.0
```

The exact command was run twice from `/home/laurent/codegatchi` in a real PTY:

```text
codegotchi run -- codex
```

On the first launch Codex displayed its normal `Hooks need review` screen for
the six generated hooks. The supervisor selected `Trust all and continue`;
no hook-trust bypass flag was used. Codex then launched interactively with
inherited colors and terminal input/output. The launcher bound an ephemeral
`127.0.0.1` port, wrote mode-0600 runtime metadata under
`/run/user/1000/codegotchi/`, and printed the fragment-token UI URL. The WSL
browser helper exited with status 2, so the printed URL was opened directly;
the embedded room showed Pixel, Connected, the desk, food, feed target,
shovel, trash, needs, and authoritative activity. The fragment was removed
from the address after token capture.

The real smoke prompt was:

```text
Inspect Cargo.toml, tell me the workspace package names, and run one harmless metadata or test-listing command. Do not modify files.
```

Codex correctly reported `codegotchi-domain` and `codegotchi-cli` and ran
`cargo metadata --no-deps --format-version 1`. Live authoritative polling and
the browser observed session registration, Thinking, Bash work, Idle, and
Waiting. A second disposable prompt asked real Codex to create
`CODEGOTCHI_REAL_SMOKE.txt` with `apply_patch`; the browser/state observed
Thinking then Editing, Codex created the marker, and the supervisor removed
only that untracked fixture. A real Cargo test-list retry later produced the
complete visible sequence `Thinking → Testing → Celebrating → Waiting`, with
`recentOutcome: Success` and no browser alert.

The installed PostToolUse contract was checked without retaining output. In
Codex 0.146.0 it supplies a model-facing string, and silent `true` and `false`
both supply an empty string, so those outcomes remain neutral. Recognized
test/build output now uses only narrow observable success/failure markers;
unknown result text remains neutral. The focused red/green regression test is
`installed_string_tool_responses_infer_only_observable_cargo_outcomes`.

## Real care, Strict, persistence, and cleanup evidence

- Guarded `debug generate-poop` created one authoritative poop. In the real
  embedded UI the supervisor selected Shovel, selected that poop, and selected
  Trash. Poop count changed from 1 to 0, cleanliness rose from `99.98752` to
  `100`, `Cleaned up` appeared, and a full reload still showed zero poop.
- Guarded `debug neglect` set hunger to `100` and behavior to `CriticalNeed`;
  `codegotchi mode strict` persisted Strict mode. Real Codex attempted exactly
  `cargo test -p codegotchi-domain -- --list`. The real PreToolUse hook denied
  it, authoritative activity became `Blocked`, and Codex reported that the
  pet's hunger was critical and no alternative command ran.
- Dragging a treat to the feed target reduced hunger `100 → 90` and inventory
  `25 → 24`; dragging fruit reduced hunger `90 → 75` and inventory `25 → 24`.
  Both showed Eating feedback and survived reload. At hunger 75 the identical
  fresh retry was allowed, ran 64 test listings with exit code 0, and drove
  the real Testing/Celebrating UI states.
- Before the first exit the persisted state was pet ID
  `291d078c-76d7-55a4-a892-162a807f4dd4`, Pixel, hunger 75, energy 100,
  happiness 100, cleanliness 100, inventory 47/24/24, zero pending poop,
  poop sequence 1, and Strict mode. The first `/exit` returned status 0.
  The second exact launch restored the same ID, needs, inventory, zero-poop
  state, poop sequence, Strict mode, and care replay history; its UI was
  Connected after reload and its `/exit` also returned status 0.
- Runtime metadata and generated `codegotchi-*.config.toml` files were absent
  after each normal exit; SQLite remained. `~/.codex/config.toml` retained
  SHA-256 `d1495adcbc6fab6465db015d92f4c5c7f126cc1a0e558537699973ec1f401833`
  and `~/.codex/auth.json` retained
  `947f7529303450e9c394d3605b763a4935a216e317b6b40b460fcff81e434062`.
- Direct byte searches of SQLite found none of the unique real prompt,
  apply-patch instruction, complete Cargo command, or deliberate failure
  command. No credential file was copied or modified. The server was reachable
  only on its printed `127.0.0.1` address and exposed no command-execution API.

Routine automation separately proves signal forwarding/Ctrl+C, typed backend
errors, WebSocket disconnect/recovery, duplicate idempotency, authentication,
fail-open transport behavior, and synthetic coexistence with a pre-existing
hook. The real sessions confirmed normal terminal interaction and exit status.

## Known limitations

- The native browser helper returned status 2 in this WSL2 environment. The
  launcher surfaced the complete usable local URL, and the embedded UI was
  opened from that URL without a development server.
- Codex 0.146.0 may repeat hook review because every run uses a uniquely named
  temporary profile. The trust flow remains explicit and is never bypassed.
- A hard-killed launcher can leave its temporary profile until manual cleanup;
  stale runtime metadata is reclaimed on a later run.
- Hosted/unsupported tools and command outcomes absent from the installed hook
  payload remain generic/neutral. This MVP does not treat hooks as a security
  boundary.
