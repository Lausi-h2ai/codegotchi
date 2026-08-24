# Codegotchi Visual Clarity Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Codegotchi immediately recognizable, mischievous, and endearing while simplifying the terminal room into a quiet frame around the mascot.

**Architecture:** Retain the existing half-block logical-pixel renderer, semantic four-tone palette, room heights, authoritative care flow, and Full/Compact/Minimal entry points. Re-author the three sprite families around a silhouette-first cat-like fantasy pet, simplify the room compositor and geometry, and replace pixel-density acceptance with explicit clarity, separation, and direct visual-review gates.

**Tech Stack:** Rust, Ratatui, Crossterm, deterministic terminal fixture, xterm/Xvfb, ImageMagick

**Spec:** `docs/superpowers/specs/2026-08-13-terminal-room-design.md`, refined by the visual decisions recorded in this plan

## Global Constraints

- Preserve the official Codex PTY pane, simulation authority, persistence, care validation, CLI arguments, and browser projection unchanged.
- Preserve `FULL_ROOM_HEIGHT = 14`, `COMPACT_ROOM_HEIGHT = 7`, the existing layout thresholds, and the rule that Codex wins constrained vertical space.
- Preserve `pet_sprite`, `pet_sprite_compact`, `pet_sprite_minimal`, `room_geometry`, `room_geometry_with_frame`, and `render_room_with_options` as the renderer interfaces.
- Preserve four distinct food sources and hitboxes, poop targets, bed sleep, floor doze, affection, snack, drag/drop, petting, and wandering behavior.
- Use a cat-like fantasy silhouette: extremely round body, unmistakable ears, paws, tail, eyes, and mouth. The neutral personality is playful mischief, expressed by one cocked ear, wide eyes, a crooked grin, and a raised tail.
- Use Tone3 for the primary mascot silhouette and facial emphasis, Tone0 for negative-space facial features, and at most one contiguous Tone2 marking. Do not use Tone1 or checkerboard texture inside the pet.
- Keep scenery low contrast, care targets medium contrast, and the mascot plus immediate feedback at the strongest contrast. Remove random backdrop texture.
- Preserve unrelated working-tree changes. Before any process cleanup, follow the repository-root `AGENTS.md`; never match or kill bare `codex` processes.

---

### Task 1: Re-author the Full mascot pose family

**Files:**
- Modify: `crates/codegotchi-cli/src/terminal/sprites.rs`
- Test: `crates/codegotchi-cli/src/terminal/sprites.rs`
- Test: `crates/codegotchi-cli/tests/terminal_room.rs`

**Interfaces:**
- Consumes: `PetPose`, `SemanticTone`, `pet_sprite(pose) -> &'static [&'static str]`, and the existing `18 x 14` logical canvas.
- Produces: a silhouette-first Full sprite for all thirteen `PetPose` variants without changing function signatures.

- [ ] **Step 1: Add failing structural sprite tests**

  Add test helpers beside the existing sprite tests that count `'.'`, `'o'`, and `'#'`, compute occupied bounds, and flood-fill occupied cells using four-way adjacency. Add assertions for every Full pose:

  ```rust
  for pose in ALL_POSES {
      let sprite = pet_sprite(pose);
      assert_eq!(sprite.len(), 14, "{pose:?} height");
      assert!(sprite.iter().all(|row| row.chars().count() == 18));
      assert_eq!(tone_count(sprite, '.'), 0, "{pose:?} must not use Tone1");
      assert!(tone_count(sprite, 'o') * 5 <= occupied_count(sprite));
      assert_eq!(primary_component_count(sprite), 1, "{pose:?} silhouette");
  }
  ```

  Treat detached reaction particles as room effects, not sprite pixels, so the mascot itself remains one connected component. Add pose-specific assertions that idle/blink share occupied bounds, floor doze is wider than tall after packing, and bed sleep fits the existing 18-column canvas.

- [ ] **Step 2: Run the sprite tests and verify RED**

  Run: `cargo test -p codegotchi-cli terminal::sprites::tests -- --nocapture`

  Expected: FAIL because the current sprites contain extensive Tone2 texture and do not satisfy the new silhouette contract.

