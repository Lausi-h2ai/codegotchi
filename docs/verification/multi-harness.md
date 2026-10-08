# Experimental multi-harness verification ledger

Re-audited on 2026-10-08 in Linux/WSL, with the working-tree implementation based
on `aee84eb08c634d8278f1740a5a2b106b785233df`. Pi, Claude Code, OhMyPi, and
Hermes adapters are experimental. The checks below describe specific tested
versions and environments, not a guarantee of complete CLI compatibility.

Generated screenshots and raw logs are local artifacts excluded from Git.
The small JSON results, inspection notes/hashes, and Markdown ledger remain
tracked. Screenshot/log links below refer to local captures, so they are not
available when browsing this ledger on GitHub. The retained verifier scripts
can regenerate captures; the original audit files remain on the audit machine.

## Completed continuation audit (2026-10-08)

The installed release executable and `target/release/codegotchi` now have SHA-256
`81951d30ed537935f2fad76446c64294156dee392fa274a7f17ab92f0d9d53a6`.
It was installed using `cargo install --path crates/codegotchi-cli --force --locked`.
The earlier build and provider evidence below remain historical.

This continuation fixed a second input bug in the terminal background filter.
A partial OSC reply or ordinary Alt+] could remain buffered indefinitely: the
session's 10 ms scheduler repeatedly cancelled the input future and restarted
its 100 ms timeout. The deadline now lives in the event stream and survives
cancellation. A regression test repeatedly cancels the input future in the same
way and requires the original event to be replayed. Seven event-stream tests
pass. The prior fix that answers native background queries is retained.

### Real keyboard and manual visual coverage

**40 complete real-CLI cases passed** on the final installed executable:
Pi 0.85.1, OhMyPi 18.8.0 (Bun 1.4.2), Claude Code 2.1.292, and Hermes
upstream `0e219331` classic UI, each in light and dark xterm backgrounds with
Auto, Mono, Soft Green, Amber, and Night room presets. These are genuine CLIs
using scripted loopback model endpoints in private test homes; this matrix
does not make authenticated provider requests.

Every case used the production launcher, generated extension/hooks, PTY,
backend, and real xterm keyboard/pointer events. Assertions check corrected
prompt text at the model endpoint, actual shell marker-file contents, native
Up/Down history, bracketed multiline paste without premature submission,
editing at Full 120×45 / Compact 100×30 / Minimal 80×21 / Narrow 64×45,
focus return, a Strict denial in the specific native tool result with its
marker file absent, food dragging with authoritative hunger 100→75, keyboard
input after care, normal exit 0, and runtime/integration cleanup. Feeding alone
does not replenish energy: the post-care probe can still be denied as exhausted.

All **80 final layout/state sheets (400 cells)** were opened and manually
inspected in this continuation. Every sheet contains either four layout cells
or six state cells: edited input, response, history, Strict response, care, and
keyboard after care. [Evidence index](multi-harness/final-evidence.md),
[behavioral receipts](multi-harness/final-results.json), and
[per-artifact visual notes and hashes](multi-harness/final-inspection.json)
identify the inspected files. This completes the interrupted preset inspection,
including all harnesses in both outer backgrounds, beyond the earlier OMP-only
light preset report. `--ui both` starts the browser backend too; these screenshots
inspect the terminal pane, not the browser room.

The native input panes remained usable at all inspected sizes, and the room
presets remained independent of the upper CLI's palette. Pi and OMP selected
readable native light palettes on white xterms, including with explicit dark
room presets. The inspection found limitations, so this is **not a claim of
defect-free visuals**:

- Hermes's native yellow logo/headings/tips are faint on white backgrounds;
  black input and tool text remain readable. The retained direct-launch baseline
  shows the same yellow-on-white issue. A dark room preset does not change the
  native Hermes theme.
- Existing pet animation can overlap furniture, care/status text, and need
  indicators within Full, Compact, and Narrow room panes. Compact examples can
  obscure part of the room status. The room does not overlap the native composer;
  the minimal room intentionally uses a short strip. This audit does not fix
  the general room renderer overlap.
- Native tool summaries can collapse details (particularly Hermes and Claude).
  Their behavioral receipts, and Claude's expanded denial frame, establish
  enforcement rather than relying on a generic completion line.

