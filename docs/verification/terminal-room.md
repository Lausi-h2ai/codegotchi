# Terminal-room verification ledger

## Current verification at HEAD

This section records the current repository status as of 2026-08-29T15:54Z.
The exact validated product-source `HEAD` is
`e8425e69beba490caca1ba7be1f6e9d4c339933f`. The ledger and retained live
evidence are a docs/evidence-only follow-up directly on that committed source;
no product-source change is an uncommitted working-tree diff.

| Item | Result |
|---|---|
| Exact validated product-source `HEAD` SHA | `e8425e69beba490caca1ba7be1f6e9d4c339933f` |
| Current repository `HEAD` | This ledger/evidence-only update directly on top of the exact source above |
| Ubuntu Rust checks | PASS — exact-head run [33259716448](https://github.com/Lausi-h2ai/codegotchi/actions/runs/33259716448) |
| Web checks, including production Playwright | PASS — exact-head run [33259716448](https://github.com/Lausi-h2ai/codegotchi/actions/runs/33259716448) |
| macOS Rust checks | PENDING/HANGING — exact-head run [33259716448](https://github.com/Lausi-h2ai/codegotchi/actions/runs/33259716448) is still in `cargo test --workspace` after about 39 minutes; formatting and Clippy passed, and the outer PTY smoke has not started |

The local rows below exercise the committed fixture fix. The hosted rows
identify the exact product-source commit above; the only follow-up commit is
documentation/evidence.

### Current committed-source verification

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace` | PASS — exit 0; all workspace suites passed |
| Concurrent database-name regression | PASS — 128 concurrent `TestDatabase` values produced unique paths |

The macOS diagnosis is now fixed at the proven cause: the pre-fix exact-head
run recorded the same timestamp-derived database path for two tests and only
509 unique paths from 512 concurrent attempts. `TestDatabase::new()` now uses
`Uuid::new_v4()`; the existing `.sqlite`, `-shm`, and `-wal` cleanup remains
unchanged, and the temporary macOS path instrumentation is gone. No SQLite
retry/timeout, sleep, serialization, or production persistence change was
added.

The clean-HEAD real-Codex production path passed against the exact committed
source in [`20260829T153644Z-3303413-verification.txt`](terminal-room/live-codex/20260829-e8425e6-auto/20260829T153644Z-3303413-verification.txt).
Its retained captures are in [`20260829-e8425e6-auto/`](terminal-room/live-codex/20260829-e8425e6-auto/).
They cover live/history scrollback, stale-click handling, mouse delivery,
selection, paste, focus, approval, care states, shell restoration, and
120×45 → 80×45 → 120×30 → 120×21 → 120×45. I visually inspected the exact
auto-theme captures for those states and sizes. The current fixture-only
change does not alter rendered production code; the prior explicit-theme
matrix below remains applicable to the unchanged terminal/session behavior:

- [`auto/private`](terminal-room/live-codex/20260827-p1-auto-private/)
- [`mono`](terminal-room/live-codex/20260827-p1-mono/)
- [`soft-green`](terminal-room/live-codex/20260827-p1-soft-green/)
- [`amber`](terminal-room/live-codex/20260827-p1-amber/)
- [`night`](terminal-room/live-codex/20260827-p1-night/)

The exact current report is overall PASS (exit status 0). Its Restoration
section still records a retained run root because the harness could not prove
full process-tree termination; the post-run local artifact scan found no
matching run-owned process, so no cleanup signal was sent.

## Historical 0.1 release evidence

The following sections are preserved as historical evidence for the 0.1
release and do not describe the current `HEAD`.

Status: **local and real-Codex gates pass; hosted final-SHA run is not
release-green**.

This ledger was rewritten on 2026-08-26 from the exact final local source
commit. The product-source scope is clean and the final evidence is retained
under this directory. The repository as a whole is intentionally dirty only
because this ledger, the visual captures, and the live-Codex evidence are
working-tree release artifacts; no product-source diff is present.

## 1. Exact final source and provenance

| Item | Evidence |
|---|---|
| Exact final source SHA | `a50104304dfddf2085f33049f75a448c94841adb` |
| Product-source scope | `crates/codegotchi-cli/{src,tests,examples,web-dist} web/{src,e2e,scripts}` |
| Relevant product-source working tree | Clean at the accepted live run |
| Scoped product-source diff SHA-256 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` (empty diff at final HEAD) |
| CodeGotchi binary SHA-256 | `671f407f89375d9425c52c448392a772c142e444536e9b5b668c90d77442f0eb` |
| Acceptance harness SHA-256 | `8e601a843cc515bc00184806af6b5b927cf517bd07e9d627e6a4d821a9736f84` |
| Workspace helper SHA-256 | `a5b41babdb507b9fd5609295345c0038db785a9edf06199c911eb8f79c323109` |
| Codex CLI | `codex-cli 0.149.1` |

The live report intentionally omits Codex arguments, CODEX_HOME contents,
runtime metadata, and bearer tokens. It also records the exact source SHA,
working-tree scope, and all four binary/tool fingerprints:
[`20260826T133957Z-2792986-verification.txt`](terminal-room/live-codex/20260826T133957Z-2792986-verification.txt).

## 2. Fresh local Linux matrix

These commands were rerun after the final product and harness changes:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace` | PASS — 421 passed, 0 failed, 10 ignored |
| `corepack pnpm test` | PASS — 5 files, 123 tests |
| `corepack pnpm lint` | PASS |
| `corepack pnpm format:check` | PASS |
| `corepack pnpm build` | PASS |
| `node web/scripts/embed-web.mjs` | PASS |

The coordinate-space regression coverage is fresh as well:

| Focused gate | Result |
|---|---|
| Full geometry/render tests (`full_wide`) | PASS — 2 |
| Nonzero-origin Full geometry, click lifecycle, and wander tests | PASS — 4 |
| Integrated 80×45 lower-pane origin test | PASS — 1 |

The implementation keeps all `RoomGeometry` rectangles in absolute terminal
coordinates. Presentation home/offset values remain room-relative internally;
the behavior collision path explicitly converts them before comparing against
the absolute care zone. The tests cover Full areas `(0,31,80,14)`,
`(0,31,81,14)`, and `(7,31,80,14)`, rendered/hitbox alignment, exactly-once
Down→Up cleaning, pet exclusion, reachable wandering, and the production
80×45 layout seam.

## 3. Browser stress result

The production Playwright command uses the built and embedded bundle; it does
not use Vite dev mode. The complete suite was run in ten fresh sequential
invocations: **10 × 17 tests passed, 0 failed**. The energy-drink diagnostics
retained the following successful layers on every exercised run: native drop,
exact `POST /api/v1/care/feed`, HTTP 200, authoritative inventory decrement,
rendered inventory propagation, and transient `Eating an energy drink`
feedback. A separate one-worker energy-drink stress also passed 20/20.

The intentionally excluded exploratory run used concurrent workers against a
single restartable fixture and failed during fixture reset with
`fixture_restarting`; it was a test-harness concurrency result, not a
production-flow failure, and caused no production change.

## 4. Terminal PTY stress result

`cargo test -p codegotchi-cli --test terminal_pty -- --nocapture` passed in
**50 consecutive fresh invocations**, five tests per invocation: **250 passed,
0 failed**. The test now frames complete lines across legal partial reads,
consumes the newline after `FAKE_SIGNAL_READY`, and waits for a complete
`FAKE_SIGNAL_PID` record. The production signal behavior was not changed.

## 5. Final promoted visual evidence

The final product-source fixture recapture is in
[`terminal-room/final-20260826-final/`](terminal-room/final-20260826-final/).
The product-source scope was unchanged by the later test-only portability cfg
commits. Every image below was opened and inspected at native resolution. The lower
pane has no seam or clipping; the mascot is recognizable; Full/Compact/Minimal
hierarchy, care targets, bed sleep, floor doze, and Auto host-palette
readability are retained. The 80-column image shows the narrow Full fallback
care target in the visible care lane.

| Capture | Native pixels | SHA-256 | Inspection |
|---|---:|---|---|
| [`full-120x45-light.png`](terminal-room/final-20260826-final/full-120x45-light.png) | 1324×904 | `76e7f2a0207910d97666d902c6383a94f9b8e6e5fa9f95f0842b42811938d85b` | PASS — Full light/default |
| [`full-120x45-dark.png`](terminal-room/final-20260826-final/full-120x45-dark.png) | 1324×904 | `2406adc0eb803424b575d7023d84d26082a39be8e333550ab70a7cf9a34f19c6` | PASS — Full dark |
| [`compact-120x30-light.png`](terminal-room/final-20260826-final/compact-120x30-light.png) | 1324×604 | `a8aedd62bdc885a11cad7953794eb99a364ba1b2eae643be178977c6877df6ac` | PASS — Compact |
| [`minimal-120x21-light.png`](terminal-room/final-20260826-final/minimal-120x21-light.png) | 1324×424 | `e4a74cbedc2b23ce6240b3e064560e62e6b09392103324fc9bd28e435cfaeed3` | PASS — Minimal |
| [`full-120x45-bed-sleep.png`](terminal-room/final-20260826-final/full-120x45-bed-sleep.png) | 1324×904 | `0e3dec1730521cadf3dce1d3357a2c35694253a599d7be114f1cefb0d0f37871` | PASS — authoritative bed sleep |
| [`full-120x45-floor-doze.png`](terminal-room/final-20260826-final/full-120x45-floor-doze.png) | 1324×904 | `bea745398fad84104bceb999e0d3f1309a4b5ff9204137644de601d0652e895f` | PASS — generic floor doze |
| [`auto-120x45-light.png`](terminal-room/final-20260826-final/auto-120x45-light.png) | 1324×904 | `a3e092ec8f0400489aac52b1c8295a62d5ba880140d66a99cc550287d1d31565` | PASS — Auto/light host |
| [`auto-120x45-dark.png`](terminal-room/final-20260826-final/auto-120x45-dark.png) | 1324×904 | `d9ffda9955e92c41936b6a5d48fc5c79803e59ddf1e9c1569a2135bd5ca11fa2` | PASS — Auto/dark host |
| [`full-80x45-care.png`](terminal-room/final-20260826-final/full-80x45-care.png) | 884×904 | `97d23fd52edbe4c3c464cb41fa24ba74d9a62be1f77b961048f088c707f4a7ab` | PASS — narrow Full fallback target |

## 6. Final real-Codex acceptance

The exact final product source was exercised through the official Codex
terminal path. The accepted run is
[`20260826T133957Z-2792986-verification.txt`](terminal-room/live-codex/20260826T133957Z-2792986-verification.txt),
with final populated and narrow-care frames:

- [`full-live-final.png`](terminal-room/live-codex/20260826T133957Z-2792986-full-live-final.png)
- [`full-live-80x45-care.png`](terminal-room/live-codex/20260826T133957Z-2792986-full-live-80x45-care.png)

Result: **PASS**. Structured receipts prove the real approval modal and
isolated approval command, authoritative pet/feed/clean/nap outcomes, the
120×45 → 120×30 → 120×21 → 120×45 resize sequence, normal shell restoration,
bounded termination restoration, parent-tty preservation, and the new 80×45
phase. At 80×45, the harness generated an authoritative poop, captured the
populated room, clicked the visible fallback target, and observed the pending
poop count change from 1 to 0 before restoring 120×45.

## 7. Hosted Ubuntu result

The final-source hosted CI run is
[32971855910](https://github.com/Lausi-h2ai/codegotchi/actions/runs/32971855910),
at `a50104304dfddf2085f33049f75a448c94841adb`. Ubuntu formatting, Clippy, and
the complete Rust workspace test job all completed **PASS**.

## 8. Hosted Web result

Run [32971855910](https://github.com/Lausi-h2ai/codegotchi/actions/runs/32971855910)
completed every Web step through bundle embedding, but its production
Playwright step returned exit code 1. The exact job log is not available yet
because the same workflow run remains open on the macOS job; therefore this
ledger does not guess whether the failure is A–F from the release checklist and
does not claim hosted Web PASS. The complete local production suite and stress
remain green as recorded in section 3.

## 9. Hosted macOS result / macOS status

No macOS runtime execution was performed locally. The designated acceptance
environment is GitHub Actions `macos-latest`. In final-source run
[32971855910](https://github.com/Lausi-h2ai/codegotchi/actions/runs/32971855910),
macOS formatting and Clippy passed, but the workspace test step remained
`in_progress` for more than an hour and the outer-PTY smoke had not started at
ledger refresh. This is recorded as **pending/unresolved**, not PASS; no local
macOS runtime claim is made. The prior hosted Clippy failure caused by stale
test-helper cfg scopes is fixed in the pushed source.

## Release disposition

The product fixes, local Linux matrix, browser stress, terminal PTY stress,
final visual evidence, and exact-source real-Codex acceptance pass. Ubuntu is
green at the pushed source. Release remains blocked by the hosted Web failure
whose diagnostics are unavailable until the run closes, and by the unresolved
hosted macOS workspace-test run. Repository status and the exact source scope
are recorded separately so neither hosted result is overstated.
