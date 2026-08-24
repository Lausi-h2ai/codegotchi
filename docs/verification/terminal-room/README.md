# Terminal Room Visual Acceptance Evidence

Status: **PASS — lower-pane visual adjudication and populated live-Codex
acceptance are clean.** These eight approved PNG candidates were captured on
2026-08-24 from an uncommitted working tree. They remain deterministic native-
resolution lower-pane evidence; the separate real-session frames are linked
below.

## Approved final captures

| File | Native cells | Native pixels | Fixture state / host | SHA-256 | Inspection result |
|---|---:|---:|---|---|---|
| [`full-120x45-light.png`](full-120x45-light.png) | 120×45 | 1324×904 | Full, `soft-green`/day, awake; xterm light host | `c7b6c4fc83d493ffaca0a73a07e2616f6b85bbc31c1f80c627a3facd7b623a41` | Cat-like mascot leads a calm layered room; named care targets, demands, and status are legible with no seam or clipping. |
| [`full-120x45-dark.png`](full-120x45-dark.png) | 120×45 | 1324×904 | Full, `night`/night, awake; xterm `-bg black -fg white` | `174bfe2298a7e44efbbeb0353023b8b581606a002a55554f5a3eacba668832c3` | Night palette and empty bars remain readable against the dark host; no capture artifact. |
| [`compact-120x30-light.png`](compact-120x30-light.png) | 120×30 | 1324×604 | Compact, `soft-green`/day, awake; xterm light host | `67088478db2ecaadd38be8efb3f95c78149adee3086d1ae45526447efc6994ba` | Complete mascot, needs, stocked food, poop, bed, and care cues fit the compact vignette. |
| [`minimal-120x21-light.png`](minimal-120x21-light.png) | 120×21 | 1324×424 | Minimal, `soft-green`/day, awake; xterm light host | `e4a74cbedc2b23ce6240b3e064560e62e6b09392103324fc9bd28e435cfaeed3` | Complete mascot and core needs, food, bed, poop, affection, and snack controls stay visible and aligned. |
| [`full-120x45-bed-sleep.png`](full-120x45-bed-sleep.png) | 120×45 | 1324×904 | Full, `soft-green`/night, authoritative bed sleep; xterm light host | `7eeea49fc12cbd5ad00fa4ee69d99a48b18cc1c6e2422f8584c341b038aa1774` | Pet is on the bed with `z z z`; the floor is separate and the bed label appears once. |
| [`full-120x45-floor-doze.png`](full-120x45-floor-doze.png) | 120×45 | 1324×904 | Full, `soft-green`/day, generic floor doze; xterm light host | `ad48af24ca0c302ac5d5c34c4b5e2b92c0e53694dd37f3644be0be97cb95bfbe` | Horizontal floor Doze is distinct from compact bed Sleep while the bed remains empty. |
| [`auto-120x45-light.png`](auto-120x45-light.png) | 120×45 | 1324×904 | Full, `auto`/day, awake; xterm light host | `4656619246b106cc038fabb465ee94306e60edcdeb5545eb37c69dc631af6d74` | Auto foreground/background and ordered intermediate dithering remain readable on the light host. |
| [`auto-120x45-dark.png`](auto-120x45-dark.png) | 120×45 | 1324×904 | Full, `auto`/night, awake; xterm `-bg black -fg white` | `1ed9a1f825e488fec80083993c7700b2bb4aaa418aa068059b2017e3c89de854` | Auto switches to the dark host's default pair and remains readable; no named-gray assumption is visible. |

## Recognizability adjudication

The 2026-08-24 native-resolution visual gate passed after direct controller
inspection and a 3–1 Luna panel with an exact-requirements tiebreak. The
approved evidence satisfies these explicit criteria:

- The mascot has a cat-like silhouette at one glance.
- Idle reads as mischievous, endearing, and playful.
- Named and Auto faces remain readable on both host polarities.
- Happy, Upset, Eating, Petted, and Yawn have distinct readable expressions.
- Horizontal floor Doze and compact bed Sleep are distinct without relying on labels.
- The mascot leads a calm visual hierarchy and furniture remains subordinate.
- Care targets are separate, distinct, and discoverable.
- Compact and Minimal retain complete, uncropped compositions.
- No seam, clipping, collision, or capture artifact is visible.

