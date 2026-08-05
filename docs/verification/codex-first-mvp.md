# Codex-first runnable MVP verification

Date of this record: 2026-08-05. This document separates automated evidence
from the supervisor-owned real Codex/browser gate. No real or paid Codex
session was run by the automated checks below.

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
| `cargo test --workspace` | PASS — 110 passed, 0 failed, 1 intentionally ignored manual installed-Codex test; doc-tests 0 |
| `corepack pnpm lint` | PASS |
| `corepack pnpm test` | PASS — 3 files, 29 tests |
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

## Supervisor pending manual observations

These are deliberately pending; no automated result below is a substitute for
the supervisor's real interactive acceptance.

- [ ] Install the binary with `cargo install --path crates/codegotchi-cli --locked`
  and resolve that installed `codegotchi` executable.
- [ ] Run `codegotchi run -- codex` with the installed Codex 0.146.0 and
  complete the normal `/hooks` trust review without bypass flags.
- [ ] Confirm the printed/opened UI is served from embedded production bytes
  and inspect the visible fragment/token behavior.
- [ ] Observe real SessionStart, thinking, Bash/unified-exec, apply_patch,
  completion/result, waiting, idle/session-end activity transitions in the
  browser.
- [ ] Exercise food care, shovel→poop→trash cleanup, refresh persistence,
  reconnect, and a typed backend error in the real browser.
- [ ] Enable Strict, observe one recognized safe development denial with care
  and retry guidance, care in the UI, and confirm a fresh retry is allowed.
- [ ] In a disposable session, run the guarded `neglect` and `generate-poop`
  demos with `CODEGOTCHI_ENABLE_DEBUG=1` and confirm no arbitrary values are
  accepted.
- [ ] Restart the real launcher in the same repository, verify pet identity
  and cared-for state, and confirm only owned runtime/profile files were
  removed while existing Codex config/credentials remain unchanged.
- [ ] Confirm no paid/real Codex session was used by routine automation; the
  supervisor owns this final gate.
