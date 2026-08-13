# CodeGotchi Terminal Room Agent-Hardening Execution Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans`. This document is a normative execution overlay for `docs/superpowers/plans/2026-08-13-terminal-room.md`; where the two conflict, **this plan wins**.

**Goal:** Execute the terminal-room implementation in small reviewed stages while eliminating ambiguity around sleep semantics, activity mapping, PTY input negotiation, dependency selection, petting distance, visual references, and visual acceptance.

**Architecture:** Keep the base plan's PTY-backed official Codex upper pane, authoritative Rust runtime, and independent terminal-room projection. Add explicit pure mapping/state interfaces and hard gates so autonomous workers cannot conflate presentation with simulation authority or declare visual success without running and inspecting the actual UI.

**Tech Stack:** Rust stable; existing Tokio/Axum/domain runtime; selected-and-recorded compatible releases of PTY/VT/Ratatui/Crossterm dependencies; official Codex CLI; deterministic visual fixture; image-capable screenshot review.

**Required design contracts:**

- `docs/superpowers/specs/2026-08-13-terminal-room-design.md`
- `docs/superpowers/specs/2026-08-13-terminal-room-agent-hardening.md`
- `docs/superpowers/plans/2026-08-13-terminal-room.md`

## Global Constraints

- The hardening design addendum is normative and wins over conflicting wording in the base design or base implementation plan.
- The official Codex binary remains the only Codex UI; never recreate or semantically scrape its visible TUI.
- `PetBehavior::Sleeping` alone is **not** authoritative nap state. Recovery-bed sleep requires a future authoritative `napping_until`.
- Broad activity mapping is exact and exhaustive; use one pure mapper and cover every `ActivityKind`.
- PTY input encoding is driven by modes negotiated by the Codex virtual terminal.
- Petting uses exactly `16.0` backend pointer-distance units per Euclidean terminal-cell path unit; backend thresholds remain `1_500 ms` and `120.0`.
- Terminal dependency selection is an explicit compatibility task with recorded versions; no wildcard or unpinned git dependencies.
- Visual work requires repository reference assets and screenshot-based inspection.
- The orchestrator does not one-shot this feature. Each logical task has an implementer gate and an independent review gate.
- Existing browser functionality and all current domain/persistence/strict-mode/care behavior remain regression requirements.

---

## Execution Model for a Strong Orchestrator + `gpt-5.6-luna max` Workers

The controller/orchestrator owns architecture, sequencing, integration, conflict resolution, and final acceptance. Use fresh `gpt-5.6-luna max` sub-agents as focused task workers and fresh reviewer sub-agents as independent gates.

### Rules

1. **One write owner at a time by default.** Do not let multiple implementation agents edit the same worktree concurrently. Read-only audits may run in parallel. If true parallel implementation is ever used, assign non-overlapping files in isolated worktrees and integrate serially.
2. **Fresh worker per task.** Give each worker only its task brief, locked interfaces, relevant source files, and the global constraints. Do not dump the entire accumulated conversation into every worker.
3. **Fresh reviewer after every task.** Reviewer must check both spec compliance and code quality from the actual diff and test evidence.
4. **Orchestrator verifies independently.** A worker saying “done” is not evidence. The orchestrator checks the diff and reruns the task's required verification before moving on.
5. **Commit task-sized units.** Do not allow a worker to implement multiple milestones in one commit.
6. **Stop at hard gates.** Codex fidelity, missing visual references, unavailable screenshot capture, or terminal restoration failures block later dependent phases.
7. **Track state in a ledger.** Use the plan-specific `.superpowers/sdd/<plan-basename>/progress.md` workspace so context loss does not cause task re-dispatch.

### Recommended phase sequence

```text
Phase 0  Preflight + references + tool capability audit
   |
Phase 1  Milestone A: Codex launch / PTY / VT fidelity
   |     HARD GATE: real Codex fidelity passes
Phase 2  Layout + compositor + theme/sprite foundation
   |     HARD GATE: screenshot review of first real room
Phase 3  Authoritative room + presentation behavior
   |     HARD GATE: sleep/activity semantics + visual behavior pass
Phase 4  Input/care + launcher integration
   |     HARD GATE: negotiated input + care interactions pass
Phase 5  Full visual/E2E/regression acceptance
         HARD GATE: live screenshots + full test matrix
```

