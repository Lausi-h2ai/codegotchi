# Task 5 focused correction re-review

Reviewer: Codex session `019fd23d-ef0e-7d41-b887-9ebe343e9a6e`

Model: `gpt-5.6-luna`; reasoning: `max`.

Range: `6d453db..f2210e5`

Verdict: **CORRECTION REQUIRED**

Focused commands passed:

- `cargo test -p codegotchi-cli --test process_wrapper -- --nocapture`: 14 passed.
- `cargo test -p codegotchi-cli --test static_assets -- --nocapture`: 2 passed.
- `cargo test -p codegotchi-cli --lib runtime_metadata -- --nocapture`: 2 passed.

## Resolved original findings

1. Signal observation is installed before setup and setup cancellation cleans resources.
2. Browser-helper nonzero exit is observed and warned without blocking Codex startup.
3. Stale metadata deletion requires a canonical UUID filename matching `runtime_id`.
4. Failed metadata writes and syncs remove the create-new file.
5. The installed-binary test fetches `/` and a hashed asset from the installed process.
6. Persistence is captured after run one and compared after run two.
7. The PTY test genuinely sends a negative-PGID SIGINT to a real `script` foreground PTY and proves one child delivery.

## MVP-blocking

- `launcher.rs` suppresses every SIGINT/SIGWINCH when wrapper and child share the terminal foreground group. It therefore also suppresses a direct `kill -INT <wrapper-pid>` or direct SIGWINCH, even though only the wrapper received it. Existing direct-signal coverage uses piped non-terminal stdio. The correction must make suppression source-aware and add a direct-signal PTY regression test while preserving the one-delivery terminal-group test.

## Backlog

- PID reuse remains possible in stale-session detection.
- A hard-killed launcher can leave its additive profile until separate cleanup.

