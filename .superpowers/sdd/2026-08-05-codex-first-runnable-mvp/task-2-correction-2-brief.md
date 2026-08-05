# Task 2 correction 2 — prove scheduled maintenance wiring

Model: `gpt-5.6-luna`; reasoning: `max`.

Base commit: `4e09469`.

This is the final permitted Task 2 correction. Address only the remaining
focused re-review finding in `task-2-rereview.md`.

## Required change

Add a deterministic, bounded test that exercises maintenance through the
production `RunningServer` scheduled maintenance task, rather than invoking
`AuthoritativeRuntime::maintenance_tick_at` directly. The test must prove:

- the running server's maintenance task invokes the runtime tick;
- an elapsed-time state change is persisted to SQLite;
- the resulting authoritative snapshot is broadcast;
- shutdown remains bounded and stops the task.

Prefer a test-injectable interval/tick source or similarly narrow seam so the
test does not add a real one-second delay or become timing-flaky. Preserve the
production one-second interval. Do not redesign the runtime or server.

## Relevant files

- `crates/codegotchi-cli/src/server.rs`
- `crates/codegotchi-cli/src/runtime.rs` only if a narrow seam is required
- `crates/codegotchi-cli/tests/backend_integration.rs`
- `.superpowers/sdd/2026-08-05-codex-first-runnable-mvp/task-2-report.md`

## Acceptance

Use TDD. Run the new focused test, existing Task 2 integration tests, then:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Do not modify unrelated files, dependencies, backlog, plan, or Git history.
Update `task-2-report.md` with the exact correction and test results. Do not
commit; the supervisor will inspect and commit shared-worktree changes.