The original dark Night attempt ended with SIGTERM (exit 143) after Pi, without
a reported assertion failure; its cause was not established. Its partial
artifacts are excluded from the 40-case count. A separate complete dark Night
retry passed for all four. The [interrupted-attempt record](multi-harness/final-matrix/dark-night/interrupted.json)
and [process audit](multi-harness/final-matrix/dark-night/process-audit.json)
are retained. No matching processes remained; no manual kills were needed.

### Native sessions and first-run flows

Pi and Claude additionally passed a streamed Escape interruption confirmed by
saved native aborted assistant records, follow-up, twelve sustained turns,
exit/restart, conversation resume with contextual recall, and a thirteenth turn.
Claude's actual native shell permission popup was keyboard-approved and its tool
result verified; Pi has no equivalent popup. Both final session sheets were
manually inspected. [Receipts](multi-harness/final-native-sessions/results.json)
and [frames](multi-harness/final-evidence.md) cover Full dark / local model only.

**Claude lifecycle limitation:** Escape aborts its native response, but CodeGotchi
can stay `Active: thinking` until the next follow-up completes. The result records
`nativeStopHookSettled: false`; `passed: true` there describes native conversation
behavior, not correct interrupt-to-idle integration. Pi settled to WaitingForUser.
OMP/Hermes interruption, long sessions, and resume have not been exercised by this
extended verifier.

OMP's genuine five-step setup was traversed with keyboard input and each screen
manually inspected: provider choice (login skipped), local test model, Unicode
glyphs, StatusBand composer, and Match terminal theme. Setup bypass was disabled,
a subsequent real tool marker was verified, and exit/runtime cleanup succeeded.
The Nerd Font preview had missing glyphs in xterm; Unicode was chosen.
[OMP first-run receipts](multi-harness/final-onboarding/omp.json).

Hermes's generated configuration was tested **without a synthetic approval
allowlist**. The four native hook command prompts were individually inspected
and approved by typing y and Return, telemetry was declined, and an exact prompt,
real shell marker, pet events, native Strict denial, and normal cleanup were
verified. A separate launch with no hooks executed its Strict probe while pet
activity stayed Idle / events 0, confirming that `run -- hermes` alone provides
the room but not activity or enforcement. [Approved-hook receipts](multi-harness/final-onboarding/hermes.json)
and [missing-hook receipts](multi-harness/final-onboarding/hermes-no-hooks.json).
These first-run checks cover Full dark only and change disposable homes,
not the user's configuration.

Only Pi is on the normal PATH. Unqualified Claude/OMP/Hermes commands still exit
2 with actionable missing-executable errors on this machine. Their real binaries
in the earlier research directory were selected using supported
`CODEGOTCHI_REAL_*` overrides; this audit did not globally install those CLIs.
[PATH checks](multi-harness/final-path-check.json). Hermes also requires
`codegotchi integration hermes`, merging those hooks into the active profile,
and its native first-use approval, as described in the README.

### Final checks and remaining coverage gaps

After the input fix, `TMPDIR=/dev/shm cargo test --workspace` passed **464 tests,
10 ignored, 0 failures**. Clippy for all targets/features with `-D warnings`,
Rust formatting, Python verifier compilation, and whitespace checks passed.
The earlier 123 web tests / 18 production Playwright checks below were not
rerun for this terminal-only fix. Fresh [check receipts](multi-harness/final-checks.json)
record the commands and final build hash.

Authenticated Pi provider evidence below is retained on the earlier binary;
it was not rerun on this final build. Claude has no available provider login/key;
authenticated Claude, OMP, and Hermes conversations remain unverified. Also
unverified: other operating systems/emulators, every native CLI theme/shortcut,
Hermes's optional TUI, OMP/Hermes extended interruption/resume, first-run screens
at other sizes/backgrounds, and new browser visual coverage. The four adapters
remain experimental with these explicit findings and gaps.

## Experimental release preparation (2026-10-07)

This historical section predates the completed continuation above. Its binary
hash, test counts, and stated gaps describe that earlier stage.

The README and working-tree implementation describe Pi, Claude Code, OhMyPi, and Hermes as
**experimental adapters**. The checks establish the specific behaviors below;
they do not promise complete compatibility with providers, native CLI themes,
or every terminal platform.

The hosted-terminal background defect is fixed. The production host keeps
Crossterm as its sole input reader, probes the outer terminal before spawning
the CLI, preserves queued keyboard/paste events, and answers child OSC 11
queries with the measured RGB background. This lets OhMyPi choose its native
light palette in a light xterm. Exact RGB components remain intact when a late
reply updates the cached background. The lower room keeps its selected palette.

