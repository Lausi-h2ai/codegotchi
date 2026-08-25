# Terminal Room Final Verification

Status: **FAIL / BLOCKED**

Ledger refreshed: **2026-08-25**.
Current corrective working tree: based on
`2d08945353df262ce0f1710834d7b4da748b1461`; corrective code-and-test binary
diff SHA-256: `425068360099c8972668aad0025f756b90fd81b3275a2b2437c11e0837900f69`.
The fingerprint covers the five modified files under `crates/` and excludes
documentation, so updating this ledger does not invalidate its own provenance.

This corrective wave fixes the Full-room 80/81-column cleaning regression,
restores the sparse window/desk silhouette, amends the normative bedroom
specification, and replaces Linux-only runtime-liveness probing with portable
Unix signal-zero probing. The local matrix and production visual review pass.
The release remains blocked because hosted Ubuntu/macOS/web checks have not run
against this corrective tree, the macOS raw-HTTP and PTY integration gates have
not been rerun, and live Codex acceptance plus final promoted screenshots must
be captured from the eventual committed SHA.

## Fresh local matrix

The following gates were rerun on 2026-08-25 against the exact corrective
working tree identified above:

| Gate | Result | Evidence |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | Fresh local run. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS | Fresh local run. |
| `cargo test --workspace` | PASS | Fresh local run; workspace tests completed with no failures. |
| `cargo test -p codegotchi-cli --test terminal_room authoritative_sleep_hitbox -- --nocapture` | PASS; 2 tests | Full and Compact bed-sleep hitboxes match their rendered sprites. |
| `cargo test -p codegotchi-cli --lib browser_helper_timeout_terminates_a_stuck_native_wait --no-fail-fast` | PASS | Native browser-helper wait terminates within the injected deadline. |
| `cargo test -p codegotchi-cli --lib response_read_has_an_outer_deadline_when_peer_dribbles_bytes --no-fail-fast` | PASS | Debug-hook transport rejects a dribbled response at its outer deadline. |
| `corepack pnpm test` | PASS; 123 tests | Fresh web test run. |
| `corepack pnpm lint` | PASS | Fresh web lint run. |
| `corepack pnpm format:check` | PASS | Fresh web format run. |
| `corepack pnpm build` | PASS | Fresh production web build. |
| `node web/scripts/embed-web.mjs` | PASS | Embedded production bundle unchanged after the fresh build. |
| `corepack pnpm playwright:test:production` | PASS; 17 tests | Fresh embedded-production browser run. |
| Production visual fixture review | PASS | Real production `terminal_room_fixture` rendered in xterm on Xvfb and was screenshot-inspected at 80 and 120 columns; the desk cue is visible and the 80-column fallback poop is cleanable and unclipped. |

These local results do not establish hosted release readiness.
In particular, a Linux run cannot establish macOS runtime behavior.

## Visual evidence

The August 24 promoted set in [`terminal-room/README.md`](terminal-room/README.md)
is now **historical, not authoritative**:

- [`full-120x45-light.png`](terminal-room/full-120x45-light.png)
- [`full-120x45-dark.png`](terminal-room/full-120x45-dark.png)
- [`compact-120x30-light.png`](terminal-room/compact-120x30-light.png)
- [`minimal-120x21-light.png`](terminal-room/minimal-120x21-light.png)
- [`full-120x45-bed-sleep.png`](terminal-room/full-120x45-bed-sleep.png)
- [`full-120x45-floor-doze.png`](terminal-room/full-120x45-floor-doze.png)
- [`auto-120x45-light.png`](terminal-room/auto-120x45-light.png)
- [`auto-120x45-dark.png`](terminal-room/auto-120x45-dark.png)

The files were captured on 2026-08-24 from an uncommitted working tree based
on `1f0f8db0aca15bc172a12e508bad2ceee3665575`. The captured product-source
diff was recorded as
`4645edebe436968a9baa55b786893bcc5151de6e9c74060b0b6ed6aad42391aa`.
Recomputing the binary diff from that base to the five product-source files in
`8843ce94d464009319f54ab970f59882e8b6e3fe` produces the same hash. The
committed product sources therefore match the recorded capture source; the
capture is not being inferred from commit time alone.

