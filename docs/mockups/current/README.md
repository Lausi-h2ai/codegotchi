# Current terminal-room visual baseline

These six PNGs are fresh screenshots of the current production terminal-room
compositor. They are diagnostic evidence of the present visual gaps, not a
visual acceptance claim. Source SHA: `20d4ad31855e2b5ff846d4d726add4b73e68c814`
(`HEAD`, 2026-08-20).

## Capture set and findings

| Screenshot | Fixture state | Dimensions / SHA-256 | Visible current-state findings |
|---|---|---:|---|
| [`full-light.png`](full-light.png) | Full, SoftGreen/day, awake; xterm default light chrome | 1324×904 / `edff240b2758847beff22febdf949bf1f553f334ac4788a6bd014353b11be664` | The 14-row room is bottom-aligned below a large blank white Codex pane. Window, shelf, wardrobe, bed, food, poop, and care text are present, but the desk/lamp/plants/rug and layered furniture from the references are absent. The pet is a small hollow ANSI ring with little face/pose detail; bars and affordances are sparse. |
| [`full-dark.png`](full-dark.png) | Full, Night/night, awake; xterm `-bg black -fg white` | 1324×904 / `d88a5af6f8d8574a745cbe8406838a984bb4d86def2ff4d45fe0b0cb7a141a4c` | Black terminal chrome and the blue room palette remain readable, but retain the same blank-pane, sparse-furniture, small-pet mismatch. Empty need bars are low contrast and the screen has none of the detailed dark host/status treatment in the compact reference. |
| [`compact-light.png`](compact-light.png) | Compact, SoftGreen/day, awake | 1324×604 / `1ed20105ce0eb5c61c933d7cc441f336cbed3cd051bde20dda6446e079f48929` | Essential pet, food, poop, bed, and demand targets survive compression, but the result is mostly a few text rows and line glyphs. Compared with the compact reference, there is no pet vignette, status/care panel, or rich room hierarchy; labels are cramped. |
| [`minimal-light.png`](minimal-light.png) | Minimal, SoftGreen/day, awake | 1324×424 / `70d02d3545652cddb192bc19ec5cf802adef15a6aa0f5606bfe42595452423c6` | `[FOOD x50]`, `[BED]`, two `[POOP]` slots, and `AFF`/`SNACK` remain visible and actionable in three rows. The tradeoff is a text-only strip with no visible room, pet sprite, status icons, or pixel-art hierarchy. |
| [`full-bed-sleep.png`](full-bed-sleep.png) | Full, SoftGreen/night, authoritative future bed nap | 1324×904 / `5b7404aa95882716f8bed4f564dfbad1f7ff49e181a81592acc28f641de5ead7` | `z z z` and the striped pet block are on the bed while the floor is empty, so the state distinction is clear. The cat/blanket is still an abstract grid rather than the reference’s curled face, blanket, and bedding detail; the bed is tight to the right edge. |
| [`full-floor-doze.png`](full-floor-doze.png) | Full, SoftGreen/day, generic sleeping without deadline | 1324×904 / `5fe41414e634758b5447acd278d6d6475de9301e13fa1458e5f9c2bff8f1f215` | The horizontal floor curl and single `z` are visibly different from bed sleep, and the bed remains empty. The floor pose is nevertheless a sparse block silhouette with none of the reference’s face, shading, or floor/decor context. |

Across the set, the ANSI lines are crisp and no obvious half-block seam or
capture artifact is visible. The dominant mismatch is visual fidelity: the
references show a large round expressive mascot, layered pixel-art room
surfaces/furniture, icon-led need bars, and a populated Codex host; the current
projection is deliberately sparse terminal line art. All six images were
opened directly after capture and compared with every asset in
`docs/mockups/terminal-room/`.

## Exact capture provenance

Environment: `/home/laurent/codegotchi` on Ubuntu 24.04 under WSL2
(`Linux 6.6.87.2-microsoft-standard-WSL2`), `xterm 390`, Noto Sans Mono 10 pt,
ImageMagick `import 6.9.12`, `xdotool 3.20160805.1`, and Rust `cargo 1.97.1`.
The fixture used a fresh TCP-only Xvfb display with no window manager:

```text
cargo build -p codegotchi-cli --example terminal_room_fixture
Xvfb :107 -screen 0 1600x1200x24 -ac -nolisten unix -listen tcp
DISPLAY=localhost:107
```

For each row, a fresh xterm was started with the listed geometry/title and
these exact fixture variables, then captured after 0.8 s with `import`:

```text
DISPLAY=localhost:107 xterm -title TITLE -geometry GEOMETRY+0+0 \
  -fa "Noto Sans Mono" -fs 10 [DARK_FLAGS] \
  -e env NO_COLOR= TERM=xterm-256color \
    CG_FIXTURE_LAYOUT=LAYOUT CG_FIXTURE_THEME=THEME \
    CG_FIXTURE_TIME_OF_DAY=TIME CG_FIXTURE_SLEEP=SLEEP \
    CG_FIXTURE_BOTTOM=1 CG_FIXTURE_PAUSE_MS=3000 CODEGOTCHI_BROWSER=none \
    target/debug/examples/terminal_room_fixture
DISPLAY=localhost:107 import -window "$(DISPLAY=localhost:107 \
  xdotool search --onlyvisible --name TITLE | head -n 1)" \
  docs/mockups/current/FILE.png
```

The exact substitutions were:

```text
CGCurrentFullLight  full    120x45  full-light.png       soft-green day   awake  [DARK_FLAGS empty]
CGCurrentFullDark   full    120x45  full-dark.png        night     night awake  -bg black -fg white
CGCurrentCompact    compact 120x30  compact-light.png    soft-green day   awake  [DARK_FLAGS empty]
CGCurrentMinimal    minimal 120x21  minimal-light.png    soft-green day   awake  [DARK_FLAGS empty]
CGCurrentBedSleep   full    120x45  full-bed-sleep.png   soft-green night bed    [DARK_FLAGS empty]
CGCurrentFloorDoze  full    120x45  full-floor-doze.png  soft-green day   doze   [DARK_FLAGS empty]
```

The six destination files are non-empty RGB PNGs and were validated with
`file`, ImageMagick `identify`, and SHA-256 immediately after capture.