The startup probe has a 500 ms deadline; an unsupported or slower terminal
initially receives a conservative dark fallback. Complete late replies are
filtered, but a reply whose fragments stall for more than 100 ms is replayed as
input to preserve ordinary keyboard events. Malformed replies are replayed in
order. Later changes to the outer terminal's colors require a session restart.
The accepted protocol is OSC 11 `rgb:` with BEL or ST termination; unsupported
color formats retain the fallback. Six event-stream tests cover startup Unicode
paste/modifier-key preservation, valid uppercase/BEL/ST replies, fallback,
timeout replay, and release-key replay. RGB component scaling/query tests and
the existing terminal input/PTY tests also pass.

The optimized binary at this historical stage was SHA-256
`9220dcd6c4c2d56f803bac0d8c8b5ac1b027c700dc03965a16b00d5509123fcc`.
Each new keyboard result records its tested binary hash.

Checks at that stage: `TMPDIR=/dev/shm cargo test --workspace` **463 passed, 10 ignored,
0 failed**; Clippy with all targets/features and `-D warnings`; Rust formatting;
**123 web tests**, web lint/formatting; **18 production Playwright tests**.

### Authenticated provider evidence

The opt-in [provider verifier](../../scripts/verify-harness-provider.py) reads
saved native session messages instead of matching echoed prompts. It uses
isolated homes and removes copied credentials and private session transcripts
on exit. The caller must explicitly select an existing Pi auth file or supply
Claude credentials locally; nothing signs the user in or changes their login.

Pi 0.85.1 passed an authenticated `openai-codex/gpt-5.5` conversation, a real shell
tool result, an interrupted response with native `stopReason=aborted`, a follow-up,
three additional turns, and process restart with `--continue`. The resumed
conversation recalled an earlier random token that was absent from the resumed
question, and both processes exited 0. Pi has no separate native approval popup
in this mode. [Results and inspected frames](multi-harness/provider-live/README.md)
retain only sanitized evidence; the source auth file remained unchanged.

**Claude authenticated provider testing is blocked**: no Anthropic API key or
Claude login is available in the Linux or inspected Windows configuration.
[The blocker](multi-harness/provider-live/claude.json) is explicit. The verifier
is available for a subsequent run with local authentication; scripted-model
results do not close this gap.

## Interactive re-audit (2026-10-07)

The findings and coverage gaps in this historical section are superseded where
the 2026-10-08 continuation above supplies fresh results.

The earlier work below established adapter contracts and fixture-driven room
rendering. It did **not** establish keyboard usability inside all four actual
interactive CLIs. This audit adds real xterm/Xvfb keyboard and pointer events
through the production launcher, generated integrations, PTYs, and backend.
The model server is scripted and local; the CLIs themselves are genuine.

Two concrete findings prevent an unqualified "all four work" conclusion:

- The `codegotchi` on PATH was an August 27 executable, different from the
  checkout's build. It rejected `integration hermes`. It has now been replaced
  with this checkout's release build using
  `cargo install --path crates/codegotchi-cli --force --locked`.
- The virtual terminal answers cursor/device queries but leaves OSC background
  queries unanswered. OhMyPi selected a dark palette in the white wrapped xterm,
  making composer text faint; the direct launch selected a light palette in the
  same kind of white xterm. Compare [wrapped composer](multi-harness/interactive/omp-full-composer.png)
  with [direct startup/composer](multi-harness/direct-baseline/omp-startup.png).
  This defect was unfixed at the re-audit; the release preparation above fixes it.
  Hermes also has poor white-terminal
  contrast, but [its direct launch](multi-harness/direct-baseline/hermes-composer.png)
  already has yellow-on-white elements; that observation alone does not isolate
  a CodeGotchi regression.

Only Pi is currently on the normal PATH. Claude, OhMyPi, and Hermes were tested
using the actual binaries installed by the earlier agent under its temporary
research directory, supplied through the documented executable overrides.
Reinstalling CodeGotchi does not install those CLIs. Hermes's generated hooks
were approved only in the disposable test home; the user's Hermes configuration
was not changed.

### Reproducible checks

The retained [acceptance script](../../scripts/verify-harness-keyboard.py)
accepts `--binary`, repeated `--program HARNESS=PATH`, `--harness`,
`--ui terminal|both`, and `--background light|dark`. For installed CLIs:

```sh
cargo build -p codegotchi-cli
python3 scripts/verify-harness-keyboard.py --ui both
# Verify the executable actually selected by your shell:
python3 scripts/verify-harness-keyboard.py --binary "$(command -v codegotchi)" --ui both
```

It uses private homes, a local HTTP model endpoint, run-owned authenticated Xvfb,
and tracked cleanup targets. It checks exact received prompt text, marker-file
contents, denials **in the specific native tool result**, and authoritative state.
It does not infer correctness from a canned screenshot or a word anywhere in
conversation history. Its `passed` field concerns behavioral assertions;
manual inspection remains necessary to assess visual correctness.

All four actual CLIs passed the following in a dark xterm with `--ui both`,
first against the checkout build and then again against the installed optimized
executable. [Release results](multi-harness/installed-keyboard/results.json) and
[the exact input actions](multi-harness/installed-keyboard/actions.json) are
retained. The installed executable and `target/release/codegotchi` have the same
SHA-256: `106e6074df217715aec8ab7cf823a848dad7da5515e4c1242d2dac4275e45aa0`.

- Type a deliberate typo, move Left, insert the correction, move End, Backspace,
  and submit; the model receives the expected text and a real shell writes the
  expected marker file.
- Recall input with Up/Down; paste two lines with real xterm PRIMARY/Shift+Insert;
  verify paste does not submit early and both lines arrive in one request.
- Resize through 100×30 Compact, 80×21 Minimal, 64×45 narrow Full, and 120×45
  Full while the composer is populated; edit it with keys at **each** size.
- Move focus to a separate run-owned window and back, then submit successfully.
- Switch to Strict, neglect the pet, submit a new request, receive a native
  tool denial, and verify that request's marker file does not exist.
- Drag the visible food onto the pet in Minimal; verify backend hunger falls
  from 100 to 75 and a care ID is recorded; submit another request afterward.
  Other neglected needs still deny work, confirming input survived the gesture.
- Exit through each harness's slash command, propagate exit code 0, and remove
  generated integration directories and runtime metadata.

Claude onboarding was traversed with actual keys in a fresh test home, including
its theme, test-key, notes, and workspace-trust prompts. OhMyPi's test home uses
`OMP_SKIP_SETUP=1`; its first-use setup remains a gap. Hermes uses its default
classic interface, generated/approved test hooks, and keyboard rejection of
first-use telemetry. The newer optional Hermes native TUI is not covered.

A first timed Hermes probe typed before its REPL was ready and produced a
malformed composer. The rerun waited for initialization and passed. That startup
race has not been isolated against an equally timed direct launch, so it is not
counted as a resolved product bug. Its [failed frame](multi-harness/keyboard-other/hermes-failed.png)
and failed result remain retained. Pi likewise rejected an early Enter while
its first-use `fd` download was in progress; the script now waits for its real
session registration. Earlier probe failures caused by a missing Bun PATH and
screenshot timeout are retained as harness-development failures, not passes.

### Verification and visual ledger

`TMPDIR=/dev/shm cargo test --workspace`: **455 passed, 10 ignored, 0 failed**.
The three previously failing vertical/Strict checks were repaired by enabling
debugging when their test servers start, preserving the guarded production
behavior. Clippy (`--workspace --all-targets -- -D warnings`), Rust formatting,
Python compilation, and `git diff --check` pass. This audit did not rerun the web
suite or paid-provider conversations.

Manually inspected every cell of these installed-release contact sheets, plus the earlier white
terminal layout sheets and native edited/response/denied frames:

| Actual CLI | Dark layouts with populated/edited composer | Dark interactive states |
| --- | --- | --- |
| Pi 0.85.1 | [Layouts](multi-harness/installed-keyboard/pi-layouts-sheet.png) | [States](multi-harness/installed-keyboard/pi-states-sheet.png) |
| OhMyPi 18.8.0 / Bun 1.4.2 | [Layouts](multi-harness/installed-keyboard/omp-layouts-sheet.png) | [States](multi-harness/installed-keyboard/omp-states-sheet.png) |
| Claude 2.1.292 | [Layouts](multi-harness/installed-keyboard/claude-layouts-sheet.png) | [States](multi-harness/installed-keyboard/claude-states-sheet.png) |
| Hermes 0e219331 | [Layouts](multi-harness/installed-keyboard/hermes-layouts-sheet.png) | [States](multi-harness/installed-keyboard/hermes-states-sheet.png) |

