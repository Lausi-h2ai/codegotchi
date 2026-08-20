# Terminal Room Release Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining release blockers on the pushed terminal-room implementation: make the claimed macOS path real, harden captured mouse semantics, eliminate the remaining browser-motion flake, finish spec-compliant adaptive Auto rendering, bring Full/Compact/Minimal visuals up to the approved reference direction, complete the real-Codex acceptance matrix, and produce fresh green release evidence.

**Architecture:** Preserve the existing product boundaries. The official Codex executable remains the PTY-hosted upper TUI, `AuthoritativeRuntime` remains the only authority for care state, terminal/browser motion remains presentation-only, and visual fidelity work stays inside the renderer/sprite/layout layer. Fix failures at their owning boundary rather than weakening tests or specifications. The final release claim requires both automated gates and image/live-terminal evidence.

**Tech Stack:** Rust 1.97.x workspace, `portable-pty 0.9.0`, `vt100 0.16.2`, `ratatui 0.30.2`, `crossterm 0.29.0`, React 19, TypeScript 5.9, Playwright 1.62, pnpm 11.20.0, GitHub Actions, xterm/Xvfb for deterministic capture, official Codex CLI.

**Spec:** `docs/superpowers/specs/2026-08-13-terminal-room-design.md`, with normative overrides from `docs/superpowers/specs/2026-08-13-terminal-room-agent-hardening.md`.

## Global Constraints

- The official Codex TUI remains the actual upper pane. Do not recreate, fork, patch, or text-scrape Codex UI semantics.
- Initial supported Unix envelope remains Linux, WSL, and macOS unless the approved spec is explicitly amended.
- The browser remains a selectable fallback/second projection.
- Authoritative needs, inventory, demands, poop, nap state, persistence, replay safety, and enforcement remain owned by the Rust runtime/domain.
- Terminal petting remains exactly `1_500 ms` minimum duration and `120.0` backend distance, with Euclidean cell path multiplied by `POINTER_DISTANCE_PER_CELL = 16.0`.
- A generic `PetBehavior::Sleeping` is never authoritative bed sleep. Only a future `napping_until` relative to `last_updated_at` selects the recovery-bed pose.
- `CareCommand::PetStroke` must continue carrying cumulative duration/distance evidence and remain domain-validated. An interrupted qualified stroke may retain already validated happiness but must never resolve affection without final `Pet`.
- The five initial terminal presets remain `auto`, `mono`, `soft-green`, `amber`, and `night`; theme selection is presentation-only.
- Auto must work against arbitrary terminal foreground/background pairs. Intermediate semantic tones must use foreground/background dithering rather than assuming named gray colors are readable.
- Full, Compact, and Minimal are purpose-built compositions. Decoration disappears before core care functionality.
- Required core care remains possible in Minimal: pet, stocked food drag-to-pet, poop cleaning, and bed interaction.
- Material renderer/sprite/theme/layout changes require screenshot -> image inspection -> reference comparison -> correction -> recapture.
- Canonical art references live in `docs/mockups/terminal-room/`. The six images in `docs/mockups/current/` are the audited **pre-fix current-state baseline**, not acceptance references.
- Final visual evidence must include production-renderer Full light/default and dark configurations, Compact, Minimal, authoritative bed sleep, generic floor doze, and a live real-Codex terminal composition.
- Do not claim completion from ANSI dumps, snapshots, or fixture screenshots alone.
- No subagent may push, merge, publish, or mutate PR metadata. Those are controller/user-authorized external actions only.
- The final integrated HEAD must pass formatting, Clippy, Rust workspace tests, web tests/lint/format/build/embed, production Playwright, hosted Ubuntu CI, and hosted macOS CI.

---

## Audit Baseline — 2026-08-20

Audited PR: `#2`, branch `docs/terminal-room-design`.

Audited pushed head before this plan: `ff16622441dd1e88460ee09b170ed08621278b6d`.

The current visual-baseline commit is one evidence-only commit after source SHA `20d4ad31855e2b5ff846d4d726add4b73e68c814`. `docs/mockups/current/README.md` records exact PNG hashes, environment, fixture variables, and direct image-review findings for the six current screenshots.

### Findings that are CLOSED and must not be reimplemented

- The previously non-hermetic terminal compositor resize regression is fixed; hosted Ubuntu Rust checks reach and pass the full workspace suite.
- Browser sustained petting and fixture isolation are fixed; the current hosted production Playwright job passes.
- Continuous terminal petting now carries `duration_ms` and `distance` through `PetStroke`; the domain independently validates the same `1500 / 120` contract as final `Pet`.
- The documented ruling for interrupted qualified strokes is implemented: earned validated happiness may remain, but affection is resolved only by final `Pet`.
- All five named terminal presets exist and parse from stable CLI spellings.
- Full-room day/night ambience exists.
- Generic browser/terminal doze is separated from authoritative future-deadline bed nap.
- Minimal picks the deterministic first stocked food across Kibble -> Treat -> Fruit -> EnergyDrink and has an explicit no-stock state.
- Focus/resize cancellation and outside-room pointer capture have regression coverage.
- Windows `Zone.Identifier` metadata was removed and ignored.
- Root README was updated for the terminal launcher flow.
- Current hosted Ubuntu Rust and web jobs are green.

### P0 — Current PR CI is still red on macOS, and the failure exposes missing product behavior

The current hosted macOS job fails in Clippy because `launch_native_browser(url)` never uses `url` outside the Linux-only branch. This is not merely a lint spelling issue: the native browser helper currently implements `xdg-open`/`gio` plus WSL `cmd.exe`, but no macOS `open` path. Therefore Browser/Both and Auto fallback cannot perform the advertised native browser launch on a claimed supported platform unless the user manually supplies `CODEGOTCHI_BROWSER`.

The older PTY verification record also explicitly says macOS descendant-cleanup acceptance was deferred. Once Clippy is fixed, hosted macOS tests must actually run, and a pseudo-terminal smoke must establish what lifecycle guarantee is supportable on macOS rather than silently preserving the old deferment.

