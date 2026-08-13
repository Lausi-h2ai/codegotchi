# Frequent Care Pressure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make CodeGotchi continue aging and producing bounded random care incidents in real wall-clock time, so ignoring it for roughly 30 minutes naturally drives the existing strict-mode enforcement toward severe blocking while returning after long absences produces a neglected but manageable room.

**Architecture:** Keep Rust as the authoritative simulation. Add a deterministic attention scheduler and persisted demand state in the domain, integrate wall-clock need progression and capped catch-up into `PetSimulation`, extend the existing need-based enforcement to happiness, expose the already-existing petting care command through the runtime/API, and render demands plus a measured petting gesture in React. The browser never owns incident timers or mutates needs optimistically.

**Tech Stack:** Rust stable, chrono, uuid v5, serde/serde_json, SQLite persistence, Axum, Tokio maintenance loop, React 19, TypeScript 5.9, Vitest/Testing Library, Playwright.

## Global Constraints

- Hunger progresses at `+25.0` points/hour in all normal activity states.
- Energy progresses at `-50.0` points/hour in all normal activity states outside the existing hammock nap recovery window.
- Attention incidents occur after deterministic randomized wall-clock delays in the inclusive range `180_000..=300_000` milliseconds.
- Incident kinds use three-item shuffle bags: every consecutive group of three contains exactly one affection, one snack, and one poop.
- Each unresolved affection/snack/poop applies `240.0` need points/hour of happiness/hunger/cleanliness pressure respectively, regardless of agent activity.
- A single elapsed-time advancement may create at most `5` missed attention incidents; elapsed need progression itself is never capped.
- After the catch-up cap is hit, discard additional historical incident objects and re-anchor the next incident to `target + next deterministic delay`.
- Existing work/digestion-generated poops and attention-generated poops use the same cleanliness pressure once present.
- Happiness joins the existing strict-mode thresholds at mild `<=30`, moderate `<=15`, severe `<=5`.
- Do not add a second blocking mechanism. `WorkPermissionPolicy` remains the only strict-mode refusal source.
- Petting validation remains at least `1_500 ms` and `120 px` pointer travel.
- One successful petting gesture resolves one oldest affection demand; one kibble/treat/fruit feed resolves one oldest snack demand; energy drinks resolve no snack demand.
- The browser owns no incident scheduler, no catch-up logic, and no optimistic demand removal.
- Preserve current authentication, privacy, replay-safety, persistence, WebSocket, motion, blinking, shovel/trash, food, and hammock behavior.
- Preserve unrelated working-tree changes.

---

## File Structure

### New domain file

- `crates/codegotchi-domain/src/attention.rs`
  - Owns `PetDemandKind`, `PetDemand`, `AttentionIncidentKind`, deterministic delay/kind/id derivation, and constants for 3–5 minute scheduling and five-incident catch-up.

### Existing domain files

- `crates/codegotchi-domain/src/lib.rs`
  - Exports attention-domain types/functions needed by tests and adapters.
- `crates/codegotchi-domain/src/pet.rs`
  - Adds pending non-poop demands to the `Pet` aggregate and snapshot restoration path.
- `crates/codegotchi-domain/src/progression.rs`
  - Changes baseline need rates, applies incident pressure, persists scheduler state, splits elapsed time on incident boundaries, performs bounded catch-up, creates incidents, resolves matching demands through care, migrates legacy snapshots, and validates attention state.
- `crates/codegotchi-domain/src/permission.rs`
  - Adds happiness to neglect tiers, reason/action types, and dominant-need selection.
- `crates/codegotchi-domain/src/behavior.rs`
  - Treats happiness `<=10` as `CriticalNeed`.

### CLI/runtime files

- `crates/codegotchi-cli/src/protocol.rs`
  - Adds bounded `PetRequest` JSON DTO.
- `crates/codegotchi-cli/src/runtime.rs`
  - Adds `AuthoritativeRuntime::pet` and persistence/broadcast coverage for wall-clock maintenance/catch-up.
- `crates/codegotchi-cli/src/server.rs`
  - Adds `/api/v1/care/pet`, maps critical happiness into denial copy, and tests the route/catch-up broadcast path.
- `README.md`
  - Documents real-time neglect, happiness enforcement, and bounded restart catch-up.

### Web files

- `web/src/protocol.ts`
  - Adds `PetDemandKind`, `PetDemand`, and attention scheduler fields to `SimulationSnapshot`.
- `web/src/client.ts`
  - Adds `CodeGotchiClient.pet(interactionMs, pointerDistance, actionId?)`.
- `web/src/client.test.ts`
  - Covers pet request shape and authoritative response publication.
- `web/src/useCodeGotchi.ts`
  - Exposes `pet(interactionMs, pointerDistance)` and feedback/error behavior.
- `web/src/App.tsx`
  - Renders demand bubbles/counts and measures pointer-duration/path-distance on the existing pet element.
- `web/src/App.css`
  - Styles demand bubbles and petting affordance without interfering with motion transforms.
- `web/src/App.test.tsx`
  - Covers demand rendering and pointer gesture adapter behavior.
- `web/e2e/fixture.mjs`
  - Seeds deterministic attention state for browser acceptance without waiting real minutes.
- `web/e2e/mvp.spec.ts`
  - Covers affection, snack, poop, critical-happiness denial, and restored overdue incident state.

---

### Task 1: Deterministic attention model and pet demand state

**Files:**
- Create: `crates/codegotchi-domain/src/attention.rs`
- Modify: `crates/codegotchi-domain/src/lib.rs`
- Modify: `crates/codegotchi-domain/src/pet.rs`

