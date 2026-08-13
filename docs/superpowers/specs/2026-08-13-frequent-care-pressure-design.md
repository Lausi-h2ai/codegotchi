# Frequent Care Pressure Design

## Goal

Make CodeGotchi behave like a creature that continues to need care in real time, not like a productivity timer that only advances while Codex is busy. The pet should usually create a small care problem every 3–5 minutes of wall-clock time, remain needy while the user is away, and greet a returning user with believable neglect: hunger, tiredness, reduced happiness, and some accumulated poop or other demands.

The feature must continue to use the authoritative Rust simulation and the existing need-based strict-mode enforcement. It must not add a second blocking system or make the browser authoritative for timers, randomness, incident creation, or need changes.

## Product Experience

The intended loop is:

1. CodeGotchi exists continuously in wall-clock time while its state persists.
2. Roughly every 3–5 real minutes, a new care incident becomes due.
3. The room makes that problem visible: an affection or snack demand appears, or a real poop appears on the floor.
4. The user can resolve it with an existing-style room interaction: pet the creature, feed it, or clean the poop.
5. If the problem is ignored, the associated need deteriorates quickly in real time.
6. Additional incidents continue to appear and may overlap.
7. Existing mild, moderate, and severe strict-mode enforcement reacts only to the resulting need values.
8. Returning after a lunch break should commonly reveal several chores and visibly worse needs.
9. Returning after a long absence should reveal a clearly neglected pet and may immediately place strict mode in the severe tier, but the room must not contain dozens or hundreds of accumulated incident objects.

The pet does not freeze because Codex is idle, waiting for the user, the browser tab is closed, the process was suspended, or the runtime was restarted. Elapsed wall-clock time is authoritative.

## Baseline Need Progression

Replace activity-dependent hunger and normal energy rates with wall-clock physiology:

- hunger: `+25.0` points/hour
- energy: `-50.0` points/hour

These rates apply regardless of `Active`, `Idle`, `WaitingForUser`, or `Blocked` activity. Hunger and energy therefore continue to worsen while the user is away.

The existing five-second hammock nap keeps its special fast energy recovery for the portion of elapsed time covered by the nap window. Normal `-50/hour` energy decay applies outside that window.

The old idle/waiting rates of `+1 hunger/hour` and `+8 energy/hour` are removed. A lunch break is not passive recovery; the caretaker must actually feed or rest the pet.

## Attention Incidents

### Incident kinds

The first version has exactly three incident kinds:

- `Affection`: creates one pending affection demand and pressures happiness until resolved.
- `Snack`: creates one pending snack demand and pressures hunger until resolved.
- `Poop`: creates one real authoritative poop using the existing poop representation and pressures cleanliness until cleaned.

No toy, minigame, medicine, thirst, sickness, or additional need is introduced.

### Wall-clock cadence

Each incident is scheduled after an inclusive randomized delay of 180–300 seconds of wall-clock time. After an incident is created, the next interval is derived immediately.

The scheduler stores an absolute `next_incident_at` timestamp. It does not pause for idle state, `WaitingForUser`, blocked work, browser closure, or runtime restart.

A new pet receives its first `next_incident_at` as `created_at + derived_delay(sequence=0)`. A persisted pet restores the exact already-scheduled timestamp, so reopening CodeGotchi does not reroll an almost-due incident.

### Deterministic variety

Incident timing and kind selection are deterministic pseudo-random domain behavior derived from the pet ID and incident sequence. Important domain behavior does not use the existing cosmetic `RandomSource` port.

Kinds use three-item shuffle bags. Every consecutive group of three incidents contains exactly one `Affection`, one `Snack`, and one `Poop`, with order deterministically shuffled per bag. The six permutations are selected from a UUID-v5 hash of `(pet_id, bag_index)`.

Each delay is independently derived from a UUID-v5 hash of `(pet_id, incident_sequence)` and mapped into the inclusive 180,000–300,000 ms range.

