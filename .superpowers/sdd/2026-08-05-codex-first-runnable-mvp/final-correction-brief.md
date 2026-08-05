# Final focused correction brief

## Scope

Resolve only final-review finding 1: a transient initial state or stream error remains visible after a later authoritative WebSocket snapshot restores the room to `Connected`.

## Root cause already reproduced

`useCodeGotchi` stores client errors but clears them only after care mutations. A successful authoritative snapshot/status recovery does not clear the stale error. The final reviewer reproduced `alerts_after_ws_recovery=1` by aborting only the first `GET /api/v1/state` while allowing the production WebSocket to connect and deliver a complete snapshot.

## Exact relevant files

- Modify: `web/src/useCodeGotchi.ts`
- Modify: `web/src/useCodeGotchi.test.tsx` if useful for focused coverage
- Modify: `web/e2e/mvp.spec.ts`
- Modify only if the production Playwright fixture needs a narrow test seam: `web/e2e/fixture.mjs`
- Refresh after green: `crates/codegotchi-cli/web-dist/**` via `node web/scripts/embed-web.mjs`
- Create report: `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/final-correction-report.md`

## Interfaces

- Consume the existing `CodeGotchiClient` observer callbacks (`onSnapshot`, `onStatus`, `onError`).
- Preserve the existing public `CodeGotchiState` interface and all backend protocols.
- A valid authoritative snapshot after a transient error must clear that stale transport/state error. Genuine later care/backend errors must still render until an actual succeeding operation or authoritative recovery.

## Mandatory TDD acceptance

1. Add the smallest real-behavior regression test first and run it red. The production change that must make it fail is removal of error clearing on successful authoritative snapshot recovery.
2. The production-bundle Playwright flow must abort the initial state request, recover through the real Rust WebSocket snapshot, show `Connected`, render the authoritative room, and have zero `role=alert` elements.
3. Make the minimal production change and run the focused test green.
4. Run `corepack pnpm test`, `corepack pnpm lint`, `corepack pnpm format:check`, `corepack pnpm build`, `node web/scripts/embed-web.mjs`, and the production Playwright suite with the known `LD_LIBRARY_PATH`.
5. Run `cargo fmt --all -- --check` and `cargo test --workspace` after refreshing embedded bytes.
6. Record exact red/green evidence and changed files in the report and commit the correction.

## Explicit exclusions

- Do not address the supervisor-owned real Codex acceptance gate; the supervisor will run it after this correction.
- Do not change backend/domain/persistence/launcher/hook behavior.
- Do not refactor the client architecture, improve styling, or implement backlog items.
- Do not add a second broad review.

