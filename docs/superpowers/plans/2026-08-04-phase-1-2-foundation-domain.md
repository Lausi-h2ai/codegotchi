# CodeGotchi Phase 1–2 Foundation and Domain Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a verified Rust/TypeScript workspace and implement CodeGotchi's pure, deterministic pet domain for Phases 1 and 2 only.

**Architecture:** A Cargo workspace initially contains one dependency-isolated domain crate, while a pnpm workspace contains one minimal React/Vite application. The pet aggregate delegates time progression, poop generation, behavior selection, and work permission to explicit domain strategies and ports; infrastructure arrives in later phases.

**Tech Stack:** Rust 2024 edition, `chrono`, `serde`, `uuid`, `thiserror`; React, TypeScript, Vite, Vitest, Testing Library, ESLint, Prettier; GitHub Actions.

## Global Constraints

- Scope is Phase 1 Foundation and Phase 2 Pet Simulation only.
- The domain crate must not depend on Axum, SQLite, Tokio, subprocess execution, OS APIs, or frontend concerns.
- Need semantics are exact: hunger `0 = full, 100 = starving`; energy `0 = exhausted, 100 = fully rested`; happiness `0 = miserable, 100 = delighted`; cleanliness `0 = filthy, 100 = completely clean`.
- Clamp every need to `0..=100` after every transition.
- All authoritative transitions are deterministic and testable with an injected fake clock.
- Important state transitions must not depend on randomness.
- Persisted/external event representations contain `schema_version`; events and care actions contain IDs and are idempotent.
- Raw prompts, source contents, full output, and raw command text are absent from domain event metadata.
- Default enforcement mode is `Decorative`; `Strict` requires explicit selection.
- Do not add daemon, HTTP, WebSocket, SQLite, process wrapper, command proxy, hooks, MCP, or polished assets.
- Follow strict red-green-refactor TDD and record the failing and passing command for each behavior group.

---

### Task 1: Workspace foundation and core aggregate

**Files:**
- Create: `Cargo.toml`
- Create: `rustfmt.toml`
- Create: `.editorconfig`
- Create: `.gitignore`
- Create: `crates/codegotchi-domain/Cargo.toml`
- Create: `crates/codegotchi-domain/src/lib.rs`
- Create: `crates/codegotchi-domain/src/clock.rs`
- Create: `crates/codegotchi-domain/src/pet.rs`
- Test: inline unit tests in the owning Rust modules

**Interfaces:**
- Produce `Clock::now() -> DateTime<Utc>`, `SystemClock`, and controllable cloneable `FakeClock`.
- Produce `Pet`, `PetNeeds`, `PetSpecies`, `PetBehavior`, `Poop`, food inventory, clamped mutation methods, and explicit agent activity/outcome states.
- Export domain APIs from `lib.rs`; production code must not expose test-only cleanup methods.

- [ ] Write tests first for need clamping at both bounds, stable defaults, fake-clock advancement, and backward clock movement.
- [ ] Run targeted tests and verify failures are caused by missing APIs.
- [ ] Implement the minimal types and clock port; use checked/saturating duration handling.
- [ ] Re-run targeted tests, then `cargo test --workspace`.
- [ ] Run `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings`.

### Task 2: Canonical events, elapsed progression, and behavior derivation

**Files:**
- Create: `crates/codegotchi-domain/src/event.rs`
- Create: `crates/codegotchi-domain/src/progression.rs`
- Create: `crates/codegotchi-domain/src/behavior.rs`
- Modify: `crates/codegotchi-domain/src/pet.rs`
- Modify: `crates/codegotchi-domain/src/lib.rs`
- Test: inline unit tests in the owning modules plus `crates/codegotchi-domain/tests/event_replay.rs`

