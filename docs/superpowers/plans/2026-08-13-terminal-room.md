# CodeGotchi Terminal Room Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run the official Codex CLI inside a PTY-backed upper pane while CodeGotchi renders a responsive, mouse-interactive pixel-art bedroom below it, reusing the existing authoritative Rust simulation and preserving browser fallback/update compatibility.

**Architecture:** Keep the current launcher/runtime/profile/server lifecycle. Replace only the final inherited-stdio Codex handoff when terminal mode is selected: build one guarded Codex invocation, spawn the official binary in a PTY, parse its ANSI/VT output into a virtual screen, and compose that screen with a Ratatui CodeGotchi room. The room consumes `AuthoritativeRuntime::subscribe()` and submits the existing `feed`, `clean`, `nap`, and `pet` methods; it owns presentation and mouse gestures only.

**Tech Stack:** Rust stable; existing Tokio/Axum/domain runtime; `portable-pty` for child PTY; `vt100` for terminal emulation; `ratatui` + `crossterm` for composition/input; existing chrono/uuid/serde/SQLite; existing React browser UI remains supported.

**Design contract:** `docs/superpowers/specs/2026-08-13-terminal-room-design.md`

## Global Constraints

- The child process is the official Codex executable resolved by existing launcher logic. Do not fork, patch, or recreate Codex.
- Never parse Codex-visible text for semantic state such as Thinking/Working. Use existing hook/domain activity only.
- Preserve exact ordinary Codex trailing-argument order and the existing generated additive profile semantics.
- Codex owns keyboard input; CodeGotchi owns mouse interaction only inside the room rectangle.
- Existing Rust simulation remains authoritative for needs, inventory, pending demands/poops, sleeping, persistence, replay safety, and strict-mode policy.
- Autonomous room behavior may express needs but must never repair them.
- Bed use is explicit; the pet never autonomously starts energy recovery.
- Preserve backend petting validation: at least `1_500 ms` and `120 px`-equivalent travel.
- Full -> Compact -> Minimal layout gives Codex priority when space shrinks.
- Existing browser UI remains available and must continue passing its current tests.
- Initial terminal-host platform envelope: Linux, WSL, macOS; native Windows is not introduced by this plan.
- Preserve current authentication, privacy, persistence, WebSocket, hook trust, strict-mode, care-pressure, motion/blinking, shovel, food, and nap behavior.

---

## Feature Set

### F1. Official Codex PTY pane

- Spawn the existing resolved Codex binary with the existing generated profile/environment.
- Parse/render ANSI/VT output into an upper rectangle.
- Forward keyboard/paste/focus and supported Codex-pane mouse events.
- Propagate resize and child exit status.

### F2. Responsive terminal room

- Full mode: ~14 rows, complete side-view bedroom.
- Compact mode: ~7 rows, simplified room.
- Minimal mode: 3 rows, pet + needs + essential care.
- Hysteresis prevents resize flicker.

### F3. Theme-adaptive monochrome pixel art

- Four semantic tones.
- Auto terminal-default rendering.
- Presets: mono, soft-green, amber, night.
- Half-block sprite packing.

### F4. Living presentation behavior

- Calm roaming/idle actions.
- Need-influenced expression only.
- Broad activity modifiers: Calm, Thinking, Working, Success, Failure, WaitingOrBlocked.
- No autonomous care.

### F5. Mouse care interactions

- Petting gesture duration/path measurement.
- Food drag/drop to pet.
- Poop cleaning.
- Explicit bed click/nap.
- Minimal-mode temporary food tray.

### F6. Browser compatibility/fallback

- `--ui auto|terminal|browser|both` before `-- codex`.
- `auto` falls back cleanly if terminal initialization is unavailable.
- Browser UI remains an optional parallel projection.

---

## File Structure

### Existing CLI/runtime files

