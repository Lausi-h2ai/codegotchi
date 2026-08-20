# Terminal Room Release Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining release blockers on the pushed terminal-room implementation: complete the claimed macOS path, harden captured mouse semantics, remove the browser-motion flake, implement spec-compliant adaptive Auto rendering, bring Full/Compact/Minimal visuals up to the approved reference direction, complete the real-Codex acceptance matrix, and produce fresh green release evidence.

**Architecture:** Preserve the existing boundaries. The official Codex executable remains the PTY-hosted upper TUI, `AuthoritativeRuntime` remains the only authority for care state, terminal/browser motion remains presentation-only, and visual work stays inside renderer/sprite/layout code. Fix failures at their owning boundary rather than weakening tests or specs. A release PASS requires automated gates plus image and live-terminal evidence.

**Tech Stack:** Rust 1.97.x workspace, `portable-pty 0.9.0`, `vt100 0.16.2`, `ratatui 0.30.2`, `crossterm 0.29.0`, React 19, TypeScript 5.9, Playwright 1.62, pnpm 11.20.0, GitHub Actions, xterm/Xvfb for deterministic capture, official Codex CLI.

**Spec:** `docs/superpowers/specs/2026-08-13-terminal-room-design.md`, with normative overrides from `docs/superpowers/specs/2026-08-13-terminal-room-agent-hardening.md`.

## Global Constraints

- The official Codex TUI remains the actual upper pane. Never recreate, fork, patch, or text-scrape Codex UI semantics.
- Initial support remains Linux, WSL, and macOS unless the approved spec is explicitly amended.
- Browser mode remains a selectable fallback/second projection.
- Authoritative needs, inventory, demands, poop, nap state, persistence, replay safety, and enforcement remain owned by the Rust runtime/domain.
- Terminal petting remains exactly `1_500 ms` minimum duration and `120.0` backend distance, with Euclidean cell path multiplied by `POINTER_DISTANCE_PER_CELL = 16.0`.
- Generic `PetBehavior::Sleeping` never means authoritative bed sleep. Only a future `napping_until` relative to `last_updated_at` selects the recovery-bed pose.
- `CareCommand::PetStroke` must continue carrying cumulative `duration_ms` and `distance` evidence and remain domain-validated. An interrupted qualified stroke may retain validated happiness but never resolves affection without final `Pet`.
- The five initial terminal presets remain `auto`, `mono`, `soft-green`, `amber`, and `night`; theme selection is presentation-only.
- Auto must work against arbitrary terminal foreground/background pairs. Intermediate tones use ordered default-foreground/default-background dithering rather than named gray assumptions.
- Full, Compact, and Minimal are purpose-built compositions. Decoration disappears before core care functionality.
- Core care remains possible in Minimal: pet, stocked food drag-to-pet, poop cleaning, and bed interaction.
- Material renderer/sprite/theme/layout changes require screenshot -> image inspection -> reference comparison -> correction -> recapture.
- Canonical references are in `docs/mockups/terminal-room/`. The six images in `docs/mockups/current/` are the audited pre-fix current-state baseline, not acceptance references.
- Final visual evidence includes Full light/default and dark, Compact, Minimal, authoritative bed sleep, generic floor doze, Auto on light/dark hosts, and a live real-Codex terminal composition.
- ANSI dumps, text snapshots, or fixture screenshots alone never satisfy final visual acceptance.
- Luna workers may commit locally but may not push, merge, publish, or mutate PR metadata. Those are controller/user-authorized external actions.
- Final integrated HEAD passes formatting, Clippy, Rust workspace tests, web tests/lint/format/build/embed, production Playwright, hosted Ubuntu CI, and hosted macOS CI.

---

## Audit Baseline — 2026-08-20

Audited PR: `#2`, branch `docs/terminal-room-design`.

Audited pushed head before this plan: `ff16622441dd1e88460ee09b170ed08621278b6d`.

`docs/mockups/current/README.md` records exact hashes, capture environment, fixture settings, and image-review notes for the six current screenshots sourced from `20d4ad31855e2b5ff846d4d726add4b73e68c814`.

### Closed findings — do not reimplement

- The terminal compositor resize test is hermetic; hosted Ubuntu reaches and passes the Rust workspace suite.
- Browser sustained petting and fixture isolation are fixed; hosted production Playwright passes.
- `PetStroke` carries cumulative evidence and the domain independently validates the same `1500 / 120` contract as final `Pet`.
- Interrupted qualified strokes retain only already validated happiness; final `Pet` alone resolves affection.
- All five named terminal presets and day/night ambience exist.
- Browser/terminal generic doze is separate from authoritative future-deadline bed nap.
- Minimal selects the first stocked food in Kibble -> Treat -> Fruit -> EnergyDrink order and has a no-stock state.
- Outside-room capture and focus/resize cancellation have regression coverage.
- `Zone.Identifier` files were removed/ignored and README launcher docs were updated.
- Current hosted Ubuntu Rust and web jobs are green.