### P0 — Visual acceptance is still a genuine implementation failure, not documentation polish

`docs/mockups/current/` and `docs/verification/terminal-room/README.md` both deliberately refuse a visual PASS.

The six current-state captures show:

- Full: recognizable room structure but sparse terminal line art; the pet is too small and visually hollow relative to the approved large round mascot; furniture/surfaces do not read as the layered bedroom reference; status/care presentation is text-heavy.
- Full dark: readable, but empty bars are low-contrast and the room lacks the stronger visual hierarchy of the dark/Compact reference.
- Compact: core controls survive but the composition is mostly text rows and line glyphs rather than a pet-centered vignette with a compact status/care panel.
- Minimal: care is functional, but the three rows are effectively a textual strip instead of a purpose-built pixel-art pet + need/care composition.
- Bed sleep vs floor doze is semantically clear, but both pet poses remain abstract blocks versus the supplied curled/sleeping component references.
- The deterministic room captures leave the Codex pane blank. They are useful renderer evidence but cannot satisfy the hardening requirement for final live-terminal visual comparison with the real Codex TUI present.

The source confirms the mismatch: the Full sprite is only `10 x 10` logical pixels (`5` terminal rows) and still carries `VISUAL_FIDELITY_UNVERIFIED`; Full/Compact renderers use prominent literal labels such as `WINDOW`, `SHELF`, `DESK`, `PET`, `FOOD`, `POOP`, and Compact/Minimal are dominated by status/control text.

### P1 — Captured terminal mouse semantics still accept invalid button sequences

`RoomInputSession::process` currently:

- handles every `Down(Left)` as a potential new gesture/drag even when a left-button capture is already active;
- handles `Drag(_)` for every mouse button and advances an active pet path / food drag regardless of which button generated the drag.

A second left press can therefore restart or mutate an existing capture, and right/middle drag events can advance a left-button care gesture. The state machine must cancel-and-consume invalid termination sequences, not reinterpret them as a new care action and not leak them into Codex.

### P1 — The browser idle-travel E2E is green once, but remains structurally flaky

The production E2E `keeps idle travel in safe regions and eventually performs a roll` still waits up to `35_000 ms` of real wall time for choreography whose production roll interval is randomly selected in the `15_000..30_000 ms` range. The previous controller run observed this exact scenario as flaky; a single green hosted run does not remove the structural timing dependency.

The motion/choreography implementation already has injectable timer, clock, random, and duration ports. Do not change production choreography timing to satisfy the E2E. Make the browser test control time deterministically.

### P1 — Auto theme still violates the approved adaptive-theme contract

`TerminalThemePreset::Auto` currently resolves `Tone1` to `DarkGray` and `Tone2` to `Gray`. The approved design explicitly requires intermediate tones to use ordered default-foreground/default-background dithering when exact terminal colors cannot be inferred. Named ANSI gray entries are not the arbitrary host foreground/background and can produce poor contrast under custom themes.

There is also an evidence gap: the current `full-light.png` uses `soft-green`, while `full-dark.png` uses `night`. Neither proves Auto against a light and dark default terminal.

### P1 — Real-Codex acceptance remains incomplete

The latest bounded real-Codex attempt reached the official TUI but failed before useful interaction because the private Xvfb host had no active window manager and `xdotool` focus/activation ended in `BadWindow`. The release record therefore still cannot claim:

- prompt editing plus a model response;
- bracketed paste in a real Codex session;
- focus reporting where observable;
- Codex mouse scrolling/clicking while mouse reporting is enabled;
- tool activity and approval/review where available;
- resize Full -> Compact -> Minimal -> Full;
- terminal pet/feed/clean/nap while Codex remains usable;
- clean exit/restoration after the complete live interaction sequence.

This is a harness/evidence blocker unless the live run uncovers a product bug.

### P2 — Release evidence and PR metadata are stale

`docs/verification/terminal-room.md` still says hosted Ubuntu/macOS CI was unavailable and records an older local workspace failure even though the pushed head now has hosted results: Ubuntu Rust and web pass, macOS fails before tests in Clippy. It must be rewritten from fresh final-HEAD evidence after the implementation tasks.

PR #2 still has a design-only title/body. Do not mutate it from a Luna worker. Prepare corrected metadata only after all release gates pass; the controller/user decides whether to apply it.

---

## Luna Execution Topology

Use one fresh Luna implementer and one fresh Luna reviewer for each task. Store each task brief/report/review in this plan's own SDD workspace. Do not reuse the previous completion plan's `.superpowers/sdd` directory.

Tasks 1-4 are logically independent and may be reasoned about independently, but if they execute in one worktree they should still commit sequentially. Task 5 consumes Task 4's final palette behavior. Task 6 consumes the Full-art vocabulary established by Task 5. Task 7 should run only after Tasks 1, 2, 5, and 6 are integrated. Task 8 is the final controller/reviewer gate after every implementation task.

For every task, the reviewer must check both spec compliance and regression risk. A passing focused test is not enough if the implementation weakens a Global Constraint.

---

### Task 1: Make macOS a Real Supported Launcher/PTY Path

**Files:**
- Modify: `crates/codegotchi-cli/src/launcher.rs`
- Modify if macOS PTY smoke exposes a real lifecycle defect: `crates/codegotchi-cli/src/terminal/pty.rs`
- Modify if macOS PTY smoke exposes a session defect: `crates/codegotchi-cli/src/terminal/session.rs`
- Modify: `.github/workflows/ci.yml`
- Test: existing launcher unit tests in `crates/codegotchi-cli/src/launcher.rs`
- Test: `crates/codegotchi-cli/tests/terminal_pty.rs`
- Test: `crates/codegotchi-cli/tests/terminal_session.rs`

