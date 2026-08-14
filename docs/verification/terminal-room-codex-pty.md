# Terminal-room Codex PTY verification

Initial H3 dependency/API gate and managed PTY evidence, recorded 2026-08-14
on the repository Rust toolchain.

## Environment

```text
rustc 1.97.1 (8bab26f4f 2026-07-14)
cargo 1.97.1 (c980f4866 2026-06-30)
Linux 6.6.87.2-microsoft-standard PREEMPT_DYNAMIC WSL2 x86_64 GNU/Linux
codex-cli 0.147.0
```

## Dependency/API gate

| Crate | Direct requirement | Resolved version | Required API/mode support | Notes |
|---|---|---|---|---|
| `portable-pty` | `0.9.0` | `0.9.0` | Direct `CommandBuilder` args/env, native `openpty`, reader clone, writer take, resize, wait, numeric exit code | `PtyCodexChild` uses the native PTY pair and direct slave spawn; no shell wrapper. |
| `vt100` | `0.16.2` | `0.16.2` | Application cursor, bracketed paste, mouse levels, Default/UTF-8/SGR encoding state | 1004 focus reporting is absent; URXVT and SGR-pixel/1016 are unsupported. H4 must add focus tracking and verify Codex's requested protocol. |
| `ratatui` | `0.30.2`, `default-features = false`, feature `crossterm_0_29` | `0.30.2` | Crossterm 0.29 backend | Shares the direct crossterm 0.29.0 package through `ratatui-crossterm 0.1.2`. |
| `crossterm` | `0.29.0`, feature `event-stream` | `0.29.0` | Async event stream for the later host | Cargo tree has one crossterm 0.29.0 stack, with no duplicate incompatible backend. |

The exact graph is recorded in `Cargo.lock`. `portable-pty`'s command builder
starts from the ambient environment; H3 applies only invocation overrides and
explicitly drops the parent slave after spawn. Unknown terminal protocols must
degrade only the affected feature.

## Initial PTY evidence

The RED test was written before the production interface and failed on the
missing `PtyCodexChild` import. The direct shebang fixture then passed with the
managed implementation:

```text
$ cargo test -p codegotchi-cli --test terminal_pty -- --nocapture
running 1 test
test managed_pty_preserves_direct_invocation_input_resize_ansi_and_exit_code ... ok
test result: ok. 1 passed; 0 failed
```

That test directly executes the fixture (never `sh -c`), verifies literal
trailing arguments and `CODEX_HOME`/`CODEGOTCHI_SESSION_FILE`, observes
`FAKE_CODEX_READY` plus ANSI color bytes, delivers one input line, resizes to
120 columns × 31 rows, observes `FAKE_CODEX_SIZE=<31 120>`, and preserves exit
code 23. Full RED/GREEN/check/fmt/clippy output and lifecycle review are in
`.superpowers/sdd/2026-08-13-terminal-room-agent-hardening/task-h3-report.md`.
