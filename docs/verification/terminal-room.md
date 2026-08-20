# Terminal Room Final Verification

Status: **FAIL — Task 8 release gate remains blocked**

Task 7's visual acceptance is retained as historical evidence only. Task 8's
final tested source head is `8f5184b34b94d877617440030aa46f635579b47b` (on top
of `bc8d288db6f23f14420d61a95772922de8343aee`). The existing six PNGs were
inspected, but were captured against the older Task 7 renderer and were not
re-captured after the Task 8 geometry fix; they therefore cannot close the
final visual gate.

| Requirement | Result |
|---|---|
| Deterministic Full care state | HISTORICAL PASS — Task 7 fixture images were inspected; no final-SHA recapture. |
| Full 120x45 light/default | BLOCKED — final-SHA visual recapture was not run. |
| Full 120x45 dark | BLOCKED — final-SHA visual recapture was not run. |
| Compact 120x30 | BLOCKED — final-SHA visual recapture was not run. |
| Minimal 120x21 | PASS — regression tests prove rendered controls and edge clicks align. |
| Wide hit regions | PASS — rendered food/poop extents are asserted in Full and Compact; edge clicks dispatch care. |
| Bed sleep vs floor doze | HISTORICAL PASS — no final-SHA recapture. |
| Automated tests | PASS on final rerun — fmt, clippy, workspace tests, web tests/lint/format/build, embed, and 17 Playwright tests. |

Task 8's final blockers are listed at the end of this document.

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

## Task 8 final release-gate disposition

Final tested source head: `8f5184b34b94d877617440030aa46f635579b47b`.
Evidence documentation commit: `31362fe`.
The prior hit-region fix is `86dc27509fbf5b34e2b23ed61779e8445e790cad`.

The carried Important gap is closed by TDD. `rendered_care_extents_are_inside_their_hit_regions`
checks every rendered food/poop cell against its actual Full and Compact hit
region; `rendered_food_and_poop_edges_dispatch_care_requests` clicks rendered
edge cells and verifies Feed/Clean/Nap behavior, including Minimal alignment.
The follow-up `rendered_food_labels_do_not_overlap_poops_in_wide_layouts`
regression test led to the final label/poop spacing fix.

Fix Round 1 additionally keeps all three normal Full poop targets disjoint
from the bed and proves a bed click dispatches Nap in
`full_three_poop_slots_stay_outside_bed_and_bed_click_naps`.

Final automated evidence:

```text
cargo fmt --all -- --check                                      PASS
cargo clippy --workspace --all-targets --all-features -- -D warnings PASS
cargo test --workspace                                        PASS on final rerun
corepack pnpm test (web)                                      PASS (120 tests)
corepack pnpm lint                                             PASS
corepack pnpm format:check                                     PASS
corepack pnpm build                                            PASS
node web/scripts/embed-web.mjs                                 PASS
corepack pnpm playwright:test                                  PASS (17 tests)
```

The first workspace invocation had a transient `full_vertical_flow` failure
that reproduced on baseline `edc0968`; the completed rerun at this final SHA
passed all workspace targets. This is retained as a flake note, not hidden.

Blocking external/manual gates remain: hosted Ubuntu/macOS GitHub Actions
results for this SHA are unavailable locally; the final-SHA six-image visual
recapture was not run after the geometry fix; and the real Codex checklist
(approval/tool flow, bracketed paste/focus, mouse care, and Full → Compact →
Minimal → Full) was not observed in this bounded run. Existing Task 5/7
captures are historical and do not substitute for those gates.

Status: **FAIL — final release gate is blocked by the explicit external/manual
items above.**