All thirteen individual pose candidates were opened and inspected directly at
1324×904. They remain candidate-only inspection artifacts, not tracked
evidence files:

| Pose candidate | SHA-256 |
|---|---|
| `pose-idle.png` | `c7b6c4fc83d493ffaca0a73a07e2616f6b85bbc31c1f80c627a3facd7b623a41` |
| `pose-blink.png` | `7e1b0f3aa4c6274bc4c068e326be4f70ff51b7c3972058d3b0829fb440da9143` |
| `pose-walk-a.png` | `fea23cb9f3f847110608998ba697b863dae13db4af47f08e20853c9594bb895a` |
| `pose-walk-b.png` | `bf39dfa1dda6dd066ce46f29a97258a01df21f4b05a071faad7fd4ec66725345` |
| `pose-sit.png` | `bdbc3f58f86dc54c76db4130086947ae9017cbb30e9fa0213f9cfed8ddaa748f` |
| `pose-doze.png` | `d467ca5f5811c7b3982402e35f085092c02708291bad8cc5bad05f969381a541` |
| `pose-yawn.png` | `eda88ac41bf7e41a15e7b2c469859500c3f16a1928fd88018766f879cee4e71f` |
| `pose-curious.png` | `31c8d9cce6fb7f8a9ba5c7bf64377b15ec814e4d2fbb1f23ac4e628ac79c1b1b` |
| `pose-happy.png` | `11122799225943176fd69b4f26c80b9390d78ab1459b80416394f590be3f7cc1` |
| `pose-upset.png` | `17a7b7c2fe3457fb80177f20062c1a01de0136a9c05e0e638b8d7115a83e7963` |
| `pose-eating.png` | `d87379a0bb5ee030c5a97906b78b3114069673a268dcdcc15451cc9758f92058` |
| `pose-petted.png` | `bccd90c67da987eac2b69031c2392d12beacc2fc32f45c8b2d45e93d6297486f` |
| `pose-sleep.png` | `67c0565cfb21401c5a094a07ef8f8de7322b169f77d5fcc2141353a36ffdf2c9` |

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
| `full-120x45-light` | `full` | `soft-green` | `day` | `awake` | `idle` | `-bg white -fg black` |
| `full-120x45-dark` | `full` | `night` | `night` | `awake` | `idle` | `-bg black -fg white` |
| `compact-120x30-light` | `compact` | `soft-green` | `day` | `awake` | `idle` | `-bg white -fg black` |
| `minimal-120x21-light` | `minimal` | `soft-green` | `day` | `awake` | `idle` | `-bg white -fg black` |
| `full-120x45-bed-sleep` | `full` | `soft-green` | `night` | `bed` | `sleep` | `-bg white -fg black` |
| `full-120x45-floor-doze` | `full` | `soft-green` | `day` | `doze` | `doze` | `-bg white -fg black` |
| `auto-120x45-light` | `full` | `auto` | `day` | `awake` | `idle` | `-bg white -fg black` |
| `auto-120x45-dark` | `full` | `auto` | `night` | `awake` | `idle` | `-bg black -fg white` |

After each fixture settled, ImageMagick `import -silent -window` captured the
visible xterm. The promoted files are exact byte copies of their approved
candidates; the fourteen destination hashes and native dimensions were
verified after promotion. The three tracked `*-newart.png` iteration files
were neither promoted nor deleted.

## Real-Codex release gate

The real-session gate is **PASS**. The durable checklist is
[`live-codex/20260824T150816Z-2359630-verification.txt`](live-codex/20260824T150816Z-2359630-verification.txt).
Its populated Full frame
[`live-codex/20260824T150816Z-2359630-full-live-final.png`](live-codex/20260824T150816Z-2359630-full-live-final.png)
shows official Codex `0.149.1`, `gpt-5.6-luna low`, the isolated `test.md`
prompt and answer, and the authoritative sleeping room after pet/feed/clean/
nap. The run also retains the real approval modal, Full/Compact/Minimal resize,
paste, focus/mouse, and same-xterm interactive-shell restoration frames, paired
with content-free input/PTY receipts; no credential or bearer token is visible.

[`live-codex/task-7-round1-blocked.txt`](live-codex/task-7-round1-blocked.txt)
is retained only as historical evidence of the pre-openbox blocked preflight.