**Interfaces:**
- Consumes: `launch_browser(url)`, `spawn_browser_command`, current Unix terminal session and existing ignored outer-PTY integration tests.
- Produces: native macOS browser launch via `/usr/bin/open`; green macOS fmt/Clippy/workspace tests; a recorded macOS pseudo-terminal smoke for the production-shared terminal session path.

- [ ] **Step 1: Add a failing/testable macOS command-selection seam instead of silencing Clippy**

Refactor native browser selection so command construction is pure and spawning remains in `spawn_browser_command`. On macOS the selected command must be:

```text
program: /usr/bin/open
arguments: [<exact CodeGotchi UI URL>]
```

Add a `#[cfg(target_os = "macos")]` unit test that asserts the exact program and exact single URL argument. Keep current Linux `xdg-open`, `gio`, and WSL fallback semantics unchanged.

Do **not** fix the hosted warning by renaming `url` to `_url`; that would leave Browser/Both/Auto fallback functionally incomplete on macOS.

- [ ] **Step 2: Implement the minimal native macOS launch path**

Use the existing `spawn_browser_command` process policy (`stdin/stdout/stderr` null, asynchronous reap) and `/usr/bin/open`. Do not shell-wrap or concatenate the URL into a command string.

- [ ] **Step 3: Run local cross-platform-safe Rust gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Expected on Linux/WSL: PASS with Linux browser behavior unchanged.

- [ ] **Step 4: Extend the hosted macOS job with one production-shared outer-PTY smoke**

After the ordinary macOS workspace test step, run an existing ignored composed-session integration test under BSD `script` so the test owns a pseudo-terminal instead of being skipped for lack of TTY. Use the macOS form accepted by the runner, with this test target and exact assertion scope:

```bash
script -q /dev/null cargo test -p codegotchi-cli --test terminal_session \
  composed_session_adapter_delivers_exact_invocation_modes_input_and_status \
  -- --ignored --nocapture
```

If BSD `script` requires argument separation on the hosted image, adapt only the wrapper syntax; do not replace the production-shared test with a fake parallel loop.

- [ ] **Step 5: Treat any macOS PTY failure as evidence, not as permission to disable the platform**

If the composed-session smoke fails because the current no-`waitid(WNOWAIT)` fallback cannot provide bounded cleanup, fix the smallest owning PTY/session behavior and add a macOS-specific regression. The acceptable outcome is bounded, non-corrupting cleanup without signaling a recycled process-group identity. Do not assert descendant cleanup stronger than the implementation proves.

- [ ] **Step 6: Controller hosted-CI gate**

After this task is committed locally, the worker stops at the commit. A controller with explicit push authorization later pushes and verifies that `Rust checks (macOS)` reaches and passes Clippy, workspace tests, and the new PTY smoke. Record the hosted run/job IDs for Task 8.

- [ ] **Step 7: Commit**

```bash
git add -- crates/codegotchi-cli/src/launcher.rs crates/codegotchi-cli/src/terminal/pty.rs crates/codegotchi-cli/src/terminal/session.rs .github/workflows/ci.yml
git commit -m "fix: complete macos launcher and terminal smoke"
```

Stage only files actually modified.

---

### Task 2: Make Room Pointer Capture a Strict Left-Button State Machine

**Files:**
- Modify: `crates/codegotchi-cli/src/terminal/input.rs`
- Modify: `crates/codegotchi-cli/tests/terminal_input.rs`
- Modify only if pane-routing coverage is missing: `crates/codegotchi-cli/src/terminal/session.rs`

**Interfaces:**
- Consumes: `RoomInputSession::{has_active_capture,cancel,process}`, `MouseEventKind`, current session rule that active room capture owns pointer events outside the room.
- Produces: one unambiguous left-button capture lifecycle where invalid button sequences cancel and are consumed without care and without Codex leakage.

- [ ] **Step 1: Write the missing RED matrix before touching production code**

Add focused tests for both capture kinds (pet gesture and food drag):

```text
active pet + second Down(Left)       -> capture cancelled, no request, no replacement capture
active food + second Down(Left)      -> capture cancelled, no request, no replacement capture
active pet + Drag(Right)             -> capture cancelled, no request
active pet + Drag(Middle)            -> capture cancelled, no request
active food + Drag(Right)            -> capture cancelled, no request
active food + Drag(Middle)           -> capture cancelled, no request
active pet/food + Drag(Left)          -> existing capture remains active
no capture + Drag(any button)         -> no request and no capture
active capture + Up(non-left)         -> capture cancelled, no request
```

For the second-left cases, place the second press on both another valid care target and empty room space across the two tests so a restart cannot pass accidentally.

- [ ] **Step 2: Implement capture-first event classification**

When `has_active_capture()` is true:

- any `Down(_)` is an invalid termination: call `cancel()`, return no care request, and do not reinterpret that same event as a new capture;
- only `Drag(Left)` may advance the captured pet/food interaction;
- `Drag(Right)` and `Drag(Middle)` cancel and return no request;
- `Up(Left)` performs the existing valid completion/cancellation logic;
- `Up(non-left)` cancels;
- movement/scroll events remain presentation-neutral and do not mutate the care gesture.

When no capture exists, only `Down(Left)` on the visible pet or a stocked food source may create one. A bare drag never creates capture.

- [ ] **Step 3: Prove invalid captured events cannot leak to Codex**

If current `TerminalSessionCore` tests do not already exercise the invalid button sequences while the pointer is outside the room, add two session-level regressions: one second `Down(Left)` and one wrong-button `Drag` during active room capture. Assert no Codex PTY bytes are emitted for the terminating event.

- [ ] **Step 4: Run focused tests**

```bash
cargo test -p codegotchi-cli --test terminal_input -- --nocapture
cargo test -p codegotchi-cli terminal::session::tests::captured_ -- --nocapture
```

Expected: all existing capture/care tests plus the new invalid-sequence matrix PASS.

