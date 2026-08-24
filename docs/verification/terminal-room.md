# Terminal Room Final Verification

Status: **FAIL / BLOCKED**

Ledger refreshed: **2026-08-24**.
Current release source SHA: `8843ce94d464009319f54ab970f59882e8b6e3fe`.

This SHA contains the August 24 renderer refresh, the approved PNG set, and
the matching visual-evidence READMEs. The lower-pane visual gate and the fresh
local matrix are green. Final release status remains FAIL/BLOCKED because the
required populated live official-Codex session is still unavailable and the
hosted workflow for this exact SHA completed with failures.

## Fresh local matrix

The following gates were rerun on 2026-08-24 against the exact current release
source SHA above:

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
| `corepack pnpm playwright:test:production` | PASS; 17 tests | Fresh local production Playwright run. |

These local results do not override the failed hosted checks described below.
In particular, a Linux run cannot establish macOS runtime behavior.

## Visual evidence

The authoritative lower-pane set is the eight-image August 24 capture set in
[`terminal-room/README.md`](terminal-room/README.md):

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

All eight promoted files are non-empty native-resolution PNGs, and their
SHA-256 hashes match the approved table in the linked README. The lower-pane
adjudication is **PASS**: the refreshed mascot is recognizable and leads the
visual hierarchy; named and Auto themes remain readable on both host
polarities; Full, Compact, and Minimal remain complete; bed sleep remains
distinct from floor doze; and no seam, clipping, collision, or capture
artifact was found.

The six matching named-state files and their hashes are recorded in
[`docs/mockups/current/README.md`](../mockups/current/README.md). The fixture
intentionally leaves the upper pane blank because it has no real Codex
process. That is not a lower-pane defect and is not presented as live-session
acceptance evidence.

## Live official-Codex acceptance

The durable record is
[`terminal-room/live-codex/task-7-round1-blocked.txt`](terminal-room/live-codex/task-7-round1-blocked.txt).
The harness correctly fails fast when private Xvfb has no supported
lightweight window manager and does not claim prompt/editing, bracketed paste,
focus or mouse reporting, model/tool activity, approval/review, Full → Compact
→ Minimal → Full Codex resize handling, final pet completion, or
terminal-control restoration receipts. The retained fixture/live attempts do
not contain a populated accepted Codex interaction frame. This gate remains
**BLOCKED**; no bearer token or auth contents are included in the evidence.

## Hosted CI

Hosted CI did run for the exact current release source SHA. Push workflow
[`32729444290`](https://github.com/Lausi-h2ai/codegotchi/actions/runs/32729444290)
completed on 2026-08-24 with overall result **FAILURE**:

| Hosted job | Result | Evidence |
|---|---|---|
| Rust checks (Ubuntu) | PASS | Formatting, clippy, and Rust workspace tests passed. |
| Rust checks (macOS) | FAIL | Formatting and clippy passed. Workspace tests failed in `authenticated_loopback_http_is_authoritative_and_replay_safe` with a connection reset and in `production_hook_reaches_authoritative_state_and_parses_the_server_response` because the processed-event assertion was not satisfied. The outer PTY smoke was skipped after the test failure. |
| Web checks | FAIL | Unit tests, lint, formatting, build, and bundle embedding passed. The production Playwright step failed: the feed flow required a retry and the energy-drink flow did not observe `Eating an energy drink` within its timeout after retry. |

Hosted Ubuntu is green, but the hosted macOS workspace tests and production
browser gate are not green, and the skipped macOS production-shared PTY smoke
has not been established as green. The final release status cannot be promoted
while those failures and the populated live-Codex acceptance gate remain
unresolved.

Historical Task 5/7/8 records remain in
[`terminal-room-codex-pty.md`](terminal-room-codex-pty.md) and Git history.
Their older source SHAs and screenshots are historical evidence only and are
not the release evidence for `8843ce94d464009319f54ab970f59882e8b6e3fe`.