- [ ] **Step 3: Replace the thirteen Full sprites**

  Keep each pose at exactly 18 logical columns by 14 logical rows. Use these pose rules:

  - `Idle`: round upright body, two pointed ears with the left ear cocked, two open negative-space eyes, crooked smile, two grounded feet, raised attached tail.
  - `Blink`: identical occupied silhouette to Idle; replace only the open eyes with horizontal closed eyes.
  - `WalkA`/`WalkB`: preserve face and body mass; alternate the two feet by one logical row and swing the attached tail between two positions.
  - `Sit`: show two front paws against the lower body and curl the tail along one side.
  - `Curious`: emphasize the cocked ear and raised tail tip while retaining both readable eyes and the grin.
  - `Happy`: use crescent eyes, a broader smile, and a high tail.
  - `Petted`: use closed happy eyes, the broad smile, lowered shoulders, and a relaxed high tail; draw hearts as room effects rather than disconnected sprite pixels.
  - `Upset`: lower both ears, use a downturned mouth, and tuck the tail against the body.
  - `Eating`: lower the face toward the food side while retaining one eye, one ear, the mouth, paws, and attached tail.
  - `Yawn`: use half-lidded eyes and one large negative-space open mouth.
  - `Doze`: create a horizontal curled floor silhouette with closed eyes and the tail wrapped into the body.
  - `Sleep`: create a compact curled silhouette with closed eyes and wrapped tail that remains visibly distinct from Doze when placed in the bed.

  Use Tone2 only for one contiguous lower-center belly patch and omit it entirely where it weakens a small expression.

- [ ] **Step 4: Run focused tests and verify GREEN**

  Run: `cargo test -p codegotchi-cli terminal::sprites::tests -- --nocapture`

  Run: `cargo test -p codegotchi-cli --test terminal_room full_mascot -- --nocapture`

  Expected: all focused sprite and Full mascot tests pass.

- [ ] **Step 5: Inspect the Full mascot patch**

  Run: `git diff -- crates/codegotchi-cli/src/terminal/sprites.rs crates/codegotchi-cli/tests/terminal_room.rs`

  Expected: only sprite contract helpers, assertions, and the thirteen Full sprite grids belong to this task; no room geometry or care behavior changes.

---

### Task 2: Give Compact and Minimal complete purpose-built mascots

**Files:**
- Modify: `crates/codegotchi-cli/src/terminal/sprites.rs`
- Modify: `crates/codegotchi-cli/src/terminal/room.rs`
- Test: `crates/codegotchi-cli/tests/terminal_room.rs`

**Interfaces:**
- Consumes: `pet_sprite_compact`, `pet_sprite_minimal`, `COMPACT_PET_WIDTH`, `COMPACT_PET_HEIGHT`, `MINIMAL_PET_WIDTH`, and `MINIMAL_PET_HEIGHT`.
- Produces: complete `12 x 10` Compact sprites and independent `9 x 6` Minimal expression families contained by their existing five-row and three-row hitboxes.

- [ ] **Step 1: Add failing responsive-mascot tests**

  Replace the single exact Minimal idle assertion with tests covering semantic pose families:

  ```rust
  for pose in ALL_POSES {
      assert_eq!(pet_sprite_compact(pose).len(), 10);
      assert!(pet_sprite_compact(pose)
          .iter()
          .all(|row| row.chars().count() == 12));
      assert_eq!(pet_sprite_minimal(pose).len(), 6);
      assert!(pet_sprite_minimal(pose)
          .iter()
          .all(|row| row.chars().count() == 9));
  }
  ```

  Assert that Compact idle has two ear peaks, two eye gaps, a mouth gap, a grounded bottom edge, and an attached tail. Assert distinct Minimal grids for neutral, blink/sleep, happy/petted, upset, yawn, and eating families. In the integration suite, assert every non-empty mascot cell stays inside `geometry.pet` at widths `24`, `32`, `40`, `80`, and `120`.

- [ ] **Step 2: Run responsive tests and verify RED**

  Run: `cargo test -p codegotchi-cli minimal_ -- --nocapture`

  Run: `cargo test -p codegotchi-cli compact_ -- --nocapture`

  Expected: FAIL because Minimal currently collapses most poses to one sprite and the current compact art violates the new tone/feature contract.

- [ ] **Step 3: Re-author Compact sprites**

  Draw every Compact pose independently on the existing 12×10 logical canvas. Preserve the Full pose semantics but simplify the belly marking before removing ears, eyes, mouth, feet, or tail. Keep the complete five terminal-row result visible; do not rely on clipping.

- [ ] **Step 4: Expand the Minimal pose mapping**

  Keep `pet_sprite_minimal(pose) -> &'static [&'static str; 6]`, but add dedicated constants and mappings for:

  - neutral movement: Idle, WalkA, WalkB, Sit, Curious;
  - closed eyes: Blink, Doze, Sleep;
  - positive: Happy, Petted;
  - negative: Upset;
  - open mouth: Yawn;
  - food-facing: Eating.

  Every 9×6 grid must show two ears, two facial anchors appropriate to the expression, a mouth or explicit closed-mouth mark, and a grounded head-and-paws silhouette. Update `minimal_pet_sprite` and geometry only if required to remove existing clipping; retain the 9×3 terminal-cell hitbox.