- [ ] **Step 5: Run Rust regression gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add -- crates/codegotchi-cli/src/terminal/input.rs crates/codegotchi-cli/tests/terminal_input.rs crates/codegotchi-cli/src/terminal/session.rs
git commit -m "fix: enforce terminal room pointer capture semantics"
```

Stage only files actually modified.

---

### Task 3: Remove Wall-Clock Randomness from the Browser Idle-Travel E2E

**Files:**
- Modify: `web/e2e/mvp.spec.ts`
- Do not modify production timing unless a new product bug is independently proven.

**Interfaces:**
- Consumes: Playwright `page.clock`, existing `data-motion-*` diagnostics, production choreography interval constants `15_000..30_000 ms`, existing production timers.
- Produces: a deterministic production-embedded E2E that proves safe travel and observes at least one roll without spending 35 seconds waiting on real time.

- [ ] **Step 1: Preserve the production contract and reproduce the current test shape**

Keep the assertions that free-time movement stays inside known safe room regions, remains grounded, and eventually produces `data-motion-action="roll"`. Remove the assumption that wall-clock polling itself is part of the product behavior.

- [ ] **Step 2: Install Playwright's clock before application startup**

In this one test, install the page clock before `page.goto(launchUrl)` so application `setTimeout`/`Date.now` are under test control. Do not override the production constants and do not add a production-only `if (E2E)` branch.

- [ ] **Step 3: Advance simulated time in bounded increments and sample the existing diagnostics**

Advance the page clock in `250 ms` increments through at least `31_000 ms` of simulated time. After each increment, sample:

```text
data-motion-mode
data-motion-action
data-motion-region
rendered pet/room geometry used by the existing grounded-position assertion
```

The 31-second horizon covers the maximum 30-second roll schedule; 250 ms sampling is comfortably shorter than the 800 ms roll duration, so the transient action cannot be skipped by one large time jump.

Record `observedRoll` and all visited safe regions exactly as the current test does. The test passes only when a roll is observed while the authoritative activity still maps to idle/free-time and every sampled region/position remains valid.

- [ ] **Step 4: Remove the special 40-second wall-clock timeout and 35-second poll**

The deterministic test must complete under the normal Playwright timeout and should take only normal page/bootstrap time in wall-clock terms.

- [ ] **Step 5: Stress the exact E2E ten times**

From `web/` after rebuilding/embedding production bytes:

```bash
corepack pnpm build
node scripts/embed-web.mjs
for i in $(seq 1 10); do
  corepack pnpm exec playwright test --config playwright.config.ts \
    --grep "keeps idle travel in safe regions and eventually performs a roll"