- Modify `crates/codegotchi-cli/Cargo.toml` — terminal/PTY dependencies.
- Modify `Cargo.lock` — pinned dependency graph.
- Modify `crates/codegotchi-cli/src/lib.rs` — expose terminal module.
- Modify `crates/codegotchi-cli/src/cli.rs` — parse CodeGotchi-owned `--ui` option.
- Modify `crates/codegotchi-cli/src/launcher.rs` — choose inherited/browser path vs terminal host at the final spawn boundary.
- Modify `crates/codegotchi-cli/src/codex_profile.rs` — factor one guarded Codex invocation used by both spawn modes.

### New `crates/codegotchi-cli/src/terminal/`

- `mod.rs` — exports and shared error type.
- `pty.rs` — PTY child spawn, reader/writer, resize, wait/exit bridge.
- `screen.rs` — `vt100` parser wrapper and virtual-screen conversion.
- `layout.rs` — Full/Compact/Minimal layout and hysteresis.
- `theme.rs` — semantic tones and presets.
- `sprites.rs` — logical pixel sprites and half-block packing.
- `behavior.rs` — non-authoritative presentation state machine.
- `input.rs` — input routing, Codex encoding, drag/petting gesture state.
- `room.rs` — room composition, hitboxes, bars, food/bed/poop/pet affordances.
- `host.rs` — terminal RAII guard and main compositor event loop.

### Tests

- Create `crates/codegotchi-cli/tests/terminal_layout.rs`.
- Create `crates/codegotchi-cli/tests/terminal_input.rs`.
- Create `crates/codegotchi-cli/tests/terminal_pty.rs`.
- Create `crates/codegotchi-cli/tests/terminal_room.rs`.
- Create `crates/codegotchi-cli/tests/terminal_launcher.rs`.
- Extend existing launcher/profile tests where appropriate.

### Docs

- Modify `README.md` after implementation.
- Keep approved visual references in `docs/mockups/terminal-room/`.

---

