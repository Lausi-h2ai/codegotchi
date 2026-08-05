# Task 5 independent review

Reviewer: Codex session `019fd23d-ef0e-7d41-b887-9ebe343e9a6e`

Model: `gpt-5.6-luna`; reasoning: `max`.

Range: `f293795..2fef54a`

Verdict: **CORRECTION REQUIRED**

## MVP-blocking

1. `launcher.rs:117` publishes metadata before signal handlers are installed at
   `launcher.rs:666`. A SIGTERM race left `session-*.json` behind. Install
   signal handling before owned resources and clean setup cancellation.
2. `launcher.rs:669` explicitly forwards SIGINT/SIGWINCH even when inherited
   stdio puts Codex in the same terminal foreground process group. Suppress the
   duplicate path and prove it with a real PTY/process-group test.
3. `launcher.rs:619` treats browser helper spawn success as launch success.
   `/bin/false` produced no warning. Observe helper exit asynchronously and
   warn on nonzero without blocking Codex.
4. `launcher.rs:513` removes any valid dead-PID `session-*.json`, including a
   valid unrelated file. Require the UUID filename to bind to the runtime ID or
   another explicit ownership marker, and test a valid unrelated file.
5. `runtime_metadata.rs:30` can leave an invalid partial file after a
   write/sync failure. Remove the create-new file if completion fails.
6. `static_assets.rs:149` never fetches `/` or a hashed asset from the installed
   binary. Keep fake Codex alive, fetch the embedded UI, then release it.
7. `process_wrapper.rs:581` captures state only after both runs and does not
   prove reload. Capture after run one and compare identity and continued state
   after run two.

## Backlog

- `/proc/<pid>` alone is vulnerable to PID reuse when deciding staleness.
- A hard kill can leave an additive profile because stale cleanup scans only
  runtime metadata.

Focused Rust/workspace/fmt/clippy gates and an independent installed-binary
HTTP probe passed. The reviewer made no repository changes.
