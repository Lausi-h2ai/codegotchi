# CodeGotchi Phase 1–2 Design

## Scope

This slice establishes the mixed Rust/TypeScript repository and implements the pure pet domain. It intentionally excludes the daemon, persistence, process wrapping, command proxies, agent integrations, MCP, and polished art.

## Options considered

### One Rust crate plus one web package — selected

Create a Cargo workspace containing only `codegotchi-domain`, plus a pnpm workspace containing `web`. This gives the domain a real dependency boundary without creating empty infrastructure packages. Future phases add daemon, storage, CLI, integration, and proxy crates when their code exists.

### One monolithic Rust crate

This is initially smaller, but it makes the required domain/infrastructure separation conventional rather than compiler-enforced. It was rejected because Phase 3 would immediately require a disruptive split.

### Full multi-crate skeleton

Creating every proposed crate now makes the eventual architecture visible, but produces empty packages and fictional interfaces. It was rejected under the specification's YAGNI guardrail.

## Repository architecture

- The root Cargo workspace owns Rust dependency versions and lints.
- `crates/codegotchi-domain` contains the aggregate, canonical events, clocks, progression strategies, care commands, poop rules, behavior derivation, random source port, and work-permission policy.
- The domain crate may depend on representation libraries (`chrono`, `serde`, `uuid`, `thiserror`) but not Axum, SQLite, Tokio, subprocess, OS, or frontend libraries.
- The root pnpm workspace owns the minimal React/Vite application in `web` and common JavaScript commands.
- GitHub Actions independently verifies Rust formatting/lints/tests and web installation/lints/tests/build.

## Domain model

`Pet` is the aggregate root. It owns needs, inventory, work and digestion points, poops, the current agent activity state, recent outcome, last-update time, and replay-protection sets for event and care IDs. Needs always use the specified unambiguous semantics and are clamped to `0..=100` after every transition.

Canonical `AgentEvent` values are versioned and identified by UUID. Applying an event first advances elapsed time using the previous explicit `AgentActivityState`, then transitions activity and applies outcome/work effects. Duplicate IDs are no-ops. Metadata contains only structured command information and never raw prompts, output, command text, or source contents.

In Phase 2, `executable_name` and `command_category` remain structured
semantic metadata fields. Payload length and content validation belongs at the
Phase 3 daemon-ingestion boundary; raw prompt, output, and command fields are
not added to the domain event.

Elapsed-time rules are deterministic. Active work raises hunger and consumes energy; inactivity raises hunger more slowly and restores energy. Existing poop decreases cleanliness. Successful test/build completion increases happiness; failures decrease it. Rates are expressed as named per-hour constants and applied linearly from elapsed seconds.

Feeding consumes authoritative inventory, reduces hunger, and adds digestion points. Work events add work points. The deterministic poop strategy emits poop only when both digestion and work thresholds are met, consuming one threshold unit for each emitted poop. Poop IDs derive reproducibly from the pet ID and poop sequence. A seeded random source exists for future cosmetic variation but cannot decide important transitions.

Care commands are typed and idempotent. Feeding validates food and inventory; cleaning validates the poop ID; petting requires both a duration and pointer-distance threshold. Invalid commands leave the aggregate unchanged.

## Behavior and permission policy

Behavior is a derived state, not a group of booleans. Selection priority is critical need, blocked/refusal, active activity, recent success/failure, sleeping, then wandering.

The permission policy consumes the pet, a structured command classification, and settings. Decorative always allows. Gentle allows and warns when care is needed. Strict may block only commands explicitly classified as safe development work. CodeGotchi controls, shell/process recovery, Git recovery, infrastructure shutdown, security remediation, and uncertain commands are always allowed. Strict mode is never the default.

## Frontend foundation

The Phase 1 frontend is a typed Vite/React application with a replaceable geometric room placeholder and an accessible heading/status. Vitest covers this real rendered behavior. PixiJS/Zustand/Playwright are deferred until the functional UI phase so Phase 1 does not introduce unused runtime systems.

## Error handling

Domain validation uses typed errors. Failed care actions are atomic. Unsupported schema versions and invalid event transitions return typed errors rather than panicking. Clock movement backwards is treated as zero elapsed time. No expected domain input may cause a panic.

## Testing

Rust tests use `FakeClock` and literal expected values. They cover clamping, active and idle progression, backward time, outcome effects, deterministic poop thresholds, inventory, every care validation branch, event and action idempotency, event replay, deterministic seeded random behavior, behavior priority, and all permission modes/exemptions. The web test renders the real application. CI runs the same formatter, linter, test, and build commands documented for local development.

## Accepted deviations

- Only the meaningful domain crate is created in this slice; empty future crates are not.
- Playwright, PixiJS, and Zustand are deferred until Phase 4 because this slice has no game renderer, client state synchronization, or browser workflow to exercise.
- Versioned serializable persistence snapshot/restore is intentionally deferred to Phase 3. Task 2's `SimulationSnapshot` is an in-memory deterministic test/read model, not a persistence DTO.
- Persistence-backed idempotency is deferred to Phase 3; the aggregate provides deterministic in-memory replay safety now and exposes state suitable for later snapshots.