done
```

Expected: 10/10 PASS with no retry required.

Then run:

```bash
cd ..
corepack pnpm playwright:test:production
```

Expected: the complete production suite PASS with zero flaky classification.

- [ ] **Step 6: Commit**

```bash
git add -- web/e2e/mvp.spec.ts crates/codegotchi-cli/web-dist
git commit -m "test: make browser idle choreography deterministic"
```

Include `web-dist` only if the repository's production embedding step changed tracked bytes.

---

### Task 4: Implement the Specified Adaptive Auto Dither

**Files:**
- Modify: `crates/codegotchi-cli/src/terminal/theme.rs`
- Modify: `crates/codegotchi-cli/src/terminal/sprites.rs`
- Modify: `crates/codegotchi-cli/src/terminal/room.rs`
- Modify: `crates/codegotchi-cli/tests/terminal_room.rs`
- Modify if fixture arguments/evidence need expansion: `crates/codegotchi-cli/examples/terminal_room_fixture.rs`

**Interfaces:**
- Consumes: `SemanticTone`, `TerminalThemePreset`, `ResolvedPalette`, logical-pixel sprite packing, Full/Compact/Minimal room rendering.
- Produces: an Auto palette whose intermediate pixel tones are represented by deterministic ordered foreground/background coverage using terminal defaults, while named truecolor presets remain unchanged.

- [ ] **Step 1: Add RED tests that capture the actual Auto contract**

Add tests proving all of the following:

```text
Auto Tone0 uses terminal default background.
Auto Tone3 uses terminal default foreground.
Auto intermediate logical pixels do not depend on Color::DarkGray or Color::Gray.
A fixed 4x4 ordered pattern gives Tone1 less foreground coverage than Tone2.
The same logical sprite produces deterministic Auto cells for repeated renders.
mono / soft-green / amber / night concrete RGB mappings remain unchanged.
```

Use an explicit 4x4 Bayer-style threshold matrix so the expected occupancy is testable rather than random.

- [ ] **Step 2: Separate fixed-palette color mapping from adaptive logical-tone sampling**

Extend `ResolvedPalette` (or an equivalent small internal type) so the renderer can distinguish `AdaptiveAuto` from fixed named presets. Add a pure coordinate-aware operation with semantics equivalent to:

```rust
sample_logical_tone(tone, logical_x, logical_y) -> SemanticTone
```

For fixed presets it returns the authored tone unchanged. For Auto:

- `Tone0` always samples to default background;
- `Tone3` always samples to default foreground;
- `Tone1` samples a low foreground density;
- `Tone2` samples a higher foreground density;
- the pattern is ordered/deterministic, never random.

After sampling, Auto pixel art must be expressible with terminal-default foreground/background only. Do not use named gray palette entries as the intermediate-tone fallback.

- [ ] **Step 3: Apply sampling before half-block packing**

In `draw_sprite_with_palette`, pass stable logical coordinates through the Auto sampler before `put_packed`. Preserve exact half-block packing and fixed-preset behavior. Avoid checkerboard seams by anchoring the ordered pattern to room/logical coordinates rather than restarting it inconsistently for adjacent layers.

For non-pixel labels/status text, use default foreground/background in Auto; visual weight should come from glyph density/segmented bars, not hard-coded gray.

- [ ] **Step 4: Run focused tests**

```bash
cargo test -p codegotchi-cli --test terminal_room -- --nocapture
cargo test -p codegotchi-cli terminal::theme -- --nocapture
```

Expected: Auto contract PASS and all existing preset/layout assertions remain green.

- [ ] **Step 5: Capture real Auto evidence on both host polarities**

Using the production `terminal_room_fixture`, capture Full at `120x45` with `CG_FIXTURE_THEME=auto` twice:

```text
light host: xterm default/light background + dark foreground
dark host:  xterm -bg black -fg white
```

Inspect both images. Tone hierarchy, pet silhouette, furniture, status bars, and care targets must remain visible in both. A green named preset is not a substitute for this check.

Store temporary task evidence in the SDD workspace. Task 8 chooses final committed evidence after the later art tasks.

- [ ] **Step 6: Commit**

```bash
git add -- crates/codegotchi-cli/src/terminal/theme.rs crates/codegotchi-cli/src/terminal/sprites.rs crates/codegotchi-cli/src/terminal/room.rs crates/codegotchi-cli/tests/terminal_room.rs crates/codegotchi-cli/examples/terminal_room_fixture.rs
git commit -m "feat: add adaptive auto terminal dithering"
```

Stage only files actually modified.

---

### Task 5: Rebuild Full Room and Mascot to Match the Approved Visual Direction

**Files:**
- Modify: `crates/codegotchi-cli/src/terminal/sprites.rs`
- Modify: `crates/codegotchi-cli/src/terminal/room.rs`
- Modify: `crates/codegotchi-cli/tests/terminal_room.rs`
- Modify if deterministic states need additional fixture controls: `crates/codegotchi-cli/examples/terminal_room_fixture.rs`

**Interfaces:**
- Consumes: Task 4 palette/dither behavior; shared `RoomGeometry`; canonical references in `docs/mockups/terminal-room/`; baseline `docs/mockups/current/full-light.png`, `full-dark.png`, `full-bed-sleep.png`, `full-floor-doze.png`.
- Produces: a pet-centered, layered 14-row Full composition with recognizable furniture/care objects and expressive pose family while preserving authoritative geometry and every current care action.

- [ ] **Step 1: Read the visual assets before editing**

The implementer must inspect all nine canonical reference images and the four Full current-baseline images. Record the five largest Full mismatches in the task report before code changes.

Treat these reference properties as load-bearing:

```text
large almost-absurdly-round mascot
recognizable eyes + ears/nubs + mouth + paws/feet + tail/body silhouette
window/desk on the left; shelf/wardrobe through the middle; bed on the right
visible laptop + lamp/desk identity
plants/flowers and a simple rug/floor layer
food/poop as physical room objects rather than primarily words
segmented, readable HUNGER / ENERGY / HAPPY / CLEAN status treatment
bed sleep visibly integrated with bedding; floor doze visibly separate
```

Tiny texture/noise from the raster sheets is not load-bearing.

- [ ] **Step 2: Enlarge and redraw the Full pose family as one coherent sprite system**

Move the Full mascot from the current `10 x 10` logical-pixel footprint toward the top of the approved `10-14 logical pixels high` range: target `12-14` logical rows (`6-7` terminal rows) and enough width for the round silhouette to read clearly.

Update **every** Full pose, not only Idle:

```text
Idle, Blink, WalkA, WalkB, Sit, Doze, Sleep, Yawn,
Curious, Happy, Upset, Eating, Petted
```

Each pose must preserve the same recognizable body scale and face language. Walk frames must visibly alternate; Blink must alter eyes; Happy/Upset/Petted/Eating must read without relying on a nearby text label; Doze must be curled on the floor; Sleep must fit the bed composition.

Delete the `VISUAL_FIDELITY_UNVERIFIED` marker only after the task's image review passes.

- [ ] **Step 3: Recompose Full geometry around the larger pet**

Keep the canonical `120`-column frame as the primary target and maintain at least one cell of right margin around the bed at that width. Adjust `full_geometry` so the rendered pet rectangle exactly covers the new sprite footprint plus intended petting target, and so stocked food/poop targets do not overlap the pet's default home position.

Add/retain assertions that every visible interactive object is covered by its hit rectangle and every hit rectangle stays inside the room.

- [ ] **Step 4: Make the bedroom read as layered scenery instead of labels**

Rework `render_furniture_full_wide` and associated sprite constants so Full visibly contains:

```text
wall/floor separation and a simple floor/rug layer
window with Day/Night treatment and curtain/frame silhouette
desk with laptop and lamp
wall shelf with book/decor rhythm
wardrobe with outline, two-door structure, and handles
bed with frame/pillow/blanket structure
at least two plant/flower accents
stocked food bowls/tray with compact counts
physical poop sprites
```

Remove redundant object-name labels (`WINDOW`, `SHELF`, `DESK`, `PET`) once the object itself is recognizable. Keep text where it communicates authoritative quantity/state rather than compensating for ambiguous art.

- [ ] **Step 5: Improve care/status visual hierarchy without reducing information**

Keep all four full need names and authoritative values, but render them as an intentional bottom status strip using segmented/icon-led visual treatment derived from reference sheet `(6)`. Use sparse text and strong fill/empty distinction. Affection/snack demands should have visible room/effect cues (heart/snack symbol or equivalent reference-driven treatment) in addition to any concise count.

Do not make an empty bar disappear on dark themes.

- [ ] **Step 6: Add structural RED/GREEN regressions**

Tests must assert at least:

```text
Full pet rendered height is 6-7 terminal rows at the canonical layout.
All required Full pose variants have the same declared logical canvas dimensions.
The bed has positive right margin at 120 columns.
Full contains non-empty desk, window, shelf, wardrobe, bed, plant, food, and poop visual regions in their intended zones.
No default Full pet hitbox overlaps stocked food sources.
Bed-sleep and floor-doze frames differ and only authoritative nap places the pet inside bed geometry.
Four need labels/bars remain present and bounded.
```

Do not make tests assert the entire ANSI frame byte-for-byte; structural assertions should allow later pixel refinement.

- [ ] **Step 7: Mandatory Full screenshot-and-inspect loop**

At minimum capture and inspect after the first coherent pass and after the final correction:

```text
120x45 SoftGreen Day awake
120x45 Night Night awake
120x45 Auto on light host awake
120x45 Auto on dark host awake
120x45 SoftGreen authoritative bed sleep
120x45 SoftGreen generic floor doze
```

Compare against `9904...jpeg`, `a483...png`, and reference sheets `(1)` through `(7)` according to the manifest. The reviewer must explicitly score pet focal scale, furniture recognizability, room layering, status/care readability, theme contrast, half-block seams, hit-region correspondence, bed-vs-doze distinction, and right-edge clipping.

If the reviewer still describes the pet as a small hollow ring or the room as sparse line art, the task is not complete.

- [ ] **Step 8: Run Rust gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p codegotchi-cli --test terminal_room --test terminal_input -- --nocapture
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add -- crates/codegotchi-cli/src/terminal/sprites.rs crates/codegotchi-cli/src/terminal/room.rs crates/codegotchi-cli/tests/terminal_room.rs crates/codegotchi-cli/examples/terminal_room_fixture.rs
git commit -m "feat: raise full terminal room visual fidelity"
```

