# Persistent Trust and Lively Animation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the already-started client-side CodeGotchi motion experience, verify the persistent hook-profile work, and ship the rebuilt embedded production bundle.

**Architecture:** Rust remains authoritative and the existing `MotionController` is the presentation-only boundary. React owns one controller for the mounted room, translates `MotionState` into deterministic DOM attributes and decorative effect nodes, and CSS resolves semantic destinations and room-relative waypoints. Playwright drives real backend events through the production fixture and observes those attributes instead of depending on animation timing or pixels.

**Tech Stack:** React 19, TypeScript 5.9, Vitest, Testing Library, CSS, Playwright, Vite, Rust/Cargo.

## Global Constraints

- Every implementation worker uses `gpt-5.6-luna` with reasoning effort `max` and receives one sub-agent cycle only.
- Rust snapshots, HTTP, WebSocket, persistence, needs, inventory, poop, and care schemas remain unchanged.
- All spontaneous movement and room interactions are cosmetic and never call backend mutation APIs.
- Authoritative mode changes interrupt roaming immediately and reach the required pose in less than one second.
- Equivalent semantic snapshots do not restart choreography.
- The pet presentation layer is pointer-transparent; existing interactive care controls remain operable.
- Reduced motion removes travel, rolls, and looping thought bubbles and uses static semantic poses with gentle fades.
- Deterministic browser selectors are `data-motion-mode`, `data-motion-action`, `data-motion-waypoint`, `data-motion-facing`, `data-motion-phase`, and `data-motion-region` on the pet presentation node.
- Work in the current branch because the unfinished foundation is uncommitted in this worktree; preserve all pre-existing user changes.

---

### Task 1: React motion adapter and deterministic DOM contract

**Files:**
- Modify: `web/src/App.tsx`
- Modify: `web/src/App.test.tsx`
- Optional create: `web/src/usePetMotion.ts`
- Optional test: `web/src/usePetMotion.test.tsx`

**Interfaces:**
- Consumes: `createMotionController(options?): MotionController`, `MotionState`, and `SimulationSnapshot` from `web/src/motion.ts`.
- Produces: one mounted controller, snapshot-to-controller updates, controller disposal, and the six exact `data-motion-*` attributes from Global Constraints.
- Produces decorative nodes for `.thought-bubbles`, `.typing-marks`, `.motion-sparkles`, and existing `.zzz`; these nodes use `aria-hidden="true"`.

- [ ] **Step 1: Write failing component tests**

Add tests that render authoritative editing, thinking, idle, napping, critical, success, and failure snapshots and assert the pet node exposes the correct semantic mode and destination. Use fake timers to prove editing settles to `action="type"`, thinking settles to `action="think"`, an editing-to-thinking rerender replaces the roaming/desk action within 999ms, equivalent editing snapshots keep the same generation/state without recreating choreography, and unmount clears timers. Assert thought bubbles and typing marks appear only for their actions. Assert the pet node is discoverable by `data-testid="pet"`.

- [ ] **Step 2: Run the focused tests and capture the expected failure**

Run: `corepack pnpm --filter @codegotchi/web exec vitest run src/App.test.tsx`

Expected: new assertions fail because the pet has no motion attributes or controller subscription.

- [ ] **Step 3: Implement the React lifecycle adapter**

Create the controller once per mounted room, initialize reduced-motion from `window.matchMedia("(prefers-reduced-motion: reduce)")`, subscribe before applying snapshots, update component state from controller emissions, apply each authoritative snapshot in an effect, respond to media-query changes without replacing the controller, and dispose the subscription/controller on unmount. Do not copy or mutate snapshots and do not call any care operation.

- [ ] **Step 4: Render the deterministic contract and effects**

On the pet node render `data-testid="pet"` plus every exact attribute from Global Constraints. For free-time room waypoints, expose the waypoint id and region and set CSS custom properties `--pet-x` and `--pet-y` from normalized coordinates as percentages. For semantic destinations use the controller destination name. Render typing marks only for `type`/`pulse` in desk mode, thought bubbles only for active thinking when reduced motion is not static, and sparkles for success. Preserve authoritative labels, napping ZZZs, and all existing care behavior.

- [ ] **Step 5: Run component and controller tests**

Run: `corepack pnpm --filter @codegotchi/web exec vitest run src/App.test.tsx src/motion.test.ts`

Expected: all focused tests pass with no leaked timers or React warnings.

### Task 2: Room-relative animation styling and interaction safety

**Files:**
- Modify: `web/src/App.css`

**Interfaces:**
- Consumes: the exact pet attributes, custom properties, and decorative class names defined by Task 1.
- Produces: room-relative positioning, facing, traveling/acting phases, roll/wander/interaction/desk/thinking/special animations, typing/monitor/thought effects, and reduced-motion overrides.

- [ ] **Step 1: Establish layout-safe motion rules**

