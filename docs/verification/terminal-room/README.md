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
| full-120x45-newart.png | 120x45 | 1324x904 | xterm default (light) | Full room with the restored round-blob pet art, furniture, four drag food sources, pending care chips |
| compact-120x30-newart.png | 120x30 | 1324x604 | xterm default (light) | Compact room with the new blob art and A/S demand counts |
| minimal-120x21-newart.png | 120x21 | 1324x424 | xterm default (light) | Minimal room with the new-art session |

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

- Capture commit (new-art set): `8769565`; earlier set: `515c2b6` (real-Codex
  interactive verification recorded in the SDD progress ledger).
- Codex: `codex-cli 0.147.0` (npm-managed; deepseek provider; YOLO mode).
- Terminal: xterm 390 on Xvfb `:99` (TCP-only, because `/tmp/.X11-unix` is
  read-only and the WSLg `:0` relay wedged during this pass), font
  Noto Sans Mono 10.
- Runtime state shown: fresh pet (Hunger 100%, Energy 0%, Happy 0%,
  Cleanliness 0% at capture time), Kibble x50, three authoritative poops in
  Full/Compact, `Calm` activity at capture time.
- Interactive session evidence (typing, prompt submission, real model reply,
  live Thinking/WaitingOrBlocked activity reflection, clean exit, terminal
  restore, no protocol leakage) is in `/tmp/codegotchi-controlled2.typescript`
  (earlier tree) and `/tmp/codegotchi-controlled3.typescript` (current tree:
  room + new pet art, typed prompt echoed, real model reply observed), and
  summarized in `.superpowers/sdd/2026-08-13-terminal-room-agent-hardening/progress.md`.

## Known visual-verification gaps (for the vision-capable pass)

- The new-art set restores the round blocky mascot style (logical-pixel
  packed); the vision pass should judge whether the proportions still match
  the reference mockups.
- Hit regions are aligned with rendered objects by construction; the vision
  pass should confirm the visual affordances still read as clickable.
- Light/dark contrast, half-block seams, and Full -> Compact -> Minimal
  degradation need image inspection.
- The `:99` captures used the xterm default (light) theme only; the old
  `full-120x45-dark.png` still covers the dark theme but predates the art
  restoration.