### P0 — macOS CI is red and the warning reveals missing product behavior

The current macOS job fails Clippy because `launch_native_browser(url)` never uses `url` outside its Linux-only body. More importantly, the native helper has Linux `xdg-open`/`gio` and WSL `cmd.exe`, but no macOS `/usr/bin/open` path. Browser/Both and Auto fallback therefore cannot natively open the browser on a claimed supported platform unless `CODEGOTCHI_BROWSER` is manually supplied.

The PTY verification record also still says macOS descendant-cleanup acceptance was deferred. After Clippy is fixed, hosted macOS tests and a pseudo-terminal smoke must establish the real lifecycle guarantee rather than preserve that deferment silently.

### P0 — visual acceptance is still an implementation failure

Both `docs/mockups/current/README.md` and `docs/verification/terminal-room/README.md` deliberately refuse a visual PASS.

- Full is recognizable but sparse line art; the mascot is too small/hollow; furniture and surfaces do not read as the layered reference; status/care treatment is text-heavy.
- Dark Full is readable but has weak empty-bar contrast and limited hierarchy.
- Compact preserves care but is mostly status text and line glyphs rather than a pet-centered vignette.
- Minimal is functionally useful but effectively a textual three-row strip instead of a purpose-built pet + needs + care composition.
- Bed sleep and floor doze are semantically distinct, but both pet poses remain abstract blocks compared with the supplied component references.
- Fixture captures leave the Codex pane blank; they cannot satisfy final live-terminal visual comparison with the real Codex TUI.

The source matches the evidence: Full pet art is `10 x 10` logical pixels (`5` terminal rows) and still marked `VISUAL_FIDELITY_UNVERIFIED`; the room uses prominent literal `WINDOW`, `SHELF`, `DESK`, `PET`, `FOOD`, and `POOP` labels to carry visual meaning.

### P1 — invalid terminal button sequences mutate captured care gestures

`RoomInputSession::process` treats every `Down(Left)` as a possible new gesture/drag even when a capture exists and advances captured state for `Drag(_)` regardless of button. A second left press can restart/mutate capture, and right/middle drags can advance a left-button care gesture. Invalid sequences must cancel-and-consume, with no care request and no Codex leakage.

### P1 — browser idle-travel remains structurally flaky

`keeps idle travel in safe regions and eventually performs a roll` still waits up to `35_000 ms` of wall time for a production roll randomly scheduled in `15_000..30_000 ms`. A prior controller run observed this scenario as flaky. One green hosted run does not remove the structural timing dependency. The motion system already exposes timer/clock/random ports; production timing must remain unchanged while the E2E controls time deterministically.

### P1 — Auto theme does not satisfy the approved adaptive baseline

`TerminalThemePreset::Auto` currently maps `Tone1` to `DarkGray` and `Tone2` to `Gray`. The design requires ordered default-foreground/default-background dithering when exact host colors cannot be inferred. The current visual evidence also uses `soft-green` for light and `night` for dark, so it does not prove Auto readability on either host polarity.

### P1 — real-Codex acceptance is incomplete

The latest live attempt reached the official TUI but the private Xvfb environment had no usable window manager; `xdotool` activation/focus failed with `BadWindow`. Current evidence therefore cannot claim the required real-session prompt/editing, paste, focus where observable, Codex mouse behavior where enabled, tool/approval flow, Full -> Compact -> Minimal -> Full resize, care actions while Codex remains usable, or complete clean exit/restoration.

### P2 — release evidence and PR metadata are stale

`docs/verification/terminal-room.md` still says hosted CI was unavailable and retains an older local workspace failure as current status. The pushed branch now has concrete hosted results: Ubuntu Rust PASS, web PASS, macOS FAIL before tests. PR #2 also still has a design-era title/body. Luna workers must not mutate PR metadata; final metadata is a controller action after all gates pass.

---

## Luna Execution Topology

Use one fresh Luna implementer and one fresh Luna reviewer per task. Use this plan's own `.superpowers/sdd/2026-08-20-terminal-room-release-hardening/` workspace and ledger; never reuse the previous plan's SDD directory.

Tasks 1-4 are logically independent. Task 5 consumes Task 4's palette behavior. Task 6 consumes the mascot/art vocabulary established by Task 5. Task 7 runs after Tasks 1, 2, 5, and 6 are integrated. Task 8 is the final controller/reviewer gate.

Every task reviewer checks spec compliance and regression risk. A focused green test is insufficient if the implementation weakens a Global Constraint.

---

### Task 1: Make macOS a Real Supported Launcher/PTY Path

**Files:**
- Modify: `crates/codegotchi-cli/src/launcher.rs`
- Modify only if the macOS PTY smoke exposes a lifecycle defect: `crates/codegotchi-cli/src/terminal/pty.rs`
- Modify only if the macOS PTY smoke exposes a session defect: `crates/codegotchi-cli/src/terminal/session.rs`
- Modify: `.github/workflows/ci.yml`
- Test: launcher unit tests in `crates/codegotchi-cli/src/launcher.rs`
- Test: `crates/codegotchi-cli/tests/terminal_pty.rs`
- Test: `crates/codegotchi-cli/tests/terminal_session.rs`