**Interfaces:**
- Produces:
  - `pub const MIN_INCIDENT_DELAY_MS: u64 = 180_000`
  - `pub const MAX_INCIDENT_DELAY_MS: u64 = 300_000`
  - `pub const MAX_CATCH_UP_INCIDENTS: usize = 5`
  - `pub enum PetDemandKind { Affection, Snack }`
  - `pub struct PetDemand { id: Uuid, kind: PetDemandKind, created_at: DateTime<Utc> }`
  - `pub enum AttentionIncidentKind { Affection, Snack, Poop }`
  - `pub fn incident_delay_ms(pet_id: Uuid, sequence: u64) -> u64`
  - `pub fn incident_kind(pet_id: Uuid, sequence: u64) -> AttentionIncidentKind`
  - `pub fn incident_id(pet_id: Uuid, sequence: u64, kind: AttentionIncidentKind) -> Uuid`
  - `Pet::pending_demands(&self) -> &[PetDemand]`
  - crate-visible `push_demand`, `remove_demand`, and timestamp-shift support used by `PetSimulation`.

- [ ] **Step 1: Write failing deterministic scheduler tests**

Create `attention.rs` with tests first. Cover inclusive delay bounds, deterministic replay, three-item shuffle bags, and distinct IDs:

```rust
#[test]
fn delay_is_deterministic_and_inclusive() {
    let pet_id = Uuid::from_u128(42);
    for sequence in 0..10_000 {
        let delay = incident_delay_ms(pet_id, sequence);
        assert!((MIN_INCIDENT_DELAY_MS..=MAX_INCIDENT_DELAY_MS).contains(&delay));
        assert_eq!(delay, incident_delay_ms(pet_id, sequence));
    }
}

#[test]
fn every_shuffle_bag_contains_each_kind_once() {
    let pet_id = Uuid::from_u128(42);
    for bag in 0..100 {
        let mut kinds = [
            incident_kind(pet_id, bag * 3),
            incident_kind(pet_id, bag * 3 + 1),
            incident_kind(pet_id, bag * 3 + 2),
        ];
        kinds.sort();
        assert_eq!(
            kinds,
            [
                AttentionIncidentKind::Affection,
                AttentionIncidentKind::Poop,
                AttentionIncidentKind::Snack,
            ]
        );
    }
}

#[test]
fn incident_ids_are_stable_and_kind_namespaces_do_not_collide() {
    let pet_id = Uuid::from_u128(42);
    let affection = incident_id(pet_id, 7, AttentionIncidentKind::Affection);
    let poop = incident_id(pet_id, 7, AttentionIncidentKind::Poop);
    assert_eq!(
        affection,
        incident_id(pet_id, 7, AttentionIncidentKind::Affection)
    );
    assert_ne!(affection, poop);
}
```

Derive `Ord`/`PartialOrd` on `AttentionIncidentKind` only to simplify deterministic test comparison.

- [ ] **Step 2: Run focused domain tests and verify RED**

Run:

```bash
cargo test -p codegotchi-domain attention -- --nocapture
```

Expected: FAIL because the attention module/types/functions do not exist.

- [ ] **Step 3: Implement deterministic derivation without mutable RNG state**

Use UUID-v5 hashes so restore only needs `pet_id` and `sequence`:

```rust
pub const MIN_INCIDENT_DELAY_MS: u64 = 180_000;
pub const MAX_INCIDENT_DELAY_MS: u64 = 300_000;
pub const MAX_CATCH_UP_INCIDENTS: usize = 5;

fn hash_u64(pet_id: Uuid, namespace: &str, index: u64) -> u64 {
    let name = format!("attention:{namespace}:{index}");
    let hash = Uuid::new_v5(&pet_id, name.as_bytes());
    u64::from_be_bytes(hash.as_bytes()[0..8].try_into().expect("uuid prefix"))
}

pub fn incident_delay_ms(pet_id: Uuid, sequence: u64) -> u64 {
    let span = MAX_INCIDENT_DELAY_MS - MIN_INCIDENT_DELAY_MS + 1;
    MIN_INCIDENT_DELAY_MS + hash_u64(pet_id, "delay", sequence) % span
}
```

For `incident_kind`, select one of the six permutations using `hash_u64(pet_id, "bag", sequence / 3) % 6`, then index by `(sequence % 3) as usize`.

For IDs use names `attention:affection:{sequence}`, `attention:snack:{sequence}`, and `attention:poop:{sequence}`.

- [ ] **Step 4: Run attention tests and verify GREEN**

Run:

```bash
cargo test -p codegotchi-domain attention -- --nocapture
```

Expected: all attention scheduler tests PASS.

- [ ] **Step 5: Add failing pet-demand aggregate tests**

In `pet.rs` tests, assert new pets start with no demands and snapshot restoration can carry demand order:

```rust
#[test]
fn pending_demands_preserve_insertion_order() {
    let at = Utc.with_ymd_and_hms(2026, 8, 13, 10, 0, 0).unwrap();
    let mut pet = Pet::new(Uuid::from_u128(1), "Mochi", PetSpecies::Cat, at);
    let first = PetDemand::new(Uuid::from_u128(10), PetDemandKind::Affection, at);
    let second = PetDemand::new(Uuid::from_u128(11), PetDemandKind::Snack, at);
    pet.push_demand(first.clone());
    pet.push_demand(second.clone());
    assert_eq!(pet.pending_demands(), &[first, second]);
}
```

- [ ] **Step 6: Add demand storage to `Pet` and exports**