Stage only files actually modified; temporary iteration screenshots stay in the SDD workspace.

---

### Task 6: Replace Compact/Minimal Text Strips with Purpose-Built Pet-Centered Compositions

**Files:**
- Modify: `crates/codegotchi-cli/src/terminal/sprites.rs`
- Modify: `crates/codegotchi-cli/src/terminal/room.rs`
- Modify: `crates/codegotchi-cli/tests/terminal_room.rs`
- Modify: `crates/codegotchi-cli/tests/terminal_input.rs` if Minimal pet geometry changes
- Modify if needed: `crates/codegotchi-cli/examples/terminal_room_fixture.rs`

**Interfaces:**
- Consumes: Task 5 mascot vocabulary and Task 4 palette; existing Compact/Minimal core-care geometry; canonical Compact reference `a483...png` plus sheets `(1)`, `(5)`, `(6)`, `(7)`; baseline `compact-light.png` and `minimal-light.png`.
- Produces: a 7-row Compact vignette and 3-row Minimal projection that still expose every required authoritative care path but visually read as CodeGotchi rather than a debug/status console.

- [ ] **Step 1: Lock functional priority before visual edits**

Retain these invariants with tests before changing layout:

```text
Compact exposes every stocked food kind and count.
Minimal exposes deterministic first stocked food, or no active food hitbox when empty.
Minimal bed remains clickable.
Minimal pending poop remains cleanable.
Minimal pet remains pettable.
Affection/snack state remains visible.
No decoration consumes a core-care hit region.
```

- [ ] **Step 2: Build a dedicated Compact composition**

Use the full seven room rows intentionally rather than spending the top rows on plain diagnostic text. The Compact frame must include:

```text
recognizable compact mascot as the focal left/center element
one small room vignette cue (window/furniture/plant silhouette)
condensed segmented H/E/HAPPY/CLEAN state treatment
condensed physical food + poop + bed targets
compact demand/effect cues
short horizontal roaming space
```

Remove redundant `PET`/activity prose where the same information is already visible from the mascot/effect. Preserve broad activity reaction through pose/effect, not a debug-looking banner.

- [ ] **Step 3: Make Minimal use an actual purpose-built pet sprite**

The three-row Minimal layout has enough horizontal space to reserve the left `7-9` columns for a real packed compact mascot while placing needs and care to the right. Replace the duplicated text/ASCII-cat treatment with:

```text
left: packed Minimal/Compact mascot across the available 3 rows
right upper: condensed need indicators
right middle: stocked FOOD / BED / POOP targets
right lower: affection/snack/activity cue when needed
```

Adjust `minimal_geometry.pet` to cover the rendered mascot rather than only one text row. Keep the deterministic first-stocked-food order and no-stock disabled state.

- [ ] **Step 4: Add degradation and hit-alignment regressions**

Tests must prove:

```text
Full -> Compact removes decoration before care.
Compact -> Minimal removes bedroom scenery but retains pet + needs + food + bed + poop + petting.
Every Minimal visible target's far edge is inside its hit rectangle.
Minimal pet geometry spans the rendered sprite footprint.
All 15 non-empty inventory masks still select a valid stocked Minimal food; zero inventory exposes no draggable food region.
Compact and Minimal status/care text never writes outside 120-column canonical buffers.
```

- [ ] **Step 5: Mandatory Compact/Minimal screenshot loop**

Capture `120x30` Compact and `120x21` Minimal after implementation. Compare side-by-side with the current baseline and canonical mapping. The final review must explicitly answer:

```text
Does Compact read as a pet vignette rather than two status rows?
Is the mascot still the first thing the eye finds?
Can every care target be identified without relying on a long label?
Does Minimal visibly contain the pet, not only the word PET or an ASCII emoticon?
Are needs readable at a glance?
Are there any half-block seams, clipping, or hit-region mismatches?
```

Do not mark PASS while the reviewer still describes either mode as "mostly text" or "text-only strip".

- [ ] **Step 6: Run focused and workspace gates**

```bash
cargo test -p codegotchi-cli --test terminal_room --test terminal_input -- --nocapture
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add -- crates/codegotchi-cli/src/terminal/sprites.rs crates/codegotchi-cli/src/terminal/room.rs crates/codegotchi-cli/tests/terminal_room.rs crates/codegotchi-cli/tests/terminal_input.rs crates/codegotchi-cli/examples/terminal_room_fixture.rs
git commit -m "feat: refine compact and minimal terminal rooms"
```

Stage only files actually modified.

---

### Task 7: Complete the Real-Codex Interaction and Live-Visual Acceptance Matrix

**Files:**
- Create: `scripts/verify-terminal-room-live.sh`
- Modify: `docs/verification/terminal-room.md`
- Modify only if the live run reveals a product defect: the exact owning terminal/launcher/input file plus its regression test