Milestone A may proceed while reference binaries are being prepared. Phase 2 visual authoring must not begin without the references.

---

## Phase 0 — Preflight Before Any Implementation Worker

### Task H0: Establish a trustworthy execution baseline

**Files:**
- Read: base design, base plan, hardening design addendum
- Read: `crates/codegotchi-domain/src/behavior.rs`
- Read: `crates/codegotchi-domain/src/event.rs`
- Read: `crates/codegotchi-domain/src/pet.rs`
- Read: `crates/codegotchi-domain/src/progression.rs`
- Create during execution: plan-specific SDD ledger only; no product code

**Produces:** a short preflight record in the SDD workspace with source-state, reference-assets, screenshot capability, and known blockers.

- [ ] **Step 1: Start from current code, not the stale documentation branch**

After the documentation PR is merged, create the implementation worktree/branch from the latest `master`. If implementation starts before merge, first integrate latest `master` into the implementation branch and resolve conflicts before coding.

Record:

```bash
git rev-parse HEAD
git status --short
git log -5 --oneline
```

Expected: clean implementation worktree and a base containing the latest domain fixes plus terminal-room docs.

- [ ] **Step 2: Verify the domain facts the hardening relies on**

Confirm from source that:

- `PetBehavior::Sleeping` can arise both from active nap and long idle;
- `SimulationSnapshot` exposes `napping_until`;
- `AgentActivityState` variants are `Idle`, `WaitingForUser`, `Active(_)`, `Blocked`;
- the current `ActivityKind` set matches the hardening mapping.

If the domain changed, stop and reconcile the documents before implementation rather than guessing.

- [ ] **Step 3: Inventory visual references**

Run:

```bash
find docs/mockups/terminal-room -maxdepth 2 -type f -print | sort
```

Record each image and what it is intended to guide. If the directory or required room/sprite references are missing, mark **VISUAL_REFS_BLOCKED**. Milestone A may continue; Task H7 and later visual work may not.

- [ ] **Step 4: Prove screenshot capability before visual tasks depend on it**

Determine one mechanism that can launch the terminal UI and produce an actual image file of it. Record the exact capture command/tool in the ledger.

The capability check passes only if the orchestrator can open the resulting image for visual inspection. ANSI logs or text snapshots do not count.

If no real terminal capture mechanism exists, record **LIVE_SCREENSHOT_BLOCKED**. A deterministic render-to-image fixture may still be built for iteration, but final acceptance remains blocked until a supported environment can capture the live terminal.

- [ ] **Step 5: Dispatch a read-only risk audit**

A fresh reviewer sub-agent reads the base plan plus hardening addendum and reports only contradictions, undefined interfaces, and dependency-order problems. The orchestrator resolves any load-bearing conflict before Task 1.

---

## Phase 1 — Harden Milestone A: Codex Fidelity First

Execute base Tasks 1 and 2 as written with TDD and per-task reviews. Apply the following replacements to base Tasks 3 and 4.

### Task H3: Dependency/API compatibility gate + PTY child

**Files:**
- Modify: `crates/codegotchi-cli/Cargo.toml`
- Modify: `Cargo.lock`
- Create/modify the base Task 3 PTY files
- Create: `docs/verification/terminal-room-codex-pty.md` early, then extend it during Task H4

**Consumes:** `CodexInvocation` from base Task 2.

**Produces:** reviewed dependency choices and `PtyCodexChild`.

- [ ] **Step 1: Audit candidate releases before adding dependencies**

In the implementation environment, inspect available releases/APIs for:

```text
portable-pty
vt100 (or an explicitly justified VT replacement)
ratatui
crossterm
```

The worker must report exact selected direct versions and answer these questions before product implementation:

```text
1. Does the combination compile on the repo's supported Rust toolchain?
2. Can the VT layer expose or be augmented to track application cursor mode?
3. Can it track bracketed paste?
4. Can it track focus reporting?
5. Can it track Codex's requested mouse tracking and encoding protocol?
6. Does resize work without shell-wrapping Codex?
```

Do not accept “latest compatible” as the report.

- [ ] **Step 2: Add explicit dependencies and record them**

Write explicit direct dependency requirements to `Cargo.toml`; let `Cargo.lock` pin the exact graph. No `*`, no unpinned git HEAD.

Add a table to `docs/verification/terminal-room-codex-pty.md`:

```markdown
| Crate | Direct requirement | Resolved version | Required API/mode support | Notes |
|---|---:|---:|---|---|
```

- [ ] **Step 3: Compile the dependency spike before PTY implementation**

Run:

```bash
cargo check -p codegotchi-cli
```

Expected: PASS. A dependency/API mismatch is resolved here, not later inside the host loop.

- [ ] **Step 4: Execute the base Task 3 fake-Codex PTY red/green cycle**

Retain the base plan requirements: args/env fidelity, ANSI output, input, reported PTY size, resize, and exit status `23`.

- [ ] **Step 5: Review and commit only the dependency/PTy task**

The reviewer checks the actual selected APIs against the input-mode requirements before approval.

---

### Task H4: Virtual terminal + negotiated input-mode read model

**Files:**
- Create/modify: `crates/codegotchi-cli/src/terminal/screen.rs`
- Create/modify: `crates/codegotchi-cli/src/terminal/input.rs` only for pure encoding primitives needed by the proof
- Extend: `crates/codegotchi-cli/tests/terminal_pty.rs`
- Extend: `docs/verification/terminal-room-codex-pty.md`

**Produces:** `CodexScreen` plus a read-only input-mode model used later by Task H10.

**Locked interface:**

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CodexInputModes {
    pub application_cursor_keys: bool,
    pub bracketed_paste: bool,
    pub focus_reporting: bool,
    pub mouse_tracking: MouseTrackingMode,
    pub mouse_encoding: MouseEncoding,
}

impl CodexScreen {
    pub fn input_modes(&self) -> CodexInputModes;
}
```

The exact variants of `MouseTrackingMode`/`MouseEncoding` may mirror the selected VT library, but they must distinguish disabled reporting from each protocol the implementation actually supports.

- [ ] **Step 1: Write failing mode-negotiation fixtures**

Feed terminal-control sequences that enable/disable the selected implementation's application cursor, bracketed paste, focus, and mouse modes, then assert `input_modes()` changes accordingly.

The tests must prove mode state comes from PTY terminal-control output, not visible text labels.

- [ ] **Step 2: Write failing input-encoding tests against mode state**

Required assertions:

```text
bracketed paste OFF -> no bracketed-paste delimiters
bracketed paste ON  -> delimiters present
focus reporting OFF -> focus event encodes to no PTY bytes
focus reporting ON  -> focus event encodes correctly
mouse reporting OFF -> upper-pane mouse event encodes to no PTY bytes
mouse reporting ON  -> event uses the negotiated mouse encoding
application cursor mode toggles cursor-key bytes as required
```

- [ ] **Step 3: Implement the smallest mode tracker/adapter needed**

If the VT crate already exposes the modes, adapt them. If it does not, add a protocol-side tracker fed by PTY bytes. Never infer modes from displayed Codex wording.

- [ ] **Step 4: Run ANSI/VT fixtures from base Task 4**

Keep cursor movement, erase, alternate screen, colors, styles, wide chars, resize, and unknown-sequence benign-degradation tests.

- [ ] **Step 5: Real Codex fidelity gate — run and visually inspect**

Run real installed Codex through the compositor prototype. Exercise:

```text
prompt entry
cursor/editing keys
multi-line editing if available
paste
streaming output
tool output
approval/review UI where available
scroll/click behavior when Codex requests mouse reporting
focus transitions where observable
resize
clean exit
```

Capture at least one screenshot and inspect it for corruption, clipping, cursor/layout errors, or terminal-control leakage. Record exact Codex version, terminal, commands exercised, screenshot path, and observed limitations in `docs/verification/terminal-room-codex-pty.md`.

**HARD GATE:** Do not dispatch room-art tasks if real Codex fidelity is materially broken.

---

## Phase 2 — Layout, Compositor, Theme, and First Visual Gate

Execute base Tasks 5 and 6 with their TDD/RAII requirements. Add screenshot inspection to compositor work as soon as a composed frame exists.

### Task H7: Deterministic visual fixture + semantic renderer + sprite foundation

**Files:**
- Create/modify base Task 7 theme/sprite files
- Create: `crates/codegotchi-cli/examples/terminal_room_fixture.rs`
- Extend: `crates/codegotchi-cli/tests/terminal_room.rs`
- Create during verification: `docs/verification/terminal-room/` selected evidence

**Consumes:** layout, terminal backend, semantic tones.

**Produces:** the production renderer's first deterministic, screenshotable room surface.

- [ ] **Step 1: Refuse visual authoring if references are unavailable**

Re-run the Phase 0 reference inventory. If required room/sprite references are still absent, mark the task BLOCKED before drawing substitute art.

- [ ] **Step 2: Build a deterministic fixture around the production renderer**

The example must not fork the room rendering logic. It should provide deterministic snapshots/activity/seed and make Full/Compact/Minimal states reproducible for humans and vision review.

Target invocation shape:

```bash
cargo run -p codegotchi-cli --example terminal_room_fixture -- \
  --layout full --theme auto --seed 1