**Interfaces:**
- Produce versioned `AgentEvent`, `AgentEventKind`, `ActivityKind`, `EventSource`, and privacy-preserving `EventMetadata`.
- Produce explicit `AgentActivityState::{Idle, WaitingForUser, Active(ActivityKind), Blocked}` transitions and `PetSimulation<C, N>` constructor injection for `Clock` and `NeedProgressionStrategy`; poop/random strategies remain Task 3 responsibilities.
- Keep `Clock` in `PetSimulation`, not in `Pet`; the aggregate stores timestamps and remains snapshot-friendly.
- Produce deterministic default progression, applied linearly from elapsed seconds and the previous state: active work adds `4.0` hunger/hour and removes `6.0` energy/hour; inactivity adds `1.0` hunger/hour and restores `8.0` energy/hour; each pending poop removes `2.0` cleanliness/hour.
- Successful test/build completion adds `8.0` happiness and resets the failure streak. A failed test/build removes `4.0 * min(consecutive_failures, 3)` happiness. Work-bearing turn/output/tool/command events add deterministic integer work points (`1` for turn/output and `5` for tool/command start).
- Derive behavior with exact priority: critical hunger (`>= 90.0`) or filth (`cleanliness <= 10.0`, hunger wins ties) > blocked > active activity > success/failure seen within 5 minutes > sleeping after 30 minutes without active work > wandering.
- Produce a single behavior coordinator implementing: critical need > blocked > active activity > recent outcome > sleep > wander.

- [ ] Write table-driven failing tests for active hunger/energy, idle hunger/energy, poop cleanliness decay, success/failure happiness, duplicate event IDs, replay equivalence, unknown schema rejection, and every behavior priority boundary.
- [ ] Verify each behavior group fails for the intended missing/wrong transition.
- [ ] Implement minimal event application, strategies, and derived behavior without framework dependencies.
- [ ] Refactor only after green so state transitions remain explicit rather than boolean-driven.
- [ ] Run the targeted test files and the complete Rust format/lint/test gates.

### Task 3: Care commands, inventory, deterministic poop, and random port

**Files:**
- Create: `crates/codegotchi-domain/src/care.rs`
- Create: `crates/codegotchi-domain/src/poop.rs`
- Create: `crates/codegotchi-domain/src/random.rs`
- Modify: `crates/codegotchi-domain/src/pet.rs`
- Modify: `crates/codegotchi-domain/src/progression.rs`
- Modify: `crates/codegotchi-domain/src/lib.rs`
- Test: inline unit tests plus `crates/codegotchi-domain/tests/care_flow.rs`

**Interfaces:**
- Produce the exact `CareCommand::{Feed { action_id, food_id }, CleanPoop { action_id, poop_id }, Pet { action_id, interaction_ms, pointer_distance }}` shapes.
- Produce typed `CareError` variants for unknown food, exhausted inventory, missing poop, insufficient duration/distance, and unsupported conditions.
- Use the literal food catalog: `kibble` removes 25 hunger and adds 40 digestion; `treat` removes 10 hunger, adds 5 happiness, and adds 20 digestion; `fruit` removes 15 hunger and adds 25 digestion. Inventory starts empty and is seeded only through an explicit aggregate inventory operation for this slice; every successful feed consumes exactly one item.
- Accept petting only when both `interaction_ms >= 1_500` and finite `pointer_distance >= 120.0`; an accepted interaction adds 10 happiness. Cleaning removes the exact poop and adds 25 cleanliness.
- Treat a repeated `action_id` as an immediate `Duplicate` total no-op before reading the clock or validating the repeated payload. Failed actions are atomic, do not record the action ID, and may be retried after the invalid condition is corrected.
- Produce deterministic poop thresholds requiring both `digestion_points >= 100` and `work_points >= 50`; consume 100 digestion and 50 work for each emitted poop and emit repeatedly while both thresholds remain satisfied.
- Derive each poop UUID v5 from the pet UUID and its monotonic `poop:{sequence}` value. Run poop generation after a successful feed and after each work-bearing event, using the care/event logical timestamp as `created_at`.
- Produce `PoopGenerationStrategy`, plus `RandomSource` and deterministic `SeededRandomSource` for cosmetic values only. Normalize a zero seed to a nonzero state; randomness must never decide care validity, need changes, inventory consumption, point consumption, poop creation, or identifiers.
- Integrate care handling with `PetSimulation` so it can share the authoritative clock, elapsed progression, behavior refresh, replay sets, and poop strategy. Invalid commands must leave the complete simulation snapshot unchanged.

- [ ] Write failing tests for every successful care transition and every validation error, proving invalid requests leave all aggregate state unchanged.
- [ ] Write failing tests around both sides of each poop threshold, multiple-threshold emission, deterministic IDs/replay, and seeded random repeatability.
- [ ] Implement minimal command handling and strategies; use a domain result that distinguishes `Applied` from `Duplicate`.
- [ ] Verify targeted tests, then run complete Rust format/lint/test gates.

### Task 4: Work-permission strategies