- [ ] **Step 5: Run responsive tests and verify GREEN**

  Run: `cargo test -p codegotchi-cli minimal_ -- --nocapture`

  Run: `cargo test -p codegotchi-cli compact_ -- --nocapture`

  Expected: all responsive sprite, containment, target, and narrow-width tests pass.

---

### Task 3: Simplify the Full and Compact room hierarchy

**Files:**
- Modify: `crates/codegotchi-cli/src/terminal/room.rs`
- Test: `crates/codegotchi-cli/tests/terminal_room.rs`

**Interfaces:**
- Consumes: `full_geometry`, `compact_geometry`, `full_wide_furniture_layout`, `render_room_backdrop`, the existing food/poop/bed renderers, and the sprite families from Tasks 1–2.
- Produces: a quiet Full room and reduced Compact vignette with unchanged authoritative target identities and room-mode thresholds.

- [ ] **Step 1: Replace density assertions with failing hierarchy assertions**

  Delete `full_room_has_layered_object_density_beyond_outline_boxes`. Add tests that:

  ```rust
  assert_eq!(random_backdrop_fill_count(&buffer, area), 0);
  assert!(largest_tone3_component_is_inside(&buffer, geometry.pet));
  assert!(tone3_count_outside_targets(&buffer, &geometry) < 48);
  assert!(has_two_column_clearance_around_pet(&buffer, geometry.pet));
  ```

  Exclude the title/status row and labeled care regions when counting high-tone cells. Retain the existing non-overlap tests across widths `80..=121`, but update furniture markers to the simplified silhouettes instead of requiring dense shelf fills. Add an assertion that Compact retains one subdued window cue but no plant or desk competes with the pet.

- [ ] **Step 2: Run room hierarchy tests and verify RED**

  Run: `cargo test -p codegotchi-cli --test terminal_room full_room -- --nocapture`

  Run: `cargo test -p codegotchi-cli --test terminal_room compact_ -- --nocapture`

  Expected: FAIL because the current backdrop adds random blocks/dots, furniture uses excessive filled pixels, and the Compact scene retains competing decoration.

- [ ] **Step 3: Remove ambient texture and simplify furniture**

  Make `render_room_backdrop` draw only the room background reset, one low-contrast wall/floor divider, and an optional sparse day/night cue. Remove the row 7–10 block/dot loops.

  Re-author Full wide furniture into these anchors:

  - left: a sparse combined window/desk silhouette;
  - left-center: one shelf with a single plant cue;
  - center: open floor reserved for care objects and roaming;
  - right-center: the mascot plus two columns of visual clearance;
  - right: the existing bed, simplified to outline, pillow, blanket, and one `BED` label.

  Remove the wide wardrobe and repeated decorative plants. Use Tone1 for scenery strokes and Tone2 only for essential furniture edges or labels. Do not use Tone3 fills in decorative furniture.

- [ ] **Step 4: Group and differentiate care objects**

  Keep four `FoodSource` entries and drag IDs, but lay them out as one pantry/tray zone. Give each a distinct sparse silhouette: kibble bowl, treat jar, fruit, and energy can. Preserve the existing labels and counts below or beside the corresponding hitbox.

  Replace repeated filled poop blocks with a small coiled three-row glyph and retain one `POOP` label per authoritative target. Render affection/snack feedback near the status strip or pet without adding disconnected pixels to the pet sprite.

  In Compact, keep the full mascot, status rows, care targets, bed, and one low-tone window. Remove the plant and all Full-only furniture.

- [ ] **Step 5: Align geometry with the calmer composition**

  Update `full_geometry`, `compact_geometry`, food anchors, poop anchors, and furniture slots only as needed so every visible object remains inside its hitbox, furniture never overlaps care targets, and the non-sleeping pet retains two columns of clear space at supported wide widths. Preserve offset-aware wandering and bed-sleep hitbox behavior.

- [ ] **Step 6: Run room and interaction tests and verify GREEN**

  Run: `cargo test -p codegotchi-cli --test terminal_room -- --nocapture`

  Expected: all room geometry, rendering, theme, sleep-state, demand, food, poop, drag, responsive, and hierarchy tests pass.

---

### Task 4: Make recognizability part of visual acceptance

**Files:**
- Modify: `crates/codegotchi-cli/examples/terminal_room_fixture.rs`
- Modify: `docs/mockups/current/README.md`
- Modify: `docs/verification/terminal-room/README.md`
- Replace after review: the six PNGs under `docs/mockups/current/`
- Replace after review: the matching eight final PNGs under `docs/verification/terminal-room/`