Layout sheets show all four sizes. State sheets show editing, tool response,
history recall, Strict denial, care, and keyboard submission after care. Native
screens retain their CLI-specific presentation; Hermes and Claude can collapse
native tool details, so denial assertions also require the actual tool result.
The familiar pet/furniture overlap is still visible within the room; the
composer remains in the upper pane.

Protocol receipts show the PTY dimensions and negotiated bracketed-paste delivery.
The white-terminal files in `interactive/` are genuine terminal-only CLI tests;
all four also reached a submitted prompt, multiline paste, and Strict denial
there. Their low-contrast frames are recorded findings, not visual passes.

Remaining gaps: every explicit room preset with the actual interactive CLIs,
every native CLI theme and shortcut, provider-backed conversations, first-use
OhMyPi setup, unconfigured Hermes hooks and Hermes's optional TUI, native macOS
and Windows terminals, and, at that time, the light-terminal palette defect (now fixed above).
The earlier fixture-driven theme/browser matrix below does not close these gaps.
No terminal renderer or browser visual implementation was changed in this audit.

## Integration contract

| CLI | Launch adapter | Setup |
| --- | --- | --- |
| Pi | Session-local Node extension, `--extension` | Automatic |
| OhMyPi (`omp`) | Same common extension API | Automatic |
| Claude (`claude`) | Temporary additive JSON `--settings` hooks | Automatic; existing CLI settings are consolidated and their hooks retained |
| Hermes | Four config-file shell hooks | Run `codegotchi integration hermes`, append the entries to the active profile's config, and approve Hermes's first-use review |

All use the real launcher, private runtime metadata, authoritative server,
native pre-tool denial, and normal child exit propagation. Non-Codex launches
do not require or mutate Codex profiles. Hermes cannot report work or enforce
Strict mode until its hooks are installed and approved; the launcher prints
that requirement. Disabled extensions/hooks and policies that prohibit hooks
can prevent integration. Hook transport failures remain fail-open.

