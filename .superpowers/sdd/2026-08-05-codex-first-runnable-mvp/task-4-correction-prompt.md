# Task 4 focused correction

Resume as the same sole Task 4 implementer. Do not create or delegate to any
agent. Read the original Task 4 brief/report and the independent review at
`.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-4-review.md`.

Fix only the two accepted MVP-blocking findings:

1. Stable replay identity. When the installed hook supplies a stable lifecycle,
   turn, or tool identity, the deterministic event ID must depend on runtime
   identity, session identity, hook/event kind, and that stable identity—not on
   mutable exit status, duration, or other event metadata. Keep the existing
   privacy-safe metadata fallback only when no stable identity exists. Preserve
   distinct PreToolUse and PostToolUse IDs. Add a focused regression proving a
   second PostToolUse delivery with the same tool_use_id but changed structured
   status/duration is a duplicate with no second mutation or broadcast.

2. Incomplete apply_patch must fail open. `apply_patch` is SafeDevelopment only
   when its verified minimum edit input is present as a non-empty command/patch
   string in the installed schema shape. Missing, non-object, non-string, or
   empty input must classify as Unknown/Uncertain for policy and therefore
   allow in Strict mode. Add focused process-level Strict regressions for these
   incomplete valid-JSON payloads. Do not weaken classification of the verified
   installed apply_patch fixture.

Do not fix the review's Backlog item. Do not modify the web UI, launcher,
domain policy semantics, persistence schema, or unrelated code. Preserve the
250 ms production hook timeout and privacy boundary.

Use TDD, run the two focused Task 4 tests, then Rust format/clippy/workspace
tests. Update `task-4-report.md` with the correction evidence and create
`task-4-correction-report.md`. Do not commit.