**Interfaces:**
- Consumes: `launch_browser(url)`, `spawn_browser_command`, current Unix terminal session, existing ignored outer-PTY integration tests.
- Produces: native macOS browser launch via `/usr/bin/open`, green macOS fmt/Clippy/workspace tests, and a hosted macOS pseudo-terminal smoke of the production-shared session path.

- [ ] **Step 1: Add a testable macOS command-selection seam**

Refactor command selection so construction is pure and process creation remains in `spawn_browser_command`. For the function argument `url`, macOS selection is exactly:

```text
program = /usr/bin/open
arguments = [url]
```

Add a `#[cfg(target_os = "macos")]` unit test using `url = "http://127.0.0.1:4173/#token=test"`; assert program `/usr/bin/open` and one identical URL argument. Preserve Linux `xdg-open`, `gio`, and WSL fallback semantics.

- [ ] **Step 2: Implement native macOS launch**

Use existing `spawn_browser_command` stdio/reaping behavior and `/usr/bin/open`. Do not shell-wrap or concatenate the URL. Do not “fix” Clippy by renaming `url` to `_url`.

- [ ] **Step 3: Run Linux/WSL-safe gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Expected: PASS; Linux/WSL behavior unchanged.

- [ ] **Step 4: Add a macOS outer-PTY CI smoke after workspace tests**

Run the existing production-shared ignored integration test under BSD `script`:

```bash
script -q /dev/null cargo test -p codegotchi-cli --test terminal_session \
  composed_session_adapter_delivers_exact_invocation_modes_input_and_status \
  -- --ignored --nocapture
```

If the hosted BSD `script` parser needs a different command delimiter, change only shell wrapper syntax; execute this same test target and assertions.

- [ ] **Step 5: Treat any PTY failure as a product finding**

If the smoke exposes a bounded-cleanup issue on the no-`waitid(WNOWAIT)` path, add a platform-specific regression and fix the smallest PTY/session owner. The acceptable behavior is bounded cleanup without signaling a recycled process-group identity. Do not claim stronger descendant cleanup than the implementation proves and do not disable macOS to make CI green.

- [ ] **Step 6: Commit**

```bash
git add -- crates/codegotchi-cli/src/launcher.rs .github/workflows/ci.yml
git add -- crates/codegotchi-cli/src/terminal/pty.rs crates/codegotchi-cli/src/terminal/session.rs 2>/dev/null || true
git commit -m "fix: complete macos launcher and terminal smoke"
```

Before commit, inspect `git diff --cached` and unstage any unchanged/unrelated file. No push.

---

### Task 2: Make Room Pointer Capture a Strict Left-Button State Machine

**Files:**
- Modify: `crates/codegotchi-cli/src/terminal/input.rs`
- Modify: `crates/codegotchi-cli/tests/terminal_input.rs`
- Modify only if pane-routing coverage is missing: `crates/codegotchi-cli/src/terminal/session.rs`

**Interfaces:**
- Consumes: `RoomInputSession::{has_active_capture,cancel,process}`, `MouseEventKind`, current active-capture pane routing.
- Produces: one left-button capture lifecycle where invalid button sequences cancel and are consumed without care and without Codex leakage.

- [ ] **Step 1: Add the RED matrix**

Cover both pet and food capture:

```text
active pet + second Down(Left)   -> cancelled, zero requests, no replacement capture
active food + second Down(Left)  -> cancelled, zero requests, no replacement capture
active pet + Drag(Right)         -> cancelled, zero requests
active pet + Drag(Middle)        -> cancelled, zero requests
active food + Drag(Right)        -> cancelled, zero requests
active food + Drag(Middle)       -> cancelled, zero requests
active pet/food + Drag(Left)     -> original capture remains active
no capture + Drag(any button)    -> no request, no capture
active capture + Up(non-left)    -> cancelled, zero requests
```

Use one second-left case over another valid target and one over empty room space so restart cannot pass accidentally.

- [ ] **Step 2: Implement capture-first event classification**

While `has_active_capture()` is true:

```text
Down(any button)        -> cancel, consume, no reinterpretation
Drag(Left)              -> advance existing capture
Drag(Right/Middle)      -> cancel, consume
Up(Left)                -> existing valid completion/cancellation logic
Up(Right/Middle)        -> cancel, consume
Move/scroll             -> never mutate care evidence
```

Without capture, only `Down(Left)` on the visible pet or stocked food starts capture. Bare Drag never starts capture.

- [ ] **Step 3: Prove invalid captured events do not reach Codex**

If session tests lack these cases, add one second-left and one wrong-button-drag regression while the physical pointer is outside the room. Assert zero PTY bytes for the terminating event.

- [ ] **Step 4: Run focused and full Rust gates**

