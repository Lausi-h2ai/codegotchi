# Frequent Care Pressure Design

## Goal

Make CodeGotchi require regular caretaker attention during active coding. While the agent is actively working, the pet should produce a small care problem roughly every 3–5 minutes. A user who reacts promptly can clear each problem with a quick existing room interaction. A user who ignores several problems should naturally move through the existing mild, moderate, and severe strict-mode enforcement tiers until normal coding becomes effectively impossible after roughly half an hour of continuous neglect.

The system must achieve this by strengthening the authoritative pet simulation and reusing the existing need-based enforcement policy. It must not add a second independent blocking system or make the browser authoritative for timers, randomness, or need changes.

## Product Experience

The intended loop is:

1. The coding agent works normally.
2. After 3–5 minutes of accumulated active agent time, CodeGotchi creates one care incident.
3. The room makes the incident obvious: an affection or snack thought bubble appears, or a new poop appears on the floor.
4. The caretaker can resolve it immediately with a short interaction: pet the creature, feed it, or clean the poop.
5. While an incident is unresolved and the agent continues active work, the associated need deteriorates quickly.
6. Additional incidents continue to arrive every 3–5 minutes of active work and may overlap.
7. The existing strict-mode policy reacts to the resulting need values. Mild neglect blocks normal development work, moderate neglect widens the blocked command set, and severe neglect allows only CodeGotchi control.
8. A caretaker who has ignored the room for about 30 minutes should normally return to several outstanding chores and at least one severely neglected need.

The system deliberately distinguishes active coding from breaks. New incidents are scheduled against accumulated active agent time, not raw wall-clock time, so leaving Codex waiting for the user or taking a break does not create a large unseen incident backlog.

## Need Progression

The existing active need rates are too slow for this product loop. Change the active progression constants to:

- active hunger: `+25.0` points/hour
- active energy: `-50.0` points/hour

Keep the existing idle rates unchanged:

- idle/waiting hunger: `+1.0` point/hour
- idle/waiting energy: `+8.0` points/hour

The five-second hammock nap keeps its existing recovery behavior.

The accelerated baseline makes food and rest relevant during an ordinary coding session, while the random incident system supplies the much faster 3–5 minute interaction cadence.

## Attention Incidents

### Incident kinds

The first version has exactly three incident kinds:

- `Affection`: CodeGotchi wants direct attention. It creates one pending affection demand and pressures happiness until resolved.
- `Snack`: CodeGotchi wants food. It creates one pending snack demand and pressures hunger until resolved.
- `Poop`: CodeGotchi creates a real authoritative poop using the existing poop representation. The poop pressures cleanliness until cleaned.

No new toy, minigame, medicine, thirst, or bathroom need is added in this feature.

### Cadence

Each incident is scheduled after an inclusive randomized delay of 180–300 seconds of accumulated active agent time. The interval is chosen again after every incident.

The incident clock advances only while the authoritative aggregate activity is active agent work. It pauses while the agent is idle, waiting for the user, or napping. A blocked call does not generate additional incident cadence by itself. Existing unresolved incidents remain present until cared for.

The schedule is simulation state and therefore persists across browser reloads and runtime restarts. Restarting CodeGotchi must not reset an almost-due incident to another fresh five-minute wait.

### Variety without starvation

Incident timing and kind selection are deterministic pseudo-random domain behavior derived from the pet ID and an incident sequence number; they do not use the existing cosmetic `RandomSource` port.

Kinds use randomized three-item shuffle bags. Every consecutive group of three incidents contains exactly one `Affection`, one `Snack`, and one `Poop`, with the order randomized per group. This prevents long random streaks that would make one care interaction disappear for an entire session while preserving an unpredictable order.

The six possible permutations are selected deterministically from a UUID-v5 hash of `(pet_id, bag_index)`. Each delay is independently derived from a UUID-v5 hash of `(pet_id, incident_sequence)` and mapped into the inclusive 180,000–300,000 ms range. The same pet snapshot therefore produces the same future schedule after restoration.

## Unresolved Incident Pressure

Incidents are intentionally cheap when handled quickly and punishing when ignored.

While the agent is actively working, each unresolved incident applies `240.0` need points/hour of additional pressure:

- every pending `Affection` demand: happiness `-240.0/hour`
- every pending `Snack` demand: hunger `+240.0/hour`
- every pending poop: cleanliness `-240.0/hour`

Pressure stacks linearly. Two unresolved affection demands therefore drain happiness at `-480.0/hour` while active.

This rate is deliberately much faster than baseline physiology. One unresolved problem costs four need points per active minute. A quick response after a minute or two is minor; repeatedly ignoring incidents causes compounding neglect. With a maximum five-minute incident spacing, six incidents can occur within 30 active minutes. The accumulated age of those six incidents is sufficient for at least one incident class to apply severe-scale pressure even before passive hunger and energy decay are considered.

Incident pressure pauses whenever active coding pauses. The pet does not continue losing hundreds of points per hour while Codex is merely waiting for the user or while the runtime is not actively being used.