**Interfaces:**
- Consumes: `CG_FIXTURE_LAYOUT`, `CG_FIXTURE_THEME`, `CG_FIXTURE_TIME_OF_DAY`, `CG_FIXTURE_SLEEP`, and `PresentationFrame { pose, offset }`.
- Produces: `CG_FIXTURE_POSE=idle|blink|walk-a|walk-b|sit|doze|sleep|yawn|curious|happy|upset|eating|petted|all`, where `all` cycles deterministically through every pose for capture.

- [ ] **Step 1: Add failing fixture pose parsing tests**

  Extract a pure `parse_pose(&str) -> Result<PetPose, String>` helper and test every accepted spelling plus rejection of an unknown value. Add a pure `fixture_poses("all")` assertion returning all thirteen poses in enum order.

- [ ] **Step 2: Run fixture tests and verify RED**

  Run: `cargo test -p codegotchi-cli --example terminal_room_fixture -- --nocapture`

  Expected: FAIL until pose parsing and deterministic pose sequencing exist.

- [ ] **Step 3: Add deterministic pose capture support**

  Read `CG_FIXTURE_POSE`, default it to `idle`, and render `PresentationFrame { pose, offset: (0, 0) }`. For `all`, clear and redraw one Full frame per pose using the existing bounded `CG_FIXTURE_PAUSE_MS`; do not add a production API or bypass `render_room_with_options`.

- [ ] **Step 4: Run build and complete automated verification**

  Run: `cargo fmt --check`

  Run: `cargo clippy -p codegotchi-cli --all-targets -- -D warnings`

  Run: `cargo test -p codegotchi-cli`

  Run: `cargo build -p codegotchi-cli --example terminal_room_fixture`

  Expected: all commands exit zero with no formatting, lint, test, or build failures.

- [ ] **Step 5: Capture the required visual matrix**

  Using the existing isolated Xvfb/xterm fixture procedure documented in `docs/verification/terminal-room/README.md`, recapture at native size:

  - Full SoftGreen/day awake, 120×45;
  - Full Night/night awake, 120×45;
  - Compact SoftGreen/day awake, 120×30;
  - Minimal SoftGreen/day awake, 120×21;
  - Full SoftGreen/night authoritative bed sleep, 120×45;
  - Full SoftGreen/day floor doze, 120×45;
  - Full Auto/day on a light host, 120×45;
  - Full Auto/night on a dark host, 120×45;
  - each of the thirteen Full poses using `CG_FIXTURE_POSE`.

  Do not use the live smoke script for lower-pane art capture and do not clean up processes by name. If a fixture process appears stuck, identify the current session ancestor/descendant tree and inspect concrete run-owned markers before considering TERM.

- [ ] **Step 6: Perform direct visual adjudication before replacing evidence**

  Open every candidate at original resolution and compare it with:

  - `docs/verification/terminal-room/task5-fix1-restored-full-120x45.png` for earlier production clarity;
  - `docs/mockups/terminal-room/a4835b9b-53d0-467f-8189-a708eea397eb.png` for mascot-led hierarchy;
  - `docs/mockups/terminal-room/ChatGPT Image 13. Aug. 2026, 22_43_21 (1).png` for silhouette and pose language.

  Reject and revise the art unless all of these are true:

  - the pet reads as cat-like within one glance at native size;
  - idle reads as playful/mischievous and endearing;
  - eyes and mouth are clear in named and Auto themes;
  - happy, upset, eating, floor doze, and bed sleep are distinguishable without labels;
  - the mascot is the strongest focal element and furniture is visibly subordinate;
  - the room remains recognizable but visually calm;
  - all care targets remain discoverable and visually distinct from the pet;
  - Compact and Minimal show complete, uncropped mascots;
  - no seam, clipping, collision, or capture artifact is visible.

- [ ] **Step 7: Replace evidence and document provenance**

  Only after the visual gate passes, replace the current/final PNG sets and update both READMEs with the exact source SHA, capture environment, dimensions, SHA-256 hashes, fixture variables, and the recognizability adjudication. Do not describe the result merely as technically clean; record the mascot and hierarchy criteria explicitly.

- [ ] **Step 8: Inspect the final change set**

  Run: `git diff --check`

  Run: `git status --short`

  Run: `git diff --stat`

  Expected: no whitespace errors; only the sprite, room, tests, fixture, approved evidence, and visual documentation files belong to this refresh, alongside any unrelated pre-existing user changes.