Demand and random-poop IDs are also deterministic UUID-v5 values derived from `(pet_id, incident_sequence)` using distinct names from the existing work/digestion poop sequence.

## Incident Pressure

Unresolved incidents are cheap when handled quickly and punishing when ignored. Pressure applies in wall-clock time, including while Codex is idle or closed.

Each unresolved item applies `240.0` need points/hour of additional pressure:

- pending `Affection`: happiness `-240.0/hour`
- pending `Snack`: hunger `+240.0/hour`
- pending poop: cleanliness `-240.0/hour`

Pressure stacks linearly. Two unresolved affection demands drain happiness at `-480/hour`.

At `240/hour`, one unresolved problem costs four need points per minute. An incident that is ignored for twenty-five minutes can by itself drive a full 100-point lower-is-better need to zero. Because the first incident is guaranteed within five minutes, thirty minutes without care must reach severe-scale neglect even under the slowest allowed cadence.

The old `-2 cleanliness/hour` per poop is replaced by the unified `-240/hour` per unresolved poop. Existing work/digestion-generated poops and random attention poops behave identically once present.

## Bounded Catch-up after Long Gaps

Real-time progression must survive long process gaps without producing absurd object counts.

Set `MAX_CATCH_UP_INCIDENTS` to `5` for any single elapsed-time advancement. This cap normally has no effect while the runtime is healthy because the maintenance loop advances once per second. It matters after restart, laptop suspend, process stall, large test jumps, or clock gaps.

When advancing from `last_updated_at` to a target timestamp:

1. Progress baseline needs and all currently unresolved incident pressure up to `next_incident_at`.
2. If the incident is due and fewer than five incidents have been created in this advancement, create it at its scheduled timestamp, increment the sequence, derive the next delay, and continue.
3. Repeat until the target is reached or five incidents have been created.
4. If more historical incidents would still be due after the fifth, deliberately discard those missed incident objects.
5. Progress the remaining elapsed wall-clock interval using the five newly created incidents plus any earlier unresolved state, so long absences still create severe neglect.
6. Re-anchor `next_incident_at` to `target + derived_delay(current_sequence)` so the next live incident arrives normally in another 3–5 minutes rather than draining a hidden backlog one maintenance tick at a time.

The cap is intentionally lossy for missed incident objects, not for elapsed need progression. After an eight-hour absence the pet may be starving, exhausted, filthy, and miserable, but the room gains at most five catch-up incidents from that single gap.

For elapsed windows that contain at most five due incidents, advancing in one large jump and advancing through one-second ticks must produce identical authoritative state.

## Pending Demand State

Add focused domain state for non-poop demands:

```rust
pub enum PetDemandKind {
    Affection,
    Snack,
}

pub struct PetDemand {
    id: Uuid,
    kind: PetDemandKind,
    created_at: DateTime<Utc>,
}
```

The authoritative simulation snapshot persists:

- `pending_demands: Vec<PetDemand>`
- `attention_sequence: u64`
- `next_incident_at: DateTime<Utc>`

`Pet` owns pending demands alongside pending poops. `PetSimulation` owns the sequence and next scheduled timestamp.

Older snapshots that predate these fields restore with an empty demand backlog and initialize `next_incident_at` to `restore_wall_clock + derived_delay(sequence=0)`. The feature does not retroactively invent incidents for time during which the old application version had no attention scheduler.

The additive fields use serde defaults plus explicit restore migration. Keep the existing snapshot schema version if backward compatibility remains clean; increase it only if implementation proves an invariant cannot be expressed safely with additive defaults.

## Care Resolution

The feature reuses actual care interactions rather than adding generic dismiss buttons.

### Affection

A successful `CareCommand::Pet` resolves exactly the oldest pending `Affection` demand, if one exists, in addition to its existing `+10 happiness` effect.

The existing domain validation remains unchanged: at least 1,500 ms of interaction and at least 120 px of pointer travel. The CLI/server/web must expose this already-existing domain command rather than weakening it.

