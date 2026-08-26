# Terminal Room Final Release Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the Full-room absolute/relative coordinate mismatch, make the hosted PTY metadata test correctly frame its stream, clean statically provable macOS test cfgs, and produce exact final release evidence without changing terminal-room authority semantics.

**Architecture:** `RoomGeometry` remains an absolute physical-terminal coordinate model. Presentation offsets and homes remain room-relative internally, with one explicit conversion at every comparison to geometry. PTY behavior remains unchanged; only the test consumes complete newline-delimited records. Release evidence is captured only after source and production bundles stop changing.

**Tech Stack:** Rust workspace with ratatui/crossterm/portable-pty, embedded React/Playwright production bundle, Bash/xterm/Xvfb live acceptance harness, GitHub Actions for hosted macOS.

**Spec:** `docs/superpowers/specs/2026-08-13-terminal-room-design.md`, `docs/superpowers/specs/2026-08-13-terminal-room-agent-hardening.md`, and `docs/superpowers/plans/2026-08-20-terminal-room-release-hardening.md`.

## Global Constraints

- All `RoomGeometry` rectangles are absolute terminal coordinates.
- Presentation home/offset values may remain room-relative, but comparisons with geometry must explicitly convert to absolute coordinates.
- Do not redesign Full/Compact/Minimal visuals or weaken care authority boundaries.
- Do not fix test-harness races with arbitrary sleeps/retries or change production PTY signal behavior without a reproducible product failure.
- Linux/WSL is the local source of truth; macOS runtime claims require hosted GitHub Actions.
- Final visual claims require real production-path captures and direct image inspection.
- Final evidence must record exact source and binary provenance without credentials, private arguments, or auth contents.

---

### Task 1: Lock absolute Full-room geometry with red regressions

**Files:**
- Modify: `crates/codegotchi-cli/tests/terminal_room.rs`
- Modify: `crates/codegotchi-cli/tests/terminal_input.rs`
- Modify: `crates/codegotchi-cli/tests/terminal_session.rs`
- Modify: `crates/codegotchi-cli/src/terminal/behavior.rs` only after red tests exist
- Modify: `crates/codegotchi-cli/src/terminal/room.rs` only after red tests exist

**Interfaces:**
- Consumes `room_geometry_with_frame`, `wide_full_care_zone`, `RoomInputSession`, `PresentationState`, `TerminalSessionCore`, and `choose_layout`.
- Produces tests for 80/81-column Full geometry at nonzero origins, render/hitbox alignment, one clean lifecycle, wander exclusion without freezing, and actual 80x45 pane placement.

- [ ] Add a table-driven geometry test for `Rect::new(0,31,80,14)`, `Rect::new(0,31,81,14)`, and `Rect::new(7,31,80,14)` with an authoritative poop. Assert all care rectangles are contained by the area, fallback `y == area.y + 8`, and fallback poop equals the reserved care zone when their dimensions match.
- [ ] Add a render test that inspects cells at the geometry poop rectangle and proves the visible fallback object is at the same physical coordinates used by `poop_hit`.
- [ ] Add a Down→Up lifecycle test for 80/81/nonzero-origin fallback poop targets and assert exactly one Clean for the authoritative UUID, with no pet capture.
- [ ] Add deterministic PresentationState coverage over enough ticks/seeds to inspect actual frames, assert the moving pet never intersects the actual fallback care zone, and assert at least two distinct reachable offsets.
- [ ] Add a TerminalSessionCore/`choose_layout` regression proving an 80x45 terminal selects `room.y == 31` and that the resulting geometry is usable.
- [ ] Run the new tests and verify they fail against HEAD for the coordinate bug.
- [ ] Implement one small absolute-coordinate conversion helper shared by fallback geometry and behavior collision checking; keep homes/offsets relative internally.
- [ ] Run focused terminal-room/input/session tests and verify green.

### Task 2: Frame PTY fixture metadata and portable test cfgs

**Files:**
- Modify: `crates/codegotchi-cli/tests/terminal_pty.rs`
- Modify: `crates/codegotchi-cli/tests/process_wrapper.rs`

**Interfaces:**
- Consumes the existing READY/PID fixture records and Linux-only helper call sites.
- Produces complete newline framing after READY, canonical CWD comparison, and macOS-clean helper scopes without dead-code suppression.

- [ ] Add/adjust a framing helper test or fixture-driven assertion that tolerates CRLF/LF and partial `Read` chunks.
- [ ] Replace the one-shot metadata read with consumption through READY’s newline and a loop until a complete `FAKE_SIGNAL_PID` line is available.
- [ ] Canonicalize observed and expected CWD paths before comparison and remove the requirement that both textual aliases appear.
- [ ] Gate `process_group_id`, `process_group_info`, `parent_process_id`, and `signal_seen_before_deadline` with `target_os = "linux"` when their callers are Linux-only; restore the recursive symlink test to `cfg(unix)` and keep `symlink` imports portable.
- [ ] Run focused PTY/process-wrapper tests and the required 50 consecutive terminal_pty invocations.

### Task 3: Verify production browser stress without speculative changes

**Files:**
- Modify production web code only if fresh diagnostics identify a production defect.
- Otherwise retain current E2E diagnostics and use the existing web scripts.

- [ ] Rebuild and embed the production bundle.
- [ ] Run the complete production Playwright suite 10 times in fresh invocations; optionally run the energy-drink test 20 times.
- [ ] If a failure occurs, classify it as drop, POST, backend response, authoritative inventory, rendered inventory, or transient feedback before considering a fix.

### Task 4: Harden live acceptance provenance and final evidence

**Files:**
- Modify: `scripts/verify-terminal-room-live.sh`
- Create/update only final evidence files under `docs/verification/terminal-room/` after production code is stable.
- Modify at end: `docs/verification/terminal-room.md`, `docs/verification/terminal-room/README.md`, and misleading historical headers.

- [ ] Add redacted report fields for exact HEAD, source-tree cleanliness, scoped product-source diff SHA when dirty, actual CodeGotchi binary SHA, harness SHA, workspace-helper SHA, and Codex CLI version.
- [ ] Add a bounded 80x45 Full acceptance phase that establishes, captures, cleans, and verifies an authoritative poop, then restores 120x45.
- [ ] Run the complete fresh local release matrix, 50x PTY stress, browser stress, and production-path visual capture matrix.
- [ ] Open and inspect every final relevant screenshot, including 80x45 care and real-Codex frames.
- [ ] Rerun official-Codex Linux acceptance from the final exact source and record hosted Ubuntu/Web/macOS reality without claiming local macOS execution.
- [ ] Perform fresh `git diff`, `git status`, and all final gates before reporting completion; do not push.
