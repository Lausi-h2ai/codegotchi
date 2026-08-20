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

## Bounded real-Codex/browser/care checklist attempt

One bounded live attempt was made with optional apps disabled; no second live
retry was made. Availability checks observed `codex-cli 0.148.0`, `xterm`,
`xdotool`, and ImageMagick `import`. The run used a private temporary
`CODEX_HOME` containing the local auth file, `CODEGOTCHI_BROWSER=none`, and:

```text
target/debug/codegotchi run --ui terminal --terminal-theme soft-green -- codex \
  --disable apps --ask-for-approval never --sandbox read-only \
  --dangerously-bypass-hook-trust
```

The official TUI trust screen and the production room rendered at 120x45.
The attempt then blocked before prompt/model/tool interaction: the shared
Xvfb host had no active-window manager, `_NET_ACTIVE_WINDOW` activation was
unsupported, and targeted `xdotool` key delivery ended in the X-server
`BadWindow`/focus error. The bounded process timed out at 50 seconds. Thus
the following are **not claimed** from this attempt: model response, tool or
approval flow, bracketed paste, observable focus, mouse care, resize
Full → Compact → Minimal → Full, pet/feed/clean/nap actions during a usable
Codex session, or clean terminal restoration. The run-owned xterm process
ended with the timeout; no broad process cleanup was performed.

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
