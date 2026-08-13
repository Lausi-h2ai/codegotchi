# CodeGotchi Terminal Room Agent-Hardening Design Addendum

**Date:** 2026-08-13  
**Status:** Normative addendum  
**Amends:** `docs/superpowers/specs/2026-08-13-terminal-room-design.md`

This addendum closes the remaining ambiguities that are risky for autonomous implementation. Where this document conflicts with the base terminal-room design, **this addendum wins**. Requirements not changed here remain governed by the base design.

## 1. Authoritative sleep vs. generic idle `Sleeping`

The current domain intentionally overloads `PetBehavior::Sleeping`:

1. it is returned while `napping_until` represents an active authoritative nap; and
2. it is also returned after 30 minutes without activity even when no authoritative nap exists.

The terminal renderer must **not** infer bed use, nap state, or energy recovery from `PetBehavior::Sleeping` alone.

The authoritative terminal sleep rule is:

```rust
let authoritative_nap = snapshot
    .napping_until
    .is_some_and(|until| snapshot.last_updated_at < until);
```

Presentation rules:

- `authoritative_nap == true` -> render the pet sleeping **in the bed** and treat the bed sleep pose as authoritative presentation.
- `authoritative_nap == false && snapshot.behavior == PetBehavior::Sleeping` -> render only harmless non-authoritative idling such as a floor doze, closed-eye sit, yawn, stretch, or ordinary Calm behavior.
- Generic idle `PetBehavior::Sleeping` must never move the pet into the recovery bed, submit `nap`, imply energy recovery, or block ordinary autonomous room life.
- The bed may be entered only after an explicit user bed interaction has produced authoritative nap state.

A regression test must construct two snapshots with `behavior == PetBehavior::Sleeping`: one with a future `napping_until`, one with `napping_until == None`, and prove that only the first selects the bed-sleep presentation.

## 2. Exact `PresentationActivity` mapping

The terminal presentation must use the authoritative structured activity state. It must not scrape or classify visible Codex terminal text.

The mapping is exact and exhaustive.

### Precedence

1. Current aggregate `AgentActivityState` wins over stale recent outcomes.
2. `Blocked` and waiting states map to `WaitingOrBlocked`.
3. An active `ActivityKind` maps according to the table below.
4. Only when aggregate activity is `Idle` may the already-derived recent outcome behavior select `Success` or `Failure`.
5. Otherwise idle maps to `Calm`.

### Mapping table

| Authoritative state | PresentationActivity |
|---|---|
| `AgentActivityState::Blocked` | `WaitingOrBlocked` |
| `AgentActivityState::WaitingForUser` | `WaitingOrBlocked` |
| `Active(ActivityKind::Idle)` | `Calm` |
| `Active(ActivityKind::Thinking)` | `Thinking` |
| `Active(ActivityKind::Reading)` | `Working` |
| `Active(ActivityKind::Searching)` | `Working` |
| `Active(ActivityKind::Editing)` | `Working` |
| `Active(ActivityKind::Testing)` | `Working` |
| `Active(ActivityKind::Building)` | `Working` |
| `Active(ActivityKind::Installing)` | `Working` |
| `Active(ActivityKind::GitOperation)` | `Working` |
| `Active(ActivityKind::DockerOperation)` | `Working` |
| `Active(ActivityKind::WebResearch)` | `Working` |
| `Active(ActivityKind::UnknownWork)` | `Working` |
| `Active(ActivityKind::Waiting)` | `WaitingOrBlocked` |
| `Active(ActivityKind::Blocked)` | `WaitingOrBlocked` |
| `Active(ActivityKind::Celebrating)` | `Success` |
| `Active(ActivityKind::Error)` | `Failure` |
| `Idle` + `PetBehavior::RecentSuccess` | `Success` |
| `Idle` + `PetBehavior::RecentFailure` | `Failure` |
| all other `Idle` | `Calm` |

