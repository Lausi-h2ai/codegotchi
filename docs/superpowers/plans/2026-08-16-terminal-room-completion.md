# Terminal Room Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the terminal-room branch into full compliance with the terminal-room design and hardening specifications, restore a green deterministic CI baseline, close remaining authority and browser-care gaps, finish missing theme/visual/platform work, and produce complete release evidence.

**Architecture:** Preserve the existing architecture: the official Codex TUI remains PTY-backed and authoritative runtime state remains the only source of truth for care state. Fix regressions at their owning boundary instead of bypassing tests, keep terminal animation/presentation state separate from authoritative care transitions, and treat visual acceptance plus real-Codex terminal behavior as first-class release gates rather than optional polish.

**Tech Stack:** Rust workspace, `portable-pty 0.9.0`, `vt100 0.16.2`, `ratatui 0.30.2`, `crossterm 0.29.0`, React/TypeScript web client, Playwright, GitHub Actions, official Codex CLI.

## Global Constraints

- Normative design: `docs/superpowers/specs/2026-08-13-terminal-room-design.md`.
- Normative override when conflicts exist: `docs/superpowers/specs/2026-08-13-terminal-room-agent-hardening.md`.
- Existing implementation plans remain historical inputs: `docs/superpowers/plans/2026-08-13-terminal-room.md` and `docs/superpowers/plans/2026-08-13-terminal-room-agent-hardening.md`.
- Official Codex CLI remains the actual TUI in the upper PTY-backed pane; do not replace or reimplement it.
- Runtime state remains authoritative. Terminal and browser projections must not optimistically mutate needs, inventory, demands, poop, or enforcement state.
- Petting qualification remains exactly `1500 ms` minimum duration and `120` backend distance units, with terminal conversion `POINTER_DISTANCE_PER_CELL = 16.0` and Euclidean cell-path accumulation.
- Generic `PetBehavior::Sleeping` must never imply authoritative bed sleep. Authoritative bed sleep requires a future `napping_until` relative to `last_updated_at`.
- Initial platform claim remains Linux, WSL, and macOS unless the spec is explicitly amended before release.
- Initial rendering presets required by the design are `auto`, `mono`, `soft-green`, `amber`, and `night`.
- Full room, Compact room, and Minimal room must remain functional and care-capable while Codex receives layout priority.
- Browser mode remains a supported fallback/second projection.
- Visual acceptance requires image-based inspection of canonical screenshots. ANSI/text snapshots alone are insufficient.
- Do not mark the branch complete until the full automated matrix passes from the final integrated HEAD and the final whole-branch review is complete.

---

## Audit Findings to Close

### P0 — Branch is not releasable while CI is red

The latest audited PR head (`7bf52cbc9cf5a2ff1df19645e7b42d3ff331ea22`) failed both Rust and web CI.

- Rust failure: `terminal::session::tests::persistent_compositor_resize_clears_stale_style_from_default_cells` reaches a non-hermetic terminal/backend path on clean GitHub Actions and errors with `No such file or directory`.
- Web failure: sustained browser petting does not clear the affection demand in Playwright.
- Because the browser care suite is serial, the later snack/poop, severe-happiness-denial, and overdue-catchup scenarios do not all execute after the first failure.
- Retry state is contaminated because the first browser test assumes the default fixture but does not explicitly reset it.

### P0 — Mandatory visual review is still explicitly incomplete

`docs/verification/terminal-room/README.md` is marked `PENDING_VISION_REVIEW`. Current screenshots are capture evidence, not acceptance evidence.

Missing or incomplete final visual evidence includes:

- current-art dark Full screenshot,
- authoritative bed-sleep screenshot,
- generic idle-doze screenshot,
- final image review of Full/Compact/Minimal composition,
- light/default and dark contrast review,
- half-block seam review,
- hit-region/affordance review,
- degradation review against canonical references.

### P1 — Continuous terminal petting weakens the authority boundary

`CareCommand::PetStroke { action_id }` currently raises authoritative happiness without carrying the duration/distance evidence required by the core petting contract. Terminal input locally decides when the stroke is qualified and the domain trusts that decision.

