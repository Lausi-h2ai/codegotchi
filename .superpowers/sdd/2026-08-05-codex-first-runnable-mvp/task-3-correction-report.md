# Task 3 correction report

Date: 2026-08-05
Base: `d1e24f4`
Task 3 implementation: `a4c00e6`
Scope: the two MVP blockers in `task-3-review.md`; no Backlog item or
unrelated refactor was implemented. No commit was created.

## Corrections

- Trash disposal now requires the poop ID already selected by a valid shovel →
  poop interaction. A direct poop → Trash drag sends no clean request and leaves
  the authoritative projection unchanged. The click/keyboard path and shovel →
  poop → Trash drag path remain supported.
- `CodeGotchiClient` now owns one complete-snapshot publication gate. Initial
  HTTP, WebSocket, feed, and clean responses all pass through it. A snapshot
  with a newer `lastUpdatedAt` replaces the projection; an older one is
  rejected. Equal timestamps are accepted only when existing continuation
  fields prove progress: replay ID sets contain the current sets and one set
  advances, or `poopSequence` advances. The hook no longer writes care
  responses directly, so stale care responses cannot regress the projection.
  No needs, inventory, or poop state is calculated optimistically.

## TDD evidence

- RED: after adding the deterministic deferred HTTP/WebSocket, equal-timestamp,
  care-response, and direct-disposal tests, the focused Vitest run failed 4 new
  tests: the direct drop called `clean`, both delayed HTTP snapshots were
  published after the newer stream snapshot, and the older care response
  replaced the newer hook projection.
- GREEN: `corepack pnpm test` passed — 3 files, 29 tests, including the
  preserved shovel-drag → poop-drag → Trash path.

## Required gates

| Gate | Result |
| --- | --- |
| `corepack pnpm test` | PASS — 3 files, 29 tests |
| `corepack pnpm lint` | PASS |
| `corepack pnpm format:check` | PASS |
| `corepack pnpm build` | PASS — final frontend build |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace` | PASS — all executed tests passed; 1 pre-existing manual Codex test ignored |
| `node web/scripts/embed-web.mjs` | PASS — refreshed `crates/codegotchi-cli/web-dist/` |
| Copied-bundle comparison and dev-artifact scan | PASS — byte-identical to `web/dist`; no `@vite/client`, `import.meta.hot`, `vite/dev`, or `__vite` markers |
| `LD_LIBRARY_PATH=/tmp/codegotchi-playwright-libs.uo9gm9/extracted/usr/lib/x86_64-linux-gnu corepack pnpm playwright:test` | PASS in the supervisor environment — 7 browser tests against the real Task 2 server |

## Changed files

Modified:

- `web/src/App.tsx`
- `web/src/App.test.tsx`
- `web/src/client.ts`
- `web/src/client.test.ts`
- `web/src/useCodeGotchi.ts`
- `web/e2e/mvp.spec.ts`
- generated `crates/codegotchi-cli/web-dist/`

Created:

- `web/src/useCodeGotchi.test.tsx`
- `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-3-correction-report.md`

No Rust source, Backlog item, or unrelated file was changed. No commit was
created.
