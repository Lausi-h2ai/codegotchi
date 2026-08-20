# Terminal Room Final Verification

Status: **PASS — ready for merge**

Task 7 Fix Round 1 is tied to renderer commit
`992e5b6833c41757169c9ee69615d170e933b561`; the evidence commit is recorded
in the final report. The six inspected PNGs and exact capture mappings are in
[terminal-room evidence](terminal-room/README.md).

| Requirement | Result |
|---|---|
| Deterministic Full care state | PASS — explicit bottom-aligned fixture seeds three poops plus Affection/Snack demands; light/dark/bed/doze frames visibly show them. |
| Full 120x45 light/default | PASS — complete room composition and seeded care state. |
| Full 120x45 dark | PASS — seeded care state remains readable in night contrast. |
| Compact 120x30 | PASS — widened bowl/poop targets and labels survive with pet/bed. |
| Minimal 120x21 | PASS — visible FOOD/BED/POOP controls start exactly at their hit rectangles. |
| Wide hit regions | PASS — focused tests cover object-width/height regions in Full and Compact. |
| Bed sleep vs floor doze | PASS — covered bed pet versus horizontal open-floor curl, same Full framing. |
| Automated tests | PASS — 70 library tests and 17 terminal-room integration tests. |

Remaining differences are non-blocking ANSI abstraction and the pre-existing
duplicate `BED BED` wide-bed label. No scoped Important blocker remains.

## Task 5 platform/Codex verification (historical record retained)

Task 5 snapshot status: **FAIL — blocking items at that checkpoint: real Codex
fidelity checklist was incomplete; macOS CI had not run on a hosted runner; and
final resize/interaction visual evidence was missing.** The detailed PTY/mode
record remains in [`terminal-room-codex-pty.md`](terminal-room-codex-pty.md).
This historical record is retained alongside the later Task 7 visual PASS; it
does not replace the final visual status above.

Candidate source SHA for the run: `a7d04c3497bb` (Task 5 changes were made
after this source checkout). Official Codex: `codex-cli 0.147.0`, installed
temporarily as `@openai/codex@0.147.0`; the host-global `0.148.0` was not used.
Environment: WSL2 Linux, xterm 390, Xvfb `:99`, Noto Sans Mono 10, outer
terminal 120×45 (1324×904 pixels).

### Automated coverage

The virtual-screen and production-seam tests are mode-driven: they feed VT
control bytes and assert encoded bytes/events from `CodexScreen::input_modes()`.
They do not parse visible Codex text.

| Requirement | Test evidence |
|---|---|
| Application cursor enabled/disabled | Existing `terminal_screen::input_modes_track_vt_controls_and_split_focus_without_visible_text`, `mode_fixtures_enable_and_disable_each_protocol_without_precedence_masking`, and `key_encoding_follows_application_mode_and_common_codex_keys`; production seam coverage in `production_session_core_uses_negotiated_modes_for_every_input_kind`. |
| Bracketed paste enabled/disabled | Existing screen fixture and `paste_and_focus_encoding_are_strictly_mode_driven`; production seam coverage in `production_session_core_uses_negotiated_modes_for_every_input_kind`. |
| Focus reporting enabled/disabled | Existing split-sequence/RIS screen fixtures and production seam coverage in `production_session_core_uses_negotiated_modes_for_every_input_kind`. |
| Mouse tracking changes | `production_session_core_encodes_every_negotiated_mouse_tracking_level` covers Press, PressRelease, ButtonMotion, and AnyMotion; disabled mode remains covered by `mouse_disabled_mode_emits_no_bytes_even_through_core_seam`. |
| Mouse encoding changes | `production_session_core_encodes_every_negotiated_mouse_coordinate_encoding` covers Default, UTF-8, and SGR. |
| Unsupported-mode isolation | `unsupported_mouse_modes_only_leave_safe_encoding_fallback_and_active_tracking` sends unsupported `?1015`/`?1016` and proves unrelated flags and event bytes survive. |

Commands and results from the focused cycle:

```text
cargo test -p codegotchi-cli --test terminal_screen unsupported_mouse_modes_only_leave_safe_encoding_fallback_and_active_tracking -- --nocapture
  1 passed; 0 failed
cargo test -p codegotchi-cli --test terminal_session production_session_core_encodes_every_negotiated_mouse -- --nocapture
  2 passed; 0 failed
cargo fmt --all -- --check
  pass
```

The CI change adds a secondary `rust-macos` job running exactly:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

The direct PTY lifecycle suite remains platform-neutral where possible and
already covers resize, exit status, SIGINT/SIGTERM/escalation, descendant
cleanup, and blocked-reader unblocking on Unix. The hosted macOS result was
pending at this checkpoint; it was not represented as local evidence.

### Real Codex and visual gate at the Task 5 checkpoint

The official Codex was launched through the production command:

```text
codegotchi run --ui terminal -- codex --ask-for-approval never --sandbox danger-full-access --dangerously-bypass-hook-trust
```

The disposable authenticated profile was scoped to a unique run directory.
The TUI loaded at Full size and the room rendered below it. The transcript
records the trust prompt, model panel, and the sent prompt
`Reply with exactly READY and no tools.`; the prompt text reached the hosted
PTY. A later bounded run remained at `Booting MCP server: codex_apps` and did
not return a model reply. Sending `/exit` and Ctrl-C bytes did not interrupt
that startup, so the exact run-owned launcher/process groups were terminated
after printing their PID/PPID/PGID/TTY records. The active session Codex
process was not matched or signalled.

Reviewed captures:

| Artifact | Finding |
|---|---|
| `docs/verification/terminal-room/task5-a7d04c3497bb-full-120x45-codex-start.png` | Official Codex 0.147.0 startup/model panel, Full room, status bars, furniture, food affordances, and pet are visible. |
| `docs/verification/terminal-room/task5-a7d04c3497bb-full-120x45-after-prompt.png` | Same production composition after prompt input; no obvious protocol leakage or clipping in the reviewed frame. |

Those PNGs were inspected directly. No GIF/video or resize frame was claimed:
`ffmpeg`/`wf-recorder` was unavailable, and the xterm window detached while
the external `codex_apps` MCP startup was pending. The Task 5 README status
therefore remained `PENDING_VISION_REVIEW` at that checkpoint.

Checklist status at the Task 5 checkpoint:

| Item | Result |
|---|---|
| Prompt entry | Partial pass — prompt reached the PTY and was echoed. |
| Model reply | Blocked by `codex_apps` MCP startup. |
| Keyboard navigation | Automated byte proof only; real run blocked. |
| Bracketed paste | Not run; no clipboard helper installed. |
| Focus reporting | Not observable before the blocker. |
| Codex mouse scroll/click | Not run. |
| Slash command / tool approval | Not run. |
| Full → Compact → Minimal → Full | Not captured; no fake evidence substituted. |
| Pet/feed/clean/nap while Codex is usable | Not run. |
| Clean exit/terminal restore | Launcher attempt produced equal baseline/final `stty -g`; Codex interaction still blocked. |