**Files:**
- Create: `crates/codegotchi-domain/src/permission.rs`
- Modify: `crates/codegotchi-domain/src/lib.rs`
- Test: inline unit tests plus `crates/codegotchi-domain/tests/permission_matrix.rs`

**Interfaces:**
- Produce `WorkPermissionPolicy::evaluate(&Pet, &CommandClassification, &PetSettings) -> WorkDecision`.
- Produce `EnforcementMode::{Decorative, Gentle, Strict}` with `Default` returning `Decorative`.
- Produce structured command category and blocking-safety/purpose types without raw command text. Only `SafeDevelopment` is blockable; `CodeGotchiControl`, `ProcessRecovery`, `ShellRecovery`, `GitRecovery`, `InfrastructureShutdown`, `SecurityRemediation`, and `Uncertain` are always allowed.
- Define critical neglect as hunger `>= 90.0` or cleanliness `<= 10.0`, with hunger winning a simultaneous tie. Produce stable structured reason codes and required actions: feed with minimum recovery 20 hunger points, or clean with minimum recovery 20 cleanliness points.
- Decorative always allows without a warning. Gentle always allows and emits a warning/required action only at critical neglect. Strict blocks only explicitly `SafeDevelopment` work at critical neglect; all protected/uncertain purposes stay allowed even when both needs are critical.
- `PetSettings::default()` must select `Decorative`; no constructor or default may opt into Strict implicitly.

- [ ] Write a literal decision matrix first for healthy/neglected pets across all three modes and all exempt purposes.
- [ ] Verify the matrix fails before implementation.
- [ ] Implement Decorative as always allow, Gentle as warn-only, and Strict as block-only for explicitly safe development work at critical thresholds.
- [ ] Add tie-breaking tests when hunger and filth are both critical and confirm the required action is deterministic.
- [ ] Run complete Rust format/lint/test gates.

### Task 5: Frontend foundation, CI, and human-facing documentation

**Files:**
- Create: `package.json`
- Create: `pnpm-workspace.yaml`
- Create: `pnpm-lock.yaml` through pnpm
- Create: `web/package.json`
- Create: `web/index.html`
- Create: `web/tsconfig.json`
- Create: `web/tsconfig.app.json`
- Create: `web/tsconfig.node.json`
- Create: `web/vite.config.ts`
- Create: `web/eslint.config.js`
- Create: `web/src/main.tsx`
- Create: `web/src/App.tsx`
- Create: `web/src/App.css`
- Create: `web/src/test/setup.ts`
- Create: `web/src/App.test.tsx`
- Create: `.github/workflows/ci.yml`
- Create: `README.md`
- Create: `docs/architecture.md`
- Create: `docs/adr/0001-rust-typescript-workspace.md`

**Interfaces:**
- Root scripts must make `pnpm test`, `pnpm build`, `pnpm lint`, and `pnpm format:check` operate on workspace packages.
- The real rendered app exposes an accessible CodeGotchi heading and labels its room as a Phase 1 placeholder.
- CI runs Rust formatting, clippy with warnings denied, Rust workspace tests, frozen pnpm install, web lint/test/build, and formatting checks.

- [ ] Create the frontend test before `App.tsx`; run it and verify the missing UI contract fails.
- [ ] Implement only the minimal accessible React room shell needed for the test; no PixiJS, Zustand, WebSocket, care interaction, or authoritative state.
- [ ] Generate and commit a frozen lockfile using a pinned `packageManager` value.
- [ ] Add lint/format/build configuration and run each command independently.
- [ ] Document setup, exact commands, architecture boundaries, accepted deferrals, and the Rust/TypeScript split ADR.
- [ ] Run the complete acceptance suite: `cargo test --workspace`, Rust format/clippy gates, `pnpm test`, `pnpm lint`, `pnpm format:check`, and `pnpm build`.

## Final validation checklist

- [ ] Repository tree contains no Phase 3+ implementation.
- [ ] `cargo tree -p codegotchi-domain` contains no framework/storage/process dependency.
- [ ] Tests demonstrate duplicate events and care actions cannot apply twice.
- [ ] Tests demonstrate important poop/state transitions are deterministic.
- [ ] Tests demonstrate strict blocking is opt-in and protected/uncertain commands remain allowed.
- [ ] Frontend does not calculate authoritative pet state.
- [ ] CI invokes all documented local acceptance commands.
- [ ] No test-only production endpoint, raw command text, prompt, source content, or complete output persistence exists.
