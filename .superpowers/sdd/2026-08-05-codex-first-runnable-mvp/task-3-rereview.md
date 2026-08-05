# Task 3 focused re-review

Scope: correction commit `6a5a17a` over base `d1e24f4`. This re-review covers
only the two MVP-blockers recorded in `task-3-review.md`; no source, test,
plan, ledger, or backlog files were changed.

## Verification

- `git diff --check d1e24f4..6a5a17a` — PASS.
- `corepack pnpm test` — PASS: 3 files, 29 tests.
- The correction report records supervisor PASS results for lint,
  format-check, build, all Rust gates, embedded-bundle refresh/scan, and
  Playwright: 7/7 against the real Task 2 server.

## Original blocker 1 — cleaning sequence

Resolved. `web/src/App.tsx:59-75` sets `cleaningPoopId` only after a shovel
drop or an armed-shovel click. `web/src/App.tsx:78-97` now requires that
selection, rejects a raw poop drag when no selection exists (or when its ID
differs), validates the ID against the authoritative `pendingPoops`, and only
then calls `clean`. The direct-disposal regression test at
`web/src/App.test.tsx:197-216` asserts no clean request and retained poop; the
valid click and drag paths are covered at `web/src/App.test.tsx:218-269` and
the browser direct-drop/persisted valid-sequence checks at
`web/e2e/mvp.spec.ts:59-101`.

No residual MVP-blocking or Backlog finding remains for this item.

## Original blocker 2 — initial HTTP/WebSocket/care snapshot races

Resolved. `web/src/client.ts:96` stores the latest accepted complete snapshot.
Initial HTTP publication (`web/src/client.ts:147-155`), WebSocket publication
(`web/src/client.ts:190-198`), and care-response publication
(`web/src/client.ts:240-253`) all pass through `publishSnapshot` at
`web/src/client.ts:285-291`. Its ordering gate at `web/src/client.ts:299-337`
rejects older timestamps and handles equal timestamps only when the
authoritative replay-ID sets or poop sequence demonstrably advance. The hook
no longer writes care responses independently (`web/src/useCodeGotchi.ts:63-89`).

Focused coverage proves delayed HTTP cannot replace a newer stream snapshot
(`web/src/client.test.ts:218-251`), equal-timestamp stale rejection
(`web/src/client.test.ts:253-286`), and an older care response cannot replace
the newer hook projection (`web/src/useCodeGotchi.test.tsx:81-131`). Reconnect
replacement remains covered at `web/src/client.test.ts:169-216`.

No residual MVP-blocking or Backlog finding remains for this item.

## Verdict

ACCEPTED