The existing low cleanliness decay of `-2.0/hour` per poop is replaced by the incident pressure above. All authoritative poops, including those produced by the existing work/digestion threshold mechanism, use the same high-pressure cleanliness behavior; there is no distinction between a random attention poop and a threshold-generated poop once it exists.

## Pending Demand State

Add a small domain type for non-poop demands:

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

The simulation snapshot persists:

- `pending_demands: Vec<PetDemand>`
- `attention_sequence: u64`
- `active_ms_until_next_incident: u64`

Demand IDs are UUID-v5 values derived from `(pet_id, incident_sequence)` so replay and restore cannot create duplicate logical incidents.

New pets initialize `active_ms_until_next_incident` from incident sequence zero. Older snapshots that predate these fields restore with no pending demands and receive a freshly derived first interval instead of generating an immediate incident. The additive snapshot fields use serde defaults and a restore migration; the snapshot schema version does not need to change for this backward-compatible addition.

## Time Advancement

The simulation must remain deterministic whether time advances in one large jump or through the production one-second maintenance tick.

When active elapsed time crosses one or more incident boundaries, `PetSimulation` splits the elapsed interval at each boundary:

1. progress baseline needs and all currently unresolved incident pressure up to the next incident boundary;
2. create exactly one incident at that logical timestamp;
3. advance the incident sequence and derive the next 180–300 second interval;
4. continue with the remaining elapsed time;
5. repeat until the target timestamp is reached.

This prevents a 30-minute test jump from incorrectly creating six incidents at the end of the interval with zero accumulated pressure. A single 30-minute jump and thirty minutes of one-second maintenance ticks must produce the same authoritative result.

Idle/waiting elapsed time uses the existing normal need progression and does not decrement `active_ms_until_next_incident`.

## Care Resolution

The feature reuses the current care commands instead of adding bespoke incident-dismiss buttons.

### Affection

A successful `CareCommand::Pet` resolves the oldest pending `Affection` demand, if one exists, in addition to its existing happiness recovery. One petting gesture resolves one demand; it does not clear an entire backlog.

The domain already validates a real petting gesture with at least 1,500 ms duration and 120 px pointer travel. The CLI/server/web currently do not expose that care command, so this feature must wire the existing domain action through the authenticated care API and room UI rather than weakening those validation rules.

### Snack

A successful `CareCommand::Feed` with kibble, treat, or fruit resolves the oldest pending `Snack` demand, if one exists, in addition to the food's existing hunger/digestion effects. An energy drink does not count as satisfying a snack request.

One feeding resolves one snack demand. Feeding remains valid when no snack demand exists.

### Poop

The existing `CleanPoop` interaction remains authoritative. Removing one poop removes exactly that poop's cleanliness pressure and restores the existing cleanliness recovery amount.

### Nap and energy drinks

Energy remains primarily a baseline physiological need. Hammock naps and energy drinks retain their existing behavior and do not resolve affection or snack demands.

## Enforcement

Do not create an incident-specific refusal policy. Strict-mode refusal continues to be computed exclusively from pet needs.

Keep the current hunger, energy, and cleanliness tier thresholds:

- mild: hunger `>= 70`, energy `<= 30`, cleanliness `<= 30`
- moderate: hunger `>= 85`, energy `<= 15`, cleanliness `<= 15`
- severe: hunger `>= 95`, energy `<= 5`, cleanliness `<= 5`

Add happiness as a fourth enforceable need using the same lower-is-worse bands as energy and cleanliness:

- mild happiness: `<= 30`
- moderate happiness: `<= 15`
- severe happiness: `<= 5`

Add `CriticalHappiness` and `RequiredAction::Pet` to the existing structured decision model. Dominant-need selection compares normalized neglect across all four needs and keeps deterministic tie breaking in this order: hunger, energy, cleanliness, happiness.

The existing blocked command scopes remain unchanged:

- mild blocks safe development work;
- moderate blocks all classified work except CodeGotchi control and uncertain work;
- severe blocks everything except CodeGotchi control.

Decorative and gentle modes continue to behave as they do now. The feature changes the pet simulation, not the meaning of the enforcement-mode switch.

## Pet Behavior

`BehaviorCoordinator` must also treat happiness `<= 10` as a `CriticalNeed`, alongside the existing critical hunger, energy, and cleanliness checks. This keeps room presentation aligned with the enforcement model when the pet has been ignored socially.

Outstanding demands do not create a new top-level `PetBehavior` variant. They are explicit snapshot state rendered by the room and can coexist with working, failure, success, and critical presentations.

## API and Protocol

Extend the authoritative snapshot JSON with the pending demands and incident scheduler fields. Rust and TypeScript protocol representations must agree exactly.

Expose the already-existing `CareCommand::Pet` through:

- `POST /api/v1/care/pet`
- a bounded request containing `actionId`, `interactionMs`, and `pointerDistance`
- `AuthoritativeRuntime::pet(...)`
- `CodeGotchiClient.pet(...)`
- `useCodeGotchi().pet(...)`

The route follows the same bearer authentication, replay-safe action ID, mutation receipt, persistence, and WebSocket broadcast path as feed, clean, and nap.

No incident creation endpoint is added. Incidents originate only from authoritative simulation time progression.

