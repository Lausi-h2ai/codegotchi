# Terminal Room Verification Evidence

Status: **PENDING_VISION_REVIEW** — captures below were produced by a
non-vision-capable orchestrator. They are preserved for inspection by a
vision-capable model (GPT-5.6 Sol or equivalent). No visual acceptance is
claimed. Functional/byte-level acceptance evidence is separate (see
`../terminal-room-codex-pty.md` and the SDD progress ledger).

## Screenshots (all marked PENDING_VISION_REVIEW)

| File | Terminal cells | Pixels | Theme | State represented |
|---|---|---|---|---|
| full-120x45-light.png | 120x45 | 1204x859 | xterm default (light) | Full room: Codex TUI upper, room lower (status bars, pet, bed, food, poops) |
| full-120x45-dark.png | 120x45 | 1204x859 | xterm `-bg black -fg white` | Full room in dark terminal-default theme |
| compact-120x30-light.png | 120x30 | 1204x574 | xterm default (light) | Compact room: condensed status line, pet, bed, food, poops |
| minimal-120x21-light.png | 120x21 | 1204x403 | xterm default (light) | Minimal room: `CG ok H.. E.. P.. C..` + FOOD/BED/POOP affordances |

## Capture method

```bash
export DISPLAY=:0   # WSLg X server
xterm -title CGShotFullLight -geometry 120x45+0+0 -fa "Noto Sans Mono" -fs 10 \
  -e sh -c 'TERM=xterm-256color cd /home/laurent/codegotchi && target/debug/codegotchi run --ui terminal -- codex'
# wait for Codex TUI + room settle (~45s), then:
import -window <window-id> docs/verification/terminal-room/full-120x45-light.png
```

Same procedure with `-geometry 120x30` and `-geometry 120x21` for Compact and
Minimal; `-bg black -fg white` for the dark theme capture.

## Metadata

- Capture commit: `515c2b6` (plus the real-Codex interactive verification
  recorded in the SDD progress ledger).
- Codex: `codex-cli 0.147.0` (npm-managed; deepseek provider; YOLO mode).
- Terminal: xterm 390 on Xvfb/WSLg `:0`, font Noto Sans Mono 10.
- Runtime state shown: fresh pet (Hunger 100%, Energy 0%, Happy 0%,
  Cleanliness 0% at capture time), Kibble x50, three authoritative poops in
  Full/Compact, `Calm` activity at capture time.
- Interactive session evidence (typing, prompt submission, real model reply,
  live Thinking/WaitingOrBlocked activity reflection, clean exit, terminal
  restore, no protocol leakage) is in `/tmp/codegotchi-controlled2.typescript`
  and summarized in `.superpowers/sdd/2026-08-13-terminal-room-agent-hardening/progress.md`.

## Known visual-verification gaps (for the vision-capable pass)

- Furniture (window, desk, shelf, wardrobe, plants) is not yet drawn in the
  room; only pet, bed, food tray, status bars, and poop dots render.
- Sprites are simple deterministic terminal silhouettes; the large round
  mascot proportions from the references are not yet matched.
- Hit regions are aligned with rendered objects by construction; the vision
  pass should confirm the visual affordances still read as clickable.
- Light/dark contrast, half-block seams, and Full -> Compact -> Minimal
  degradation need image inspection.