Add `pending_demands: Vec<PetDemand>` beside `pending_poops`, initialize it empty in `Pet::new`, add it to `Pet::from_snapshot`, and expose read-only access plus crate-visible mutation helpers. Add `shift_created_at` to `PetDemand` mirroring `Poop` so future-clock reanchoring can preserve relative ages.

Update `lib.rs`:

```rust
pub mod attention;
pub use attention::{
    AttentionIncidentKind, MAX_CATCH_UP_INCIDENTS, MAX_INCIDENT_DELAY_MS,
    MIN_INCIDENT_DELAY_MS, PetDemand, PetDemandKind, incident_delay_ms,
    incident_id, incident_kind,
};
```

- [ ] **Step 7: Run domain tests and commit**

Run:

```bash
cargo test -p codegotchi-domain
cargo fmt --all -- --check
```

Expected: PASS.

Commit:

```bash
git add crates/codegotchi-domain/src/attention.rs crates/codegotchi-domain/src/lib.rs crates/codegotchi-domain/src/pet.rs
git commit -m "feat: add deterministic CodeGotchi attention incidents"
```

---

### Task 2: Wall-clock physiology, incident pressure, scheduling, and bounded catch-up

**Files:**
- Modify: `crates/codegotchi-domain/src/progression.rs`
- Modify: `crates/codegotchi-domain/src/pet.rs`

**Interfaces:**
- Consumes Task 1 deterministic attention functions/types.
- Produces `SimulationSnapshot` fields:
  - `pending_demands: Vec<PetDemand>` with `#[serde(default)]`
  - `attention_sequence: u64` with `#[serde(default)]`
  - `next_incident_at: Option<DateTime<Utc>>` with `#[serde(default)]`; new/restored current simulations always normalize this to `Some(...)`.
- `PetSimulation` stores `attention_sequence: u64` and `next_incident_at: DateTime<Utc>` internally after construction/restore.

- [ ] **Step 1: Replace old activity-rate tests with failing wall-clock physiology tests**

Add/modify progression tests so Active, Idle, WaitingForUser, and Blocked all produce the same one-hour baseline result from a healthy pet:

```rust
for activity in [
    AgentActivityState::Idle,
    AgentActivityState::WaitingForUser,
    AgentActivityState::Blocked,
    AgentActivityState::Active(ActivityKind::Editing),
] {
    let mut pet = pet_with_activity(activity);
    DefaultNeedProgressionStrategy.progress(&mut pet, Duration::hours(1), activity);
    assert_eq!(pet.needs().hunger(), 25.0);
    assert_eq!(pet.needs().energy(), 50.0);
}
```

Retain a dedicated nap-overlap test proving only the nap-covered slice uses `NAP_ENERGY_PER_HOUR`.

- [ ] **Step 2: Run focused progression tests and verify RED**

Run:

```bash
cargo test -p codegotchi-domain progression::tests -- --nocapture
```

Expected: FAIL because current active and idle rates differ.

- [ ] **Step 3: Implement wall-clock baseline plus unresolved pressure**

Replace the activity-specific constants with:

```rust
const HUNGER_PER_HOUR: f32 = 25.0;
const ENERGY_PER_HOUR: f32 = -50.0;
const INCIDENT_PRESSURE_PER_HOUR: f32 = 240.0;
```

In `DefaultNeedProgressionStrategy::progress`, count pending demand kinds and pending poops before taking `needs_mut()`:

```rust
let affection_count = pet
    .pending_demands()
    .iter()
    .filter(|demand| demand.kind() == PetDemandKind::Affection)
    .count() as f32;
let snack_count = pet
    .pending_demands()
    .iter()
    .filter(|demand| demand.kind() == PetDemandKind::Snack)
    .count() as f32;
let poop_count = pet.pending_poops().len() as f32;

needs.adjust_hunger(
    (HUNGER_PER_HOUR + INCIDENT_PRESSURE_PER_HOUR * snack_count) * elapsed_hours,
);
needs.adjust_happiness(-INCIDENT_PRESSURE_PER_HOUR * affection_count * elapsed_hours);
needs.adjust_cleanliness(-INCIDENT_PRESSURE_PER_HOUR * poop_count * elapsed_hours);
needs.adjust_energy(
    ENERGY_PER_HOUR * (elapsed_hours - nap_hours) + NAP_ENERGY_PER_HOUR * nap_hours,
);
```

Keep the trait argument for activity compatibility but rename it `_previous_activity` because progression no longer depends on it.

- [ ] **Step 4: Add failing stacking-pressure tests**

Add one-hour/quarter-hour tests that demonstrate one incident means four points per minute and two incidents double it. Use short intervals to avoid clamping masking arithmetic:

```rust
let mut pet = healthy_pet();
pet.push_demand(affection(at));
pet.push_demand(affection(at));
DefaultNeedProgressionStrategy.progress(&mut pet, Duration::minutes(5), AgentActivityState::Idle);
assert_eq!(pet.needs().happiness(), 60.0); // 2 * 240/h * 1/12 h
```

Add equivalent snack and poop cases and verify the same result in `WaitingForUser`.

- [ ] **Step 5: Persist scheduler fields and initialize new pets**

Extend `SimulationSnapshot` and `PetSimulation`. In `with_poop_strategy` derive the first due time from the pet's initial timestamp:

```rust
let attention_sequence = 0;
let next_incident_at = initial_timestamp
    + Duration::milliseconds(incident_delay_ms(pet.id(), attention_sequence) as i64);
```

Include `pending_demands`, `attention_sequence`, and `next_incident_at: Some(self.next_incident_at)` in `snapshot()`.

- [ ] **Step 6: Add failing incident-boundary and catch-up tests**