**Interfaces:**
- Consumes: final terminal host from Tasks 1-6, official installed Codex CLI, xterm or another supported real terminal, existing mode-driven PTY input implementation, CodeGotchi runtime care paths.
- Produces: a repeatable live acceptance procedure and evidence for the missing base/hardening checklist without relying on Codex rendered-text scraping in product code.

- [ ] **Step 1: Make the live harness fail fast on the environment problem that blocked the previous attempt**

`scripts/verify-terminal-room-live.sh` must:

```text
require an installed `codex` and print its version
require xterm + xdotool + ImageMagick import when using the capture path
accept/reuse an existing usable DISPLAY, or start a private Xvfb display
when using private Xvfb, require/start a lightweight window manager before xdotool activation
verify window activation succeeds before launching the timed interaction sequence
use CODEGOTCHI_BROWSER=none for terminal-only acceptance
use a private temporary CodeGotchi state/data directory so care evidence is isolated
never print bearer tokens or copied auth contents
clean up only processes/temp paths it created
```

If a window manager is unavailable, the script must stop with a clear prerequisite message instead of repeating the previous `_NET_ACTIVE_WINDOW`/`BadWindow` timeout.

- [ ] **Step 2: Keep authentication and state handling safe**

The script may point Codex at an existing authorized `CODEX_HOME`, but must not copy auth file contents into logs or committed evidence. Use a temporary `XDG_DATA_HOME`/CodeGotchi state location so feed/nap/pet/clean acceptance cannot mutate the user's ordinary pet database.

If deterministic poop setup is needed, use an existing CodeGotchi debug/fixture capability only in this isolated state. Do not add a production backdoor solely for acceptance.

- [ ] **Step 3: Run the real official Codex input-fidelity checklist**

In the production terminal host, demonstrate and record:

```text
ordinary prompt entry and editing/navigation keys
one bracketed/multiline paste while Codex has negotiated paste mode
focus out/in where Codex enables focus reporting and the environment makes it observable
scrolling/clicking while Codex requests mouse reporting, when the current Codex UI exposes such a surface
model response and tool activity
one harmless approval/review interaction when the current Codex invocation exposes one
trailing ordinary Codex arguments remain honored
```

For an approval probe, use only a harmless operation in a disposable temporary directory. If the installed Codex version cannot produce an approval under the chosen safe invocation, record `not available in this invocation/version` with the exact version and command rather than fabricating a PASS.

- [ ] **Step 4: Exercise responsive layout and every room care action while Codex remains alive**

Resize the same live host through:

```text
120x45 -> Full
120x30 -> Compact
120x21 -> Minimal
120x45 -> Full
```

At each transition verify Codex remains usable and receives the matching PTY rectangle. Across the session perform:

```text
petting with a qualifying gesture
feeding a stocked item by drag-to-pet
cleaning one authoritative poop when present in the isolated state
explicit bed/nap interaction
```

After each care action, verify the visual change settles from the authoritative snapshot rather than an optimistic local mutation.

- [ ] **Step 5: Capture final live composition evidence**

Capture at least one `120x45` live frame where the **real Codex TUI is visibly populated in the upper pane and the final Full room is visible below**. Inspect it against the canonical Full reference for composition hierarchy. The upper pane must unmistakably be real Codex, not fixture text or a recreated UI.

Also confirm the bearer token is absent from the visible composed terminal.

- [ ] **Step 6: Prove clean exit/restoration**

Exit Codex normally and run one bounded interrupt/termination case. Confirm cursor visibility, mouse capture, raw mode, and alternate screen are restored and the terminal remains usable. Record child exit status/restoration result.

- [ ] **Step 7: Fix only defects actually exposed by the live run**

If the live run reveals incorrect wire encoding, resize, pane routing, signal cleanup, or care behavior, first add the smallest deterministic regression to the existing terminal test suite, then fix the owning production layer. Re-run the relevant focused suite and this live checklist. Do not patch the acceptance script to hide a product defect.

- [ ] **Step 8: Commit the repeatable harness and provisional evidence**

```bash
git add -- scripts/verify-terminal-room-live.sh docs/verification/terminal-room.md
git commit -m "test: add real codex terminal acceptance harness"
```

If product fixes were required, commit them separately before the evidence commit so reviewers can distinguish code from evidence.

---

### Task 8: Final Visual Review, Fresh Matrix, and Release Evidence

**Files:**
- Modify: `docs/mockups/current/README.md`
- Replace with fresh captures: `docs/mockups/current/full-light.png`
- Replace with fresh captures: `docs/mockups/current/full-dark.png`
- Replace with fresh captures: `docs/mockups/current/compact-light.png`
- Replace with fresh captures: `docs/mockups/current/minimal-light.png`
- Replace with fresh captures: `docs/mockups/current/full-bed-sleep.png`
- Replace with fresh captures: `docs/mockups/current/full-floor-doze.png`
- Modify: `docs/verification/terminal-room/README.md`
- Replace selected final screenshots under: `docs/verification/terminal-room/`
- Modify: `docs/verification/terminal-room.md`
- Modify if final web build changes: `crates/codegotchi-cli/web-dist/`

**Interfaces:**
- Consumes: all Tasks 1-7, current canonical reference manifest, hosted CI after an authorized push.
- Produces: one final integrated release record whose PASS/FAIL claims match fresh evidence; no stale SHA/run/test claims.

- [ ] **Step 1: Start from a clean integrated tree and record the exact source SHA**

```bash
git status --short
git rev-parse HEAD
```

Expected: clean worktree before final verification. Record the SHA in the SDD ledger and every evidence README generated from it.