Continuous feedback is desirable, but authoritative happiness changes must still be validated at the runtime/domain boundary. The final pointer-up `Pet` action must remain the action that resolves the oldest affection demand.

### P1 — Required theme presets are missing

`crates/codegotchi-cli/src/terminal/theme.rs` currently implements semantic tones and Auto styling only. The design requires initial presets `auto`, `mono`, `soft-green`, `amber`, and `night`, plus a user-selectable preset surface.

### P1 — Real-Codex terminal fidelity is only partially evidenced

Current evidence shows real Codex launch, prompt/reply activity, broad activity reflection, and clean exit. The spec/hardening acceptance matrix additionally requires explicit evidence for input-mode fidelity and responsive behavior, including paste, focus where observable, keyboard navigation, scrolling/clicking while Codex mouse reporting is active, and Full → Compact → Minimal → Full resizing.

### P1 — macOS support is claimed by the design but not verified

Linux/WSL cleanup behavior is evidenced. The verification notes explicitly defer the macOS terminal-session acceptance path. CI currently runs only on Ubuntu.

Either implement and verify the macOS lifecycle path or amend the supported-platform claim before release. Do not leave implementation and documentation contradictory.

### P2 — Documentation and repository hygiene are stale

- Root `README.md` still documents the pre-terminal launcher/browser-first flow and does not explain `--ui auto|terminal|browser|both`.
- PR #2 title/body still describe a design-only branch even though the branch now contains the complete implementation attempt.
- `docs/verification/terminal-room.md`, referenced by the original plan as the final evidence record, is absent.
- Windows alternate-data-stream files such as `*:Zone.Identifier` are committed next to mockup/reference images and should be removed and ignored.

### P2 — Full-room ambience is incomplete

The Full room contains the required furniture, but the window is static. The design calls for simple day/night ambience in Full mode.

---

## File Map

### Runtime / care authority

- Modify: `crates/codegotchi-core/src/care.rs` — authoritative care command validation and transitions, including continuous petting semantics.
- Modify: runtime module exposing `AuthoritativeRuntime::pet`, `pet_stroke`, or the equivalent current methods — ensure every authoritative happiness mutation is domain-validated.
- Test: existing core/runtime care tests plus new pet-stroke boundary tests.

### Terminal input / rendering / session

- Modify: `crates/codegotchi-cli/src/terminal/input.rs` — terminal gesture evidence generation only; no unvalidated authoritative state mutation.
- Modify: `crates/codegotchi-cli/src/terminal/session.rs` — hermetic compositor resize behavior/tests and continuous-petting dispatch.
- Modify: `crates/codegotchi-cli/src/terminal/theme.rs` — typed presets and style mapping.
- Modify: `crates/codegotchi-cli/src/terminal/room.rs` — selected theme use and Full-mode ambience.
- Modify if necessary: launcher/CLI argument module defining `UiMode` — add terminal theme selection without changing existing `--ui` semantics.
- Test: `crates/codegotchi-cli/tests/terminal_input.rs`.
- Test: `crates/codegotchi-cli/tests/terminal_room.rs`.
- Test: `crates/codegotchi-cli/tests/terminal_session.rs` and/or existing inline session tests.

### Browser care

- Modify: web pet gesture component containing `data-testid="pet"` pointer handlers.
- Modify: browser E2E fixture/reset support if required for deterministic per-test state.
- Test: Playwright care-pressure suite containing `resolves an affection demand with a real sustained petting gesture`.

### CI / docs / evidence

- Modify: `.github/workflows/ci.yml` — add macOS verification if platform support remains in scope.
- Modify: `README.md` — current launcher modes, terminal behavior, care controls, browser fallback.
- Modify: `.gitignore` — prevent future `Zone.Identifier` files.
- Create: `docs/verification/terminal-room.md` — final machine + manual acceptance record.
- Update: `docs/verification/terminal-room/README.md` — transition from pending captures to reviewed evidence only after actual image inspection.
- Remove: committed `docs/mockups/terminal-room/*:Zone.Identifier` and any other tracked `*:Zone.Identifier` files.

---

### Task 1: Restore a Green, Hermetic Baseline

**Files:**
- Modify: `crates/codegotchi-cli/src/terminal/session.rs`
- Test: existing inline test `persistent_compositor_resize_clears_stale_style_from_default_cells`
- Modify only if needed: terminal test helper / in-memory writer used by session tests