Write deterministic tests using `FakeClock` and explicit `current_state_at` timestamps:

1. Set `next_incident_at` through a restored snapshot to exactly `start + 3 minutes` and advance to one millisecond before due: zero new incidents.
2. Advance one millisecond: exactly one new affection/snack/poop according to `incident_kind`.
3. Restore the same state twice; advance one copy in a single jump containing <=5 incidents and the other in one-second steps; assert snapshots are equal.
4. Restore `next_incident_at = start + 3 minutes`, advance 24 hours, assert exactly five newly created attention incidents, needs are clamped to severe extrema as appropriate, and `next_incident_at > target && next_incident_at <= target + 5 minutes`.
5. Advance one second again after capped catch-up and assert no sixth hidden-backlog incident appears immediately.

- [ ] **Step 7: Split elapsed progression at due timestamps**

Refactor `advance_elapsed_to` into an orchestration method and a single-segment helper:

```rust
fn progress_segment_to(
    &mut self,
    target: DateTime<Utc>,
    previous_activity: AgentActivityState,
) -> Duration {
    let elapsed = self.pet.advance_to(target);
    if elapsed > Duration::zero() {
        self.progression.progress(&mut self.pet, elapsed, previous_activity);
    }
    self.pet.clear_expired_nap(target);
    elapsed
}
```

Then implement `advance_elapsed_to` with this exact control flow:

```rust
let start = self.pet.last_updated_at();
let mut created = 0usize;

while self.next_incident_at <= target && created < MAX_CATCH_UP_INCIDENTS {
    let due = self.next_incident_at.max(self.pet.last_updated_at());
    self.progress_segment_to(due, previous_activity);
    self.create_attention_incident(due);
    self.attention_sequence = self.attention_sequence.saturating_add(1);
    self.next_incident_at = due
        + Duration::milliseconds(
            incident_delay_ms(self.pet.id(), self.attention_sequence) as i64,
        );
    created += 1;
}

if created == MAX_CATCH_UP_INCIDENTS && self.next_incident_at <= target {
    self.progress_segment_to(target, previous_activity);
    self.next_incident_at = target
        + Duration::milliseconds(
            incident_delay_ms(self.pet.id(), self.attention_sequence) as i64,
        );
} else {
    self.progress_segment_to(target, previous_activity);
}

target.signed_duration_since(start).max(Duration::zero())
```

Implement `create_attention_incident(created_at)`:

```rust
match incident_kind(self.pet.id(), self.attention_sequence) {
    AttentionIncidentKind::Affection => self.pet.push_demand(PetDemand::new(
        incident_id(self.pet.id(), self.attention_sequence, AttentionIncidentKind::Affection),
        PetDemandKind::Affection,
        created_at,
    )),
    AttentionIncidentKind::Snack => self.pet.push_demand(PetDemand::new(
        incident_id(self.pet.id(), self.attention_sequence, AttentionIncidentKind::Snack),
        PetDemandKind::Snack,
        created_at,
    )),
    AttentionIncidentKind::Poop => self.pet.push_poop(Poop::new(
        incident_id(self.pet.id(), self.attention_sequence, AttentionIncidentKind::Poop),
        created_at,
    )),
}
```

- [ ] **Step 8: Implement legacy restore migration and timestamp reanchoring**

In `with_poop_strategy_from_snapshot`, validate normal persisted fields first, then normalize missing attention schedule:

```rust
let next_incident_at = snapshot.next_incident_at.unwrap_or_else(|| {
    clock.now()
        + Duration::milliseconds(incident_delay_ms(snapshot.pet_id, snapshot.attention_sequence) as i64)
});
```

Pass `snapshot.pending_demands` into `Pet::from_snapshot`.

Update `reanchor_snapshot_to_wall_clock` so future-dated snapshot repair shifts `next_incident_at` when present and shifts each demand's `created_at` alongside poop timestamps.

Update `validate_snapshot` to reject duplicate pending demand IDs. Do not reject a missing `next_incident_at`; `None` is the legacy migration marker.

- [ ] **Step 9: Add explicit 30-minute severe-neglect invariant test**

Test several pet IDs (at least 100 deterministic schedules) from healthy state. For each, advance exactly 30 minutes with no care and strict mode, then assert:

```rust
let decision = WorkPermissionPolicy::evaluate(
    simulation.pet(),
    &classification(CommandPurpose::Uncertain),
    &PetSettings::new(EnforcementMode::Strict),
);
assert!(decision.is_blocked(), "pet_id={pet_id}");
```

This proves at least one need reaches severe, because uncertain work remains allowed at moderate neglect.

- [ ] **Step 10: Run domain progression suite and commit**

Run:

```bash
cargo test -p codegotchi-domain progression::tests -- --nocapture
cargo test -p codegotchi-domain
cargo fmt --all -- --check
```

Expected: PASS.

Commit:

```bash
git add crates/codegotchi-domain/src/progression.rs crates/codegotchi-domain/src/pet.rs
git commit -m "feat: run CodeGotchi care pressure on wall clock time"
```

---

### Task 3: Resolve demands through care and make happiness enforceable

**Files:**
- Modify: `crates/codegotchi-domain/src/progression.rs`
- Modify: `crates/codegotchi-domain/src/permission.rs`
- Modify: `crates/codegotchi-domain/src/behavior.rs`

**Interfaces:**
- Extends `WorkReasonCode` with `CriticalHappiness`.
- Extends `RequiredAction` with `Pet { minimum_happiness_recovery: f32 }` using `20.0` as the required recovery payload for consistency with existing required-action reporting.
- Care commands remain unchanged.