## Interfaces to Lock

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiMode {
    Auto,
    Terminal,
    Browser,
    Both,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchRequest {
    pub ui_mode: UiMode,
    pub trailing_codex_arguments: Vec<OsString>,
}
```

```rust
#[derive(Clone, Debug)]
pub struct CodexInvocation {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub environment: Vec<(OsString, OsString)>,
}
```

`CodexInvocation` contains injected `--profile <name>` before ordinary trailing args and exact `CODEX_HOME`/`CODEGOTCHI_SESSION_FILE` values.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomLayoutMode {
    Full,
    Compact,
    Minimal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalLayout {
    pub codex: Rect,
    pub room: Rect,
    pub room_mode: RoomLayoutMode,
}

pub fn choose_layout(
    terminal: Rect,
    previous: Option<RoomLayoutMode>,
) -> TerminalLayout;
```

Initial explicit thresholds:

- Enter Full at total height `>= 40`; remain Full until `< 36`.
- Enter Compact at height `>= 26`; remain Compact until `< 22`.
- Otherwise Minimal.
- Room targets: Full `14`, Compact `7`, Minimal `3` rows.
- Reserve at least `18` rows for Codex whenever possible.
- With `< 21` total rows, keep Minimal `3` and give the rest to Codex.
- If no usable one-row Codex pane plus 3-row room is possible, `terminal` errors and `auto` falls back before child spawn.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemePreset {
    Auto,
    Mono,
    SoftGreen,
    Amber,
    Night,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticTone {
    Tone0,
    Tone1,
    Tone2,
    Tone3,
}
```

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationActivity {
    Calm,
    Thinking,
    Working,
    Success,
    Failure,
    WaitingOrBlocked,
}
```

---

## Milestone A — Prove Codex Fidelity First

The riskiest part is terminal hosting, not pixel art. Do not invest heavily in room assets until real Codex behaves correctly in a PTY/compositor.

### Task 1: Add UI-mode parsing without touching Codex args

**Files:**
- Modify `crates/codegotchi-cli/src/cli.rs`
- Modify `crates/codegotchi-cli/src/launcher.rs`

**Produces:** `UiMode`, `LaunchRequest`, `parse_launch_request`.

- [ ] **Step 1: Write failing parser tests**

```rust
#[test]
fn parses_terminal_ui_before_separator() {
    let parsed = parse_launch_request(&os(&[
        "--ui", "terminal", "--", "codex", "--model", "gpt-5.6",
    ])).unwrap();
    assert_eq!(parsed.ui_mode, UiMode::Terminal);
    assert_eq!(parsed.trailing_codex_arguments, os(&["--model", "gpt-5.6"]));
}

#[test]
fn defaults_to_auto() {
    let parsed = parse_launch_request(&os(&["--", "codex", "--search"])).unwrap();
    assert_eq!(parsed.ui_mode, UiMode::Auto);
    assert_eq!(parsed.trailing_codex_arguments, os(&["--search"]));
}

#[test]
fn codex_ui_argument_after_separator_is_not_consumed() {
    let parsed = parse_launch_request(&os(&["--", "codex", "--ui", "browser"])).unwrap();
    assert_eq!(parsed.trailing_codex_arguments, os(&["--ui", "browser"]));
}
```

Also test browser/both, duplicate `--ui`, unknown/missing value, missing separator, non-Codex agent, and existing profile conflicts.

- [ ] **Step 2: Verify RED**

```bash
cargo test -p codegotchi-cli cli launcher::tests -- --nocapture
```

- [ ] **Step 3: Implement minimal parser**

Only parse tokens before the exact `--`; preserve all tokens after `codex` verbatim.

- [ ] **Step 4: Verify GREEN and commit**

```bash
cargo test -p codegotchi-cli cli launcher::tests -- --nocapture
git add crates/codegotchi-cli/src/cli.rs crates/codegotchi-cli/src/launcher.rs
git commit -m "feat: parse terminal room ui modes"
```

---

### Task 2: Factor one guarded Codex invocation

**Files:**
- Modify `crates/codegotchi-cli/src/codex_profile.rs`
- Modify `crates/codegotchi-cli/src/launcher.rs`

**Produces:** `CodexInvocation` and `PersistentCodexProfileGuard::invocation(...)`.

- [ ] **Step 1: Add failing equivalence test**

```rust
let invocation = guard.invocation(Path::new("/usr/bin/codex"), &os(&["--model", "x"]));
assert_eq!(invocation.program, PathBuf::from("/usr/bin/codex"));
assert_eq!(
    invocation.arguments,
    os(&["--profile", profile.profile_name(), "--model", "x"])
);
assert_eq!(env_value(&invocation, "CODEX_HOME"), Some(profile.codex_home().as_os_str()));
assert_eq!(
    env_value(&invocation, "CODEGOTCHI_SESSION_FILE"),
    Some(profile.session_file().as_os_str())
);
```

- [ ] **Step 2: Verify RED**

```bash
cargo test -p codegotchi-cli codex_profile -- --nocapture
```

- [ ] **Step 3: Implement invocation + inherited adapter**

```rust
impl CodexInvocation {
    pub fn std_command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.arguments);
        command.envs(self.environment.iter().cloned());
        command
    }
}
```

Keep profile verification/guard alive through actual spawn.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p codegotchi-cli codex_profile launcher -- --nocapture
git commit -am "refactor: share guarded codex invocation"
```

---

### Task 3: Spawn fake/real Codex through a PTY and preserve exit/resize

**Files:**
- Modify `crates/codegotchi-cli/Cargo.toml`
- Modify `Cargo.lock`
- Create `crates/codegotchi-cli/src/terminal/mod.rs`
- Create `crates/codegotchi-cli/src/terminal/pty.rs`
- Create `crates/codegotchi-cli/tests/terminal_pty.rs`

**Produces:** `PtyCodexChild` with reader, writer, resize, wait.

- [ ] **Step 1: Add dependencies**

Use current compatible releases of `portable-pty`, `vt100`, `ratatui`, and `crossterm`; pin through `Cargo.lock`.

- [ ] **Step 2: Write a failing fake-Codex PTY integration test**

The fake child must:

1. assert it received expected args/env;
2. emit ANSI cursor/color output;
3. read one input line;
4. report PTY size;
5. exit with status `23`.

Assertions:

```rust
assert!(output.contains("FAKE_CODEX_READY"));
child.resize(120, 31)?;
write!(child.writer(), "hello\r")?;
assert_eq!(child.wait()?.code(), Some(23));
```

- [ ] **Step 3: Verify RED**

```bash
cargo test -p codegotchi-cli --test terminal_pty -- --nocapture
```

- [ ] **Step 4: Implement PTY spawn**

`spawn(invocation, rows, cols)` must execute the official program and environment from `CodexInvocation`; do not shell-wrap it.

- [ ] **Step 5: Verify GREEN + existing launcher tests**

```bash
cargo test -p codegotchi-cli --test terminal_pty -- --nocapture
cargo test -p codegotchi-cli launcher codex_profile -- --nocapture
git commit -am "feat: run codex in a managed pty"
```

---

### Task 4: Virtual terminal screen and Codex rendering fidelity

**Files:**
- Create `crates/codegotchi-cli/src/terminal/screen.rs`
- Test in `crates/codegotchi-cli/tests/terminal_pty.rs`

**Produces:** a `CodexScreen` wrapping `vt100::Parser`.

- [ ] **Step 1: Add failing ANSI fixture tests**

Cover cursor movement, erase, alternate screen, 16/256/truecolor, bold/underline, wide chars, resize, and unknown escape input.

Example:

```rust
let mut screen = CodexScreen::new(4, 20);
screen.process(b"hello\x1b[2;1Hworld");
assert_eq!(screen.text_at(0, 0, 5), "hello");
assert_eq!(screen.text_at(1, 0, 5), "world");
```

Unknown sequences must not panic.

- [ ] **Step 2: Verify RED, implement parser wrapper, verify GREEN**

```bash
cargo test -p codegotchi-cli terminal_screen -- --nocapture
```

- [ ] **Step 3: Manual proof gate**

Before continuing to room art, run a real installed Codex through the parser/compositor prototype and verify prompt entry, streaming text, tool output, approval UI where available, scroll behavior, and resize. Record findings in `docs/verification/terminal-room-codex-pty.md`.

**Stop the implementation if Codex fidelity is materially broken. Fix PTY/screen semantics before Milestone B.**

---

## Milestone B — Compositor and Responsive Room Foundation

### Task 5: Layout engine with hysteresis and Codex-priority sizing

**Files:**
- Create `crates/codegotchi-cli/src/terminal/layout.rs`
- Create `crates/codegotchi-cli/tests/terminal_layout.rs`

- [ ] **Step 1: Write failing table-driven tests** for heights 60, 40, 39, 36, 35, 26, 25, 22, 21, 10 and previous modes.

```rust
let layout = choose_layout(Rect::new(0, 0, 120, 45), None);
assert_eq!(layout.room_mode, RoomLayoutMode::Full);
assert_eq!(layout.room.height, 14);
assert_eq!(layout.codex.height, 31);
```

Also verify transition hysteresis and at least 18 Codex rows whenever possible.

- [ ] **Step 2: Verify RED, implement pure `choose_layout`, verify GREEN**.

- [ ] **Step 3: Commit**

```bash
git commit -am "feat: add responsive terminal room layout"
```

---

### Task 6: Terminal host RAII + frame/event loop

**Files:**
- Create `crates/codegotchi-cli/src/terminal/host.rs`
- Modify `crates/codegotchi-cli/src/terminal/mod.rs`
- Create `crates/codegotchi-cli/tests/terminal_launcher.rs`

**Produces:** `run_terminal_session(invocation, runtime) -> Result<ExitStatus, TerminalError>`.

- [ ] **Step 1: Write failing cleanup tests using an injectable terminal backend**.

Assert raw mode/alternate-screen/mouse capture are restored after:

- normal child exit;
- child spawn error;
- render error;
- input-loop error;
- termination signal.

- [ ] **Step 2: Implement `TerminalGuard`**

```rust
struct TerminalGuard<B: TerminalBackend> {
    backend: B,
    restored: bool,
}

impl<B: TerminalBackend> Drop for TerminalGuard<B> {
    fn drop(&mut self) {
        if !self.restored {
            let _ = self.backend.restore();
        }
    }
}
```

- [ ] **Step 3: Implement event loop**

The loop selects among crossterm input, PTY output, authoritative snapshot updates, animation tick, child exit, and resize/signal handling.

- [ ] **Step 4: Verify cleanup tests and commit**.

---

### Task 7: Semantic theme + half-block sprite renderer

**Files:**
- Create `crates/codegotchi-cli/src/terminal/theme.rs`
- Create `crates/codegotchi-cli/src/terminal/sprites.rs`
- Create `crates/codegotchi-cli/tests/terminal_room.rs`

- [ ] **Step 1: Write failing 4-tone packing tests**

Represent sprite pixels as semantic tones and assert two logical vertical pixels pack into one terminal cell using `▀`, `▄`, `█`, or blank plus foreground/background style.

- [ ] **Step 2: Write Auto-theme snapshot tests** for default foreground/background only, then explicit preset tests.

- [ ] **Step 3: Implement sprite assets** for pet idle/blink/walk A/B/sit/curious/happy/upset/eat/petted/sleep and Full/Compact furniture silhouettes.

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p codegotchi-cli --test terminal_room -- --nocapture
git commit -am "feat: add terminal pixel sprite and theme system"
```

---

## Milestone C — Authoritative Room and Living Behavior

### Task 8: Room composition from authoritative snapshots

**Files:**
- Create `crates/codegotchi-cli/src/terminal/room.rs`
- Extend `crates/codegotchi-cli/tests/terminal_room.rs`

**Produces:** room render model + stable hitboxes keyed by authoritative IDs.

- [ ] **Step 1: Write failing room-model tests**

Given a seeded `SimulationSnapshot`, assert:

- status bars map Hunger/Energy/Happiness/Cleanliness correctly;
- pending poop IDs create poop hitboxes;
- food counts create only stocked drag sources;
- bed hitbox exists in Full/Compact/Minimal;
- demand affordances reflect authoritative pending demand counts;
- Minimal exposes all recovery actions.

- [ ] **Step 2: Implement Full composition** following approved mockup: window/desk/shelf/wardrobe/bed/plants/open floor/status bar.

- [ ] **Step 3: Implement Compact and Minimal compositions** as separate layouts, not image scaling.

- [ ] **Step 4: Verify deterministic room snapshots and commit**.

---

### Task 9: Autonomous presentation behavior without autonomous care

**Files:**
- Create `crates/codegotchi-cli/src/terminal/behavior.rs`
- Extend `crates/codegotchi-cli/tests/terminal_room.rs`

**Produces:** `PresentationState` and deterministic seeded transitions.

- [ ] **Step 1: Write failing behavior tests**

For a fixed seed/time:

- calm state eventually wanders/sits/inspects furniture;
- tired state can yawn/linger near bed but never emit a Nap action;
- hungry state can linger near food but never emit Feed;
- lonely state can seek attention but never resolve Affection;
- Thinking/Working modifiers expire back into calm behaviors while the external activity remains active;
- Success/Failure are short reactions.

- [ ] **Step 2: Implement pure presentation transitions** with no reference to runtime mutation methods.

Use an enum such as:

```rust
enum IdleIntent {
    Wander(Point),
    Sit,
    Inspect(RoomObject),
    LookOutWindow,
    Yawn,
    WatchCodex,
    Celebrate,
    Worry,
}
```

No `Feed`, `Clean`, `Nap`, or `Pet` variants are permitted.

- [ ] **Step 3: Verify and commit**.

---

## Milestone D — Mouse Interaction and Launcher Integration

### Task 10: Input routing and care gestures

**Files:**
- Create `crates/codegotchi-cli/src/terminal/input.rs`
- Create `crates/codegotchi-cli/tests/terminal_input.rs`
- Modify `crates/codegotchi-cli/src/terminal/host.rs`

- [ ] **Step 1: Write failing routing tests**

- key events always route to Codex;
- room mouse events never reach Codex;
- Codex-pane mouse events encode to PTY when enabled;
- coordinates transform correctly after resize.

- [ ] **Step 2: Write failing petting tests**

```rust
let mut gesture = PetGesture::begin(Point::new(10, 5), now);
gesture.move_to(Point::new(14, 5));
gesture.move_to(Point::new(14, 9));
let request = gesture.finish(now + Duration::milliseconds(1600));
assert!(request.interaction_ms >= 1500);
assert!(request.pointer_distance > 0.0);
```

Use a documented scale from terminal-cell travel to the backend `pointer_distance` metric. Calibrate so an intentional drag across approximately 8-12 terminal cells can exceed the existing 120 threshold; preserve the backend threshold itself.

- [ ] **Step 3: Write failing food drag/drop tests**

Stocked item -> drag ghost -> drop on pet -> exactly one `feed(action_id, food_id)` call. Drop elsewhere -> no mutation call.

- [ ] **Step 4: Write failing poop/bed tests**

Click authoritative poop -> one `clean` call. Click bed -> one `nap` call. Presentation only transitions to sleep after successful receipt/snapshot.

- [ ] **Step 5: Implement input state machine and verify**.

---

### Task 11: Integrate terminal/browser/both/auto into launcher

**Files:**
- Modify `crates/codegotchi-cli/src/launcher.rs`
- Modify `crates/codegotchi-cli/src/cli.rs`
- Modify `crates/codegotchi-cli/src/lib.rs`
- Extend `crates/codegotchi-cli/tests/terminal_launcher.rs`

- [ ] **Step 1: Write failing launcher-mode tests**

Use fake Codex/browser/terminal backends to assert:

- `browser` exactly preserves inherited stdio + browser launch;
- `terminal` does not launch browser and requires terminal init;
- `both` starts terminal host + browser;
- `auto` uses terminal when interactive/init succeeds;
- `auto` falls back to browser before child spawn on terminal init failure;
- no mode spawns Codex twice;
- metadata/profile cleanup remains exactly once.

- [ ] **Step 2: Refactor only the final spawn boundary**

Everything before profile verification remains shared. After `CodexInvocation` is built:

```rust
match request.ui_mode {
    UiMode::Browser => spawn_inherited(...),
    UiMode::Terminal => run_terminal_session(...),
    UiMode::Both => run_terminal_and_browser(...),
    UiMode::Auto => run_terminal_or_fallback(...),
}
```

- [ ] **Step 3: Verify launcher/runtime/profile regression suite**.

- [ ] **Step 4: Commit**

```bash
git commit -am "feat: integrate codegotchi terminal room launcher"
```

---

## Milestone E — Documentation, Compatibility, Acceptance

### Task 12: README, CI, and end-to-end verification

**Files:**
- Modify `README.md`
- Modify `.github/workflows/ci.yml` only if PTY tests need a supported Linux package/setup.
- Create `docs/verification/terminal-room.md`.

- [ ] **Step 1: Document user-facing behavior**

Include:

```text
codegotchi run -- codex
codegotchi run --ui terminal -- codex
codegotchi run --ui browser -- codex
codegotchi run --ui both -- codex
```

Document Full/Compact/Minimal behavior, mouse interactions, theme presets, browser fallback, and supported platforms.

- [ ] **Step 2: Run all Rust gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 3: Run all web gates**

```bash
corepack pnpm lint
corepack pnpm test
corepack pnpm format:check
corepack pnpm build
node web/scripts/embed-web.mjs
corepack pnpm playwright:test
```

Expected: PASS.

- [ ] **Step 4: Real Codex acceptance on WSL/Linux**

Record evidence for:

1. official Codex starts inside upper pane;
2. prompt entry/editing/paste work;
3. normal tool activity renders correctly;
4. approval/review UI works where available;
5. terminal resize transitions Full -> Compact -> Minimal -> Full;
6. Codex PTY receives corresponding sizes;
7. pet can be petted;
8. food can be dragged to pet;
9. poop can be cleaned;
10. bed starts authoritative nap;
11. long Thinking/Working still includes calm pet behaviors;
12. Ctrl-C/normal exit restore the terminal fully;
13. browser mode still works;
14. installing/running a current newer Codex build does not require CodeGotchi UI-code changes if its terminal protocol is compatible.

- [ ] **Step 5: Commit verification/docs**

```bash
git add README.md docs/verification/terminal-room.md .github/workflows/ci.yml
git commit -m "docs: verify terminal room integration"
```

---

## Test Matrix

| Area | Unit | Integration | Real acceptance |
|---|---|---|---|
| UI option parsing | yes | launcher | command examples |
| Profile/args/env fidelity | yes | PTY fake child | real Codex |
| ANSI/VT rendering | fixtures | fake Codex | real Codex |
| Resize/hysteresis | pure tests | PTY resize | interactive resize |
| Theme/sprites | snapshot tests | room render | dark/light terminals |
| Autonomous behavior | seeded pure tests | host loop | observation |
| Petting | gesture tests | runtime method | mouse |
| Feeding | drag tests | runtime inventory | mouse |
| Poop | hitbox tests | runtime poop IDs | mouse |
| Bed | hitbox tests | runtime nap | mouse |
| Terminal restoration | fake backend | signal/error tests | Ctrl-C/exit |
| Browser fallback | parser/launcher | server/browser tests | browser launch |
| Codex updates | no text parsing tests | ANSI compatibility | newer Codex smoke |

---

## Risks and Mitigations

### Risk: Codex uses terminal behavior not handled by the emulator

**Mitigation:** use a mature VT parser; prove real Codex fidelity before room work; keep `--ui browser` and `auto` fallback.

### Risk: Mouse capture interferes with Codex

**Mitigation:** route strictly by pane rectangle; forward upper-pane mouse protocol events; disable room ownership outside its rectangle; test scroll/click behavior.

### Risk: terminal corruption on panic/error/signal

**Mitigation:** RAII `TerminalGuard`, injectable backend tests for every exit path, real Ctrl-C acceptance.

### Risk: room steals too much vertical space

**Mitigation:** explicit Codex-priority thresholds and automatic Full/Compact/Minimal transitions.

### Risk: terminal themes make art unreadable

**Mitigation:** Auto uses terminal defaults and dithering; fixed palettes are optional rendering presets.

### Risk: presentation accidentally becomes simulation authority

**Mitigation:** presentation state contains no care mutation variants and is not persisted; all care goes through existing runtime methods.

### Risk: implementation duplicates browser logic

**Mitigation:** share domain/runtime authority, not view code. Terminal room and React remain independent projections of the same snapshot/care contract.

---

## Definition of Done

The implementation is complete only when all of the following are true:

- [ ] Official Codex TUI is the actual child UI in the upper PTY pane; no semantic Codex frontend was recreated.
- [ ] Profile, args, env, hook trust, exit status, resize, and signals preserve current launcher behavior.
- [ ] Full, Compact, Minimal layouts work automatically with hysteresis and Codex priority.
- [ ] Full room matches the approved side-view bedroom direction.
- [ ] Four-tone Auto rendering works on dark and light terminal themes.
- [ ] Initial theme presets are implemented.
- [ ] Pet has idle/walk/sit/blink/curious/happy/upset/eat/petted/sleep presentation frames.
- [ ] Thinking/Working modify behavior but do not eliminate calm idling.
- [ ] Autonomous behavior never feeds, pets, cleans, naps, or otherwise changes authoritative needs.
- [ ] Petting, feeding, poop cleaning, and bed interactions use the existing runtime care methods and authoritative snapshots.
- [ ] Minimal mode retains all care needed to recover from strict-mode blocking.
- [ ] `--ui auto|terminal|browser|both` behaves as specified.
- [ ] Existing browser UI remains functional.
- [ ] Rust fmt, Clippy, workspace tests, web tests/lint/format/build, and Playwright are green.
- [ ] Real Codex WSL/Linux acceptance is recorded and the terminal is clean after exit/error/Ctrl-C.
