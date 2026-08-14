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

## H4a virtual screen and input-mode evidence

H4a was implemented on 2026-08-14 as the non-interactive terminal core. The
focused RED run was performed before the production modules existed and failed
with unresolved `CodexScreen`, mode-model, and encoder imports. After the
minimal implementation, the focused GREEN fixture passed:

```text
$ cargo test -p codegotchi-cli --test terminal_screen -- --nocapture
running 6 tests
test input_modes_track_vt_controls_and_split_focus_without_visible_text ... ok
test key_encoding_follows_application_mode_and_common_codex_keys ... ok
test mouse_encoding_honors_protocol_and_tracking_level ... ok
test malformed_and_split_control_input_never_panics_or_activates_focus ... ok
test paste_and_focus_encoding_are_strictly_mode_driven ... ok
test screen_handles_ansi_state_and_read_only_cell_access ... ok
test result: ok. 6 passed; 0 failed
```

`CodexScreen` wraps `vt100::Parser` with explicit dimensions and bounded
scrollback, exposes read-only screen/cell/cursor helpers, and maps every
public vt100 mouse mode and encoding into stable CodeGotchi enums. An
incremental fixed-size CSI tracker recognizes split `ESC[?1004h/l` focus
controls; visible text and malformed/unknown/truncated controls do not change
the mode model. The same raw bytes are still always fed to vt100.

Pure encoders cover UTF-8 text, Enter/Tab/BackTab/Backspace/Escape, control and
Alt chords, navigation/editing keys, common F1-F24 keys, bracketed paste,
conditional focus-in/focus-out, and negotiated Default/X10-compatible,
UTF-8, and SGR mouse bytes. Mouse tracking filters are explicit: Press,
PressRelease, ButtonMotion, and AnyMotion progressively add release, drag, and
unbuttoned movement events. Legacy and UTF-8 out-of-range coordinates degrade
to empty output.

The required checks passed:

```text
$ cargo test -p codegotchi-cli --test terminal_pty -- --nocapture
test result: ok. 1 passed; 0 failed
$ cargo check -p codegotchi-cli
Finished `dev` profile
$ cargo fmt --all -- --check
ok
$ cargo clippy -p codegotchi-cli --all-targets -- -D warnings
Finished
```

This is a pure screen/read-model and encoding proof only. It does not include
the terminal host/event loop, raw mode, Ratatui compositor, room routing,
pane coordinate ownership/transforms, care interactions, or the real installed
Codex fidelity gate; those remain later task scope.
