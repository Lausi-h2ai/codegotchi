# ADR 0001: One Rust domain crate and one TypeScript web package

- Status: Accepted
- Date: 2026-08-05

## Context

The Phase 1/2 repository needs a real dependency boundary between deterministic
pet rules and the future runtime/client infrastructure. It also needs a small
frontend that can be built and tested without pretending that the daemon,
persistence, process integration, or finished renderer already exist.

## Decision

Use one Cargo workspace containing the meaningful `codegotchi-domain` crate and
one pnpm workspace containing the `web` package.

The Rust crate owns pure domain semantics and its explicit ports/strategies.
The web package owns the typed React/Vite presentation foundation. A future
daemon will host the live authoritative domain runtime; the current web
placeholder is not an authority.

## Alternatives considered

### One monolithic Rust crate

This would be smaller at first, but domain/infrastructure separation would be
conventional rather than compiler-enforced. A Phase 3 daemon would then force a
disruptive split at the moment infrastructure begins to grow. It was rejected.

### A full empty multi-crate skeleton

Creating daemon, storage, CLI, integration, wrapper, proxy, and adapter crates
now would make a future diagram look complete while providing fictional
interfaces and empty packages. It conflicts with the YAGNI guardrail and would
make the current dependency graph harder to understand. It was rejected.

## Consequences

Positive consequences:

- Rust compilation enforces the domain/infrastructure dependency boundary now.
- The web package can prove a real rendered UI path without adding unused game
  runtime systems.
- Future packages can be introduced at the point their concrete boundary is
  understood.

Trade-offs:

- The repository does not yet show every future deployment package.
- The current room is a placeholder and cannot demonstrate live synchronization
  or care workflows.
- Future daemon and persistence work will need explicit integration contracts.

These trade-offs are intentional. They are tracked as later slices rather than
implemented as empty scaffolding.