### Snack

A successful feed with kibble, treat, or fruit resolves exactly the oldest pending `Snack` demand, if one exists, in addition to normal food effects. An energy drink does not satisfy a snack request.

Feeding remains allowed when no snack demand exists.

### Poop

Existing `CleanPoop` removes exactly the selected poop. Removing that poop immediately removes its cleanliness pressure and retains the existing `+25 cleanliness` recovery.

### Energy

Hammock naps and energy drinks retain existing behavior. They do not resolve affection or snack demands.

## Enforcement

Do not add incident-specific refusal logic. `WorkPermissionPolicy` remains the only source of strict-mode blocking.

Keep the current hunger, energy, and cleanliness thresholds:

- mild: hunger `>= 70`, energy `<= 30`, cleanliness `<= 30`
- moderate: hunger `>= 85`, energy `<= 15`, cleanliness `<= 15`
- severe: hunger `>= 95`, energy `<= 5`, cleanliness `<= 5`

Add happiness as a fourth enforceable need:

- mild happiness: `<= 30`
- moderate happiness: `<= 15`
- severe happiness: `<= 5`

Add `CriticalHappiness` and `RequiredAction::Pet` to the structured decision model. Dominant-need selection compares normalized neglect across all four needs with deterministic tie breaking: hunger, energy, cleanliness, happiness.

Blocked command scopes remain unchanged:

- mild blocks safe development work;
- moderate blocks all classified work except CodeGotchi control and uncertain work;
- severe blocks everything except CodeGotchi control.

Decorative and gentle modes keep their current meanings.

## Pet Behavior

`BehaviorCoordinator` must treat happiness `<= 10` as `CriticalNeed`, alongside current hunger, energy, and cleanliness checks.

Outstanding affection/snack demands do not add a new top-level `PetBehavior`. They are explicit snapshot state and may coexist with working, success, failure, sleeping, and critical presentation.

## API and Protocol

Extend the authoritative snapshot JSON and TypeScript protocol with pending demands, attention sequence, and `nextIncidentAt`.

Expose the existing `CareCommand::Pet` through:

- `POST /api/v1/care/pet`
- request fields `actionId`, `interactionMs`, `pointerDistance`
- `AuthoritativeRuntime::pet(...)`
- `CodeGotchiClient.pet(...)`
- `useCodeGotchi().pet(...)`

The route uses the same bearer authentication, replay-safe action IDs, persistence, mutation receipt, and WebSocket broadcast path as feed, clean, and nap.

No incident creation API is added. Incidents originate only from simulation time advancement.

## Room UI

The browser remains a projection of authoritative state.

Render a compact demand stack near the pet:

- affection: heart icon plus accessible text `Needs attention`
- snack: bowl/food icon plus accessible text `Wants a snack`
- multiple demands of one kind collapse visually into one bubble with a count badge

Poops remain physical floor objects and are not duplicated as thought bubbles.

Make the existing pet element pointer-interactive for petting. Pointer-down records start time and position; pointer-move accumulates path length; pointer-up sends measured `interactionMs` and `pointerDistance`. The backend decides whether the gesture is valid.

The UI must not optimistically clear a demand or mutate needs. It waits for the authoritative mutation response / WebSocket snapshot.

Feeding, shovel/trash cleaning, hammock use, movement, blinking, and existing activity presentation remain intact.

## Persistence and Restart Behavior

SQLite remains the sole persisted source of truth.

A restart restores pending demands, pending poops, need values, attention sequence, and the exact `next_incident_at`. The first maintenance advancement then applies elapsed wall-clock progression and creates up to five missed incidents.

Examples of intended behavior:

- Return after ~20 minutes: expect clearly worse hunger/energy and commonly several total demands/poops, depending on the deterministic schedule and prior state.
- Return after ~30–60 minutes without care: strict mode should commonly be severe and normal coding effectively unavailable until care is performed.
- Return after overnight or several days: needs clamp at their extrema; at most five incident objects are added for the long catch-up advancement; the pet is severely neglected rather than the room containing an unbounded backlog.

