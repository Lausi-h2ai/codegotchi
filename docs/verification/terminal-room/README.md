# Terminal Room Visual Acceptance Evidence

Status: **HISTORICAL ONLY — Task 8 final visual gate is BLOCKED**

Renderer/code commit for the frames below: `992e5b6833c41757169c9ee69615d170e933b561`.
These Task 7 frames were opened as raster images, but they are not final-SHA
evidence for Task 8's `bc8d288db6f23f14420d61a95772922de8343aee` renderer.

| File | Cells | Pixels | State/theme | Result |
|---|---:|---:|---|---|
| [full-120x45-light.png](full-120x45-light.png) | 120x45 | 1324x904 | Seeded Full, SoftGreen/day | Poop objects and Affection/Snack care state are visibly present in the complete room. |
| [full-120x45-dark.png](full-120x45-dark.png) | 120x45 | 1324x904 | Seeded Full, Night | Same seeded care state remains readable against the dark palette. |
| [compact-120x30-light.png](compact-120x30-light.png) | 120x30 | 1324x604 | Bounded production run, one generated poop | Widened bowl/poop targets and labels remain visible with the compact pet and bed. |
| [minimal-120x21-light.png](minimal-120x21-light.png) | 120x21 | 1324x424 | Bounded production run, one generated poop | `[FOOD x47]`, `[BED]`, and `[POOP]` are visible controls at their geometry starts. |
| [full-120x45-bed-sleep.png](full-120x45-bed-sleep.png) | 120x45 | 1324x904 | Seeded authoritative bed sleep | Covered pet is on the bed with `z z z`; seeded poop/care state remains visible. |
| [full-120x45-floor-doze.png](full-120x45-floor-doze.png) | 120x45 | 1324x904 | Seeded generic floor doze | Clearly horizontal/curling floor pet with `z`, same bottom-aligned Full framing, empty bed. |

## Historical Fix-round review

- The wide food regions now cover the bowl, label, and count rows; wide poop
  regions cover the four-row object width/height. Focused geometry tests assert
  those bounds in both Full and Compact.
- Minimal renders controls at the same x/y as `RoomGeometry`: `◉ PET` at x0,
  `[FOOD xN]` at x7, `[BED]` at x19, and each `[POOP]` at x27 + 8n on row 1.
  The cat mark remains on row 2.
- The floor-doze sprite is a horizontal curl rather than an upright blob. The
  seeded doze and bed frames share the same bottom room placement, so the
  context comparison is like-for-like.
- Full deterministic frames are seeded by the bounded fixture, which calls
  the production renderer and explicitly adds three poops plus Affection and
  Snack demands. Compact/Minimal use explicit named runtime metadata and an
  explicit debug command; no wildcard runtime selection is part of the final
  mapping.

Remaining differences are abstract ANSI art versus the raster references, the
bounded Codex pane/terminal background above integrated Compact/Minimal
captures, and the existing duplicate `BED BED` label in the wide bed
projection. These historical frames do not close Task 8's final visual gate.

## Capture environment and exact commands

- Ubuntu workspace, `xterm 390`, `Noto Sans Mono` 10 pt, `Xvfb :99 -screen 0
  1400x1000x24`, `DISPLAY=:99`.
- Full seeded frames used the same bottom-aligned fixture command, varying only
  title/theme/day-night/sleep state:

  ```bash
  DISPLAY=:99 xterm -title CGTask7FixtureFullSeededLight -geometry 120x45+0+0 \
    -fa "Noto Sans Mono" -fs 10 -e sh -c \
    'cd /home/laurent/codegotchi-task7 && exec env NO_COLOR= TERM=xterm-256color \
    CG_FIXTURE_LAYOUT=full CG_FIXTURE_THEME=soft-green CG_FIXTURE_TIME_OF_DAY=day \
    CG_FIXTURE_SLEEP=awake CG_FIXTURE_BOTTOM=1 CG_FIXTURE_PAUSE_MS=30000 \
    CODEGOTCHI_BROWSER=none target/debug/examples/terminal_room_fixture'
  ```

  Dark used `-bg black -fg white`, `CG_FIXTURE_THEME=night`, and
  `CG_FIXTURE_TIME_OF_DAY=night`; bed used `CG_FIXTURE_SLEEP=bed`; doze used
  `CG_FIXTURE_SLEEP=doze`. All four used fresh xterm processes and the same
  explicit `CG_FIXTURE_BOTTOM=1` framing.

- Compact used runtime metadata
  `/tmp/codegotchi-task7-runtime-r1-compact/codegotchi/session-1801520e-52cf-45a2-9a5c-a14a2e71cead.json`
  with state root `/tmp/codegotchi-task7-state-r1-compact` and:

  ```bash
  DISPLAY=:99 xterm -title CGTask7IntegratedCompactR1 -geometry 120x30+0+0 \
    -fa "Noto Sans Mono" -fs 10 -e sh -c 'cd /home/laurent/codegotchi-task7 && exec env \
    NO_COLOR= TERM=xterm-256color XDG_STATE_HOME=/tmp/codegotchi-task7-state-r1-compact \
    XDG_RUNTIME_DIR=/tmp/codegotchi-task7-runtime-r1-compact \
    CODEX_HOME=/tmp/codegotchi-task7-codex-r1-compact CODEGOTCHI_BROWSER=none \
    CODEGOTCHI_REAL_CODEX=/tmp/cg-task7-fake-codex.sh FAKE_COMPOSED_HOLD_SECONDS=120 \
    target/debug/codegotchi run --ui terminal --terminal-theme soft-green -- codex'
  CODEGOTCHI_SESSION_FILE=/tmp/codegotchi-task7-runtime-r1-compact/codegotchi/session-1801520e-52cf-45a2-9a5c-a14a2e71cead.json \
    CODEGOTCHI_ENABLE_DEBUG=1 target/debug/codegotchi debug generate-poop
  ```

- Minimal used runtime metadata
  `/tmp/codegotchi-task7-runtime-r1-minimal/codegotchi/session-7d515d05-d7f1-4163-8316-fad0e4d73f19.json`
  with state root `/tmp/codegotchi-task7-state-r1-minimal`, the same command
  at geometry 120x21, and this explicit poop seed:

  ```bash
  CODEGOTCHI_SESSION_FILE=/tmp/codegotchi-task7-runtime-r1-minimal/codegotchi/session-7d515d05-d7f1-4163-8316-fad0e4d73f19.json \
    CODEGOTCHI_ENABLE_DEBUG=1 target/debug/codegotchi debug generate-poop
  ```

Screenshots were taken with `import -window <window-id> <file>` after settle.
No video tool was available; bed/doze are the ordered state frames.

## Historical automated verification

```text
cargo fmt --all -- --check                         PASS
cargo test -p codegotchi-cli --test terminal_room  PASS (17 passed)
cargo test -p codegotchi-cli --lib                 PASS (70 passed)
git diff --check                                   PASS
```

Reviewer/process: the Task 7 implementer read the independent review, wrote
focused failing tests, verified the red failures, implemented the scoped fix,
ran the green focused/full suites, and inspected all six Task 7 PNGs after the
last recapture.

## Task 8 disposition

The Task 8 hit-region gap is closed in source and tests at
`bc8d288db6f23f14420d61a95772922de8343aee`; see
[`terminal-room.md`](../terminal-room.md) for the full matrix and blocker
record. Final-SHA visual recapture, hosted Ubuntu/macOS CI evidence, and the
remaining real-Codex interaction checklist are still outstanding.

Status: **FAIL — historical images only; final visual/manual gates remain
open.**