```

If exact argument names need to follow existing CLI conventions, lock them in the task before implementation and use them consistently in verification docs.

- [ ] **Step 3: Complete the base four-tone/half-block red-green tests**

Implement the initial pet frames and furniture silhouettes required by the base plan.

- [ ] **Step 4: Capture Full `120 x 45` and visually compare to references**

Start the fixture, size the terminal to `120 x 45`, capture an image, open it with image-capable review, and record the three largest material deltas from the primary Full-room reference.

The first review explicitly checks:

```text
Codex/room vertical hierarchy
side-view bedroom readability
pet scale and focal prominence
window/desk/shelf/wardrobe/bed/plant balance
status-bar alignment
four-tone readability
half-block seams/clipping
```

- [ ] **Step 5: Iterate when deltas are material**

Adjust renderer/sprites, rerun `terminal_room` tests, recapture, and reinspect. Do not stop at “snapshot tests pass” when the image still looks materially wrong.

- [ ] **Step 6: Independent visual reviewer gate**

A fresh reviewer receives the reference images plus final screenshot, not only the code diff. Approval requires both implementation quality and visual-spec compliance.

---

## Phase 3 — Authoritative Room + Hardened Presentation Semantics

### Task H8: Base authoritative room composition + responsive visual evidence

Execute base Task 8, then capture and inspect all three canonical sizes:

```text
Full    120 x 45
Compact 120 x 30
Minimal 120 x 21
```

The reviewer must confirm that decoration disappears before core care and that hitboxes visually correspond to objects after each resize.

Selected final Full/Compact/Minimal screenshots may be committed under `docs/verification/terminal-room/`; iteration screenshots may remain temporary.

---

### Task H9: Exact activity mapping + sleep semantic split + living behavior

**Files:**
- Create/modify: `crates/codegotchi-cli/src/terminal/behavior.rs`
- Extend: `crates/codegotchi-cli/tests/terminal_room.rs`

**Produces:** pure `PresentationActivity` mapping and presentation state with explicit authoritative-nap distinction.

**Locked activity mapper behavior:**

```rust
pub fn presentation_activity(snapshot: &SimulationSnapshot) -> PresentationActivity {
    match snapshot.activity {
        AgentActivityState::Blocked | AgentActivityState::WaitingForUser => {
            PresentationActivity::WaitingOrBlocked
        }
        AgentActivityState::Active(ActivityKind::Idle) => PresentationActivity::Calm,
        AgentActivityState::Active(ActivityKind::Thinking) => PresentationActivity::Thinking,
        AgentActivityState::Active(ActivityKind::Waiting | ActivityKind::Blocked) => {
            PresentationActivity::WaitingOrBlocked
        }
        AgentActivityState::Active(ActivityKind::Celebrating) => PresentationActivity::Success,
        AgentActivityState::Active(ActivityKind::Error) => PresentationActivity::Failure,
        AgentActivityState::Active(
            ActivityKind::Reading
            | ActivityKind::Searching
            | ActivityKind::Editing
            | ActivityKind::Testing
            | ActivityKind::Building
            | ActivityKind::Installing
            | ActivityKind::GitOperation
            | ActivityKind::DockerOperation
            | ActivityKind::WebResearch
            | ActivityKind::UnknownWork,
        ) => PresentationActivity::Working,
        AgentActivityState::Idle => match snapshot.behavior {
            PetBehavior::RecentSuccess => PresentationActivity::Success,
            PetBehavior::RecentFailure => PresentationActivity::Failure,
            _ => PresentationActivity::Calm,
        },
    }
}
```

If Rust exhaustiveness or domain imports require syntactic adjustment, preserve this exact mapping.

**Locked nap rule:**

```rust
pub fn has_authoritative_nap(snapshot: &SimulationSnapshot) -> bool {
    snapshot
        .napping_until
        .is_some_and(|until| snapshot.last_updated_at < until)
}
```

- [ ] **Step 1: Write an exhaustive table test for every `ActivityKind`**

Every current variant must appear. A new future variant must require an explicit mapping decision.

- [ ] **Step 2: Write the two-Sleeping regression test**

Create two otherwise-equivalent snapshots with `PetBehavior::Sleeping`:

```text
A: future napping_until -> bed sleep presentation
B: napping_until None  -> non-bed idle doze/yawn/Calm presentation
```

Assert B never selects a bed target or emits a care action.

- [ ] **Step 3: Execute the base seeded living-behavior tests**

Keep the prohibition on `Feed`, `Clean`, `Nap`, or `Pet` variants in autonomous intents.

- [ ] **Step 4: Visually inspect behavioral states**

Capture at least:

```text
Calm roaming/idle
Thinking modifier
Working modifier
Success
Failure
WaitingOrBlocked
generic idle doze
true authoritative bed sleep
```

The visual review must distinguish the last two unambiguously.

---

## Phase 4 — Input/Care and Launcher Integration

### Task H10: Mode-driven input routing + exact petting conversion

Execute base Task 10 with the following locked replacement for its calibration placeholder.

**Locked constant and formula:**

```rust
pub const POINTER_DISTANCE_PER_CELL: f32 = 16.0;

