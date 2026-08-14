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
| `vt100` | `0.16.2` | `0.16.2` | Application cursor, bracketed paste, mouse levels, Default/UTF-8/SGR encoding state | 1004 focus reporting is absent; URXVT and SGR-pixel/1016 are unsupported. H4a adds bounded protocol-side focus tracking for 1004. |
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
running 4 tests
test managed_pty_preserves_direct_invocation_input_resize_ansi_and_exit_code ... ok
test result: ok. 4 passed; 0 failed
```

That test directly executes the fixture (never `sh -c`), verifies literal
trailing arguments and `CODEX_HOME`/`CODEGOTCHI_SESSION_FILE`, observes
`FAKE_CODEX_READY` plus ANSI color bytes, delivers one input line, resizes to
120 columns × 31 rows, observes `FAKE_CODEX_SIZE=<31 120>`, and preserves exit
code 23. Full RED/GREEN/check/fmt/clippy output and lifecycle review are in
`.superpowers/sdd/2026-08-13-terminal-room-agent-hardening/task-h3-report.md`.

## H4a virtual screen and input-mode evidence

H4a was implemented on 2026-08-14 as the non-interactive terminal core. The
fix round keeps the same non-interactive boundary and addresses the negotiated
wire details found in review. The focused RED run added exact Home/End,
mouse-release, isolated mode-toggle, split-RIS, and malformed-input assertions;
it failed only on the missing application Home sequence, SGR button release
code, and RIS focus reset. After the minimal implementation, the focused GREEN
fixture passed:

```text
$ cargo test -p codegotchi-cli --test terminal_screen -- --nocapture
running 10 tests
test input_modes_track_vt_controls_and_split_focus_without_visible_text ... ok
test key_encoding_follows_application_mode_and_common_codex_keys ... ok
test mouse_encoding_honors_protocol_and_tracking_level ... ok
test mouse_wire_encodings_preserve_modifiers_wheels_and_release_forms ... ok
test mode_fixtures_enable_and_disable_each_protocol_without_precedence_masking ... ok
test malformed_and_split_control_input_never_panics_or_activates_focus ... ok
test paste_and_focus_encoding_are_strictly_mode_driven ... ok
test ris_clears_split_focus_reporting_and_still_resets_vt_screen ... ok
test screen_handles_ansi_state_and_read_only_cell_access ... ok
test unknown_and_truncated_controls_leave_subsequent_screen_text_usable ... ok
test result: ok. 10 passed; 0 failed
```

`CodexScreen` wraps `vt100::Parser` with explicit dimensions and bounded
scrollback, exposes read-only screen/cell/cursor helpers, and maps every
public vt100 mouse mode and encoding into stable CodeGotchi enums. An
incremental fixed-size tracker recognizes split `ESC[?1004h/l` focus controls
and clears focus on split `ESC c` RIS; visible text and malformed/unknown/
truncated controls do not change the mode model. The same raw bytes are still
always fed to vt100, so RIS also resets screen contents and attributes. Mode
fixtures explicitly cover enable/disable for `?1`, `?2004`, split `?1004`,
isolated `?9`/`?1000`/`?1002`/`?1003`, and `?1005`/`?1006` encoding mappings.

Pure encoders cover UTF-8 text, Enter/Tab/BackTab/Backspace/Escape, control and
Alt chords, navigation/editing keys, common F1-F24 keys, bracketed paste,
conditional focus-in/focus-out, and negotiated Default/X10-compatible,
UTF-8, and SGR mouse bytes. Home/End use CSI `H`/`F` normally and SS3 `H`/`F`
in application-cursor mode; modified Home/End remain CSI `1;modifier H/F`.
Mouse wire fixtures prove Shift/Alt/Control bits, horizontal wheel codes 66/67,
legacy release code 3 plus modifiers, SGR left/middle/right release codes 0/1/2
with lowercase `m`, and UTF-8 out-of-range coordinates degrading to empty
output. Mouse tracking filters are explicit: Press, PressRelease,
ButtonMotion, and AnyMotion progressively add release, drag, and unbuttoned
movement events.

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

## H6b production renderer evidence

The Codex VT state now has a production `render_codex` path that paints an
immutable `CodexScreen` into a clipped Ratatui `Buffer`. The renderer maps
`Default` to `Color::Reset`, ANSI indexes 0–15 to Ratatui's named palette,
indexes 16–255 to `Color::Indexed`, and RGB values to `Color::Rgb`. It resets
each destination cell before applying foreground/background and bold, dim,
italic, underline, and inverse (`REVERSED`) modifiers. Combining text and
wide-cell continuation geometry are retained without emitting a duplicate
continuation glyph; a wide lead is blanked when its continuation would be
clipped or absent from the backing buffer. The VT cursor is returned only when
visible and inside the clipped area, translated by the area origin. Zero-sized,
overflowing, and partially clipped areas are covered without panics or writes
outside the supplied area.

The focused RED run was written before the renderer and failed on the missing
`render_codex` export. The GREEN suite uses hand-derived `Buffer` assertions,
a Ratatui `TestBackend` composition, and an in-memory Crossterm backend. The
serialization proof forces Crossterm color output only for the test, asserts
non-reset 38/48 ANSI sequences for ANSI/indexed/RGB cells produced by the
production renderer, and restores the prior global color policy. The suite
covers non-zero origins, clipping (including a wide lead whose continuation is
missing from the backing buffer), zero-size areas, default/ANSI/indexed/truecolor
mapping, all required modifiers, blank-cell style, combining/wide geometry,
cursor visibility and translation, and screen immutability:

```text
$ cargo test -p codegotchi-cli --test terminal_render -- --nocapture
running 13 tests
... all 13 tests ... ok
test result: ok. 13 passed; 0 failed
```

The deterministic fixture is
`crates/codegotchi-cli/examples/terminal_codex_fixture.rs`. It feeds
hand-authored ANSI/VT bytes covering cursor placement, erase/background,
ANSI/indexed/truecolor colors, all renderer modifiers, Unicode combining and
wide glyphs, and hidden/visible cursor transitions through this exact
production renderer. It uses the H6a `TerminalGuard` lifecycle and Ratatui
Crossterm backend, deliberately calls Crossterm's
`force_color_output(true)` in the fixture process so ambient `NO_COLOR` cannot
erase the visual proof, holds the rendered frame for
`CODEGOTCHI_TERMINAL_FIXTURE_MS` (default 3 seconds), and restores the
terminal on normal and error exits. This override is fixture-only; production
CodeGotchi rendering remains respectful of the user's `NO_COLOR` setting.

PTY execution evidence (this is pre-real-Codex and pre-room, and is not a
real-Codex fidelity or first-room gate) was captured with:

```text
$ script -q -c 'stty rows 24 cols 80; TERM=xterm-256color CODEGOTCHI_TERMINAL_FIXTURE_MS=500 cargo run -p codegotchi-cli --example terminal_codex_fixture' /tmp/codegotchi-terminal-fixture.scriptlog
```

The resulting ANSI transcript is at
`/tmp/codegotchi-terminal-fixture.scriptlog`. The first visual capture attempt
at `/tmp/codegotchi-codex-renderer-fixture.png` is retained as failed evidence:
ambient `NO_COLOR=1` suppressed Crossterm's 38/48 color sequences even though
the production `Buffer` cells retained their styles.

After the fixture-only color override, the orchestrator ran the same production
renderer in an 80 x 24 xterm on Xvfb and captured
`/tmp/codegotchi-codex-renderer-fixture-pass2.png`. Direct image inspection on
2026-08-14 confirmed distinct ANSI red/blue, indexed magenta/yellow, truecolor
cyan, inverse styling, aligned wide/combining Unicode, and a correctly placed
visible cursor with no obvious wide-cell seam. This passes the renderer fixture
visual loop only; it remains pre-real-Codex and pre-room evidence and does not
satisfy either later hard gate.

## H6c interactive PTY session evidence

Round 3 is based on commit `d08d277ded0166ba3f0626a944caad30dbce9d6e`
(`fix: make pty session cleanup race-safe`). This pass is Linux/WSL-first.
macOS terminal-session acceptance is explicitly deferred: nix 0.31 exposes
the `WNOWAIT` flag on Apple targets but not its safe `waitid` wrapper, so the
unsupported fallback disarms the cached PGID after portable-pty reaps and
before any post-reap cleanup path can signal it; no macOS natural-leader
descendant cleanup claim is made here.

The H6c session now composes the H6a `TerminalGuard`, managed
`PtyCodexChild`, H4a `CodexScreen`/input encoders, and H6b `render_codex` over
the whole physical terminal. `TerminalSessionCore` is the deterministic
production seam: one bounded PTY output chunk updates the same VT screen that
the real adapter renders, and every key, paste, focus, and mouse event reads
the negotiated mode model at the moment it is encoded. The adapter does not
install signals; it consumes an externally supplied bounded
`TerminalSessionSignal` receiver with `Interrupt`, `Terminate`, and
`WindowChange` values.

The adjudicated review RED runs were intentional. Before the process-control
methods and scheduler existed, the PTY tests failed with unresolved
PtyCodexChild interrupt/terminate methods. Before the fairness implementation,
the scheduler regression failed with unresolved SessionWorkKind/session_work_order
symbols. Before the composed adapter seam, the integration test could not
import TerminalSessionEventSource/run_terminal_session_with_events.

```text
$ cargo test -p codegotchi-cli --test terminal_session --no-fail-fast
error[E0432]: unresolved imports `TerminalSessionError`,
`initialize_terminal_and_spawn`
```

The focused GREEN integration suite passes seven deterministic session tests. It proves
entry failure calls no spawn callback; successful entry calls exactly one
callback with exact invocation and rows/columns; negotiated application
cursor, bracketed paste, focus, and SGR mouse bytes are exact while disabled
mouse is silent; resize is not transposed; VT state is bounded; a closed
signal receiver can be disabled without polling spin; and queued ESRCH from
Interrupt/Terminate is benign while the actual exit status remains available.
The direct PTY suite now passes four tests, including exact signal statuses and
cancellation cleanup. The production session module has nine unit tests for
reader ownership/backpressure, scheduler ordering, benign ESRCH handling, and
restoration context.

The scheduler rotates signal, event, child-poll, and output branches on each
turn. At most one bounded output chunk is handled per turn, so continuous PTY
output cannot starve control work. Duplicate same-signal values are ignored,
but Interrupt followed by Terminate escalates. Unix uses the PTY's
setsid/process-group leader metadata for explicit SIGINT/SIGTERM/SIGKILL
delivery; non-Unix retains the portable-pty fallback.

The dedicated PTY reader runs on one thread and sends at most 16 messages of at
most 8 KiB each. On Unix native PTYs it clones the master descriptor and polls
readiness with a 25 ms timeout, so cancellation reaches a checkpoint even when
a slave holder survives. `SessionResources` enforces cancellation order:
signal/kill the child group, drop the bounded output receiver, then cancel and
boundedly join the reader. The guard waits at most 100 ms; if a non-native
fallback violates cancellation it reports a cleanup failure or detaches on
drop rather than hanging the session. Normal cleanup terminates or kills the
PTY group, polls the direct child to a deadline, and joins the reader.
Linux/other waitid-capable Unix targets observe exit with
`waitid(WEXITED | WNOHANG | WNOWAIT)`, consume the one-shot descendant group
while the leader is still a zombie, and only then let portable-pty reap it;
unsupported targets disarm the identity before post-reap cleanup. Drop uses
bounded polling and transfers an unreaped child handle to a process-wide
reaper, never an unbounded wait. The bounded reader proof is scoped to the PTY
child/process group and descendants, not escaped sessions.

Round 3 focused RED/GREEN evidence:

```text
$ cargo test -p codegotchi-cli --lib session_resources_drop_is_bounded_when_reader_read_stays_blocked -- --nocapture
FAILED: reader cleanup synchronously waited for a blocked read
$ cargo test -p codegotchi-cli --lib direct_kill_disarms_cached_group_identity_before_reap -- --nocapture
FAILED: no method named `mark_direct_kill`

$ cargo test -p codegotchi-cli --lib session_resources_drop_is_bounded_when_reader_read_stays_blocked -- --nocapture
ok
$ cargo test -p codegotchi-cli --lib direct_kill_disarms_cached_group_identity_before_reap -- --nocapture
ok
$ cargo test -p codegotchi-cli --test terminal_pty -- --nocapture
ok. 4 passed
```

The production-shared injected event source is used by the ignored outer-PTY
integration tests; it is not a parallel test loop:

```text
$ script -q -c 'stty rows 24 cols 80; TERM=xterm-256color cargo test -p codegotchi-cli --test terminal_session composed_session_adapter_delivers_exact_invocation_modes_input_and_status -- --ignored --nocapture' /tmp/codegotchi-h6c-resize.scriptlog
test composed_session_adapter_delivers_exact_invocation_modes_input_and_status ... ok
test result: ok. 1 passed; 0 failed; finished in 1.63s

$ script -q -c 'stty rows 24 cols 80; TERM=xterm-256color cargo test -p codegotchi-cli --test terminal_session composed_session_adapter_fairly_handles_signals_during_continuous_output -- --ignored --nocapture' /tmp/codegotchi-h6c-fairness-1.scriptlog
test composed_session_adapter_fairly_handles_signals_during_continuous_output ... ok
test result: ok. 1 passed; 0 failed; finished in 1.12s

$ script -q -c 'stty rows 24 cols 80; TERM=xterm-256color cargo test -p codegotchi-cli --test terminal_session composed_session_adapter_cancellation_under_output_flood_completes -- --ignored --nocapture' /tmp/codegotchi-h6c-cancel.scriptlog
test composed_session_adapter_cancellation_under_output_flood_completes ... ok
test result: ok. 1 passed; 0 failed

$ script -q -c 'stty rows 24 cols 80; TERM=xterm-256color cargo test -p codegotchi-cli --test terminal_session composed_session_adapter_closed_signal_receiver_completes_without_spin -- --ignored --nocapture' /tmp/codegotchi-h6c-closed.scriptlog
test composed_session_adapter_closed_signal_receiver_completes_without_spin ... ok
test result: ok. 1 passed; 0 failed

$ script -q -c 'stty rows 24 cols 80; TERM=xterm-256color cargo test -p codegotchi-cli --test terminal_session composed_session_adapter_cleans_descendant_after_natural_leader_exit -- --ignored --nocapture' /tmp/codegotchi-h6c-natural-leader.scriptlog
test composed_session_adapter_cleans_descendant_after_natural_leader_exit ... ok
test result: ok. 1 passed; 0 failed
```

The composed adapter records exact direct argv/env and exact negotiated wire
bytes for application cursor, bracketed paste, focus, and SGR mouse. It
changes the outer physical PTY from 24 x 80 to 31 x 120, sends a production
resize event/signal, and the fake child reports size=24 80 and
resized-size=31 120, proving rows and columns are not transposed. Its child
status is exactly 0. The continuous-output test runs the same production loop
with a closed signal branch and asserts exact 130 for Interrupt and exact 143
for Interrupt-to-Terminate escalation. It passed three consecutive runs
(1.14s, 1.17s, and 1.14s).

The older direct real-adapter fixture also asserts exact status 130 after the
externally supplied Interrupt:

```text
test real_session_adapter_spawns_fixture_and_reaps_after_external_interrupt ... ok
```

All of this is fake-PTY evidence, not a real Codex run. No H6c screenshot was
captured in this worker, and no real-Codex fidelity, first-room, launcher,
layout, or room-art gate is claimed. The earlier H6b renderer screenshot
remains renderer-fixture evidence only. The composed resize regression uses an
RAII outer-PTY size guard, so an assertion panic restores the original size;
explicit success restoration is marked complete and is not repeated by Drop.

## H6d production launcher routing evidence

H6d routes the existing launch lifecycle into the H6c terminal session at the
final Codex handoff. Shared repository state, runtime, metadata, server, hook
profile, and profile guard setup remain in `run_async`. The authoritative
`Arc<AuthoritativeRuntime>` remains owned by the launcher while a clone is
passed to the server. `SignalController` is installed once; Browser/inherited
waits consume it directly, while terminal attempts forward the same controller
to one bounded `TerminalSessionSignal` channel. No disconnected signal channel
is allocated before a terminal attempt.

The four routes are production callbacks, not a test-only duplicate:

| mode | browser URL/helper | Codex handoff | Auto fallback |
|---|---|---|---|
| Browser | yes | inherited stdio, one child | n/a |
| Terminal | no | H6c PTY session, one invocation | none |
| Both | yes | H6c PTY session, one invocation | none |
| Auto | only after terminal initialization failure | PTY on success, otherwise inherited stdio | only `Initialization` before PTY spawn |

Terminal mode never emits the bearer-token URL. Browser and Auto fallback retain
the existing URL/browser helper behavior. Terminal session errors after the
initialization boundary are surfaced and never retried through inherited stdio.
Portable-pty signal descriptions are normalized (`Interrupt`, `Terminated`,
and standard aliases) so terminal exits preserve conventional 128+signal
status codes.

Focused RED/GREEN evidence:

```text
$ cargo test -p codegotchi-cli launcher::tests::terminal_exit_status_preserves_portable_pty_signal_codes -- --nocapture
FAILED: left 1, right 130

$ cargo test -p codegotchi-cli launcher -- --nocapture
test result: ok. 18 passed; 0 failed

$ cargo test -p codegotchi-cli --test process_wrapper explicit_ui_modes_route_the_production_launcher_once -- --nocapture
test result: ok. 1 passed; 0 failed

$ cargo test -p codegotchi-cli --test terminal_session launcher_terminal_entry_retains_signals_when_initialization_fails -- --nocapture
test result: ok. 1 passed; 0 failed
```

The process-wrapper integration invokes the compiled binary in Browser,
Terminal, Both, and Auto modes. It records one fake-Codex PID for Browser and
Auto, no child for terminal-only initialization failure or Both without a
physical terminal, conditional browser-helper execution, metadata cleanup, and
absence of `#token=` output in terminal mode. The session integration proves
the launcher-aware entry seam returns its bounded signal receiver when physical
terminal initialization fails and does not invoke the profile-before-spawn
callback.

This is Linux/WSL production-routing evidence with a fake Codex and no real
Codex fidelity claim. macOS terminal lifecycle remains deferred, and the real
Codex/first-room/screenshot gates remain later orchestrator work.
