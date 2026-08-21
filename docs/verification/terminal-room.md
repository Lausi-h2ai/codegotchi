# Terminal Room Final Verification

Status: **FAIL — implementation fixes are complete, but visual acceptance, the unconstrained workspace gate, hosted CI, and the real-Codex interaction gate remain open**

Final integrated source SHA: `a611202729bcbcc37beb2c5498bede8002f3cdc0`.
The preceding implementation SHA is
`8a1feffb627a3b05c9edc56b3b2383fcb2857da2`; `a611202` is a bundle-only
follow-up that refreshes `crates/codegotchi-cli/web-dist` from the passing web
build. The six final-candidate images and their exact hashes are recorded in
[`terminal-room/README.md`](terminal-room/README.md).

## Final fix wave summary

- Terminal room input now has explicit capture/cancel behavior. Captured pet
  and food gestures remain owned by the room while the pointer leaves the
  room, are suppressed from Codex, and cancel on outside release, focus loss,
  resize, or invalid termination. Pet release also requires the pointer to
  be inside both room and pet hit regions; pet priority wins an overlap.
- Minimal selects the deterministic first stocked food kind (Kibble, Treat,
  Fruit, then EnergyDrink), omits a zero-stock hit region, and renders a
  disabled `[FOOD none]` label when inventory is empty. Full/Compact emit only
  stocked sources and no longer reserve sparse-food gaps.
- Browser generic `Sleeping` is now floor doze. Only a future
  `napping_until` deadline selects authoritative hammock/bed napping; expired
  deadlines and generic sleeping do not.
- The wide bed overlay no longer adds a second label, and the rendered
  food/poop spacing and pet-vs-food overlap behavior have regression coverage.

## Automated evidence

| Command | Result | Provenance/notes |
|---|---|---|
| `cargo fmt --all -- --check` | PASS | Fresh at final SHA `a611202`. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS | Fresh at final SHA `a611202`. |
| `cargo test --workspace` | **FAIL / blocker** | Fresh unconstrained run on the implementation tree (`8a1feff`, before the bundle-only `a611202` commit) reached `strict_flow` and first failed with a fail-open JSON `null`; an isolated reproduction failed earlier with loopback `HTTP transport failed: Resource temporarily unavailable (os error 11)`. The existing strict-flow harness passed once with `--test-threads=1`. It was not rerun after the bundle-only commit under the bounded recovery instruction; no Rust source changed between those SHAs. |
| `cargo test -p codegotchi-cli --test terminal_input --test terminal_room -- --nocapture` | PASS | 15 input tests and 21 room tests, including all 15 nonempty inventory masks, no-stock rendering, sparse spacing, overlap, and one-BED checks. |
| `cargo test -p codegotchi-cli --lib cancels_both_room_capture_kinds -- --nocapture` | PASS | 2 focus/resize cancellation tests. |
| `cargo test -p codegotchi-cli terminal::session::tests::captured_ -- --nocapture` | PASS | 2 captured pet/food outside-room suppression tests. |
| `pnpm test` | PASS | 5 files, 123 tests. |
| `pnpm lint` | PASS | Clean. |
| `pnpm format:check` | PASS | Clean. |
| `pnpm build` | PASS | Vite production build. |
| `node web/scripts/embed-web.mjs` | PASS | Produced the bundle committed by `a611202`. |
| `pnpm playwright:test` | PASS | 17 production-embedded Playwright tests; 57.7 seconds. |
| `cargo build -p codegotchi-cli --example terminal_room_fixture` | PASS | Built the production renderer fixture at final source SHA. |

The focused browser suite was red before the semantic change (four old
sleeping/napping expectations), then green after the deadline-only projection
was implemented. The Rust capture tests were likewise written and run red
before the corresponding production APIs/behavior, then green. The sparse
food test was explicitly observed red before the gap fix and green afterward.

The unconstrained workspace failure is retained as evidence, not hidden. Its
error is an environment/timing symptom already seen in the repository's
strict-flow history; the source changes in this wave do not touch the Codex
hook/server transport. The single-threaded strict-flow run passed, but that is
not substituted for a passing unconstrained workspace gate.

## Visual evidence

All six canonical implementation captures were recaptured after the final
integrated source SHA using the real `terminal_room_fixture` production
compositor and inspected directly against all nine supplied reference assets.
The evidence directory is [`terminal-room/README.md`](terminal-room/README.md).

Verdict: **PENDING_VISION_REVIEW**. The captures are recognizable, legible,
and state-correct (including one visible BED label, authoritative bed sleep,
and floor doze), but the ANSI projection is materially sparser and less
pixel-art-detailed than the supplied references. The reference manifest was
updated to enumerate the two composition references and seven component
reference sheets with their actual roles. No visual PASS is claimed.

## Real-Codex acceptance harness

