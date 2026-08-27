# Terminal-room final visual evidence

This directory contains the final visual matrix and the exact-source live
Codex evidence for the 2026-08-26 release-hardening wave. The authoritative
ledger is [`../terminal-room.md`](../terminal-room.md); it records local,
browser, PTY, hosted, and provenance status separately.

## Exact source

- Source SHA: `a50104304dfddf2085f33049f75a448c94841adb`
- Product-source scope was clean when the final images and accepted live run
  were produced.
- The later source commits only corrected Linux/macOS test cfg scopes; they do
  not alter the rendered product or the captured care geometry.
- The repository-relative final fixture set is
  [`final-20260826-final/`](final-20260826-final/).
- The accepted official-Codex record is
  [`live-codex/20260826T133957Z-2792986-verification.txt`](live-codex/20260826T133957Z-2792986-verification.txt).

## Inspected final matrix

All nine images were opened at native resolution after capture. The matrix
covers the required lower-pane layouts, themes, states, and narrow Full care
target:

| Image | Cells | Host/state | Result |
|---|---:|---|---|
| [`full-120x45-light.png`](final-20260826-final/full-120x45-light.png) | 120×45 | Full, light/default | PASS |
| [`full-120x45-dark.png`](final-20260826-final/full-120x45-dark.png) | 120×45 | Full, dark | PASS |
| [`compact-120x30-light.png`](final-20260826-final/compact-120x30-light.png) | 120×30 | Compact | PASS |
| [`minimal-120x21-light.png`](final-20260826-final/minimal-120x21-light.png) | 120×21 | Minimal | PASS |
| [`full-120x45-bed-sleep.png`](final-20260826-final/full-120x45-bed-sleep.png) | 120×45 | authoritative bed sleep | PASS |
| [`full-120x45-floor-doze.png`](final-20260826-final/full-120x45-floor-doze.png) | 120×45 | generic floor doze | PASS |
| [`auto-120x45-light.png`](final-20260826-final/auto-120x45-light.png) | 120×45 | Auto on light host | PASS |
| [`auto-120x45-dark.png`](final-20260826-final/auto-120x45-dark.png) | 120×45 | Auto on dark host | PASS |
| [`full-80x45-care.png`](final-20260826-final/full-80x45-care.png) | 80×45 | populated Full fallback care | PASS |

Inspection found a recognizable mascot, no pane seam or clipping, visible
care targets, distinct bed sleep and floor doze, readable Auto palettes, and
the intended Full/Compact/Minimal hierarchy. The narrow Full frame visibly
contains the fallback poop marker in the same care lane used by the geometry.

## Real-Codex frames

The exact accepted run includes the populated final frame and the new 80×45
care frame:

- [`20260826T133957Z-2792986-full-live-final.png`](live-codex/20260826T133957Z-2792986-full-live-final.png)
- [`20260826T133957Z-2792986-full-live-80x45-care.png`](live-codex/20260826T133957Z-2792986-full-live-80x45-care.png)

The live report pairs those supervised screenshots with structured runtime
receipts. It does not scrape Codex text to infer care outcomes and does not
print arguments, credential contents, or bearer tokens. Its 80×45 receipt
proves an authoritative poop was cleaned and the pending-poop count decreased.

## Capture provenance

The fixture was built from the exact source SHA with the production
`terminal_room_fixture` example and captured through fresh xterm/Xvfb
instances. The browser and live acceptance paths likewise used the embedded
production bundle and the actual managed terminal path. Native dimensions and
SHA-256 values for every final image are recorded in the parent ledger.

Hosted macOS is not represented by these Linux captures; GitHub Actions
`macos-latest` remains the designated macOS compile/test environment.