- [ ] **Step 2: Run the complete local automated matrix fresh from that SHA**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
corepack pnpm test
corepack pnpm lint
corepack pnpm format:check
corepack pnpm build
node web/scripts/embed-web.mjs
corepack pnpm playwright:test:production
```

Expected: every command exits `0`. A Playwright test reported as flaky/retried is not a clean release PASS; investigate and remove the nondeterminism.

- [ ] **Step 3: Repeat the formerly flaky browser test**

Run the Task 3 single-test command ten times again against the integrated production bundle. Expected: 10/10 clean PASS with no retry.

- [ ] **Step 4: Recapture the six current-state images from final source**

Use the exact documented fixture/capture method and canonical sizes. Replace the six `docs/mockups/current/*.png` files and update their README with:

```text
final source SHA
exact fixture state/theme/host polarity
pixel dimensions
SHA-256
short direct-inspection result
```

The `full-light`/`full-dark` baseline names may retain the approved named-preset comparison, but final verification must additionally retain Auto-on-light and Auto-on-dark evidence under `docs/verification/terminal-room/`.

- [ ] **Step 5: Dispatch an independent image-capable visual reviewer**

The reviewer gets no implementation narrative, only:

```text
all nine canonical references in docs/mockups/terminal-room/
all six fresh final current-state captures
Auto light + Auto dark captures
one populated live-Codex Full capture from Task 7
the visual sections of both normative specs
```

The review must explicitly score:

```text
Full bedroom composition and layering
mascot scale/silhouette/facial readability
furniture recognizability and balance
status/care hierarchy
Compact vignette quality
Minimal pet + care readability
light/dark Auto contrast
named preset contrast
half-block seams/clipping/checkerboard noise
visible-object vs hit-region correspondence
bed sleep vs generic doze
Full -> Compact -> Minimal degradation order
real Codex dominance/legibility in the live composition
```

Any high-severity mismatch is a FAIL and returns to the owning visual task. Do not relabel `PENDING_VISION_REVIEW` to PASS based only on automated tests.

- [ ] **Step 6: Controller push/hosted-CI gate**

Workers stop before push. Once the controller/user explicitly authorizes a push of the final implementation commits, push the branch and require a fresh workflow on the exact final source SHA:

```text
Rust checks (Ubuntu): PASS
Rust checks (macOS): PASS, including macOS PTY smoke
Web checks: PASS
```

Record workflow run ID, job IDs, and conclusions in `docs/verification/terminal-room.md`. Do not cite an older green run for a newer source SHA.

- [ ] **Step 7: Rewrite the final verification document from evidence, not history**

`docs/verification/terminal-room.md` must no longer say hosted CI is unavailable or preserve an obsolete local blocker as the current verdict. It must contain:

```text
final source SHA
automated local matrix and exact results
hosted Ubuntu/macOS/web run IDs and results
real Codex version + invocation + interaction checklist results
responsive/care acceptance results
final visual reviewer verdict and evidence filenames
known non-blocking limitations, if any
one unambiguous final status: PASS or FAIL
```

Historical failed attempts can remain linked, but they must be clearly historical.

- [ ] **Step 8: Run the final whole-branch reviewer**

Dispatch a fresh high-judgment reviewer over `master...HEAD`, both specs, this plan, final CI evidence, and visual evidence. The reviewer must specifically search for:

```text
spec requirements accidentally dropped during visual changes
new optimistic care mutations
Codex text scraping or keyboard theft
mouse hitboxes no longer matching art
platform-specific cfg gaps
unbounded terminal cleanup/wait paths
stale evidence claims
new browser regressions
```

Allow one final scoped fix wave and one scoped re-review. After that, adjudicate residual findings explicitly in the SDD ledger rather than silently looping indefinitely.

- [ ] **Step 9: Commit final evidence**

```bash
git add -- docs/mockups/current docs/verification/terminal-room docs/verification/terminal-room.md crates/codegotchi-cli/web-dist
git commit -m "docs: record terminal room release verification"
```

Stage only changed evidence/bundle files.

- [ ] **Step 10: Prepare, but do not apply, PR metadata**

If and only if final status is PASS, write a recommended PR title/body into the SDD report summarizing the implemented terminal room, platform support, tests, and visual/live evidence. Do not update PR #2 unless the controller/user separately authorizes that external mutation.

---

## Final Definition of Done

The branch is merge-ready only when every item below is true on the same integrated source state:

- [ ] Hosted Ubuntu Rust is green.
- [ ] Hosted macOS Rust is green and executes the production-shared pseudo-terminal smoke.
- [ ] Native macOS browser launch uses `/usr/bin/open`; Browser/Both/Auto fallback are not Linux-only.
- [ ] Hosted web CI is green.
- [ ] `cargo test --workspace` is green unconstrained on the ordinary Linux/WSL verification environment.
- [ ] Production Playwright is green with no flaky classification; idle choreography is deterministic under test-controlled time.
- [ ] Second-left-down and wrong-button-drag capture regressions are green for pet and food interactions.
- [ ] PetStroke evidence-bearing domain validation remains intact.
- [ ] Auto uses ordered default foreground/background dithering and is visually reviewed on both light and dark host themes.
- [ ] Full no longer reads as sparse line art; the large round expressive mascot is the visual focal point and the bedroom furniture/surfaces are recognizable.
- [ ] Compact reads as a purpose-built pet vignette rather than status/debug rows.
- [ ] Minimal visibly contains a pet sprite plus concise needs/core care rather than a text-only strip.
- [ ] Bed sleep and floor doze remain unmistakably distinct and authoritative semantics are unchanged.
- [ ] Every visible care target matches its hit geometry after the art/layout changes.
- [ ] A real official Codex session proves prompt/editing, paste, focus where observable, mouse behavior where enabled, tool activity, approval/review where available, Full -> Compact -> Minimal -> Full resize, pet/feed/clean/nap, and clean exit/restoration.
- [ ] At least one final live screenshot visibly contains the real populated Codex TUI above the final room.
- [ ] An independent image-capable reviewer gives no high-severity mismatch against the canonical reference set.
- [ ] `docs/verification/terminal-room.md` describes the final HEAD truthfully and is no longer marked pending/fail for already-resolved historical gates.
- [ ] Final whole-branch review is complete with all load-bearing findings fixed or explicitly adjudicated.

Only after all of these are true should the controller consider changing PR #2 from its current design-era metadata to release/implementation metadata and making it merge-ready.