`CriticalNeed` is orthogonal to `PresentationActivity`; need-driven visuals may modify expression/room behavior without rewriting Codex activity semantics.

The implementation must use one pure mapping function and a table-driven test that covers every current `ActivityKind` variant. Adding a new `ActivityKind` must cause a compile error or failing exhaustive test until its terminal mapping is chosen deliberately.

## 3. PTY input fidelity is mode-driven

The virtual terminal is not just a screen buffer. It also owns the terminal modes negotiated by Codex. Input encoding must respect those modes.

The terminal host must maintain an input-mode read model derived from Codex PTY output. At minimum the implementation must account for modes that affect:

- application cursor-key encoding;
- bracketed paste;
- focus reporting;
- mouse tracking enable/disable and tracking level;
- the negotiated mouse coordinate/encoding protocol used by the current supported Codex release.

A suitable interface is:

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CodexInputModes {
    pub application_cursor_keys: bool,
    pub bracketed_paste: bool,
    pub focus_reporting: bool,
    pub mouse_tracking: MouseTrackingMode,
    pub mouse_encoding: MouseEncoding,
}
```

The exact enum variants may follow the terminal library API, but the behavior is fixed:

- Paste delimiters are emitted only when Codex has enabled bracketed paste; otherwise paste content is sent without bracket markers.
- Focus-in/focus-out sequences are emitted only when Codex has enabled focus reporting.
- Upper-pane mouse events are encoded only while Codex has enabled a mouse-tracking mode, and they must use the active encoding/protocol rather than a hard-coded sequence.
- Cursor/editing keys whose encoding changes under application mode must honor the active virtual-terminal mode.
- Room mouse events are never forwarded to Codex.
- Unsupported/unknown modes may degrade the affected input feature, but must not crash or corrupt the terminal host.

If the chosen VT parser does not expose all required modes, the implementation must add a small protocol-side mode tracker or choose a parser that does. It must **not** work around missing mode state by inspecting visible Codex text.

The real-Codex fidelity gate must exercise paste, focus where observable, keyboard navigation, scrolling/clicking where Codex requests mouse reporting, and resize.

## 4. Exact terminal-cell petting conversion

Do not calibrate petting ad hoc.

Petting path distance is measured as the sum of Euclidean distances between successive terminal-cell pointer positions and converted to the existing backend-compatible `pointer_distance` metric with a fixed scale:

```rust
const POINTER_DISTANCE_PER_CELL: f32 = 16.0;

pointer_distance = segment_distances_in_terminal_cells.sum::<f32>()
    * POINTER_DISTANCE_PER_CELL;
