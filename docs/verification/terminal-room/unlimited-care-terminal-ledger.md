# Unlimited care and upper Codex terminal verification

Date: 2026-08-26 (Linux)

This ledger records the verification for the unlimited care-item projection
and the upper-pane terminal input changes. Existing release-audit captures in
this directory were left untouched.

## Product contract exercised

- New and restored product snapshots contain `u32::MAX` for Kibble, Treat,
  Fruit, and Energy drink. Feeding applies its normal meter effect while the
  care-item availability remains unchanged.
- Full, Compact, and Minimal terminal views show only care-item names (`KIB`,
  `TRT`, `FRT`, `ENE`); the browser likewise shows names only. No `∞` or
  quantity is rendered.
- With Codex mouse tracking disabled, a plain wheel in the upper pane moves
  CodeGotchi-local Codex history. Shift-drag selection, PRIMARY copy, and
  native terminal paste remain owned by the Linux terminal emulator.

## Automated verification

- `cargo fmt --all -- --check` — PASS
- `cargo test --workspace` — PASS
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS
- `web/npm test` — 123 tests PASS
- `web/npm run lint` — PASS
- `web/npm run format:check` — PASS
- `npm run playwright:test:production` — 17 tests PASS through the embedded
  production bundle and Rust fixture
- `git diff --check` — PASS

## Real Linux production-path inspection

The live harness used the actual CodeGotchi binary, xterm, Xvfb/openbox, and
installed Codex 0.149.1. It exercised and visually inspected these layouts and
states in each listed theme:

- Full 120×45: initial/care, upper-pane scroll before/after, Shift selection,
  and native multiline paste
- Full 80×45: narrow Full fallback care layout
- Compact 120×30
- Minimal 120×21

Themes and reports:

- [`auto report`](live-codex/20260826-unlimited-care-final4/20260826T153324Z-2934890-verification.txt)
  — product acceptance PASS; inspected captures in
  [`auto evidence`](live-codex/20260826-unlimited-care-final4/)
- [`mono report`](live-codex/20260826-unlimited-care-mono/20260826T153518Z-2945728-verification.txt)
  — care, scroll, selection, paste, and layout captures inspected in
  [`mono evidence`](live-codex/20260826-unlimited-care-mono/)
- [`soft-green report`](live-codex/20260826-unlimited-care-soft-green/20260826T153711Z-2964275-verification.txt)
  — care, scroll, selection, paste, and layout captures inspected in
  [`soft-green evidence`](live-codex/20260826-unlimited-care-soft-green/)
- [`amber report`](live-codex/20260826-unlimited-care-amber/20260826T153902Z-2982947-verification.txt)
  — product acceptance PASS; inspected captures in
  [`amber evidence`](live-codex/20260826-unlimited-care-amber/)
- [`night report`](live-codex/20260826-unlimited-care-night/20260826T154013Z-2993682-verification.txt)
  — care, scroll, selection, paste, and layout captures inspected in
  [`night evidence`](live-codex/20260826-unlimited-care-night/)

Representative inspected artifacts include the `full-live-care.png`,
`full-live-80x45-care.png`, `120x30.png`, `120x21.png`,
`full-live-codex-before-scroll.png`, `full-live-codex-after-scroll.png`,
`full-live-selection.png`, and `full-live-paste.png` captures in each theme
directory.

## Coverage gap

The harness controller had no parent tty, so its shell exits were status 1 and
the reports are marked blocked even where the product acceptance checklist was
PASS. Mono, soft-green, and night also timed out before their same-xterm normal
exit/restoration receipt; their care, rendering, scroll, selection, and paste
checks completed and were visually inspected. Run-owned processes were gone
after each run and a follow-up command confirmed the controlling session
survived. This is a harness/outer-terminal evidence gap, not a product
behavior failure.
