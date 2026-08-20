# Terminal Room Visual Acceptance Evidence

Status: **ACCEPTED** (Task 7 visual review, 2026-08-20)

Final renderer commit: `c3f45f79d914ca57097f74d23c905046d1c4d79c`.
Every frame below was opened as a raster image and compared with the canonical
references under `docs/mockups/terminal-room/`.

| File | Cells | Pixels | State/theme | Result |
|---|---:|---:|---|---|
| [full-120x45-light.png](full-120x45-light.png) | 120x45 | 1324x904 | Full, xterm default/light, SoftGreen | Window/desk/shelf/wardrobe/bed/plants/open floor, large pet, food and poop read clearly. |
| [full-120x45-dark.png](full-120x45-dark.png) | 120x45 | 1324x904 | Full, night palette | Contrast, silhouettes, and affordances survive the dark palette. |
| [compact-120x30-light.png](compact-120x30-light.png) | 120x30 | 1324x604 | Compact, SoftGreen, one poop | Decoration reduces before pet/care; bed, food, and poop remain readable. |
| [minimal-120x21-light.png](minimal-120x21-light.png) | 120x21 | 1324x424 | Minimal, SoftGreen, one poop | PET/FOOD/BED/POOP and status/care text remain visible. |
| [full-120x45-bed-sleep.png](full-120x45-bed-sleep.png) | 120x45 | 1324x904 | Full, authoritative bed sleep | Covered pet is on the bed with `z z z`; click target is unambiguous. |
| [full-120x45-floor-doze.png](full-120x45-floor-doze.png) | 120x45 | 1324x904 | Full, generic floor doze | Curled pet is on open floor with `z`; bed remains empty. |

## Visual review

- Full reads as a cozy room around a large rounded pet. The cabinet silhouette
  is the wardrobe; its printed label is omitted to avoid narrow-room overlap.
- Idle, floor-doze, and bed-sleep sprites retain distinct face/blanket detail.
- Food bowls, bed, and poop have visible affordances aligned with their hit
  regions. A real xterm mouse click produced the bed-sleep frame.
- Full -> Compact -> Minimal removes decoration/art first while retaining the
  pet and care targets; Minimal is intentionally text-forward.
- Light/default and night contrast are legible. Raster inspection found no
  clipping, stale cells, or unintended half-block seams.

Non-blocking differences: ANSI art is more abstract than raster mockups;
integrated screenshots include the bounded Codex pane above the room; and the
floor-doze state uses the deterministic fixture because the public integrated
CLI has no force-doze command. The fixture calls the same production renderer.
No P0/P1 visual blocker remains.

## Capture environment and commands

- Ubuntu workspace; `xterm 390`; `Noto Sans Mono` 10 pt; `Xvfb :99 -screen 0
  1400x1000x24`; `DISPLAY=:99`.
- Integrated Full/Compact/Minimal/bed captures used the bounded production
  command below (fresh state directories and geometry/theme varied per frame):

  ```bash
  DISPLAY=:99 xterm -title CGTask7IntegratedFullColor -geometry 120x45+0+0 \
    -fa "Noto Sans Mono" -fs 10 -e sh -c 'cd /home/laurent/codegotchi-task7 && exec env \
    NO_COLOR= TERM=xterm-256color XDG_STATE_HOME=/tmp/codegotchi-task7-statec \
    XDG_RUNTIME_DIR=/tmp/codegotchi-task7-runtimec CODEX_HOME=/tmp/codegotchi-task7-codexc \
    CODEGOTCHI_REAL_CODEX=/tmp/cg-task7-fake-codex.sh FAKE_COMPOSED_HOLD_SECONDS=120 \
    CODEGOTCHI_BROWSER=none target/debug/codegotchi run --ui terminal \
    --terminal-theme soft-green -- codex'
  ```

  Dark Full used `-bg black -fg white` and `--terminal-theme night`; Compact
  and Minimal used 120x30 and 120x21 with fresh state directories. Bed sleep
  used the Full command followed by:

  ```bash
  DISPLAY=:99 xdotool mousemove --sync 1155 790 click 1
  ```

- Poops in integrated frames were generated with:

  ```bash
  CG_META=$(find /tmp/codegotchi-task7-runtime*/codegotchi -maxdepth 1 -type f \
    -name 'session-*.json' -print -quit)
  CODEGOTCHI_SESSION_FILE="$CG_META" CODEGOTCHI_ENABLE_DEBUG=1 \
    target/debug/codegotchi debug generate-poop
  ```

- Floor doze used the deterministic production-renderer fixture:

  ```bash
  DISPLAY=:99 xterm -title CGTask7FixtureFloorDoze -geometry 120x45+0+0 \
    -fa "Noto Sans Mono" -fs 10 -bg black -fg white -e sh -c \
    'cd /home/laurent/codegotchi-task7 && exec env NO_COLOR= TERM=xterm-256color \
    CG_FIXTURE_LAYOUT=full CG_FIXTURE_THEME=night CG_FIXTURE_TIME_OF_DAY=day \
    CG_FIXTURE_SLEEP=doze CG_FIXTURE_PAUSE_MS=30000 CODEGOTCHI_BROWSER=none \
    target/debug/examples/terminal_room_fixture'
  ```

Screenshots were taken with `import -window <window-id> <file>` after settle.
No video tool was available; the bed/doze PNGs are the ordered state sequence.

## Automated verification

```text
cargo fmt --all -- --check                         PASS
cargo test -p codegotchi-cli --lib                 PASS (69 passed)
cargo test -p codegotchi-cli --test terminal_room  PASS (15 passed)
git diff --check                                   PASS
```

Reviewer/process: fresh implementer performed one substantive screenshot ->
inspection -> correction -> recapture loop, then inspected every final PNG.