**Interfaces:**
- Consumes: current persistent compositor and resize implementation.
- Produces: a resize regression test that exercises only in-memory/test-owned terminal state and does not require `/dev/tty`, a real PTY, or process-global terminal state.

- [ ] **Step 1: Reproduce the exact failing Rust test locally in CI-like conditions**

```bash
cargo test --workspace terminal::session::tests::persistent_compositor_resize_clears_stale_style_from_default_cells -- --nocapture
```

Expected before the fix: failure or evidence that the test path reaches a real terminal/backend resource instead of remaining purely in memory.

- [ ] **Step 2: Make the regression test explicitly hermetic**

Keep the assertion unchanged in meaning: draw styled text such as `STALE`, resize the persistent compositor, then assert the newly exposed/default cells are blank and carry default style.

The test must construct a backend/writer that owns its output buffer in memory. It must not call code that implicitly opens a real terminal. If production resize behavior itself requires a terminal operation, split the implementation into a pure state-resize function plus a thin terminal I/O wrapper and test the pure function here.

- [ ] **Step 3: Run the focused Rust test until it passes**

```bash
cargo test --workspace terminal::session::tests::persistent_compositor_resize_clears_stale_style_from_default_cells -- --nocapture
```

Expected: PASS on a non-interactive shell.

- [ ] **Step 4: Run the full Rust gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/codegotchi-cli/src/terminal/session.rs
git commit -m "test: make terminal resize regression hermetic"
```

---

### Task 2: Fix Browser Sustained Petting and Isolate Every E2E Scenario

**Files:**
- Modify: web component containing the `data-testid="pet"` pointer handlers
- Modify: browser care-pressure Playwright spec containing `resolves an affection demand with a real sustained petting gesture`
- Modify if needed: browser fixture/reset helper used by that suite

**Interfaces:**
- Consumes: browser pointer gesture `{duration_ms, distance}` and the runtime pet endpoint/action.
- Produces: deterministic per-test fixture initialization and a sustained gesture that reliably sends runtime-qualified evidence (`duration >= 1500`, `distance >= 120`) without relying on animation timing.

- [ ] **Step 1: Instrument the failing test to identify the broken boundary before changing product code**

Capture, in the Playwright test, the outgoing pet request/action and the post-action authoritative snapshot. Determine which of these is true:

1. no pet request is emitted,
2. request duration is below `1500`,
3. request distance is below `120`,
4. backend rejects/ignores an otherwise valid request,
5. backend accepts it but the UI assertion races the authoritative update.

Do not keep the instrumentation after the root cause is proven unless it improves future failure diagnostics.

- [ ] **Step 2: Make the first test explicitly initialize its own fixture**

At the beginning of every serial care-pressure test, call the existing fixture/reset endpoint/helper with that test's required state. The first test must no longer inherit whatever fixture a previous retry left behind.

- [ ] **Step 3: Add a failing browser test for the exact sustained gesture contract**

Use a gesture with generous deterministic margin rather than the threshold itself, for example:

```ts
await page.getByTestId('pet').hover();
await page.mouse.down();
await page.waitForTimeout(900);
await page.mouse.move(startX + 96, startY, { steps: 16 });
await page.waitForTimeout(900);
await page.mouse.move(startX + 96, startY + 96, { steps: 16 });
await page.mouse.up();
```

The exact coordinates can follow the existing test helper, but the resulting captured request must assert `duration_ms >= 1500` and `distance >= 120` before asserting the affection demand disappears.

- [ ] **Step 4: Fix the owning layer only**

If the request is missing or under-reports movement, fix the pointer-handler lifecycle. If the runtime accepts the request but the UI races, wait for the authoritative state transition instead of a presentation-only signal. Do not weaken backend thresholds and do not directly clear the demand in the browser.

- [ ] **Step 5: Prove serial isolation by running the suite normally and with retries**

```bash
corepack pnpm playwright:test
```

Then run the affected spec with retries enabled using the repository's Playwright CLI/configuration. Expected: the first test begins from the same fixture on initial run and retry, and all later tests execute.

- [ ] **Step 6: Commit**

```bash
git add <web-pet-component> <playwright-care-spec> <fixture-helper-if-changed>
git commit -m "fix: make browser petting and care e2e deterministic"
```

---

### Task 3: Restore Authoritative Validation for Continuous Petting

**Files:**
- Modify: `crates/codegotchi-core/src/care.rs`
- Modify: runtime module exposing continuous petting
- Modify: `crates/codegotchi-cli/src/terminal/input.rs`
- Modify: `crates/codegotchi-cli/src/terminal/session.rs`
- Test: core/runtime care tests
- Test: `crates/codegotchi-cli/tests/terminal_input.rs`

**Interfaces:**
- Consumes: cumulative gesture evidence generated by terminal input.
- Produces: a continuous-petting API in which every authoritative happiness mutation is validated against domain-owned gesture evidence; pointer-up `Pet` remains responsible for resolving the affection demand.

- [ ] **Step 1: Lock the desired semantics with failing domain tests**

Add tests for all of the following:

```text
1500 ms + 112 distance  -> reject/no authoritative happiness increase
1500 ms + 128 distance  -> continuous happiness increase allowed
1499 ms + 128 distance  -> reject/no authoritative happiness increase
non-finite/invalid distance -> reject
replayed action_id -> no duplicate authoritative increase
qualified gesture interrupted before final Pet -> must not resolve affection demand
```

Also explicitly decide and encode whether a qualified interrupted gesture may retain happiness already earned before interruption. Whichever behavior is chosen must be documented in `care.rs` and tested; only the final `Pet` may resolve the demand.

- [ ] **Step 2: Replace trust in frontend qualification with evidence-bearing validation**

Do not keep a domain command whose only proof is `PetStroke { action_id }`. Use one of these two acceptable shapes:

```rust
PetStroke {
    action_id: ActionId,
    duration_ms: u64,
    distance: f64,
}
```

or a domain-owned gesture/session token that can only be obtained after the runtime validates the same cumulative duration/distance evidence.

The runtime/domain layer must enforce the same `1500` / `120` contract used by final `Pet`.

- [ ] **Step 3: Keep terminal input responsible only for measurement**

`RoomInputSession` may continue to accumulate Euclidean cell path using `POINTER_DISTANCE_PER_CELL = 16.0` and emit periodic continuous-feedback requests after qualification, but the request must include evidence that the authoritative layer independently validates.

- [ ] **Step 4: Preserve exact terminal distance regressions**

Run and retain tests proving:

```text
7 horizontal cells  = 112 -> below threshold
8 horizontal cells  = 128 -> above threshold
3x4 diagonal cells  = 80  -> Euclidean conversion
```

- [ ] **Step 5: Run focused and full tests**

```bash
cargo test --workspace terminal_input
cargo test --workspace care
cargo test --workspace
```

Expected: all PASS and no terminal-only code path can mutate authoritative happiness without domain validation.

- [ ] **Step 6: Commit**

```bash
git add crates/codegotchi-core/src/care.rs crates/codegotchi-cli/src/terminal/input.rs crates/codegotchi-cli/src/terminal/session.rs <runtime-file> <care-tests>
git commit -m "fix: validate continuous petting authoritatively"
```

---

### Task 4: Implement the Required Terminal Theme Presets and Full-Room Ambience

**Files:**
- Modify: `crates/codegotchi-cli/src/terminal/theme.rs`
- Modify: `crates/codegotchi-cli/src/terminal/room.rs`
- Modify: launcher/CLI argument file that currently defines `UiMode`
- Test: `crates/codegotchi-cli/tests/terminal_room.rs`
- Add/update CLI parsing tests where `--ui` is already tested

**Interfaces:**
- Produces: typed terminal theme selection with values `auto`, `mono`, `soft-green`, `amber`, `night`.
- Produces: a resolved four-tone palette passed into room rendering instead of calling Auto styling directly from individual draw sites.
- Produces: deterministic Full-room day/night ambience suitable for screenshot fixtures.

- [ ] **Step 1: Add failing parsing and palette tests**

Define a typed enum equivalent to:

```rust
pub enum TerminalThemePreset {
    Auto,
    Mono,
    SoftGreen,
    Amber,
    Night,
}
```

Add tests that every required string parses and that every preset resolves all semantic `Tone0..Tone3` values.

- [ ] **Step 2: Add an explicit user selection surface**

Use a terminal-specific option such as:

```text
--terminal-theme auto|mono|soft-green|amber|night
```

Default to `auto`. Preserve existing `--ui auto|terminal|browser|both` behavior.

If maintainers prefer a different public CLI spelling, amend the design/spec in the same commit before implementing the alternate spelling; do not leave the user-selection requirement implicit.

- [ ] **Step 3: Resolve theme once and pass semantic tones through rendering**

Move room drawing away from scattered `auto_style` calls. The room renderer should consume a resolved palette/style mapping so every sprite, border, status element, and furniture element follows the selected preset consistently.

- [ ] **Step 4: Add simple deterministic day/night window ambience in Full mode**

The Full room window should visibly differ between day and night while remaining within the four-tone art system. Derive ambience from a small presentation-only input such as fixture-controlled time-of-day; never let ambience alter care state.

Screenshot fixtures must be able to force day or night deterministically.

- [ ] **Step 5: Run focused terminal rendering tests**

```bash
cargo test --workspace terminal_room
cargo test --workspace
```

Expected: all presets parse/render, Auto remains the default, and Full mode contains deterministic ambience without changing Compact/Minimal care semantics.

- [ ] **Step 6: Commit**

```bash
git add crates/codegotchi-cli/src/terminal/theme.rs crates/codegotchi-cli/src/terminal/room.rs <cli-arg-file> crates/codegotchi-cli/tests/terminal_room.rs <cli-tests>
git commit -m "feat: complete terminal themes and room ambience"
```

---

### Task 5: Complete PTY Fidelity and Platform Acceptance

**Files:**
- Modify only if acceptance exposes defects: `crates/codegotchi-cli/src/terminal/pty.rs`
- Modify only if acceptance exposes defects: `crates/codegotchi-cli/src/terminal/screen.rs`
- Modify only if acceptance exposes defects: `crates/codegotchi-cli/src/terminal/session.rs`
- Modify: `.github/workflows/ci.yml`
- Update: `docs/verification/terminal-room-codex-pty.md`
- Create/update: `docs/verification/terminal-room.md`

**Interfaces:**
- Consumes: Codex-negotiated terminal mode state already tracked by the virtual screen.
- Produces: verified input encoding and cleanup behavior across claimed platforms.

- [ ] **Step 1: Add/confirm automated mode-driven input tests**

The automated test matrix must cover at least:

```text
application cursor keys enabled/disabled
bracketed paste enabled/disabled
focus reporting enabled/disabled
mouse tracking mode changes
mouse encoding mode changes
unknown/unsupported terminal mode degrades only that feature
```

Tests must verify encoded bytes/events from virtual-screen mode state and must not parse visible Codex text to infer activity.

- [ ] **Step 2: Add macOS CI if macOS remains a supported initial platform**

Add a macOS Rust job that runs at minimum:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Platform-specific lifecycle tests must exercise clean child/PTY termination without Linux-only assumptions.

If the implementation cannot support macOS yet, update the design/spec and README to remove macOS from the initial support claim before merging. Do not silently skip the platform.

- [ ] **Step 3: Run and record the real Codex fidelity checklist**

Using the official Codex CLI version recorded in verification docs, explicitly verify and record:

```text
prompt entry and model reply
normal keyboard navigation
bracketed paste path
focus reporting where observable
Codex scrolling while mouse reporting is active
Codex clicking while mouse reporting is active
slash command interaction
tool output / approval flow where available
resize Full -> Compact -> Minimal -> Full
terminal petting while Codex remains usable
feeding
poop cleaning
nap
clean exit with terminal restoration
```

- [ ] **Step 4: Fix only failures discovered by the matrix, with a regression test first**

Do not broaden PTY changes speculatively. Every production change in this task must correspond to a reproduced acceptance failure and a focused automated regression where practical.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/ci.yml crates/codegotchi-cli/src/terminal docs/verification/terminal-room-codex-pty.md docs/verification/terminal-room.md
git commit -m "test: complete terminal platform and codex fidelity gates"
```