- [ ] **Step 1: Write failing one-demand-per-care tests**

In progression tests construct ordered demand backlogs:

```rust
#[test]
fn petting_resolves_only_oldest_affection_demand() {
    let mut simulation = simulation_with_demands([
        demand(1, PetDemandKind::Affection),
        demand(2, PetDemandKind::Snack),
        demand(3, PetDemandKind::Affection),
    ]);

    simulation.apply_care(&CareCommand::Pet {
        action_id: Uuid::from_u128(99),
        interaction_ms: 1_500,
        pointer_distance: 120.0,
    }).unwrap();

    assert_eq!(
        simulation.pet().pending_demands().iter().map(|d| d.id()).collect::<Vec<_>>(),
        vec![Uuid::from_u128(2), Uuid::from_u128(3)]
    );
}
```

Add food cases proving kibble/treat/fruit each remove only the oldest snack, while `EnergyDrink` removes none.

- [ ] **Step 2: Implement care-linked demand removal**

Add a small private helper in `PetSimulation`:

```rust
fn resolve_oldest_demand(&mut self, kind: PetDemandKind) {
    if let Some(index) = self
        .pet
        .pending_demands()
        .iter()
        .position(|demand| demand.kind() == kind)
    {
        self.pet.remove_demand(index);
    }
}
```

Call it after successful state mutation:

- `CareCommand::Pet` => `Affection`
- `Kibble | Treat | Fruit` => `Snack`
- `EnergyDrink` => none

Do not resolve a demand before food inventory consumption succeeds.

- [ ] **Step 3: Write failing happiness enforcement boundary tests**

In `permission.rs` mirror existing hunger/energy/cleanliness boundary tests with happiness at `30`, `15`, and `5`. Assert mild blocks safe development only, moderate preserves uncertain, severe blocks uncertain, and CodeGotchi control always remains allowed.

Add a dominant-need test in which happiness is more neglected than the other three needs and returns `CriticalHappiness` + `RequiredAction::Pet`.

- [ ] **Step 4: Implement happiness tiers and structured reason/action**

Add:

```rust
const MILD_HAPPINESS: f32 = 30.0;
const MODERATE_HAPPINESS: f32 = 15.0;
const SEVERE_HAPPINESS: f32 = 5.0;
const MINIMUM_HAPPINESS_RECOVERY: f32 = 20.0;
```

Include happiness in `NeglectLevel::from_needs`. Extend `WorkReasonCode::as_str()` with `"critical_happiness"`. Extend `RequiredAction::minimum_recovery_points()` for `Pet`.

In `dominant_need`, use normalized scores and deterministic tie order hunger -> energy -> cleanliness -> happiness. Preserve the existing first three tie priorities.

- [ ] **Step 5: Write failing critical-behavior test and implement it**

Add a `behavior.rs` test with happiness exactly `10.0` and all other needs healthy. Expected `PetBehavior::CriticalNeed`.

Add after cleanliness check:

```rust
if pet.needs().happiness() <= 10.0 {
    return PetBehavior::CriticalNeed;
}
```

- [ ] **Step 6: Run domain tests and commit**

Run:

```bash
cargo test -p codegotchi-domain permission::tests -- --nocapture
cargo test -p codegotchi-domain progression::tests -- --nocapture
cargo test -p codegotchi-domain behavior -- --nocapture
cargo test -p codegotchi-domain
```

Expected: PASS.

Commit:

```bash
git add crates/codegotchi-domain/src/progression.rs crates/codegotchi-domain/src/permission.rs crates/codegotchi-domain/src/behavior.rs
git commit -m "feat: enforce affection and resolve pet demands"
```

---

### Task 4: Persist/broadcast wall-clock catch-up and expose petting API

**Files:**
- Modify: `crates/codegotchi-cli/src/protocol.rs`
- Modify: `crates/codegotchi-cli/src/runtime.rs`
- Modify: `crates/codegotchi-cli/src/server.rs`

**Interfaces:**
- Produces:

```rust
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PetRequest {
    pub action_id: Uuid,
    pub interaction_ms: u64,
    pub pointer_distance: f32,
}
```

- Produces:

```rust
pub fn pet(
    &self,
    action_id: Uuid,
    interaction_ms: u64,
    pointer_distance: f32,
) -> Result<MutationReceipt, RuntimeError>
```

- Adds authenticated route `POST /api/v1/care/pet`.

- [ ] **Step 1: Add failing runtime petting mutation/replay tests**

In `runtime.rs` tests create a pet with one affection demand in a restored snapshot. Call runtime `pet(...)` with a fixed action ID twice. Assert first receipt has `duplicate == false`, removes one affection demand and raises happiness; second has `duplicate == true` with no second removal/recovery.

- [ ] **Step 2: Add runtime `pet` adapter**

Implement:

```rust
pub fn pet(
    &self,
    action_id: Uuid,
    interaction_ms: u64,
    pointer_distance: f32,
) -> Result<MutationReceipt, RuntimeError> {
    self.apply_care(CareCommand::Pet {
        action_id,
        interaction_ms,
        pointer_distance,
    })
}
```

No special persistence path: reuse `apply_care`.

- [ ] **Step 3: Add failing protocol/server route tests**

Add serialization/deserialization test for camelCase `PetRequest`:

```json
{"actionId":"00000000-0000-0000-0000-000000000001","interactionMs":1500,"pointerDistance":120.0}
```

Add server test asserting an authenticated POST to `/api/v1/care/pet` returns 200 for a valid gesture and 422 `insufficient_duration` / `insufficient_distance` for invalid gestures.

- [ ] **Step 4: Wire the pet endpoint**

