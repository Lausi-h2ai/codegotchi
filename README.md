# CodeGotchi

CodeGotchi is a mixed Rust/TypeScript workspace for a small coding companion.
The repository currently contains the Phase 1 workspace foundation and the
Phase 2 deterministic pet domain.

## Current Phase 1/2 scope

- `crates/codegotchi-domain` is a pure Rust domain crate containing the pet
  aggregate, deterministic time progression, canonical events, care commands,
  poop rules, behavior derivation, and work-permission policy.
- `web` is a typed Vite/React application with an accessible heading and a
  static, CSS-only geometric room placeholder.
- Rust remains the home of authoritative domain transitions. The web package
  is presentation-only and does not calculate or own authoritative pet state.

The current room is intentionally a functional foundation, not the finished
game client. There are no care controls, API clients, WebSockets, browser
state store, daemon, persistence, process wrapper, command proxy, hooks, MCP
integration, or final art in this slice.

## Prerequisites

- Node.js 22.22.2 or newer on the Node 22 line
- Rust stable with `cargo`, `rustfmt`, and `clippy`
- Corepack, included with supported Node.js distributions

The root `package.json` pins pnpm to `11.20.0`. Activate Corepack and confirm
the selected version before installing dependencies:

```sh
corepack enable
corepack pnpm --version
corepack pnpm install --frozen-lockfile
```

The version command should print `11.20.0`.

## Local checks

Run these commands from the repository root. The four root pnpm scripts target
the `web` workspace package.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace

corepack pnpm test
corepack pnpm lint
corepack pnpm format:check
corepack pnpm build
```

For a local frontend preview after a build, run `corepack pnpm --filter @codegotchi/web dev`
and open the URL printed by Vite.

## Deliberately deferred

Later vertical slices may add PixiJS rendering, Zustand client state,
Playwright browser workflows, a daemon that becomes the runtime authority,
SQLite persistence, a process wrapper, a command proxy, agent/application
adapters, WebSockets and API clients, hooks, an embedded bundle, and polished
art. These are planned product or infrastructure work, not missing pieces of
the Phase 1/2 foundation.

See [the architecture](docs/architecture.md), [ADR 0001](docs/adr/0001-rust-typescript-workspace.md),
and [the backlog](docs/backlog.md) for the boundaries and sequencing.