Make `.pet` a pointer-transparent presentation layer above the room floor but below or non-blocking for `.feed-target`, `.poop`, `.shovel`, `.trash-target`, and `.hammock`. Keep existing controls pointer-operable. Resolve free-time `--pet-x`/`--pet-y` within the illustration and define fixed safe positions for desk, thinking, hammock, critical, success, and failure destinations.

- [ ] **Step 2: Add phase, facing, and action styling**

Use attribute selectors for travel under one second, horizontal facing, full rolls, wandering, sitting, window watching, shelf inspection, furniture circling, desk sitting/typing, upward thinking, celebration, flinch, shake, and pulse. Transform composition must preserve destination placement rather than replacing it accidentally.

- [ ] **Step 3: Add semantic effects**

Animate alternating typing marks and monitor activity at the desk. Animate upward thought bubbles in thinking mode. Animate sparkles and warnings for outcome/critical effects while retaining existing ZZZ behavior for naps.

- [ ] **Step 4: Add reduced-motion behavior**

Under `@media (prefers-reduced-motion: reduce)`, disable travel, roll, wander, circle, looping thought bubbles, typing loops, sparkles, shakes, and other repeated transforms. Retain static semantic positions with a short opacity fade and keep care controls unchanged.

- [ ] **Step 5: Run formatting and lint checks**

Run: `corepack pnpm --filter @codegotchi/web format:check`

Run: `corepack pnpm --filter @codegotchi/web lint`

Expected: both commands pass.

### Task 3: Production-browser motion acceptance

**Files:**
- Modify: `web/e2e/mvp.spec.ts`
- Modify if needed: `web/e2e/fixture.mjs`
- Modify if needed: `crates/codegotchi-cli/examples/task3_fixture.rs`

**Interfaces:**
- Consumes: the exact pet attributes from Task 1 and visual behavior from Task 2.
- Produces: browser coverage for multi-region idle travel, rolls, thinking, desk work, interruption, reduced motion, and care-control operability.

- [ ] **Step 1: Add a reusable event sender and deterministic observation helpers**

Add browser-side helpers that post valid uniquely identified hook events to `/api/v1/events`, locate `[data-testid="pet"]`, and poll semantic attributes. Do not add test-only production APIs. Use the existing authenticated backend fixture.

- [ ] **Step 2: Write failing idle and roll acceptance tests**

Observe free-time motion across multiple `data-motion-region` values and eventually `data-motion-action="roll"`. Bound polling time to the controller's documented 15–30 second roll interval plus travel allowance. Assert all observed regions are presentation-only and the authoritative activity label remains idle/waiting.

- [ ] **Step 3: Write failing semantic-transition tests**

Post events producing thinking/searching and editing/testing states. Assert thinking reaches waypoint `thinking` with action `think` and visible thought bubbles; desk work reaches waypoint `desk` with action `type`, typing marks, and active monitor styling. Begin from free-time movement, post active work, and assert the semantic mode and destination switch immediately and settle in less than one second.

- [ ] **Step 4: Write failing reduced-motion and care-operation tests**

Emulate reduced motion before navigation and assert the pet remains `phase="static"` with no roll or looping thought bubbles after semantic updates. In normal motion, exercise feed, hammock, shovel/poop/trash, and restock where enabled while the pet is moving; assert each authoritative workflow still succeeds.

- [ ] **Step 5: Run the production Playwright suite**

Run: `corepack pnpm playwright:test`

Expected after Tasks 1 and 2 are present: all production browser scenarios pass.

### Task 4: Embedded bundle, compatibility audit, and final focused fixes

**Files:**
- Rebuild: `crates/codegotchi-cli/web-dist/index.html`
- Rebuild: `crates/codegotchi-cli/web-dist/assets/*`
- Modify only if verification exposes a defect: files already owned by Tasks 1–3 or persistent-profile files.

**Interfaces:**
- Consumes: completed source and browser tests from Tasks 1–3.
- Produces: embedded production bytes containing motion attributes/animations and a source tree that passes the complete validation matrix.

- [ ] **Step 1: Build and embed production assets**

Run: `corepack pnpm --filter @codegotchi/web build`

Run: `node web/scripts/embed-web.mjs`

Confirm stale hashed assets are replaced only by the embedding script and the embedded JavaScript contains the six motion attribute names.

- [ ] **Step 2: Run frontend verification**

Run: `corepack pnpm test`

Run: `corepack pnpm lint`

Run: `corepack pnpm format:check`

Run: `corepack pnpm build`

Run: `corepack pnpm playwright:test`

- [ ] **Step 3: Run Rust and repository verification**

Run: `cargo fmt --all -- --check`

Run: `cargo test --workspace`

Run: `git diff --check`

- [ ] **Step 4: Audit every plan requirement**

Confirm persistent profiles remain content-addressed/private/persistent; each activity maps to the intended semantic destination; idle roaming, interactions, pause ratio, and rolls remain cosmetic; authoritative changes interrupt immediately; equivalent snapshots do not restart; reduced motion is static; controls work while the pet moves; production bytes are embedded; and no protocol/schema changes were introduced.
