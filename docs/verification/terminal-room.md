# Terminal Room Final Verification

Status: **FAIL / BLOCKED**

Current hardening source SHA: `2557115dfa26ac6a425ae199e9ef32a7fea06d31`.
Visual-capture source SHA: `c8371251de26db2cf9d5795873ee95c33ccd4800`.
The PNG hashes below are tied to the visual-capture SHA, not silently promoted
to fresh captures for the current hardening commit. No push or PR metadata
change was made.

The lower-pane visual gate is complete and clean. Final release status remains
FAIL/BLOCKED only because the required populated live official-Codex session
has not been accepted and hosted CI has not run on an authorized push.

## Fresh local matrix

The Rust and web gates below were run against the current hardening tree whose
code is committed at the exact SHA above. The fixture row retains its explicit
older visual-capture provenance.

| Gate | Result | Evidence |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | Fresh local run. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS | Fresh local run. |
| `cargo test --workspace` | PASS | Fresh local run; workspace tests completed with no failures. |
| `cargo test -p codegotchi-cli --test terminal_room authoritative_sleep_hitbox -- --nocapture` | PASS; 2 tests | Full and Compact bed-sleep hitboxes match their rendered sprites. |
| `cargo test -p codegotchi-cli --lib browser_helper_timeout_terminates_a_stuck_native_wait --no-fail-fast` | PASS | Native browser-helper wait terminates within the injected deadline. |
| `cargo test -p codegotchi-cli --lib response_read_has_an_outer_deadline_when_peer_dribbles_bytes --no-fail-fast` | PASS | Debug-hook transport rejects a dribbled response at its outer deadline. |
| `CRATE_CC_NO_DEFAULTS=1 cargo check -p codegotchi-cli --target x86_64-apple-darwin` | PASS | Final-validation artifact `/home/laurent/codegotchi/.superpowers/sdd/2026-08-20-terminal-room-release-hardening/final-validation/final-abfd7ce/macos-cfg.log` reaches and passes Rust cfg/type checking. |
| `corepack pnpm test` | PASS; 123 tests | Fresh web test run. |
| `corepack pnpm lint` | PASS | Fresh web lint run. |
| `corepack pnpm format:check` | PASS | Fresh web format run. |
| `corepack pnpm build` | PASS | Fresh production web build. |
| `node web/scripts/embed-web.mjs` | PASS | Embedded production bundle unchanged after the fresh build. |
| `corepack pnpm playwright:test:production` | PASS; 17 tests | Fresh production Playwright run; no retry classification. |
| `final idle-travel/roll stress (10 runs)` | PASS; 10/10 | Final-validation artifact `/home/laurent/codegotchi/.superpowers/sdd/2026-08-20-terminal-room-release-hardening/final-validation/final-abfd7ce/idle-stress.log` records `PASS 1` through `PASS 10`. |
| `cargo build -p codegotchi-cli --example terminal_room_fixture` | PASS | The eight captures were built from visual-capture SHA `c8371251de26db2cf9d5795873ee95c33ccd4800`; no new capture is claimed for `2557115dfa26ac6a425ae199e9ef32a7fea06d31`. |

The final Apple cfg check passed with `CRATE_CC_NO_DEFAULTS=1`; this proves the
Rust cfg/type path only and does not execute macOS runtime behavior. The hosted
Apple runtime/production-shared PTY smoke remains **UNAVAILABLE / NOT CLAIMED**;
the source uses rustix's `waitid(WNOWAIT)` pre-reap probe and includes a
macOS-only unit regression, but no hosted macOS execution was available here.

The final-validation artifact also records ten consecutive deterministic
idle-travel/roll runs, all passing. This is local browser stress evidence only;
it does not claim live official-Codex acceptance or hosted release gates.

## Visual evidence

The six current-state PNGs are recorded in
[`docs/mockups/current/README.md`](../mockups/current/README.md). The final
verification set is recorded in
[`terminal-room/README.md`](terminal-room/README.md) and contains:

- [`full-120x45-light.png`](terminal-room/full-120x45-light.png)
- [`full-120x45-dark.png`](terminal-room/full-120x45-dark.png)
- [`compact-120x30-light.png`](terminal-room/compact-120x30-light.png)
- [`minimal-120x21-light.png`](terminal-room/minimal-120x21-light.png)
- [`full-120x45-bed-sleep.png`](terminal-room/full-120x45-bed-sleep.png)
- [`full-120x45-floor-doze.png`](terminal-room/full-120x45-floor-doze.png)
- [`auto-120x45-light.png`](terminal-room/auto-120x45-light.png)
- [`auto-120x45-dark.png`](terminal-room/auto-120x45-dark.png)

All eight images were recaptured from the production compositor on
2026-08-22, opened directly, and verified as non-empty RGB PNGs at their
expected pixel dimensions. The lower-pane adjudication is **PASS**: Full has
the larger focal mascot and layered furniture, Compact reads as a pet vignette,
Minimal retains the mascot and core care targets, Auto is readable on both
host polarities, and bed sleep remains distinct from floor doze. No visible
half-block seam, capture artifact, or care-target clipping was found.

The fixture intentionally leaves the upper pane blank because it is
Codex-free. That is not a lower-pane visual defect and is not presented as
live acceptance evidence.

Exact source SHA, fixture substitutions, dimensions, hashes, host polarity,
and capture environment are recorded in the two linked README files rather
than inferred from older screenshots.

## Live official-Codex acceptance

The durable record is
[`terminal-room/live-codex/task-7-round1-blocked.txt`](terminal-room/live-codex/task-7-round1-blocked.txt).
The harness correctly fails fast when private Xvfb has no supported lightweight
window manager and does not claim prompt/editing, bracketed paste, focus or
mouse reporting, model/tool activity, approval/review, Full → Compact →
Minimal → Full Codex resize handling, final pet completion, or terminal-control
restoration receipts. The retained fixture/live attempts do not contain a
populated accepted Codex interaction frame. This gate remains **BLOCKED**; no
bearer token or auth contents are included in the evidence.

## Hosted CI and PR boundary

No fresh hosted workflow exists for the exact hardening source SHA because no
push was authorized in this worker. Hosted Ubuntu Rust, hosted macOS Rust
(including the production-shared PTY smoke), and hosted web checks are
**UNAVAILABLE / NOT CLAIMED**. No PR title, body, labels, or other metadata
were changed.

The local implementation and lower-pane visual evidence are green. The final
release status intentionally remains **FAIL / BLOCKED** only for the missing
populated live official-Codex acceptance and the unavailable hosted-CI gate.

Historical Task 5/7/8 records remain in
[`terminal-room-codex-pty.md`](terminal-room-codex-pty.md) and Git history;
their older source SHAs and screenshots are not evidence for this source.