pub fn pointer_distance(path: &[Point]) -> f32 {
    path.windows(2)
        .map(|segment| {
            let dx = f32::from(segment[1].x) - f32::from(segment[0].x);
            let dy = f32::from(segment[1].y) - f32::from(segment[0].y);
            dx.hypot(dy)
        })
        .sum::<f32>()
        * POINTER_DISTANCE_PER_CELL
}
```

Use the repository's actual point coordinate type; preserve the formula and scale.

- [ ] **Step 1: Write exact distance tests**

Required expectations:

```rust
assert_eq!(horizontal_cells(7), 112.0);
assert_eq!(horizontal_cells(8), 128.0);
assert_eq!(diagonal_3_4_cells(), 80.0);
```

With sufficient duration, 7 cells remains below backend threshold and 8 cells exceeds it.

- [ ] **Step 2: Route Codex input through `CodexInputModes`**

Keys/paste/focus/upper-pane mouse encoding consumes mode state from Task H4. Do not duplicate a second terminal-mode parser in the input module.

- [ ] **Step 3: Retain room/Codex ownership tests**

Room mouse events never reach Codex. Codex-pane mouse events only reach the PTY when the negotiated mode requests them.

- [ ] **Step 4: Complete food, poop, bed, and petting runtime tests from base Task 10**

Bed presentation changes to recovery-bed sleep only after authoritative nap state is observed.

- [ ] **Step 5: Run a screenshot interaction check**

After resizing each layout, visually confirm rendered food/bed/poop/pet affordances still align with their hit regions. Use a debug hitbox overlay only during verification; do not ship it as normal UI unless separately approved.

---

### Task H11: Launcher modes

Execute base Task 11 unchanged in semantics, with one additional real smoke matrix after tests:

```text
--ui terminal
--ui browser
--ui both
--ui auto in interactive terminal
--ui auto with forced terminal-init failure/fallback fixture
```

Confirm no path spawns Codex twice and no fallback occurs after a terminal-mode child has already been spawned.

---

## Phase 5 — Final Visual and End-to-End Acceptance

### Task H12: Full verification evidence

Execute every base Task 12 Rust/web/Playwright gate. Add the following mandatory visual acceptance.

- [ ] **Step 1: Capture final canonical screenshots**

From a live supported environment, capture:

```text
Full dark/default-compatible theme       120 x 45
Full light/default-compatible theme      120 x 45
Compact                                  120 x 30
Minimal                                  120 x 21
Authoritative bed sleep                  Full or Compact
Generic idle doze (not nap)              Full or Compact
```

Use the actual official Codex child for at least the final Full live-terminal capture. The deterministic fixture is not enough for this final item.

- [ ] **Step 2: Image-based final review**

A fresh visual reviewer receives:

```text
all repository reference room/sprite images
final screenshots
base design
hardening design addendum
```

The reviewer reports PASS/FAIL for each:

```text
composition hierarchy
room readability
pet scale/style
furniture balance
status/care readability
Full/Compact/Minimal degradation
light/dark contrast
half-block rendering quality
true nap vs idle doze distinction
obvious input/render corruption
```

Any high-severity mismatch returns to the owning task; do not waive it with “terminal art is subjective.”

- [ ] **Step 3: Record evidence**

`docs/verification/terminal-room.md` must include:

```text
implementation commit SHA
Codex version
terminal/OS
terminal dependency versions
reference asset manifest
commands used for tests
commands/tool used for screenshots
paths to selected screenshots
visual-review findings and fixes
known limitations
```

- [ ] **Step 4: Run the complete automated matrix fresh**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
corepack pnpm lint
corepack pnpm test
corepack pnpm format:check
corepack pnpm build
node web/scripts/embed-web.mjs
corepack pnpm playwright:test
```

