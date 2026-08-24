# Current terminal-room visual evidence

Status: **PASS — lower-pane visual adjudication is clean; the separate
populated live-Codex release gate now also passes.** These six approved PNG
candidates were captured on 2026-08-24 from an uncommitted working tree.
They are the current named-state mockups; the matching eight verification
captures are documented in
[`docs/verification/terminal-room/README.md`](../../verification/terminal-room/README.md).

## Approved current captures

| Screenshot | Native fixture state / host | Native cells / pixels / SHA-256 | Direct inspection |
|---|---|---:|---|
| [`full-light.png`](full-light.png) | Full, `soft-green`/day, awake; xterm light host | 120×45 / 1324×904 / `c7b6c4fc83d493ffaca0a73a07e2616f6b85bbc31c1f80c627a3facd7b623a41` | Cat-like mascot leads a calm layered room; named care targets, demands, and status remain readable with no seam or clipping. |
| [`full-dark.png`](full-dark.png) | Full, `night`/night, awake; xterm `-bg black -fg white` | 120×45 / 1324×904 / `174bfe2298a7e44efbbeb0353023b8b581606a002a55554f5a3eacba668832c3` | Night palette and empty bars remain readable against the dark host; no capture artifact. |
| [`compact-light.png`](compact-light.png) | Compact, `soft-green`/day, awake; xterm light host | 120×30 / 1324×604 / `67088478db2ecaadd38be8efb3f95c78149adee3086d1ae45526447efc6994ba` | Complete mascot, needs, stocked food, poop, bed, and care cues fit the compact vignette. |
| [`minimal-light.png`](minimal-light.png) | Minimal, `soft-green`/day, awake; xterm light host | 120×21 / 1324×424 / `e4a74cbedc2b23ce6240b3e064560e62e6b09392103324fc9bd28e435cfaeed3` | Complete mascot and core needs, food, bed, poop, affection, and snack controls stay visible and aligned. |
| [`full-bed-sleep.png`](full-bed-sleep.png) | Full, `soft-green`/night, bed sleep; xterm light host | 120×45 / 1324×904 / `7eeea49fc12cbd5ad00fa4ee69d99a48b18cc1c6e2422f8584c341b038aa1774` | Pet is visibly on the bed with `z z z`; floor care objects remain separate and the bed label is singular. |
| [`full-floor-doze.png`](full-floor-doze.png) | Full, `soft-green`/day, floor doze; xterm light host | 120×45 / 1324×904 / `ad48af24ca0c302ac5d5c34c4b5e2b92c0e53694dd37f3644be0be97cb95bfbe` | Horizontal floor Doze is visibly distinct from compact bed Sleep; the bed remains empty. |

## Recognizability adjudication

The 2026-08-24 native-resolution gate passed after direct controller
inspection and a 3–1 Luna panel with an exact-requirements tiebreak. The
result is adjudicated as follows:

- The mascot has a cat-like silhouette at one glance.
- Idle reads as mischievous, endearing, and playful.
- Named and Auto faces remain readable on both host polarities.
- Happy, Upset, Eating, Petted, and Yawn have distinct readable expressions.
- Horizontal floor Doze and compact bed Sleep are distinct without relying on labels.
- The mascot leads a calm visual hierarchy and furniture remains subordinate.
- Care targets are separate, distinct, and discoverable.
- Compact and Minimal retain complete, uncropped compositions.
- No seam, clipping, collision, or capture artifact is visible.

All thirteen individual pose candidates were inspected at 1324×904. Their
candidate-only hashes are recorded in the verification README; those pose
PNGs are not promoted as tracked evidence files.

The fixture deliberately leaves the upper pane without a real Codex process,
so these deterministic lower-pane captures are not themselves live evidence.
The separate authorized live session is now **PASS** and is recorded in
[`docs/verification/terminal-room/live-codex/20260824T150816Z-2359630-verification.txt`](../../verification/terminal-room/live-codex/20260824T150816Z-2359630-verification.txt).

## Capture provenance

The final correction recapture source was an **uncommitted working tree**, based on base
commit `1f0f8db0aca15bc172a12e508bad2ceee3665575`. Its exact product-source
working-tree binary diff was measured with this source-only command:

```text
git diff --binary -- crates/codegotchi-cli/examples/terminal_room_fixture.rs crates/codegotchi-cli/src/terminal/behavior.rs crates/codegotchi-cli/src/terminal/room.rs crates/codegotchi-cli/src/terminal/sprites.rs crates/codegotchi-cli/tests/terminal_room.rs | sha256sum
```

The command output was
`4645edebe436968a9baa55b786893bcc5151de6e9c74060b0b6ed6aad42391aa`.
This records the exact source-only scope used for the final recapture; `HEAD`
alone is not asserted as the exact capture source.

Environment: Ubuntu 24.04 under WSL2, Linux
`6.6.87.2-microsoft-standard-WSL2`, xterm `390`, ImageMagick `6.9.12-98`,
xdotool `3.20160805.1`, and Rust cargo `1.97.1`. The display used a fresh,
TCP-only, dynamically allocated Xvfb transport with `-nolisten unix
-listen tcp -displayfd`, an X screen of `1600x1200x24`, and Noto Sans Mono
10 pt.

The fixture was built with:

```text
cargo build -p codegotchi-cli --example terminal_room_fixture
```

Each capture used a fresh xterm with `-geometry 120x45`, `120x30`, or
`120x21`, `-fa "Noto Sans Mono" -fs 10`, and a display published as
`127.0.0.1:<displayfd>`. Xvfb was started with the equivalent of:

```text
Xvfb -displayfd 3 -screen 0 1600x1200x24 -ac -nolisten unix -listen tcp -terminate 1
```

Common fixture variables were `TERM=xterm-256color`, `NO_COLOR=`,
`CODEGOTCHI_BROWSER=none`, `CG_FIXTURE_BOTTOM=1`, and
`CG_FIXTURE_PAUSE_MS=5000`. The exact per-frame substitutions were:

| Capture | `CG_FIXTURE_LAYOUT` | `CG_FIXTURE_THEME` | `CG_FIXTURE_TIME_OF_DAY` | `CG_FIXTURE_SLEEP` | `CG_FIXTURE_POSE` | Host |
|---|---|---|---|---|---|---|
| `full-light` | `full` | `soft-green` | `day` | `awake` | `idle` | `-bg white -fg black` |
| `full-dark` | `full` | `night` | `night` | `awake` | `idle` | `-bg black -fg white` |
| `compact-light` | `compact` | `soft-green` | `day` | `awake` | `idle` | `-bg white -fg black` |
| `minimal-light` | `minimal` | `soft-green` | `day` | `awake` | `idle` | `-bg white -fg black` |
| `full-bed-sleep` | `full` | `soft-green` | `night` | `bed` | `sleep` | `-bg white -fg black` |
| `full-floor-doze` | `full` | `soft-green` | `day` | `doze` | `doze` | `-bg white -fg black` |

After each fixture settled, ImageMagick `import -silent -window` captured the
visible xterm. The six mockup PNGs and matching verification copies were
promoted byte-for-byte from the approved candidate set; all are non-empty RGB
PNGs and the hashes above were computed from the promoted files.
