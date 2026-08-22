# Terminal Room Visual Acceptance Evidence

Status: **PASS — lower-pane visual adjudication is clean; populated live-Codex
acceptance remains a separate blocked release gate.** These eight PNGs were
recaptured from the production Rust compositor at source SHA
`c8371251de26db2cf9d5795873ee95c33ccd4800` on 2026-08-22. The six named-state
frames also update the matching files in `docs/mockups/current/`.

## Final-candidate captures

| File | Cells | Pixels | Fixture state / host | SHA-256 | Inspection result |
|---|---:|---:|---|---|---|
| [`full-120x45-light.png`](full-120x45-light.png) | 120×45 | 1324×904 | Full, SoftGreen/day, awake; xterm default light host | `7bc9b05cf8af684e5f9fda35d669755ac1e6e0c2e9be3ea2079931a0cbb86d99` | Layered room, large mascot, stocked care targets, demands, and status strip are legible; no seam or clipping. |
| [`full-120x45-dark.png`](full-120x45-dark.png) | 120×45 | 1324×904 | Full, Night/night, awake; xterm `-bg black -fg white` | `83adc8d59e2a50ff4c72dc088cdbadf2f3ccaa0555957aafdc624c969003e55f` | Blue night composition and empty bars remain readable against the dark host; no capture artifact. |
| [`compact-120x30-light.png`](compact-120x30-light.png) | 120×30 | 1324×604 | Compact, SoftGreen/day, awake; xterm default light host | `180625c741eac200b20b2c76d3defe1f13b9592e6e64169bcd4908d6986578ba` | Mascot-led seven-row vignette preserves needs, stocked food, poop, bed, and care cues. |
| [`minimal-120x21-light.png`](minimal-120x21-light.png) | 120×21 | 1324×424 | Minimal, SoftGreen/day, awake; xterm default light host | `f87796b01124a3744783a8ea6512fe5ed8100cc76e00f6b35d98a49d59557305` | Three-row packed mascot, needs, food, bed, poop, affection, and snack controls fit cleanly. |
| [`full-120x45-bed-sleep.png`](full-120x45-bed-sleep.png) | 120×45 | 1324×904 | Full, SoftGreen/night, future authoritative bed nap | `50d5ecd24b940e8e3a7b8f852b158afd9540412cab0438afdda4dabdb2f2a216` | Pet is on the bed with `z z z`; the floor is separate and the bed label appears once. |
| [`full-120x45-floor-doze.png`](full-120x45-floor-doze.png) | 120×45 | 1324×904 | Full, SoftGreen/day, generic sleeping without deadline | `84780797a151d412db4fb4925079fe6263636e9ba5370c3fe38d9d0cb8d6aaaf` | Horizontal floor doze is distinct from bed sleep while the bed remains empty. |
| [`auto-120x45-light.png`](auto-120x45-light.png) | 120×45 | 1324×904 | Full, Auto/day, awake; xterm default light host | `6efb9088e6926050ba044af5cf48d7b6f57f1d45584c8805684c431523ff4053` | Default foreground/background and ordered intermediate dithering remain readable on the light host. |
| [`auto-120x45-dark.png`](auto-120x45-dark.png) | 120×45 | 1324×904 | Full, Auto/night, awake; xterm `-bg black -fg white` | `fba969ec9c09f677f1a6766723c1c152dea22148899448f6da91fd188dbf2200` | Auto switches to the dark host's default pair and remains readable; no named-gray assumption is visible. |

## Inspection adjudication

All eight fresh PNGs were opened directly on 2026-08-22. The lower pane is
clean at Full, Compact, and Minimal sizes: the mascot is the visual focal
element, room layers remain identifiable, and every core care target is
visible without a seam, xterm capture artifact, or visible hit-target
clipping. Auto is readable on both host polarities. Authoritative bed sleep
and generic floor doze remain unmistakably distinct.

This is a **visual PASS for the lower pane**. The fixture intentionally has no
real Codex process, leaving the upper host pane blank; that limitation does
not invalidate the lower-pane adjudication. It does mean these captures do
not claim the populated live-Codex requirement. The live gate remains
**BLOCKED** until an authorized session supplies the required non-text
receipts and a populated Full frame.

## Capture provenance

The fixture was rebuilt from the exact source SHA recorded above:

```text
cargo build -p codegotchi-cli --example terminal_room_fixture
```

The capture environment was Ubuntu 24.04 under WSL2
(`Linux 6.6.87.2-microsoft-standard-WSL2`), fresh TCP-only Xvfb display
`:127` with a `1600x1200x24` screen, `xterm 390`, Noto Sans Mono 10 pt,
ImageMagick `6.9.12-98`, `xdotool 3.20160805.1`, and Rust `cargo 1.97.1`.
Every frame used a fresh xterm with `-fa "Noto Sans Mono" -fs 10`,
`CG_FIXTURE_BOTTOM=1`, `CG_FIXTURE_PAUSE_MS=5000`, `TERM=xterm-256color`,
`NO_COLOR=`, and `CODEGOTCHI_BROWSER=none`. The fixture calls the production
`render_room_with_options` compositor and contains no Codex UI.

The exact state substitutions were:

```text
full-120x45-light       full    soft-green day   awake  default light host
full-120x45-dark        full    night      night awake  -bg black -fg white
compact-120x30-light    compact soft-green day   awake  default light host
minimal-120x21-light    minimal soft-green day   awake  default light host
full-120x45-bed-sleep   full    soft-green night bed    default light host
full-120x45-floor-doze full    soft-green day   doze   default light host
auto-120x45-light       full    auto       day   awake  default light host
auto-120x45-dark        full    auto       night awake  -bg black -fg white
```

After each fixture settled, ImageMagick `import -silent -window` captured the
visible xterm found by `xdotool`. The six current-state files and matching
final-evidence files were captured in the same pass; dimensions and SHA-256
values in the table were checked immediately afterward.

## Real-Codex boundary

[`live-codex/task-7-round1-blocked.txt`](live-codex/task-7-round1-blocked.txt)
remains the durable live-session record. It correctly fails fast when private
Xvfb has no supported lightweight window manager and does not claim prompt,
model/tool, approval, paste, focus/mouse, resize, final-pet, or restoration
receipts. No bearer token or auth contents are included in this evidence.