```bash
cargo test -p codegotchi-cli --test terminal_input -- --nocapture
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -- crates/codegotchi-cli/src/terminal/input.rs crates/codegotchi-cli/tests/terminal_input.rs
git add -- crates/codegotchi-cli/src/terminal/session.rs 2>/dev/null || true
git commit -m "fix: enforce terminal room pointer capture semantics"
```

Inspect staged diff first. No push.

---

### Task 3: Remove Wall-Clock Randomness from Browser Idle-Travel E2E

**Files:**
- Modify: `web/e2e/mvp.spec.ts`
- Do not change production choreography timing unless a separate product bug is proven.

**Interfaces:**
- Consumes: Playwright `page.clock`, existing `data-motion-*` diagnostics, production roll interval `15_000..30_000 ms`, roll duration `800 ms`.
- Produces: deterministic production-embedded E2E proving safe idle travel and at least one roll without a 35-second wall-clock race.

- [ ] **Step 1: Preserve the current semantic assertions**

Keep safe-region, grounded-position, idle/free-time, and observed-roll checks. Remove only the assumption that real wall time is part of the assertion.

- [ ] **Step 2: Install Playwright clock before app startup**

Call `page.clock.install()` before `page.goto(launchUrl)` in this test. Do not add a production `E2E` branch and do not reduce production roll intervals.

- [ ] **Step 3: Advance simulated time in 250 ms increments through 31 seconds**

After each increment, sample existing motion attributes and rendered geometry. `31_000 ms` covers the maximum `30_000 ms` roll scheduling horizon; `250 ms` is shorter than the `800 ms` roll duration, so a transient roll cannot be skipped by one large jump.

Pass only when at least one sample observes `data-motion-mode="free_time"` with `data-motion-action="roll"`, all sampled regions are allowed, and pet feet remain grounded.

- [ ] **Step 4: Remove the special 40-second test timeout and 35-second real-time poll**

The test must finish under normal Playwright timeout using ordinary page/bootstrap wall time.

- [ ] **Step 5: Stress the exact test ten times**

```bash
cd web
corepack pnpm build
node scripts/embed-web.mjs
for i in $(seq 1 10); do
  corepack pnpm exec playwright test --config playwright.config.ts \
    --grep "keeps idle travel in safe regions and eventually performs a roll"
done
cd ..
corepack pnpm playwright:test:production
```

Expected: 10/10 focused PASS and a clean full production suite with no flaky classification.

- [ ] **Step 6: Commit**

```bash
git add -- web/e2e/mvp.spec.ts crates/codegotchi-cli/web-dist
git commit -m "test: make browser idle choreography deterministic"
```

Stage `web-dist` only if embedding changed tracked bytes. No push.

---

### Task 4: Implement the Specified Adaptive Auto Dither

**Files:**
- Modify: `crates/codegotchi-cli/src/terminal/theme.rs`
- Modify: `crates/codegotchi-cli/src/terminal/sprites.rs`
- Modify: `crates/codegotchi-cli/src/terminal/room.rs`
- Modify: `crates/codegotchi-cli/tests/terminal_room.rs`
- Modify only if fixture controls need expansion: `crates/codegotchi-cli/examples/terminal_room_fixture.rs`

**Interfaces:**
- Consumes: `SemanticTone`, `TerminalThemePreset`, `ResolvedPalette`, logical-pixel packing.
- Produces: Auto intermediate tones rendered by deterministic ordered host-default foreground/background coverage; named presets unchanged.

- [ ] **Step 1: Add RED tests for Auto**

Prove:

```text
Tone0 uses terminal default background.
Tone3 uses terminal default foreground.
Auto intermediate pixels do not depend on Color::DarkGray or Color::Gray.
A fixed 4x4 ordered pattern gives Tone1 lower foreground occupancy than Tone2.
Repeated rendering of the same logical sprite yields identical cells.
mono / soft-green / amber / night concrete mappings are unchanged.
```

- [ ] **Step 2: Separate fixed color mapping from adaptive logical-tone sampling**

Extend `ResolvedPalette` or a small internal companion type with a pure coordinate-aware operation:

```rust
sample_logical_tone(tone: SemanticTone, logical_x: u16, logical_y: u16) -> SemanticTone
```

Fixed presets return authored tones unchanged. Auto samples `Tone0` as background, `Tone3` as foreground, `Tone1` at low foreground density, and `Tone2` at higher foreground density using a deterministic 4x4 Bayer-style threshold pattern.

After sampling, Auto pixel art uses only terminal-default foreground/background; it never requires named Gray/DarkGray as inferred host colors.

- [ ] **Step 3: Sample before half-block packing**

Apply coordinate-aware sampling inside `draw_sprite_with_palette` before `put_packed`. Anchor the ordered pattern to stable room/logical coordinates so adjacent layers do not restart with visible checkerboard seams. For non-pixel labels/status text, Auto uses default foreground/background; visual weight comes from glyph density/segmentation rather than named gray.