```

For each pointer segment:

```text
segment_cells = sqrt(delta_column^2 + delta_row^2)
```

Consequences that must be tested:

- 7 horizontal cells -> `112.0` -> below the existing `120.0` threshold.
- 8 horizontal cells -> `128.0` -> above the threshold.
- a 3-by-4 cell diagonal segment -> 5 cells -> `80.0`.
- The backend threshold remains exactly `120.0`; only the terminal-to-backend conversion is new.
- The duration threshold remains exactly `1_500 ms`.

This is deliberately a stable logical interaction unit, not an attempt to infer physical monitor pixels from terminal font geometry.

## 5. Visual-reference contract

Visual implementation is reference-driven, not prose-only.

The canonical repository location for terminal-room mockups and sprite references is:

```text
docs/mockups/terminal-room/
```

Before visual work begins, the orchestrator must inventory every image in that directory and record which files are used for:

- overall Full-room composition;
- Compact/Minimal direction when available;
- pet silhouette and scale;
- individual poses/animations when available;
- theme/palette direction.

The full-room concept provided during planning is the primary composition reference once it is materialized in this directory. Supplied sprite references are style/pose references; agents must not silently replace them with unrelated generated art.

**If the required reference binaries are not present locally, Milestone A may continue, but visual implementation is BLOCKED before sprite/room authoring.** The agent must report the missing references instead of inventing them.

## 6. Mandatory screenshot-and-inspect loop

Any task that materially changes the terminal compositor, room layout, sprite art, theme, animation framing, status bars, or mouse affordances has a visual gate in addition to automated tests.

For each applicable task:

1. Start the actual CodeGotchi terminal screen or the deterministic terminal-room fixture that uses the same production renderer.
2. Put it in the target layout/theme/state.
3. Capture a screenshot of the rendered terminal.
4. Inspect the screenshot visually with image-capable review against the repository references.
5. Record the largest mismatches.
6. Adjust the implementation when the mismatch is material.
7. Re-run automated tests.
8. Capture and inspect again.
9. Repeat until no high-severity visual mismatch remains.

A textual frame dump, ANSI log, snapshot test, or successful compile is **not** a substitute for screenshot inspection.

Recommended canonical capture sizes:

- Full: `120 x 45` terminal cells;
- Compact: `120 x 30` terminal cells;
- Minimal: `120 x 21` terminal cells.

At least the final Full-room evidence must be inspected in both a dark and light/default-terminal theme configuration when the environment supports both.

The visual reviewer should explicitly check:

- Codex remains the dominant upper pane and is not visually recreated by CodeGotchi;
- the lower area reads immediately as a cozy side-view bedroom;
- the pet is the focal point and remains legible at terminal scale;
- furniture is recognizable but does not overpower the pet or status information;
- status bars and care affordances are aligned and readable;
- half-block packing has no obvious seams, clipping, or accidental checkerboard noise;
- light/dark theme adaptation retains contrast;
- Full -> Compact -> Minimal removes decoration before core care;
- interactive hit regions visually correspond to the rendered object positions;
- authoritative bed sleep is visibly distinct from harmless generic idle dozing.

Selected final screenshots and the inspection notes belong in `docs/verification/terminal-room/`. Temporary iteration captures do not need to be committed.

If the execution environment cannot take a real terminal screenshot, the agent must mark the visual gate **BLOCKED** rather than claiming it passed. A deterministic renderer-to-image fixture may be used for iteration, but final acceptance still requires a live terminal capture on a supported Linux/WSL/macOS environment.

## 7. Dependency-selection contract

Dependency versions must not be left as an implicit "current compatible releases" choice.

The PTY task owns a short dependency/API compatibility gate before production code:

- inspect currently available releases of `portable-pty`, `vt100` (or an explicitly justified replacement), `ratatui`, and `crossterm` in the implementation environment;
- verify the selected combination compiles on the repository's supported Rust toolchain;
- verify the VT choice can expose or support the input-mode requirements in Section 3;
- write explicit direct dependency requirements to `crates/codegotchi-cli/Cargo.toml` and exact resolved versions to `Cargo.lock`;
- record the selected versions and any parser limitations in `docs/verification/terminal-room-codex-pty.md`;
- do not use wildcard versions, unpinned git HEAD dependencies, or a library combination whose required input modes are known to be inaccessible.

This selection is part of the PTY proof task and must be reviewed before the real-Codex fidelity gate.

## Additional acceptance criteria

The terminal-room feature is not acceptable until all of the following are demonstrated:

- generic 30-minute idle `PetBehavior::Sleeping` does not put the pet in the recovery bed;
- authoritative `napping_until` does put the pet in the recovery-bed sleep pose after explicit user interaction;
- every `ActivityKind` maps deterministically to one broad `PresentationActivity`;
- paste/focus/mouse/key encoding follows Codex-negotiated virtual-terminal modes;
- an 8-cell qualifying pet gesture can exceed `pointer_distance == 120.0` while a 7-cell gesture cannot;
- applicable visual tasks include screenshot evidence and image-based review, not only textual snapshots;
- the final live terminal is visually compared against the supplied room/sprite references before completion is claimed.