Import `PetRequest`, add:

```rust
.route("/api/v1/care/pet", post(pet_handler))
```

and:

```rust
async fn pet_handler(
    State(state): State<AppState>,
    BoundedJson(request): BoundedJson<PetRequest>,
) -> Response {
    match state.runtime.pet(
        request.action_id,
        request.interaction_ms,
        request.pointer_distance,
    ) {
        Ok(receipt) => mutation_response(receipt),
        Err(error) => runtime_error_response(error),
    }
}
```

- [ ] **Step 5: Add critical-happiness denial copy**

Extend `denial_reason`:

```rust
WorkReasonCode::CriticalHappiness => {
    "The pet refuses this action because it desperately needs attention. Pet it in the CodeGotchi UI, then retry the Codex request afterward."
}
```

Add a focused test verifying strict denial includes this message when happiness is severe and the other needs are healthy.

- [ ] **Step 6: Add maintenance persistence/broadcast catch-up test**

Use a persisted snapshot with `last_updated_at` in the past and `next_incident_at` also in the past. Call `maintenance_tick_at(target)` and assert:

- returns `true`;
- persisted reloaded snapshot contains generated incidents and progressed needs;
- a subscribed receiver receives that exact snapshot;
- long-gap fixture adds at most five catch-up incidents;
- a second tick one second later does not immediately drain another missed backlog.

This test is important because incidents can appear without any Codex hook event.

- [ ] **Step 7: Run CLI/runtime tests and commit**

Run:

```bash
cargo test -p codegotchi-cli runtime -- --nocapture
cargo test -p codegotchi-cli server -- --nocapture
cargo test -p codegotchi-cli protocol -- --nocapture
cargo test --workspace
cargo fmt --all -- --check
```

Expected: PASS.

Commit:

```bash
git add crates/codegotchi-cli/src/protocol.rs crates/codegotchi-cli/src/runtime.rs crates/codegotchi-cli/src/server.rs
git commit -m "feat: expose authoritative CodeGotchi petting care"
```

---

### Task 5: Extend the browser protocol and client care adapter

**Files:**
- Modify: `web/src/protocol.ts`
- Modify: `web/src/client.ts`
- Modify: `web/src/client.test.ts`
- Modify: `web/src/useCodeGotchi.ts`

**Interfaces:**
- Produces:

```ts
export type PetDemandKind = "affection" | "snack";

export interface PetDemand {
    id: string;
    kind: PetDemandKind;
    createdAt: string;
}
```

- Extends `SimulationSnapshot` with:

```ts
pendingDemands: PetDemand[];
attentionSequence: number;
nextIncidentAt: string;
```

- Produces:

```ts
CodeGotchiClient.pet(
    interactionMs: number,
    pointerDistance: number,
    actionId?: string,
): Promise<CareResponse>
```

- Exposes `useCodeGotchi().pet(interactionMs, pointerDistance): Promise<void>`.

- [ ] **Step 1: Update protocol fixture builders first**

Search web tests for inline `SimulationSnapshot` fixtures and add:

```ts
pendingDemands: [],
attentionSequence: 0,
nextIncidentAt: "2026-08-13T12:05:00Z",
```

before changing the interface, so TypeScript failures remain localized.

- [ ] **Step 2: Extend `protocol.ts`**

Add exact demand types and the three snapshot fields. Keep them mandatory in the current browser protocol because the Rust runtime normalizes legacy state before serving it.

- [ ] **Step 3: Write failing client pet-request test**

In `client.test.ts`, invoke:

```ts
await client.pet(1_750, 180, "00000000-0000-0000-0000-000000000099");
```

Assert the fetch call uses:

- URL `/api/v1/care/pet`
- method `POST`
- bearer header
- JSON body:

```json
{
  "actionId": "00000000-0000-0000-0000-000000000099",
  "interactionMs": 1750,
  "pointerDistance": 180
}
```

and publishes the returned snapshot through the existing `publishSnapshot` path.

- [ ] **Step 4: Implement client pet care**

Expand the private care action union to include `"pet"`, then add:

```ts
public async pet(
    interactionMs: number,
    pointerDistance: number,
    actionId: string = createActionId(),
): Promise<CareResponse> {
    return this.care("pet", {
        actionId,
        interactionMs: String(interactionMs),
        pointerDistance: String(pointerDistance),
    });
}
```

Do **not** keep `care` typed as `Record<string, string>` if that forces numeric values into strings. Instead change its body type to `Record<string, string | number>` and send actual JSON numbers:

```ts
return this.care("pet", { actionId, interactionMs, pointerDistance });
```

- [ ] **Step 5: Expose petting from `useCodeGotchi`**

Add to `CodeGotchiState`:

```ts
pet: (interactionMs: number, pointerDistance: number) => Promise<void>;
```

Implement with the same authoritative/error pattern as feed/clean/nap:

```ts
const pet = useCallback(async (interactionMs: number, pointerDistance: number) => {
    const client = clientRef.current;
    if (!client) return;
    try {
        await client.pet(interactionMs, pointerDistance);
        setError(null);
        setFeedback("Got some attention ♡");
    } catch (nextError) {
        setError(asClientError(nextError));
    }
}, []);
```

- [ ] **Step 6: Run focused web adapter tests and commit**

Run:

```bash
corepack pnpm --filter @codegotchi/web test -- src/client.test.ts
corepack pnpm --filter @codegotchi/web lint
corepack pnpm --filter @codegotchi/web build
```

Expected: PASS.

Commit:

```bash
git add web/src/protocol.ts web/src/client.ts web/src/client.test.ts web/src/useCodeGotchi.ts
git commit -m "feat: add browser petting care protocol"
```