Those files predate the desk restoration and care-first poop reflow. The
corrective production visual review passed at widths 80 and 120, but the eight
promoted final frames must be recaptured from the eventual final committed SHA
and re-adjudicated before release.

The six matching named-state files and their hashes are recorded in
[`docs/mockups/current/README.md`](../mockups/current/README.md). The fixture
intentionally leaves the upper pane blank because it has no real Codex
process. That is not a lower-pane defect and is not presented as live-session
acceptance evidence.

## Live official-Codex acceptance

Status: **BLOCKED / MUST RERUN**.

The August 24 live acceptance record remains useful historical evidence for the
prior renderer, but it is not acceptance for the current care geometry, restored
desk, or runtime-liveness fix. A new live official-Codex session must cover the
complete release checklist from the final committed SHA.

The durable record is
[`terminal-room/live-codex/20260824T150816Z-2359630-verification.txt`](terminal-room/live-codex/20260824T150816Z-2359630-verification.txt).
It records an exit-status-zero run against official `codex-cli 0.149.1` using
`gpt-5.6-luna` with low reasoning in an isolated workspace and Codex home.
The real TUI edited and submitted `Read this file called test.md and tell me
what it says.`, read the run-owned file with live tool activity, and answered
`My grandma is a wonderful woman.`

The same session captured the official approval modal and approved one exact
temp-workspace command, a real multiline xterm paste, focus out/in, upper-pane
click/scroll routing, authoritative pet/feed/clean/nap results, and the Full →
Compact → Minimal → Full cycle. Content-free receipts verify negotiated paste
and focus delivery plus the actual physical-terminal/Codex-PTY resize pairs.
Normal exit restored the same xterm to a real interactive shell with matching
PTY state and an executed command receipt; a separate bounded termination case
also restored its PTY state, and the controller terminal survived with matching
`stty` state. The populated final frame is
[`20260824T150816Z-2359630-full-live-final.png`](terminal-room/live-codex/20260824T150816Z-2359630-full-live-final.png),
and the restored-shell receipt is
[`20260824T150816Z-2359630-normal-exit-restored-shell-input.png`](terminal-room/live-codex/20260824T150816Z-2359630-normal-exit-restored-shell-input.png).

The invocation used `--ask-for-approval on-request --sandbox read-only`.
Approval is proven by the retained official modal, the exact isolated
`touch approval-probe.txt` request, the run-owned file receipt, and the return
to authoritative `WaitingForUser`. Direct inspection found no bearer token or
authentication content in any retained PNG. The older
[`task-7-round1-blocked.txt`](terminal-room/live-codex/task-7-round1-blocked.txt)
remains historical preflight evidence only.

## Hosted CI

Hosted CI has **not run** for the corrective tree. The prior workflow for
`8843ce94d464009319f54ab970f59882e8b6e3fe`, run
[`32729444290`](https://github.com/Lausi-h2ai/codegotchi/actions/runs/32729444290),
completed on 2026-08-24 with overall result **FAILURE** and is historical only:

| Hosted job | Result | Evidence |
|---|---|---|
| Rust checks (Ubuntu) | PASS | Formatting, clippy, and Rust workspace tests passed. |
| Rust checks (macOS) | FAIL | Formatting and clippy passed. Workspace tests failed in `authenticated_loopback_http_is_authoritative_and_replay_safe` with a connection reset and in `production_hook_reaches_authoritative_state_and_parses_the_server_response` because the processed-event assertion was not satisfied. The outer PTY smoke was skipped after the test failure. |
| Web checks | FAIL | Unit tests, lint, formatting, build, and bundle embedding passed. The production Playwright step failed: the feed flow required a retry and the energy-drink flow did not observe `Eating an energy drink` within its timeout after retry. |

The corrective tree must receive a fresh complete hosted Ubuntu/macOS/web run,
including the macOS outer PTY smoke. The previously observed macOS raw-HTTP
reset and Linux PTY signal-escalation failure must be investigated and rerun
rather than inferred fixed from timing changes. Release remains blocked until
all hosted gates and live Codex acceptance pass from one exact committed source
SHA.

Historical Task 5/7/8 records remain in
[`terminal-room-codex-pty.md`](terminal-room-codex-pty.md) and Git history.
Their older source SHAs and screenshots are historical evidence only and are
not release evidence for the corrective working tree identified above.