---

### Task 6: Refresh User Documentation and Repository Hygiene

**Files:**
- Modify: `README.md`
- Modify: `.gitignore`
- Remove: all tracked `*:Zone.Identifier` files
- Update: PR #2 title/body after the branch documentation is committed

**Interfaces:**
- Produces: documentation matching the actual launcher and terminal-room behavior.

- [ ] **Step 1: Rewrite launcher documentation around the current UI modes**

Document:

```text
codegotchi run -- codex ...
codegotchi run --ui auto -- codex ...
codegotchi run --ui terminal -- codex ...
codegotchi run --ui browser -- codex ...
codegotchi run --ui both -- codex ...
```

Explain that `auto` prefers terminal integration when interactive initialization succeeds and otherwise falls back according to the implemented launcher behavior.

- [ ] **Step 2: Document terminal care controls**

Include concise instructions for:

```text
petting by sustained pointer gesture
feeding by dragging stocked food to the pet
cleaning poop
using the bed for explicit nap/energy recovery
Minimal-mode care affordances
browser fallback/second projection
terminal theme selection
```

- [ ] **Step 3: Remove Windows ADS metadata and prevent recurrence**

```bash
find . -name '*:Zone.Identifier' -print
```

Delete every tracked result, then add this ignore rule:

```gitignore
*:Zone.Identifier
```