- [ ] **Step 4: Run focused gates**

```bash
cargo test -p codegotchi-cli --test terminal_room -- --nocapture
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: PASS.

- [ ] **Step 5: Capture Auto on both host polarities**

Using the production `terminal_room_fixture` at `120x45`, capture `CG_FIXTURE_THEME=auto` on:

```text
light host: light/default background with dark/default foreground
dark host:  xterm -bg black -fg white
```

Inspect pet, furniture, status bars, care targets, and intermediate-tone hierarchy. Store task captures in this plan's SDD workspace; Task 8 selects final evidence.

- [ ] **Step 6: Commit**

```bash
git add -- crates/codegotchi-cli/src/terminal/theme.rs crates/codegotchi-cli/src/terminal/sprites.rs crates/codegotchi-cli/src/terminal/room.rs crates/codegotchi-cli/tests/terminal_room.rs
git add -- crates/codegotchi-cli/examples/terminal_room_fixture.rs 2>/dev/null || true
git commit -m "feat: add adaptive auto terminal dithering"
```

Inspect staged diff first. No push.

---

### Task 5: Raise Full Room and Mascot Visual Fidelity

**Files:**
- Modify: `crates/codegotchi-cli/src/terminal/sprites.rs`
- Modify: `crates/codegotchi-cli/src/terminal/room.rs`
- Modify: `crates/codegotchi-cli/tests/terminal_room.rs`
- Modify only if deterministic state controls need expansion: `crates/codegotchi-cli/examples/terminal_room_fixture.rs`

**Interfaces:**
- Consumes: Task 4 palette/dither behavior, shared `RoomGeometry`, canonical references, baseline `full-light.png`, `full-dark.png`, `full-bed-sleep.png`, `full-floor-doze.png`.
- Produces: pet-centered layered Full composition with expressive pose family and recognizable physical care objects while preserving authoritative geometry.

- [ ] **Step 1: Inspect references before editing**

Inspect all nine canonical images and the four Full baseline images. Record the five largest mismatches in the task report. Preserve these load-bearing properties:

```text
large almost-absurdly-round mascot
eyes + ears/nubs + mouth + paws/feet + tail/body silhouette
window/desk left; shelf/wardrobe middle; bed right
visible laptop + lamp/desk identity
plants/flowers plus simple rug/floor layering
food/poop as physical objects, not primarily words
segmented readable HUNGER / ENERGY / HAPPY / CLEAN treatment
bed sleep integrated with bedding; floor doze clearly separate
```

Tiny raster texture is expendable.

- [ ] **Step 2: Redraw the complete Full pose family at 12-14 logical rows**

Target `6-7` terminal rows and enough width for the round silhouette. Update every Full pose:

```text
Idle Blink WalkA WalkB Sit Doze Sleep Yawn Curious Happy Upset Eating Petted
```

All poses share one declared logical canvas. WalkA/B visibly alternate; Blink changes eyes; Happy/Upset/Petted/Eating read without a `PET` label; Doze curls on floor; Sleep fits bed. Remove `VISUAL_FIDELITY_UNVERIFIED` only after image review passes.

- [ ] **Step 3: Recompose Full geometry for the larger mascot**

At canonical width `120`, keep the bed at least one cell inside the right edge. Make the pet hit rectangle cover the complete rendered mascot and ensure the default pet position does not overlap stocked food sources or poop targets. Every visible target remains inside room bounds.

- [ ] **Step 4: Make furniture visually self-identifying**

Full visibly contains:

```text
wall/floor separation + simple rug/floor layer
Day/Night framed window with curtains
recognizable desk + laptop + lamp
shelf with books/decor rhythm
wardrobe with outer frame, two doors, handles
bed with frame, pillow, blanket
at least two plant/flower accents
stocked food bowl/tray sprites with concise counts
physical poop sprites
```

Remove literal `WINDOW`, `SHELF`, `DESK`, and `PET` labels once the object itself communicates identity. Keep text for authoritative quantity/state, not to compensate for ambiguous art.

- [ ] **Step 5: Improve status and care hierarchy**

Keep all four need names/values in an intentional bottom strip with segmented/icon-led fill/empty treatment. Empty segments remain visible on dark hosts. Affection/snack use visible room/effect cues plus concise counts.

- [ ] **Step 6: Add structural tests**

Assert:

```text
Full mascot occupies 6-7 terminal rows.
All Full pose canvases have equal dimensions.
Bed has positive right margin at width 120.
Desk/window/shelf/wardrobe/bed/plants/food/poop occupy intended non-empty regions.
Default pet hitbox does not overlap stocked food sources.
Bed sleep and floor doze differ; only authoritative nap places pet in bed geometry.
Four need labels/bars remain bounded.
```

Do not snapshot the entire ANSI frame byte-for-byte.

- [ ] **Step 7: Mandatory Full screenshot loop**

Capture and inspect at least twice during the task:

```text
120x45 SoftGreen Day awake
120x45 Night Night awake
120x45 Auto light-host awake
120x45 Auto dark-host awake
120x45 SoftGreen authoritative bed sleep
120x45 SoftGreen generic floor doze
```

Compare with `9904...jpeg`, `a483...png`, and sheets `(1)` through `(7)`. Reviewer scores mascot focal scale, furniture recognizability, room layering, status/care readability, contrast, half-block seams, hit-region correspondence, bed/doze distinction, and edge clipping. If the reviewer still calls the pet a small hollow ring or the room sparse line art, the task fails.

- [ ] **Step 8: Run Rust gates**

```bash
cargo test -p codegotchi-cli --test terminal_room --test terminal_input -- --nocapture
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add -- crates/codegotchi-cli/src/terminal/sprites.rs crates/codegotchi-cli/src/terminal/room.rs crates/codegotchi-cli/tests/terminal_room.rs
git add -- crates/codegotchi-cli/examples/terminal_room_fixture.rs 2>/dev/null || true
git commit -m "feat: raise full terminal room visual fidelity"
```

Temporary iteration screenshots stay in the SDD workspace. No push.

---

### Task 6: Build Purpose-Built Compact and Minimal Compositions

**Files:**
- Modify: `crates/codegotchi-cli/src/terminal/sprites.rs`
- Modify: `crates/codegotchi-cli/src/terminal/room.rs`
- Modify: `crates/codegotchi-cli/tests/terminal_room.rs`
- Modify if Minimal pet geometry changes: `crates/codegotchi-cli/tests/terminal_input.rs`
- Modify only if needed for capture state: `crates/codegotchi-cli/examples/terminal_room_fixture.rs`

**Interfaces:**
- Consumes: Tasks 4-5 palette/mascot vocabulary, existing Compact/Minimal care geometry, Compact reference `a483...png`, sheets `(1)`, `(5)`, `(6)`, `(7)`, and current Compact/Minimal baselines.
- Produces: a seven-row Compact vignette and three-row Minimal view that retain every core care path while reading visually as CodeGotchi rather than a debug console.

- [ ] **Step 1: Lock care-priority tests first**

Retain/prove:

```text
Compact exposes every stocked food kind/count.
Minimal exposes deterministic first stocked food or no active food hitbox when empty.
Minimal bed is clickable.
Minimal pending poop is cleanable.
Minimal pet is pettable.
Affection/snack state remains visible.
Decoration never consumes a core-care hit region.
```

- [ ] **Step 2: Use all seven Compact rows intentionally**

Compact includes:

```text
recognizable compact mascot as focal element
one small window/furniture/plant vignette cue
condensed segmented H/E/HAPPY/CLEAN indicators
physical food + poop + bed targets
compact demand/effect cues
short horizontal roaming space
```

Remove redundant `PET`/activity prose where pose/effect already communicates it. Broad activity remains visible through mascot/effect rather than a debug-looking banner.

- [ ] **Step 3: Make Minimal render a real packed mascot**

Reserve the left `7-9` columns across all three rows for a packed Minimal/Compact sprite. Put condensed needs and care to the right:

```text
left: 3-row packed mascot
right upper: condensed need indicators
right middle: active FOOD / BED / POOP targets
right lower: affection/snack/activity cue when present
```

Update `minimal_geometry.pet` to cover the rendered sprite footprint. Preserve first-stocked-food ordering and no-stock disabled state.

- [ ] **Step 4: Add degradation/hit regressions**

Assert:

```text
Full -> Compact removes decoration before care.
Compact -> Minimal removes bedroom scenery but retains pet + needs + food + bed + poop + petting.
Every Minimal visible target's far edge lies in its hit rectangle.
Minimal pet geometry spans the rendered sprite.
All 15 non-empty inventory masks select a stocked Minimal food; zero inventory has no draggable food.
Compact/Minimal never write outside canonical 120-column buffers.
```

- [ ] **Step 5: Mandatory Compact/Minimal image review**

Capture `120x30` Compact and `120x21` Minimal. Reviewer answers explicitly:

```text
Does Compact read as a pet vignette rather than status rows?
Is the mascot the first visual focus?
Can care targets be identified without long labels?
Does Minimal visibly contain a sprite rather than only PET/ASCII text?
Are needs readable at a glance?
Any seams, clipping, or hit mismatches?
```

A verdict of “mostly text” or “text-only strip” is a task FAIL.

- [ ] **Step 6: Run gates**

```bash
cargo test -p codegotchi-cli --test terminal_room --test terminal_input -- --nocapture
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add -- crates/codegotchi-cli/src/terminal/sprites.rs crates/codegotchi-cli/src/terminal/room.rs crates/codegotchi-cli/tests/terminal_room.rs crates/codegotchi-cli/tests/terminal_input.rs
git add -- crates/codegotchi-cli/examples/terminal_room_fixture.rs 2>/dev/null || true
git commit -m "feat: refine compact and minimal terminal rooms"
```

Inspect staged diff. No push.

---

### Task 7: Complete Real-Codex Interaction and Live-Visual Acceptance

**Files:**
- Create: `scripts/verify-terminal-room-live.sh`
- Modify: `docs/verification/terminal-room.md`
- Modify product/test files only if the live run exposes a concrete product defect.

**Interfaces:**
- Consumes: final host from Tasks 1-6, official installed Codex, xterm or another supported real terminal, current mode-driven PTY encoding and care paths.
- Produces: repeatable live acceptance and evidence for missing base/hardening requirements without product text scraping.

- [ ] **Step 1: Make the harness fail fast on the previous Xvfb focus problem**

The script:

```text
prints installed codex version
requires xterm + xdotool + ImageMagick import for automated capture
reuses a usable DISPLAY or starts private Xvfb
when starting private Xvfb, starts/requires a lightweight window manager before xdotool activation
verifies window activation before timed interaction
sets CODEGOTCHI_BROWSER=none
uses a private temporary CodeGotchi data/state directory
never prints bearer tokens or auth contents
cleans up only processes/temp paths it created
```

If no window manager is available, exit with a clear prerequisite instead of repeating `_NET_ACTIVE_WINDOW`/`BadWindow` timeouts.

- [ ] **Step 2: Isolate auth and pet state safely**

The script may reference an already authorized `CODEX_HOME` but never logs/copies secret contents. Use temporary `XDG_DATA_HOME`/CodeGotchi state so feed/nap/pet/clean do not mutate normal pet state. If deterministic poop setup is required, use only an existing debug/fixture capability inside that isolated state; do not add a production backdoor for acceptance.

- [ ] **Step 3: Run the official Codex fidelity checklist**

Demonstrate:

```text
ordinary prompt entry + editing/navigation keys
one bracketed/multiline paste while Codex has negotiated paste mode
focus out/in where Codex enables reporting and it is observable
scroll/click behavior where current Codex enables mouse reporting
model response + tool activity
one harmless approval/review interaction when current Codex exposes one
trailing ordinary Codex arguments remain honored
```

An approval probe may only touch a disposable temporary directory. If the installed Codex version/invocation cannot safely expose approval, record `not available in this invocation/version` with exact Codex version and command; never fabricate PASS.

- [ ] **Step 4: Exercise resize and room care during the same live session**

Resize:

```text
120x45 Full -> 120x30 Compact -> 120x21 Minimal -> 120x45 Full
```

At each step verify Codex remains usable and its PTY rectangle follows. Across the session perform qualifying petting, stocked food drag-to-pet, one authoritative poop clean when available in isolated state, and explicit nap. Verify authoritative snapshots settle every care result.

- [ ] **Step 5: Capture a populated live Full frame**

Capture at least one `120x45` frame with the real populated Codex TUI visibly above the final Full room. Inspect it against the canonical Full composition hierarchy and confirm no bearer token is visible.

- [ ] **Step 6: Prove terminal restoration**

Exit normally and run one bounded interrupt/termination case. Confirm cursor, mouse capture, raw mode, alternate screen, and terminal usability restore correctly; record child exit/restoration result.

- [ ] **Step 7: Fix only defects the live run proves**

For any wire encoding, resize, routing, signal cleanup, or care bug: first add the smallest deterministic regression in the existing suite, then fix the owning production layer and rerun focused + live tests. Never patch the harness to hide a product defect.

- [ ] **Step 8: Commit harness and provisional evidence**

```bash
git add -- scripts/verify-terminal-room-live.sh docs/verification/terminal-room.md
git commit -m "test: add real codex terminal acceptance harness"
```

Commit any product fix separately before evidence. No push.

---

### Task 8: Final Visual Review, Fresh Matrix, and Release Evidence

**Files:**
- Replace/update: `docs/mockups/current/README.md` and its six PNGs
- Replace/update selected final screenshots and README under `docs/verification/terminal-room/`
- Modify: `docs/verification/terminal-room.md`
- Modify if final web embedding changed: `crates/codegotchi-cli/web-dist/`

**Interfaces:**
- Consumes: Tasks 1-7, canonical reference manifest, hosted CI after authorized push.
- Produces: one final integrated release record whose PASS/FAIL claims match fresh evidence.

- [ ] **Step 1: Record clean integrated source SHA**

```bash
git status --short
git rev-parse HEAD
```

Expected: clean worktree. Record SHA in ledger and generated evidence.

- [ ] **Step 2: Run the complete local matrix fresh**

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

Expected: every command exits `0`. A Playwright retry/flaky classification is not a clean release PASS.

- [ ] **Step 3: Repeat the former idle flake ten times**

Use Task 3's focused command against the integrated production bundle. Expected: 10/10 PASS, no retry.

- [ ] **Step 4: Recapture the six `docs/mockups/current` images**

Use canonical sizes/states and update README with final source SHA, exact fixture state/theme/host polarity, pixel dimensions, SHA-256, and direct inspection result. Git history preserves the old baseline.

- [ ] **Step 5: Commit final Auto and live visual evidence**

Under `docs/verification/terminal-room/`, retain Auto-light, Auto-dark, and the populated live-Codex Full capture in addition to Full named-preset, Compact, Minimal, bed-sleep, and floor-doze evidence.

- [ ] **Step 6: Dispatch an independent image-capable reviewer**

Give the reviewer only:

```text
all canonical references in docs/mockups/terminal-room/
all fresh final current-state captures
Auto light + Auto dark captures
populated live-Codex Full capture
visual sections of both normative specs
```

Require explicit scoring of Full composition/layering, mascot scale/silhouette/face, furniture balance, status/care hierarchy, Compact vignette, Minimal pet/care readability, Auto contrast, named preset contrast, half-block seams/clipping, hit-region correspondence, bed-vs-doze, degradation order, and real-Codex dominance.

Any high-severity mismatch is FAIL and returns to the owning visual task. Never change `PENDING_VISION_REVIEW` to PASS from automated tests alone.

- [ ] **Step 7: Controller push and hosted-CI gate**

Workers stop before push. After explicit controller/user authorization, push final implementation commits and require a fresh workflow on that exact source SHA:

```text
Rust checks (Ubuntu): PASS
Rust checks (macOS): PASS including PTY smoke
Web checks: PASS
```

Record run/job IDs in `docs/verification/terminal-room.md`; never cite an older green run for a newer SHA.

- [ ] **Step 8: Rewrite final verification from evidence**

`docs/verification/terminal-room.md` contains final source SHA, local matrix results, hosted run IDs/results, real Codex version/invocation/checklist, responsive/care results, visual reviewer verdict/evidence filenames, known non-blocking limitations, and one unambiguous final status: PASS or FAIL. Old failed attempts may remain only as clearly historical evidence.

- [ ] **Step 9: Final whole-branch review**

Dispatch a fresh high-judgment reviewer over `master...HEAD`, both specs, this plan, final CI evidence, and visual evidence. Search specifically for dropped spec requirements, optimistic care mutations, Codex text scraping/keyboard theft, art/hitbox mismatch, platform cfg gaps, unbounded cleanup/waits, stale evidence, and browser regressions.

Allow one final scoped fix wave and one scoped re-review; then adjudicate residual findings explicitly in the SDD ledger.

- [ ] **Step 10: Commit final evidence**

```bash
git add -- docs/mockups/current docs/verification/terminal-room docs/verification/terminal-room.md
git add -- crates/codegotchi-cli/web-dist 2>/dev/null || true
git commit -m "docs: record terminal room release verification"
```

Inspect staged diff. No push from a worker.

- [ ] **Step 11: Prepare but do not apply PR metadata**

If final status is PASS, write a recommended PR title/body into the SDD report summarizing implementation, platform support, automated gates, real-Codex evidence, and visual acceptance. Only the controller/user may update PR #2.

---

## Final Definition of Done

The branch is merge-ready only when all items are true on the same integrated source state:

- [ ] Hosted Ubuntu Rust is green.
- [ ] Hosted macOS Rust is green and executes the production-shared pseudo-terminal smoke.
- [ ] Native macOS browser launch uses `/usr/bin/open`; Browser/Both/Auto fallback is not Linux-only.
- [ ] Hosted web CI is green.
- [ ] `cargo test --workspace` is green on ordinary Linux/WSL verification.
- [ ] Production Playwright is green with no flaky classification; idle choreography is deterministic under test-controlled time.
- [ ] Second-left-down and wrong-button-drag regressions are green for pet and food capture.
- [ ] Evidence-bearing `PetStroke` domain validation remains intact.
- [ ] Auto uses ordered default foreground/background dithering and is visually reviewed on light and dark hosts.
- [ ] Full no longer reads as sparse line art; a large round expressive mascot is focal and bedroom furniture/surfaces are recognizable.
- [ ] Compact reads as a purpose-built pet vignette rather than status/debug rows.
- [ ] Minimal visibly contains a pet sprite plus concise needs/core care rather than a text-only strip.
- [ ] Bed sleep and floor doze remain unmistakably distinct with unchanged authority semantics.
- [ ] Every visible care target matches hit geometry after visual changes.
- [ ] A real official Codex session proves prompt/editing, paste, focus where observable, mouse behavior where enabled, tool activity, approval/review where available, Full -> Compact -> Minimal -> Full resize, pet/feed/clean/nap, and clean exit/restoration.
- [ ] At least one final live screenshot visibly contains the real populated Codex TUI above the final room.
- [ ] An independent image-capable reviewer reports no high-severity mismatch against canonical references.
- [ ] `docs/verification/terminal-room.md` describes final HEAD truthfully and no longer reports resolved historical gates as current blockers.
- [ ] Final whole-branch review is complete with load-bearing findings fixed or explicitly adjudicated.

Only then should the controller consider updating PR #2's design-era metadata and making it merge-ready.
