# Terminal Room Visual Acceptance Evidence

Status: **PENDING_VISION_REVIEW — final-candidate captures were recaptured and inspected; visual acceptance remains open**

Renderer/source commit for these six frames: `a611202729bcbcc37beb2c5498bede8002f3cdc0`.
The source commit contains the terminal-room fixes and the refreshed embedded
web bundle. These images were captured after that commit with the production
Rust room renderer; they are not synthetic mockups.

## Final-candidate captures

| File | Cells | Pixels | Fixture state | SHA-256 | Inspection result |
|---|---:|---:|---|---|---|
| [full-120x45-light.png](full-120x45-light.png) | 120x45 | 1324x904 | Full, SoftGreen/day, awake | `edff240b2758847beff22febdf949bf1f553f334ac4788a6bd014353b11be664` | Room, pet, stocked food, poop, bed, demands, and status strip visible; abstract/sparse versus reference. |
| [full-120x45-dark.png](full-120x45-dark.png) | 120x45 | 1324x904 | Full, Night/night, awake | `d88a5af6f8d8574a745cbe8406838a984bb4d86def2ff4d45fe0b0cb7a141a4c` | Dark palette remains legible; same visual gap remains. |
| [compact-120x30-light.png](compact-120x30-light.png) | 120x30 | 1324x604 | Compact, SoftGreen/day, awake | `1ed20105ce0eb5c61c933d7cc441f336cbed3cd051bde20dda6446e079f48929` | Compact pet/status/care targets are visible and separated; it is not a pixel-art reference match. |
| [minimal-120x21-light.png](minimal-120x21-light.png) | 120x21 | 1324x424 | Minimal, SoftGreen/day, awake | `70d02d3545652cddb192bc19ec5cf802adef15a6aa0f5606bfe42595452423c6` | Condensed need row, stocked `[FOOD x50]`, bed, poop, and demand controls are visible. |
| [full-120x45-bed-sleep.png](full-120x45-bed-sleep.png) | 120x45 | 1324x904 | Full, SoftGreen/night, future authoritative nap | `5b7404aa95882716f8bed4f564dfbad1f7ff49e181a81592acc28f641de5ead7` | Pet is on the bed with `z z z`; bed label is rendered once. |
| [full-120x45-floor-doze.png](full-120x45-floor-doze.png) | 120x45 | 1324x904 | Full, SoftGreen/day, generic `Sleeping` without deadline | `5fe41414e634758b5447acd278d6d6475de9301e13fa1458e5f9c2bff8f1f215` | Curled floor doze and one `z` are visible; the bed remains empty. |

## Inspection verdict

All six files were opened directly after capture and compared with every
listed canonical reference in `docs/mockups/terminal-room/`. The captures
prove the production projection and the required state distinctions:

- Full frames include the window, shelf, wardrobe, desk, pet, stocked food,
  poop objects, bed, demand markers, and status strip; the dark frame remains
  readable.
- Compact and Minimal retain care controls at their constrained dimensions.
- Authoritative bed sleep places the pet on the bed, while generic sleeping
  is a curled floor doze with an empty bed.
- The wide bed has one visible `BED` label, and sparse food sources do not
  reserve an extra gap.

The result is still **PENDING_VISION_REVIEW**, not a visual PASS: the supplied
references show detailed pixel-art sprites, layered furniture, richer status
icons, and a much larger round mascot, while the terminal renderer remains a
deliberately simplified ANSI projection. The exact gap is recorded rather
than converted into an acceptance claim.

## Capture provenance

The fixture was built from the source commit above:

```text
cargo build -p codegotchi-cli --example terminal_room_fixture
```

Each frame used a fresh private Xvfb display, `xterm` with Noto Sans Mono at
the listed cell size, `CG_FIXTURE_BOTTOM=1`, and
`target/debug/examples/terminal_room_fixture`. Full light/dark, Compact,
Minimal, future-deadline bed sleep, and no-deadline floor doze were captured
as separate fixture states. The fixture calls the same
`render_room_with_options` production compositor used by the terminal host.

The older `task5-*` and `task7` images in this directory remain historical
records; they are not evidence for the source commit above.
