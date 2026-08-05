# Task 6 focused re-review

Scope: only the three MVP-blocking findings in `task-6-review.md`, evaluated
against `git diff 48fb968..0006f59` and the corrected vertical-flow test.

## Verdict: ACCEPTED

All three prior findings are resolved. No implementation or backlog files were
changed by this re-review.

## Finding status

1. **Fresh Strict retry — RESOLVED.**

   `crates/codegotchi-cli/tests/full_vertical_flow.rs:862-890` constructs the
   retry payload, translates both events, and explicitly asserts distinct event
   IDs at `:868`. It records the denial ID as already processed and the retry ID
   as absent before retry at `:871-890`. After the hook subprocess returns the
   exact allow output `{}` at `:891-893`, the test requires both IDs and a
   changed replay set at `:894-911`. A duplicate denial event can no longer
   satisfy this proof.

2. **Duplicate care total no-op — RESOLVED.**

   `snapshot_from_mutation_response` at `:441-450` removes and validates only
   the `duplicate` envelope field, then requires the complete snapshot shape.
   The feed path compares the complete first and duplicate snapshots at
   `:648-672`; the clean path does the same at `:701-725`. This covers needs,
   activity, timestamps, points, poop state, inventory, outcomes, and replay
   state rather than only one field.

3. **Process-level privacy evidence — RESOLVED.**

   The restart flow now sends six installed sensitive fixtures—prompt,
   command, patch/source, and complete tool-output cases—through the launched
   hook subprocess at `:555-582`. It then checks both authenticated HTTP state
   and the SQLite snapshot at `:584-594`; the helper checks prompt, source,
   full command, and complete output markers at `:486-505`. The final state is
   checked again at `:759`, without logging sensitive payloads. The required
   privacy evidence is therefore exercised through the compiled launcher path,
   not merely checked against a state that never received sensitive input.

## Focused test run

- `cargo test -p codegotchi-cli --test full_vertical_flow -- --nocapture` —
  PASS, 2 passed, 0 failed.
