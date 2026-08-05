# Task 3 report — functional pet room

Date: 2026-08-05
Task brief base: `c0189e4` (Task 2 result: `e0b7fcf`)
Scope: Task 3 only; no commit created.

## Implementation

- Added the typed camelCase snapshot protocol, authenticated HTTP care client,
  one-shot `#token=` extraction/removal, UUID care IDs, bounded WebSocket
  reconnect, and complete-snapshot replacement.
- Replaced the static room with an accessible authoritative room projection,
  activity/behavior presentations, food drag/drop, keyboard feed, shovel → poop
  → trash cleaning, connection states, backend errors, and returned-snapshot
  feedback. No snapshot, need, inventory, or poop state is stored in
  `localStorage` or calculated locally.
- Added Vitest coverage and a development-only real-Rust-server Playwright
  fixture. The fixture seeds only through existing runtime/domain APIs and has
  no browser mutation or command-execution route.
- Added browser WebSocket subprotocol authentication while preserving existing
  bearer-authenticated WebSocket clients and loopback binding.
- Added the Vite-to-`crates/codegotchi-cli/web-dist/` copy script and generated
  the bundle from `web/dist/`.

## TDD evidence

- RED: the first `corepack pnpm test` run failed because the newly referenced
  `web/src/client.ts` and `web/src/useCodeGotchi.ts` modules did not exist; both
  suites collected zero tests.
- GREEN: final `corepack pnpm test` passed: 2 files, 23 tests.

## Required gates

| Gate | Result |
| --- | --- |
| `corepack pnpm test` | PASS — 23 tests |
| `corepack pnpm lint` | PASS |
| `corepack pnpm format:check` | PASS |
| `corepack pnpm build` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace` | PASS — all executed tests passed; 1 pre-existing manual Codex test ignored |
| `node web/scripts/embed-web.mjs` | PASS — copied `web/dist/` to `crates/codegotchi-cli/web-dist/` |
| Copied-bundle Vite check | PASS — no `@vite/client`, `import.meta.hot`, `vite/dev`, or `__vite` markers |
| `LD_LIBRARY_PATH=/tmp/codegotchi-playwright-libs.uo9gm9/extracted/usr/lib/x86_64-linux-gnu corepack pnpm playwright:test` | PASS — 6 browser tests against the real Rust backend |

Playwright itself and Chromium were installed with:

```text
corepack pnpm --filter @codegotchi/web exec playwright install chromium
```

The supervising environment did not have `libnspr4`, `libnss3`, or
`libasound2t64` installed system-wide. The supervisor reproduced the loader
failure, downloaded those three Ubuntu packages without root, extracted them to
a temporary directory, and scoped `LD_LIBRARY_PATH` only to the Playwright
command. This exposed and fixed two application-level failures before the green
run: duplicate React refresh setup in the Vite fixture and overlapping poop
controls that prevented selecting an authoritative object.

## Changed files

Modified:

- `crates/codegotchi-cli/src/server.rs`
- `crates/codegotchi-cli/tests/websocket_integration.rs`
- `package.json`
- `pnpm-lock.yaml`
- `web/package.json`
- `web/src/App.css`
- `web/src/App.test.tsx`
- `web/src/App.tsx`
- `web/vite.config.ts`
- `.gitignore`

Created:

- `crates/codegotchi-cli/examples/task3_fixture.rs`
- generated `crates/codegotchi-cli/web-dist/`
- `web/e2e/fixture.mjs`
- `web/e2e/mvp.spec.ts`
- `web/playwright.config.ts`
- `web/scripts/embed-web.mjs`
- `web/src/client.test.ts`
- `web/src/client.ts`
- `web/src/protocol.ts`
- `web/src/useCodeGotchi.ts`

No unrelated files were changed, and no commit was made.
