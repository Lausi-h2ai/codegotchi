# CodeGotchi architecture

## Current shape

CodeGotchi deliberately has two small workspaces:

| Area | Current responsibility | Current authority |
| --- | --- | --- |
| `crates/codegotchi-domain` | Pure pet rules, events, care, progression, behavior, and permission decisions | Domain transitions are deterministic and testable in Rust |
| `web` | Accessible presentation shell and CSS-only room placeholder | No authoritative pet state |

The Rust workspace owns dependency versions and the domain crate has no
framework, storage, subprocess, operating-system, asynchronous-runtime, or
frontend dependency. The pnpm workspace owns the web package and its local
quality gates.

## Domain/infrastructure boundary

The domain is intentionally independent of the environment in which a pet is
run. It accepts explicit clocks and strategy/port implementations, operates on
structured events and care commands, and returns typed decisions and errors.
It does not read files, launch processes, open sockets, persist data, or parse
raw agent commands.

Infrastructure is a future boundary. A future daemon will be the runtime
authority: it will own the live simulation, accept validated inputs at an
ingestion boundary, persist or replay the domain state as appropriate, and
publish projections to clients. That daemon does not exist in this slice, so
the current web app is a static presentation shell rather than a hidden
authority.

The intended direction is:

```text
future agent/process adapters -> future daemon (runtime authority)
                                      |                 \
                                      v                  v
                              Rust domain rules     future web client
                                                        |
                                             current CSS placeholder only
```

Raw prompts, source contents, complete output, and raw command text stay out
of the domain event representation. Classification and payload validation
belong at the future daemon-ingestion boundary.

## Why the frontend is intentionally small

The Phase 1/2 web package proves the build, lint, format, and rendered-test
paths with a real React render. The room uses HTML elements and CSS geometric
shapes for a pet, desk, bowl, window, and room. It has no client state store,
network client, WebSocket, care behavior, or browser-authoritative simulation.
Renderer technology and browser workflows should arrive with the product
vertical slice that can exercise them.

## Evolution path

The next layers can be added without moving domain rules into the browser:

1. Add daemon infrastructure around the existing Rust domain boundary.
2. Add persistence and ingestion/adapters where their concrete interfaces are
   needed.
3. Replace the static web room with a client projection and user-facing
   workflows, keeping the daemon as authority.

Empty future crates are not created early. Each new package should arrive when
its implementation and boundary are needed, as recorded in the ADR and
[backlog](backlog.md).