Verify:

```bash
git ls-files | grep 'Zone.Identifier' && exit 1 || true
```

Expected: no tracked ADS metadata files remain.

- [ ] **Step 4: Update PR #2 metadata to describe the implementation branch**

Replace the design-only title/body with a summary that accurately states the branch now contains the terminal-room implementation, current verification status, and remaining/closed acceptance gates.

Do this only after the implementation work above is committed so the PR description reflects reality.

- [ ] **Step 5: Commit repository documentation/hygiene changes**

```bash
git add README.md .gitignore docs/mockups/terminal-room
git commit -m "docs: align terminal room usage and repository hygiene"
```

---

### Task 7: Complete Mandatory Visual Acceptance

**Files:**
- Update/add PNG evidence under `docs/verification/terminal-room/`
- Update: `docs/verification/terminal-room/README.md`
- Update: `docs/verification/terminal-room.md`

**Interfaces:**
- Consumes: deterministic terminal-room fixture and final integrated renderer.
- Produces: image-based acceptance evidence tied to the final candidate commit.

- [ ] **Step 1: Capture the canonical viewport matrix from the current integrated candidate**

Capture at least:

```text
Full    120x45 light/default
Full    120x45 dark
Compact 120x30
Minimal 120x21
Full authoritative bed sleep
Full generic idle doze
```