Do not claim completion from prior worker logs; the orchestrator runs these against the final integrated HEAD.

- [ ] **Step 5: Final whole-branch reviewer**

Use the strongest available reviewer model. It reviews the complete implementation diff, verification evidence, and screenshots. Only after this gate is clean does the orchestrator proceed to branch finishing/PR work.

---

## Reviewer Hotspots

Every task reviewer should explicitly look for these failure modes when relevant:

- `PetBehavior::Sleeping` used as a proxy for `napping_until`;
- a room animation calling runtime mutation methods autonomously;
- activity mapping that collapses `Thinking` into generic Working;
- recent outcome overriding a current blocked/active state;
- visible Codex text being parsed for semantic activity;
- hard-coded paste/focus/mouse bytes emitted without checking virtual-terminal modes;
- upper-pane mouse bytes emitted while Codex mouse reporting is disabled;
- petting distance based on guessed physical pixels or a scale other than `16.0` per terminal-cell path unit;
- wildcard/unrecorded terminal dependencies;
- visual tests that only compare text/snapshots and never inspect an image;
- a worker claiming “matches the mockup” without attaching or opening a screenshot;
- Compact/Minimal layouts hiding a care action needed for recovery;
- terminal cleanup paths that leave raw mode, mouse capture, alternate screen, or cursor state dirty.

## Completion Rule

The terminal-room implementation is complete only when the base plan's Definition of Done **and** the hardening addendum's additional acceptance criteria are both satisfied with fresh evidence.