Task 7 adds [`scripts/verify-terminal-room-live.sh`](../../scripts/verify-terminal-room-live.sh).
It prints the installed Codex version, requires `xterm`, `xdotool`,
ImageMagick `import`, and a usable X display, redirects CodeGotchi XDG
state/runtime/data/config/cache/home paths into a run-owned temporary tree,
sets `CODEGOTCHI_BROWSER=none`, and never prints Codex arguments, auth
contents, runtime metadata, or bearer tokens. Codex arguments are supplied to
the launcher through a private NUL-delimited file only when the harness's
explicit `CODEGOTCHI_LIVE_HARNESS=1` seam is present; the file must be inside
the run root, owned by the current user, and mode `0600`. State polling keeps
the bearer token in a private curl config file. Cleanup verifies start time,
role, and the exact per-run environment marker, walks every live descendant,
and sends `SIGTERM` before bounded escalation; a second marker scan catches
descendants reparented after root death. Diagnostics omit command lines, and
blocked cleanup removes argument, metadata, state, debug, wrapper, and xterm
log files and removes or redacts run-owned Xauthority credentials before
retaining only redacted evidence. If cleanup is blocked, the report and log
print the retained run-root path; inherited user Xauthority is never removed.
A private `Xvfb` path uses an authenticated local Unix socket and is available
only when a supported lightweight window manager is installed.

The harness does not capture or parse a Codex screen transcript. Feed, clean,
and nap can be checked from redacted authoritative state snapshots. Pet is
deliberately separate: the snapshot's care IDs do not distinguish a final
`Pet` completion from a `PetStroke`, so the final pet result is reported as
manual/unavailable and blocks completion. Prompt/model/tool activity, hook
trust, approval policy receipt, bracketed paste, focus delivery, mouse
protocol delivery, and Codex PTY resize handling likewise have no non-text
receipt and are reported manual/unavailable. An xterm outer-geometry change
is recorded only as that narrow measurement, not as proof of Codex PTY
handling. Any required manual/unavailable check makes the harness exit
nonzero with final status `BLOCKED`.

The fresh availability check on 2026-08-21 used `codex-cli 0.149.0`,
`xterm`, `xdotool`, ImageMagick `import`, and inherited `DISPLAY=:0`.
The explicit private-display preflight was:

```text
CODEGOTCHI_LIVE_NO_BUILD=1 DISPLAY= \
  CODEGOTCHI_LIVE_OUTPUT_DIR=/tmp/codegotchi-live-evidence-t7-prereq \
  ./scripts/verify-terminal-room-live.sh
```

It exited `1` immediately with:

```text
no lightweight window manager is installed; private Xvfb acceptance requires openbox, fluxbox, xfwm4, icewm, matchbox-window-manager, jwm, or twm
```

This is the required fail-fast result for the former `_NET_ACTIVE_WINDOW` /
`BadWindow` loop. No private X server was started by that check.

A bounded live run then reused `DISPLAY=:0` with an explicitly selected
authorized `CODEX_HOME` (credentials were referenced, never copied or
printed):

```text
CODEGOTCHI_LIVE_NO_BUILD=1 CODEGOTCHI_LIVE_TIMEOUT_SEC=10 \
  CODEGOTCHI_LIVE_CODEX_HOME=/home/laurent/.codex \
  CODEGOTCHI_LIVE_OUTPUT_DIR=/tmp/codegotchi-live-evidence-t7-rerun \
  ./scripts/verify-terminal-room-live.sh
```

The production room and the official Codex pane rendered at terminal geometry
`120x45` during an earlier preliminary run. Its temporary capture was
`/tmp/codegotchi-live-evidence-t7-rerun/20260821T200006Z-1672739-full-live-populated.png`
(`724x589`, SHA-256
`afdc62b0857de763b9c47bbe152b1beec7ad7f486df07d7d25d9d076e4441d59`). Direct
image inspection confirmed the real Codex pane above a visible CodeGotchi
room, with no visible bearer token. It was not a populated interaction frame:
the Codex pane was still on its official `Hooks need review` screen, and the
shared display subsequently failed the window-activation check; the harness
terminated only its run-owned xterm.
The corrected harness now labels this pre-interaction frame `full-live-initial`
and reports `BLOCKED` with a nonzero exit when authoritative care snapshots or
required interaction checks do not settle. Screenshots are retained as visual
evidence only; no `xterm` screen log is retained, and no `full-live-populated`
claim is made.

The durable round-one blocked report is
[`live-codex/task-7-round1-blocked.txt`](terminal-room/live-codex/task-7-round1-blocked.txt).
Therefore these items remain **not claimed**: ordinary prompt/model response,
tool activity, approval/review, bracketed paste, observable Codex focus
reporting, mouse protocol behavior, Codex PTY handling for Full → Compact →
Minimal → Full resize, final pet completion, hook trust, and any visual
terminal-control cleanup proof. Feed/clean/nap snapshots are asserted only
when their exact state deltas settle; normal exit and bounded termination are
reported separately from restoration, while restoration's terminal controls
remain manual/unavailable. The report alongside the capture is intentionally
provisional and nonzero-blocked; the live gate remains open until a display
with a functioning window manager/focus path and an authorized Codex session
can supply the unavailable non-text receipts.

Browser behavior is covered by the passing production Playwright suite and
the focused motion/App tests; this does not turn the blocked live Codex TUI
checklist into a manual PASS.

Hosted Ubuntu/macOS CI results and PR metadata are external to this local
workspace and remain unavailable. No claim is made for those gates.

## Historical records

Earlier Task 5/7/8 records remain in
[`terminal-room-codex-pty.md`](terminal-room-codex-pty.md) and the prior git
history. Their old SHAs and captures are intentionally not presented as
evidence for `a611202`.
