# Task 6 review

## Verdict

MVP-blocking test-validity gaps remain. The focused runtime, workspace, web,
and production-embedded browser gates pass, but the new acceptance harness does
not fully prove three mandatory claims. Do not mark Task 6 accepted until the
test evidence is corrected.

## MVP-blocking findings

1. **The Strict flow does not prove that the retry is a fresh tool use.**
   `crates/codegotchi-cli/tests/full_vertical_flow.rs:806-831` changes the
   fixture text and computes `retry_event`, but never asserts
   `retry_event.id != denial_event.id` (or otherwise observes a non-duplicate
   backend response). If the retry were accidentally replayed with the denial
   ID, care would make the current state allowable and the duplicate event
   could still produce `{}`; both `contains_id` assertions would then inspect
   the same ID. This does not satisfy the mandatory deny → care → fresh retry
   allow proof.

2. **The process flow does not verify a duplicate care action is a total
   no-op.** At `:584-594`, the duplicate feed assertion compares only
   `inventory`; it does not compare the complete snapshot before and after the
   duplicate. At `:633-648`, the duplicate clean assertion checks only that
   `pendingPoops` is empty. A faulty duplicate implementation could still
   change needs, activity, timestamps, points, outcomes, or replay state while
   passing these assertions. The mandatory process-level duplicate care proof
   requires full authoritative-state equality (as the event duplicate check at
   `:522-541` already does).

3. **The restart privacy assertion is vacuous for the process under test.**
   The restart flow sends only `session_start.json` (`:506-510`), which contains
   none of the forbidden prompt, source, command, or output values; it then
   searches the resulting HTTP and SQLite snapshots for those values
   (`:682-703`). The Strict flow does send the raw command fixture at
   `:777-812`, but never checks that flow's serialized or persisted state for
   leakage. Consequently the new launcher/restart path can pass even if it
   mishandles sensitive hook fields. Existing `hook_runtime_flow` coverage is
   useful, but it does not make the Task 6 process-level evidence described in
   the report non-vacuous. The mandatory privacy check needs sensitive installed
   fixtures to pass through this flow and a post-ingestion HTTP/SQLite check,
   without printing their contents.

## Backlog

No new backlog entry was appended. The production Playwright reconnect test at
`web/e2e/mvp.spec.ts:146-178` proves a fresh connection and `Connected` status,
but not a changed authoritative snapshot received after reconnect; this is
already tracked by the existing “Strengthen the Playwright reconnect scenario”
item in `docs/backlog/codex-first-mvp-followups.md`. The fixture's abrupt
SIGTERM/SQLite-sidecar cleanup limitation is also already tracked there.

## Tests run

- `cargo test -p codegotchi-cli --test full_vertical_flow -- --nocapture` —
  PASS, 2 tests.
- `cargo test -p codegotchi-cli --test full_vertical_flow -- --nocapture
  --test-threads=1` — PASS, 2 tests.
- `cargo test -p codegotchi-cli --test hook_runtime_flow -- --nocapture` —
  PASS, 1 test.
- `cargo test -p codegotchi-cli --test strict_flow -- --nocapture
  --test-threads=1` — PASS, 1 test.
- `cargo fmt --all -- --check` — PASS.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  PASS.
- `cargo test --workspace` — PASS, 110 passed, 0 failed, 1 intentionally
  ignored manual Codex test; doc-tests had 0 tests.
- `corepack pnpm lint` — PASS.
- `corepack pnpm test` — PASS, 3 files / 29 tests.
- `corepack pnpm format:check` — PASS.
- `LD_LIBRARY_PATH=/tmp/codegotchi-playwright-libs.uo9gm9/extracted/usr/lib/x86_64-linux-gnu corepack pnpm playwright:test` — PASS, 7 production-embedded Playwright tests.

An initial attempt to launch three Cargo test commands concurrently hit OS
`EAGAIN` resource contention; the affected tests passed when rerun individually
and in the workspace gate.