---

### Task 6: Render demands and measure a real petting gesture

**Files:**
- Modify: `web/src/App.tsx`
- Modify: `web/src/App.css`
- Modify: `web/src/App.test.tsx`

**Interfaces:**
- Consumes `snapshot.pendingDemands` and `useCodeGotchi().pet`.
- Produces no new persisted/browser-authoritative state; local pointer state exists only for measuring one gesture.

- [ ] **Step 1: Add failing demand-bubble rendering tests**

Create snapshot fixtures with two affection demands and one snack demand. Assert:

```tsx
expect(screen.getByText("Needs attention")).toBeInTheDocument();
expect(screen.getByText("Wants a snack")).toBeInTheDocument();
expect(screen.getByTestId("demand-affection-count")).toHaveTextContent("2");
expect(screen.getByTestId("demand-snack-count")).toHaveTextContent("1");
```

Also assert poops remain represented only by existing poop floor buttons, not a new poop demand bubble.

- [ ] **Step 2: Implement grouped demand presentation**

Derive counts from authoritative snapshot each render:

```ts
const affectionDemandCount = snapshot.pendingDemands.filter(
    (demand) => demand.kind === "affection",
).length;
const snackDemandCount = snapshot.pendingDemands.filter(
    (demand) => demand.kind === "snack",
).length;
```

Inside the room illustration, add a `demand-stack` sibling near the pet, not inside motion-transform logic. Render one bubble per nonzero kind, with accessible visible text and a count badge. Use `aria-live="polite"` on the stack so newly arrived needs are announced without taking focus.

- [ ] **Step 3: Add failing pointer-gesture adapter test**

Mock `useCodeGotchi` or the client seam already used by `App.test.tsx`. Fire pointer events against `data-testid="pet"` with controlled timestamps/coordinates:

```tsx
fireEvent.pointerDown(pet, { clientX: 10, clientY: 10 });
fireEvent.pointerMove(pet, { clientX: 70, clientY: 10 });
fireEvent.pointerMove(pet, { clientX: 130, clientY: 10 });
// advance mocked performance/Date clock to >=1500ms
fireEvent.pointerUp(pet, { clientX: 130, clientY: 10 });
expect(petCare).toHaveBeenCalledWith(1_500, 120);
```

Use the test environment's fake clock rather than trusting synthetic event `timeStamp` values.

- [ ] **Step 4: Implement path-distance measurement without changing pet motion**

Add local refs/state:

```ts
interface PetGesture {
    startedAt: number;
    lastX: number;
    lastY: number;
    distance: number;
    pointerId: number;
}

const petGestureRef = useRef<PetGesture | null>(null);
```

Import `useRef`. On pointer down:

- ignore secondary mouse buttons;
- call `event.currentTarget.setPointerCapture(event.pointerId)`;
- store `performance.now()`, coordinates, distance `0`, pointer ID.

On pointer move for the captured pointer:

```ts
const dx = event.clientX - gesture.lastX;
const dy = event.clientY - gesture.lastY;
gesture.distance += Math.hypot(dx, dy);
gesture.lastX = event.clientX;
gesture.lastY = event.clientY;
```

On pointer up:

```ts
const duration = Math.max(0, Math.round(performance.now() - gesture.startedAt));
const distance = gesture.distance;
petGestureRef.current = null;
void pet(duration, distance);
```

On pointer cancel, clear the ref and send nothing.

Do not locally clear demand bubbles; they disappear only when the returned/broadcast snapshot changes.

- [ ] **Step 5: Add backend-error UI regression test**

Make mocked pet care reject with `{ code: "insufficient_duration", message: "petting duration is below the minimum" }`. Verify the authoritative demand remains rendered and the existing error banner shows the backend message.

- [ ] **Step 6: Style the demand stack and petting cursor**

Add CSS that positions `.demand-stack` relative to the room rather than modifying `.pet` transform. Demand bubbles should be compact, readable, and pointer-events none so they never intercept petting. Add `touch-action: none` and an appropriate grab/pet cursor to the pet element only if it does not break existing drag/drop targets.

- [ ] **Step 7: Run App tests, lint, build, and commit**

Run:

```bash
corepack pnpm --filter @codegotchi/web test -- src/App.test.tsx
corepack pnpm --filter @codegotchi/web lint
corepack pnpm --filter @codegotchi/web format:check
corepack pnpm --filter @codegotchi/web build
```

Expected: PASS.

Commit:

```bash
git add web/src/App.tsx web/src/App.css web/src/App.test.tsx
git commit -m "feat: make CodeGotchi demand hands-on attention"
```

---

### Task 7: Production browser acceptance for demands, petting, and overdue catch-up

**Files:**
- Modify: `web/e2e/fixture.mjs`
- Modify: `web/e2e/mvp.spec.ts`

**Interfaces:**
- Fixture must seed authoritative Rust state; it must not inject browser-only demand state.

- [ ] **Step 1: Extend fixture state builder with attention fields**

Where the production fixture constructs/persists a simulation snapshot, add deterministic fields matching Rust JSON camelCase:

```js
pendingDemands: [],
attentionSequence: 0,
nextIncidentAt: new Date(Date.now() + 5 * 60_000).toISOString(),
```

Add fixture modes/arguments that can seed:

- one affection demand;
- one snack demand;
- one poop;
- severe happiness with otherwise healthy needs;
- an overdue `nextIncidentAt` plus past `lastUpdatedAt` for catch-up.

Keep all fixture timestamps near the current wall clock so existing future-time reanchoring logic does not hide the scenario.