Contracts checked against the upstream [Pi extension documentation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md),
[OhMyPi extension documentation](https://github.com/can1357/oh-my-pi/blob/main/docs/extensions.md),
[Claude hook documentation](https://code.claude.com/docs/en/hooks), and
[Hermes hook documentation](https://hermes-agent.nousresearch.com/docs/user-guide/features/hooks/).

## Automated checks

Passed:

- `cargo check --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all -- --check` and `git diff --check`
- `TMPDIR=/dev/shm cargo test -p codegotchi-cli --test harness_adapters --test harness_launch`:
  eight contract tests plus eight production launch scenarios (four harnesses,
  browser and non-TTY auto fallback). Includes shell/file-edit denials, care
  commands allowed during neglect, sanitized events, failure outcomes, private
  integration files, existing Claude hooks, argument forwarding, exit status,
  and cleanup.
- Domain tests: 62; CLI library tests: 107.
- Existing Codex profile, hook fixture/runtime, and process-wrapper checks.
- Existing terminal behavior, input, layout, render, room, screen, PTY, and
  session suites; static asset, permission catch-up, and websocket checks.
  Optional installed-Codex interactive tests retain their existing ignored status.

Historical result before the interactive re-audit: the workspace was **not green**: two `full_vertical_flow` tests and one
`strict_flow` test fail at debug commands. The server changes already present
when this task began require debugging to be enabled at server startup; those
older test helpers enable it only on the later CLI command. Those server
changes were preserved. The new launch tests enable debugging at startup.

The final launch checks used tmpfs to avoid the existing 250 ms HTTP I/O limit
being exceeded by a busy filesystem. This does not establish reliability under
heavy disk contention; production timeout/fail-open behavior was unchanged.

## Real CLI tool execution

Disposable configurations and a local scripted model server exercised actual
CLIs, generated adapters, the CodeGotchi hook executable, and real backend.
The scripted model requested a shell action that creates a sentinel file.
Each CLI created that file in Decorative mode and prevented its creation in
Strict mode after neglect. The model received the tool result and completed.
No real provider credentials or paid model requests were used.

| CLI | Version tested | Decorative / Strict |
| --- | --- | --- |
| Pi | 0.85.1 | Pass / Pass |
| Claude | 2.1.292 | Pass / Pass |
| OhMyPi | 18.8.0 (Bun 1.4.2) | Pass / Pass |
| Hermes | upstream `0e219331`, reported `vgit.0e21933 (2026.9.24)` | Pass / Pass |

Sanitized results: [eight real CLI scenarios](multi-harness/actual-local-model.json).
Hermes's disposable test home approved only the four reviewed generated hooks;
the user's configuration and trust approvals were not changed.

## Production visual inspection

The executable ran in a real xterm PTY on a private Xvfb display. The checked-in
`tests/fixtures/fake-harness.mjs` drove **generated adapters and the actual hook
binary/server**, rather than injecting renderer state. Snapshots were checked
before capture. All 560 room captures were visually inspected through the
following 20 contact sheets, including every cell:

| Harness | Auto | Mono | Soft green | Amber | Night |
| --- | --- | --- | --- | --- | --- |
| Pi | [View](multi-harness/inspection-pi-auto.png) | [View](multi-harness/inspection-pi-mono.png) | [View](multi-harness/inspection-pi-soft-green.png) | [View](multi-harness/inspection-pi-amber.png) | [View](multi-harness/inspection-pi-night.png) |
| Claude | [View](multi-harness/inspection-claude-auto.png) | [View](multi-harness/inspection-claude-mono.png) | [View](multi-harness/inspection-claude-soft-green.png) | [View](multi-harness/inspection-claude-amber.png) | [View](multi-harness/inspection-claude-night.png) |
| OhMyPi | [View](multi-harness/inspection-omp-auto.png) | [View](multi-harness/inspection-omp-mono.png) | [View](multi-harness/inspection-omp-soft-green.png) | [View](multi-harness/inspection-omp-amber.png) | [View](multi-harness/inspection-omp-night.png) |
| Hermes | [View](multi-harness/inspection-hermes-auto.png) | [View](multi-harness/inspection-hermes-mono.png) | [View](multi-harness/inspection-hermes-soft-green.png) | [View](multi-harness/inspection-hermes-amber.png) | [View](multi-harness/inspection-hermes-night.png) |

Each sheet has four layout columns: full 120×45, compact 100×30, minimal 80×21,
and narrow full 64×45; seven state rows: idle, thinking, shell work, success,
failure, waiting, and Strict block. All source images are retained and indexed
by [matrix.json](multi-harness/matrix.json). Auto resolved to the light palette
in this terminal; the three explicit color themes and mono were inspected.

Actual installed CLI startup screens were also inspected at all four sizes:
[Pi](multi-harness/inspection-actual-pi.png),
[Claude](multi-harness/inspection-actual-claude.png),
[OhMyPi](multi-harness/inspection-actual-omp.png), and
[Hermes](multi-harness/inspection-actual-hermes.png). Pi and OhMyPi loaded the
extension and registered a session. Claude displayed its first-run theme chooser;
Hermes displayed its unapproved shell-hook review. OhMyPi's normal main screen
used its test-only `OMP_SKIP_SETUP=1` setting in a disposable home; the production
adapter does not set it. A complete fresh-install onboarding flow is not covered.

The production embedded browser bundle was served by `codegotchi run --ui browser`
and opened in Chromium with its private launch token. The same adapter fixture
exercised seven states for all four harnesses at 1280×900 and 390×844. All 56
captures were visually inspected via these room sheets; full-page images and
[browser-matrix.json](multi-harness/browser-matrix.json) retain the controls:

| Harness | Idle / thinking / work / success | Failure / waiting / block |
| --- | --- | --- |
| Pi | [View](multi-harness/inspection-browser-pi-a.png) | [View](multi-harness/inspection-browser-pi-b.png) |
| Claude | [View](multi-harness/inspection-browser-claude-a.png) | [View](multi-harness/inspection-browser-claude-b.png) |
| OhMyPi | [View](multi-harness/inspection-browser-omp-a.png) | [View](multi-harness/inspection-browser-omp-b.png) |
| Hermes | [View](multi-harness/inspection-browser-hermes-a.png) | [View](multi-harness/inspection-browser-hermes-b.png) |

Observed existing presentation behavior: terminal waiting and blocked share
`WaitingOrBlocked`; browser waiting immediately after failure retains its recent
upset presentation. Pet animation can overlap terminal furniture/text, and the
mobile browser can clip the moving pet at the room edge. These renderer behaviors
were not changed by the harness adapters.

Remaining coverage gaps: macOS/native Windows terminal hosts, other browser
engines, other CLI versions, paid/provider-backed conversations, every external
CLI theme, first-run onboarding beyond the captured prompts, remote/subagent
processes that do not inherit the session environment, and, at that time, combined `--ui both` with a real external CLI. The interactive
re-audit above now covers real input/resize/food-care gestures for each CLI.