Browser reload has no timing effect because the browser owns no incident clock.

## Testing

### Domain

Add deterministic tests covering:

- hunger progresses `+25/hour` and energy `-50/hour` in active, idle, waiting, and blocked states;
- hammock overlap still uses nap recovery for only the covered interval;
- delay derivation is always in inclusive 180–300 seconds;
- every three consecutive incident kinds contain one affection, one snack, and one poop;
- identical pet ID + sequence derives identical delays, kinds, and IDs;
- wall-clock incident scheduling continues regardless of activity state;
- one large advancement and equivalent one-second ticks match when at most five incidents are due;
- a long advancement creates exactly five catch-up incidents, progresses needs across the full elapsed interval, and re-anchors the next incident after the target;
- affection, snack, and poop pressure apply `240/hour` per unresolved item in every activity state and stack linearly;
- petting resolves exactly one oldest affection demand;
- kibble/treat/fruit resolve exactly one oldest snack demand;
- energy drinks do not resolve snack demands;
- cleaning one poop removes only that poop's pressure;
- happiness participates in mild/moderate/severe enforcement;
- severe happiness blocks uncertain work like the other severe needs;
- `BehaviorCoordinator` reports `CriticalNeed` at happiness `<= 10`;
- thirty minutes of wall-clock time without care reaches severe neglect for every tested legal 3–5 minute schedule/shuffle-bag ordering;
- legacy snapshots migrate to an empty attention backlog and a first incident scheduled after restore, not in the past.

### Runtime/server

Cover persistence and broadcast behavior for automatically generated incidents and the pet endpoint. A maintenance tick that crosses an incident boundary must persist and broadcast even without a Codex hook event. A restart fixture must prove elapsed offline wall-clock time is applied on the first maintenance advancement and that catch-up is capped at five incidents.

### Web

Add tests for protocol fields, demand bubbles/count badges, pointer gesture measurement, the pet care request, backend validation errors, authoritative clearing of one demand, and regression coverage for existing food/poop/hammock/motion/blink behavior.

### End-to-end

Extend the production Playwright fixture with deterministic persisted demand state instead of waiting real minutes:

1. show an affection demand;
2. perform a valid petting gesture and observe authoritative removal;
3. show a snack demand and satisfy it with food;
4. show a poop and clean it through shovel/trash;
5. verify strict denial copy can request petting for critical happiness;
6. restore a snapshot whose `nextIncidentAt` is in the past and verify catch-up state appears after maintenance without browser-owned timers.

## Acceptance Criteria

The feature is complete when:

- incidents occur at randomized 3–5 minute wall-clock intervals;
- idle/waiting time and browser closure do not freeze needs or incident timing;
- runtime restart applies elapsed real time;
- one long catch-up advancement creates at most five missed incident objects;
- hunger and energy use `+25/hour` and `-50/hour` across normal wall-clock time;
- ignored affection, snack, and poop incidents degrade happiness, hunger, and cleanliness respectively at `240/hour` each;
- multiple incidents stack;
- happiness is a first-class strict-mode blocking need;
- no blocking mechanism exists outside `WorkPermissionPolicy`;
- thirty minutes with no care reaches the existing severe near-total refusal tier;
- restarting neither clears existing demands nor rerolls an already scheduled future incident;
- the browser cannot manufacture, dismiss, schedule, or locally mutate incidents;
- returning after a long absence produces severe neglect and a bounded physical backlog rather than freezing the pet or generating unbounded objects;
- all existing Rust, web, formatting, lint, build, and Playwright quality gates remain green.

## Out of Scope

This feature does not add a store, replenishing food economy, toys, play minigames, medicine, sickness, thirst, pet death, desktop notifications, sounds, OS-level blocking, new enforcement modes, configurable difficulty, or a generic incident scripting engine.
