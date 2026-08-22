# Current terminal-room visual evidence

Status: **PASS — lower-pane visual adjudication is clean; the fixture upper
pane is intentionally Codex-free.** These six PNGs are fresh production
compositor captures at source SHA `c8371251de26db2cf9d5795873ee95c33ccd4800`
(`HEAD`, 2026-08-22). They supersede the audited pre-hardening baseline while
Git history retains the older screenshots and review notes.

## Capture set

| Screenshot | Fixture state / host | Dimensions / SHA-256 | Direct inspection |
|---|---|---:|---|
| [`full-light.png`](full-light.png) | Full, SoftGreen/day, awake; xterm default light host | 1324×904 / `7bc9b05cf8af684e5f9fda35d669755ac1e6e0c2e9be3ea2079931a0cbb86d99` | Clean layered room, large mascot, stocked care targets, demands, and status strip; no visible seam or clipping. |
| [`full-dark.png`](full-dark.png) | Full, Night/night, awake; xterm `-bg black -fg white` | 1324×904 / `83adc8d59e2a50ff4c72dc088cdbadf2f3ccaa0555957aafdc624c969003e55f` | Blue night palette and empty bars remain readable against the dark host; no capture artifact. |
| [`compact-light.png`](compact-light.png) | Compact, SoftGreen/day, awake; xterm default light host | 1324×604 / `180625c741eac200b20b2c76d3defe1f13b9592e6e64169bcd4908d6986578ba` | Mascot-led vignette retains needs, stocked food, poop, bed, and care cues within seven rows. |
| [`minimal-light.png`](minimal-light.png) | Minimal, SoftGreen/day, awake; xterm default light host | 1324×424 / `f87796b01124a3744783a8ea6512fe5ed8100cc76e00f6b35d98a49d59557305` | Three-row packed mascot plus needs, food, bed, poop, affection, and snack controls remain visible and aligned. |
| [`full-bed-sleep.png`](full-bed-sleep.png) | Full, SoftGreen/night, future authoritative bed nap | 1324×904 / `50d5ecd24b940e8e3a7b8f852b158afd9540412cab0438afdda4dabdb2f2a216` | Pet is visibly on the bed with `z z z`; floor care objects remain separate and the bed label is singular. |
| [`full-floor-doze.png`](full-floor-doze.png) | Full, SoftGreen/day, generic sleeping without deadline | 1324×904 / `84780797a151d412db4fb4925079fe6263636e9ba5370c3fe38d9d0cb8d6aaaf` | Horizontal floor doze is visibly distinct from bed sleep; the bed remains empty. |

## Lower-pane adjudication

All six files were opened directly on 2026-08-22 and compared against the
canonical references in `docs/mockups/terminal-room/`. The lower pane is
visually clean at every required size: Full reads as a layered room with a
large focal mascot, Compact retains a coherent pet vignette, and Minimal
retains the mascot and every core care target. No half-block seam, xterm
capture artifact, or visible care-target clipping was found. The authoritative
bed-sleep pose and generic floor-doze pose remain unmistakably different.

The fixture deliberately leaves the upper pane blank, so these six images do
not claim populated real-Codex acceptance. That is a separate live-session
gate; it is not a lower-pane visual defect. Auto light/dark evidence is stored
with the final verification set in
[`docs/verification/terminal-room/README.md`](../../verification/terminal-room/README.md).

## Exact capture provenance

Environment: Ubuntu 24.04 under WSL2 (`Linux 6.6.87.2-microsoft-standard-WSL2`),
fresh TCP-only Xvfb display `:127` with a `1600x1200x24` screen, `xterm 390`,
Noto Sans Mono 10 pt, ImageMagick `6.9.12-98`, `xdotool 3.20160805.1`, and
Rust `cargo 1.97.1`. The fixture was rebuilt from the source SHA above:

```text
cargo build -p codegotchi-cli --example terminal_room_fixture
Xvfb :127 -screen 0 1600x1200x24 -ac -nolisten unix -listen tcp
```

Each frame used a fresh xterm with `-geometry` set to `120x45`, `120x30`, or
`120x21`, `-fa "Noto Sans Mono" -fs 10`, `TERM=xterm-256color`, `NO_COLOR=`,
`CG_FIXTURE_BOTTOM=1`, `CG_FIXTURE_PAUSE_MS=5000`, and
`CODEGOTCHI_BROWSER=none`. The fixture used the following substitutions:

```text
full-light       full    soft-green day   awake  default light host
full-dark        full    night      night awake  -bg black -fg white
compact-light    compact soft-green day   awake  default light host
minimal-light    minimal soft-green day   awake  default light host
full-bed-sleep   full    soft-green night bed    default light host
full-floor-doze full    soft-green day   doze   default light host
```

After the fixture settled, ImageMagick `import -silent -window` captured the
visible xterm. The six current-state PNGs and their matching final-evidence
copies were captured in the same pass; all are non-empty RGB PNGs and the
hashes above were computed immediately afterward.