## Room UI

The browser remains a projection of authoritative state.

### Demand presentation

Render a compact demand stack anchored near the pet:

- affection demand: heart icon and accessible text `Needs attention`
- snack demand: food/bowl icon and accessible text `Wants a snack`
- if multiple demands of the same kind are pending, show one bubble with a numeric count badge

Poops continue to render as floor objects and are not duplicated as a separate poop thought bubble.

The demand stack is visible independently of the cosmetic thinking bubbles used for agent activity. It must compose with blinking, movement, desk work, nap, success, and failure presentation rather than replacing those systems.

### Petting gesture

Make the pet itself pointer-interactive without changing its room-motion authority. A pointer-down on the pet starts a local gesture measurement; pointer movement accumulates path distance; pointer-up sends `interactionMs` and `pointerDistance` through the new pet care endpoint. The backend remains responsible for deciding whether the gesture satisfies the existing 1,500 ms / 120 px minimum.

The UI may show transient feedback for a successful pet but must not optimistically remove a demand or change happiness before the authoritative snapshot arrives.

### Existing interactions

Feeding remains click/drag based. Poop cleaning remains shovel/trash based. The hammock remains the energy recovery interaction. No extra modal, confirmation dialog, or generic `Resolve demand` button is added.

## Persistence and Restart Behavior

The SQLite snapshot remains the sole persisted source of truth. Pending demands, schedule sequence, and remaining active interval survive restart.

A restart with two affection demands and 90 active seconds remaining until the next incident restores exactly that state. Browser reloads do not affect timing because the browser owns no incident timer.

Old snapshots without attention state restore with an empty backlog and a fresh deterministic first interval. They must not fail snapshot validation or spawn an immediate incident.

## Testing

### Domain

Add deterministic tests covering:

- active hunger progresses at `+25/hour` and active energy at `-50/hour`;
- idle/waiting rates remain `+1/hour` and `+8/hour`;
- incident delays are always within inclusive 180–300 seconds;
- every three consecutive incident kinds contain one affection, one snack, and one poop;
- the same pet ID and sequence derive the same schedule after restore;
- active time advances the incident countdown while idle/waiting time does not;
- a large elapsed jump and equivalent small ticks create identical incidents and need values;
- affection, snack, and poop pressure each apply `240/hour` per unresolved item and stack linearly;
- petting resolves exactly one oldest affection demand;
- non-energy food resolves exactly one oldest snack demand;
- energy drinks do not resolve snack demands;
- cleaning one poop removes only that poop's pressure;
- happiness participates in mild/moderate/severe enforcement;
- severe happiness blocks uncertain work just like other severe needs;
- `BehaviorCoordinator` reports critical need at happiness `<= 10`;
- 30 minutes of uninterrupted active work with no care reaches severe neglect under every tested deterministic shuffle-bag ordering and the policy would permit only CodeGotchi control.

### Runtime/server

Cover persistence and broadcast behavior for automatically generated incidents and the new pet endpoint. A maintenance tick crossing an incident boundary must persist and broadcast the changed snapshot even when no Codex hook event occurs at that instant.

### Web

Add tests for:

- protocol parsing/types for pending demands;
- affection and snack demand bubbles and count badges;
- pointer gesture measurement and the pet care request;
- failed/insufficient petting leaves authoritative state unchanged and shows backend error;
- feeding clears one snack demand only after authoritative response;
- existing poop, hammock, motion, blink, and food interactions remain functional.

### End-to-end

Extend the production Playwright fixture with deterministic incident state so a browser test can verify the complete care loop without waiting real minutes:

1. show an affection demand;
2. perform a valid petting gesture;
3. observe the demand disappear from the authoritative snapshot;
4. show a snack demand and satisfy it with food;
5. show a poop and clean it with the existing shovel/trash interaction;
6. verify strict-mode denial copy can now request petting for critical happiness.

## Acceptance Criteria

The feature is complete when all of the following are true:

- During continuous active coding, incidents occur at randomized 3–5 minute active-time intervals.
- Breaks and `WaitingForUser` periods do not consume the incident countdown.
- A caretaker who addresses incidents promptly can keep working with short room interactions.
- Ignored affection, snack, and poop incidents visibly degrade happiness, hunger, and cleanliness respectively.
- Multiple ignored incidents stack rather than replacing one another.
- Hunger and energy use the new `+25/hour` and `-50/hour` active rates.
- Happiness is a first-class strict-mode blocking need.
- No new blocking mechanism exists outside `WorkPermissionPolicy`.
- Thirty active minutes without care produces severe-scale neglect in deterministic simulation coverage and therefore reaches the existing near-total strict-mode refusal tier.
- Restarting the browser or runtime neither clears pending demands nor rerolls an already-scheduled incident.
- The browser cannot manufacture, dismiss, or locally mutate incidents.
- All existing Rust, web, formatting, lint, build, and Playwright quality gates remain green.

## Out of Scope

This feature does not add a store, replenishing food economy, toys, play minigames, medicine, sickness, thirst, pet death, desktop notifications, sounds, OS-level blocking, new enforcement modes, configurable difficulty, or a generic incident scripting engine.