Use deterministic fixture state and record the exact commit SHA and capture command in the verification README.

- [ ] **Step 2: Perform actual image inspection, not text/ANSI inspection**

For every screenshot, inspect the image itself against the canonical references in `docs/mockups/terminal-room/`.

The reviewer must explicitly assess:

```text
room composition and proportions
pet silhouette/readability
bed, desk, shelf, wardrobe, plants, food, floor, poop
status/care readability
Full -> Compact -> Minimal degradation
light/default contrast
dark contrast
half-block seams/artifacting
mouse hit regions and visible affordances
bed sleep vs generic doze visual distinction
```

- [ ] **Step 3: Fix each visual defect and recapture the affected image**

Use the loop required by the hardening spec:

```text
start -> screenshot -> image inspection -> reference comparison -> fix -> recapture
```

Do not change `PENDING_VISION_REVIEW` until the final images have actually been reviewed after the last fix.

- [ ] **Step 4: Record visual acceptance**

Update `docs/verification/terminal-room/README.md` with:

```text
final candidate commit SHA
capture environment
canonical screenshot filenames
review result for each required visual dimension
known non-blocking differences from references, if any
reviewer identity/process
```

- [ ] **Step 5: Commit**

```bash
git add docs/verification/terminal-room docs/verification/terminal-room.md
git commit -m "docs: record terminal room visual acceptance"
```

---

### Task 8: Final Integrated Release Gate and Whole-Branch Review

**Files:**
- Modify only if a release-gate failure produces a new fix.
- Update: `docs/verification/terminal-room.md` with the final integrated result.