- [ ] **Step 2: Add affection browser test**

Open a fixture with one affection demand. Verify `Needs attention`, then perform a real Playwright pointer gesture on the pet lasting >=1.5 seconds with >=120 px cumulative travel. Assert the demand disappears after the server response/WebSocket update and happiness increases.

Do not use direct HTTP to resolve the demand in this test; exercise the room interaction.

- [ ] **Step 3: Add snack and poop interaction test**

Start with one snack demand and one poop. Click/drag kibble through the existing UI, assert exactly one snack demand disappears, then use the existing shovel + trash path and assert the poop disappears. Verify the final state contains neither outstanding item.

- [ ] **Step 4: Add severe-happiness strict denial test**

Run strict mode with happiness `<=5` and other needs healthy. Trigger a safe development PreToolUse fixture and assert the denial reason contains `desperately needs attention` / `Pet it in the CodeGotchi UI`.

- [ ] **Step 5: Add overdue restart/catch-up test**

Seed a persisted snapshot with `lastUpdatedAt` well in the past and `nextIncidentAt` in the past. Start the production fixture and wait for maintenance. Fetch `/api/v1/state` through the authenticated fixture helper and assert:

- needs reflect elapsed wall-clock deterioration;
- at least one attention incident exists;
- no more than five catch-up attention incidents were created for the gap;
- `nextIncidentAt` is now in the future;
- refreshing the browser does not reroll or clear the state.

- [ ] **Step 6: Run production Playwright suite and commit**

Run:

```bash
node web/scripts/embed-web.mjs
corepack pnpm playwright:test
```

On WSL if required:

```bash
LD_LIBRARY_PATH=/usr/lib/wsl/lib corepack pnpm playwright:test
```

Expected: PASS.

Commit:

```bash
git add web/e2e/fixture.mjs web/e2e/mvp.spec.ts
git commit -m "test: cover real-time CodeGotchi neglect flow"
```

---

### Task 8: Documentation and full verification

**Files:**
- Modify: `README.md`

**Interfaces:**
- No new runtime interfaces. This task verifies and documents the completed behavior.

- [ ] **Step 1: Update README strict-mode/care documentation**

Replace statements implying needs only meaningfully change during active work. Document:

- hunger `+25/hour` and energy `-50/hour` in real time;
- 3–5 minute wall-clock affection/snack/poop incidents;
- happiness as a strict-mode need;
- unresolved incident pressure;
- maximum five catch-up incident objects per long elapsed advancement;
- long absence can immediately restore into severe strict-mode blocking;
- petting gesture requirements and the fact that the room/backend remain authoritative.

Keep the warning that strict mode is a pet-care interaction, not an OS security boundary.

- [ ] **Step 2: Run all Rust quality gates**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Expected: all PASS with zero warnings.

- [ ] **Step 3: Run all web quality gates**

Run:

```bash
corepack pnpm lint
corepack pnpm test
corepack pnpm format:check
corepack pnpm build
node web/scripts/embed-web.mjs
corepack pnpm playwright:test
```

Expected: all PASS.

- [ ] **Step 4: Run targeted timing invariants one final time**

Run exact focused tests added in Tasks 2–4 and verify these product invariants from assertions, not manual observation:

- 30 minutes unattended => severe strict denial for uncertain work;
- 24-hour single catch-up => max five new incident objects and future `next_incident_at`;
- second maintenance tick after catch-up => no immediate hidden backlog;
- legacy snapshot => no retroactive incident burst, first incident scheduled 3–5 minutes after restore;
- one affection pet gesture => exactly one affection removed;
- energy drink => snack demand remains.

- [ ] **Step 5: Commit docs and verification-ready state**

```bash
git add README.md
git commit -m "docs: explain real-time CodeGotchi care pressure"
```

---

## Final Acceptance Checklist

Before declaring the feature complete, verify every item below from tests or a production browser run:

- [ ] Hunger rises `25` points per real hour in Active, Idle, WaitingForUser, and Blocked states.
- [ ] Energy falls `50` points per real hour outside hammock nap overlap in all normal states.
- [ ] Incidents are scheduled at deterministic randomized 180–300 second wall-clock intervals.
- [ ] Every three incident kinds contain affection + snack + poop exactly once.
- [ ] Browser closure does not own or reset the schedule.
- [ ] Runtime restart restores the exact future due time or catches up overdue time.
- [ ] A single long catch-up adds no more than five missed incident objects.
- [ ] Long catch-up still progresses needs across the complete elapsed wall-clock interval.
- [ ] No hidden backlog emits another five incidents on the next one-second maintenance tick.
- [ ] Each affection demand drains happiness at `240/hour` until resolved.
- [ ] Each snack demand raises hunger at `240/hour` until resolved.
- [ ] Each poop drains cleanliness at `240/hour` until cleaned.
- [ ] Incident pressure stacks linearly.
- [ ] Petting one valid gesture resolves exactly one oldest affection demand.
- [ ] Kibble/treat/fruit resolve exactly one oldest snack demand.
- [ ] Energy drinks do not resolve snack demands.
- [ ] Cleaning one poop removes only that poop.
- [ ] Happiness participates in mild/moderate/severe strict enforcement.
- [ ] Severe happiness blocks uncertain work and denial copy tells the user to pet CodeGotchi.
- [ ] Thirty unattended minutes from healthy state reach severe strict-mode blocking across the deterministic schedule test set.
- [ ] Demand bubbles are authoritative and survive browser reload.
- [ ] Existing food, poop, hammock, movement, blinking, authentication, persistence, and WebSocket flows remain green.
- [ ] Full Rust + web + Playwright quality gates pass.