**Interfaces:**
- Consumes: all previous tasks merged into one candidate HEAD.
- Produces: a single auditable release decision for PR #2.

- [ ] **Step 1: Run the full automated matrix fresh from integrated HEAD**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
corepack pnpm test
corepack pnpm lint
corepack pnpm format:check
corepack pnpm build
node scripts/embed-web.mjs
corepack pnpm playwright:test
```

Use the repository's exact package scripts if names differ, but record the actual commands and results in `docs/verification/terminal-room.md`.

Expected: all PASS. No result from an earlier commit counts as the final gate.

- [ ] **Step 2: Confirm GitHub Actions is green on the pushed final SHA**

Required jobs must pass on the same commit that owns the final evidence. If macOS remains a claimed platform, the macOS verification job must also pass.

- [ ] **Step 3: Re-run the real Codex acceptance checklist on the final SHA**

At minimum confirm:

```text
official Codex TUI PTY works
prompt and response work
tool activity works
approval flow where available
resize Full -> Compact -> Minimal -> Full
petting works
feeding works
cleaning works
nap works
browser mode remains functional
clean exit restores terminal
```

- [ ] **Step 4: Perform a whole-branch spec review**

Review the complete PR diff, not only the latest commits, against both normative specs. Explicitly verify:

```text
runtime authority preserved
no autonomous care repair
nap vs doze semantics correct
activity mapping exhaustive
PTY input is mode-driven
pet distance conversion exact
all required theme presets present
all three layouts care-capable
browser fallback functional
visual review complete
platform claims match evidence
documentation matches actual behavior
```

- [ ] **Step 5: Record final status and commit evidence**

`docs/verification/terminal-room.md` must end with one unambiguous status:

```text
PASS — ready for merge
```

or

```text
FAIL — blocking items: <explicit list>
```

Do not mark PASS while any known P0/P1 item in this plan remains open.

```bash
git add docs/verification/terminal-room.md
git commit -m "docs: record terminal room final verification"
```

---

## Final Definition of Done

The terminal-room work is complete only when all items below are true on the same final PR head:

- [ ] Rust fmt, Clippy, and workspace tests pass.
- [ ] Web unit, lint, format, build/embed, and Playwright gates pass.
- [ ] Browser sustained petting resolves affection authoritatively and its E2E suite is fixture-isolated.
- [ ] Continuous terminal petting cannot change authoritative happiness without domain validation.
- [ ] Exact pet thresholds/conversion remain covered by regression tests.
- [ ] `auto`, `mono`, `soft-green`, `amber`, and `night` are implemented and user-selectable.
- [ ] Full room contains deterministic day/night ambience.
- [ ] Full, Compact, and Minimal layouts remain care-capable.
- [ ] Generic doze and authoritative bed sleep remain semantically and visually distinct.
- [ ] Real Codex input-mode fidelity checklist is completed and recorded.
- [ ] Linux/WSL acceptance is complete.
- [ ] macOS acceptance is complete, or the initial platform claim has been explicitly amended before merge.
- [ ] Final canonical screenshots are captured from the final candidate and image-reviewed.
- [ ] `PENDING_VISION_REVIEW` is removed only after completed visual acceptance.
- [ ] README matches the current launcher and care UX.
- [ ] No `*:Zone.Identifier` files remain tracked.
- [ ] PR title/body describe the implementation branch accurately.
- [ ] Final whole-branch review finds no uncovered requirement from either normative spec.
- [ ] GitHub Actions is green on the final commit.

## Recommended Execution Order

1. Task 1 — restore deterministic Rust CI.
2. Task 2 — restore browser care CI and E2E isolation.
3. Task 3 — close the authoritative petting architecture gap before adding more polish.
4. Task 4 — finish required theme/ambience features.
5. Task 5 — close PTY/platform/fidelity verification gaps.
6. Task 6 — refresh docs and repository hygiene.
7. Task 7 — perform final visual acceptance only after functionality is stable.
8. Task 8 — run all release gates from one final integrated SHA.

This ordering prevents expensive visual recapture before behavior stabilizes and ensures final screenshots/evidence belong to the actual merge candidate rather than an intermediate commit.
