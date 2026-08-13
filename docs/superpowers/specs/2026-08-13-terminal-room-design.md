# CodeGotchi Terminal Room Design

**Date:** 2026-08-13  
**Status:** Approved design  
**Feature:** Interactive CodeGotchi room embedded alongside the official Codex CLI

## Summary

CodeGotchi should feel like a virtual pet that literally lives beside the coding session, without replacing, forking, or visually reimplementing Codex. The terminal becomes a two-pane host: the upper pane is the real official Codex TUI running in a PTY, while the lower pane is a persistent interactive CodeGotchi bedroom rendered as adaptive monochrome pixel art.

The central product principle is:

> **Codex remains Codex; CodeGotchi owns only the room around it.**

This preserves Codex functionality and future update compatibility while giving CodeGotchi a much stronger sense of place and direct interaction than the browser-only projection.

The room is not a small status widget. It is the pet's home: a cozy side-view child's bedroom with a window, laptop desk, shelf, bed, wardrobe, plants/flowers, food area, floor space, and room objects. The pet wanders and interacts with decorative furniture autonomously, while authoritative care actions such as feeding, petting, cleaning, and sleeping remain explicit user interactions.

## Goals

1. Make CodeGotchi feel physically present in the same terminal session as Codex.
2. Reuse the official `codex` executable and its complete TUI instead of rebuilding a Codex frontend.
3. Keep compatibility with future Codex releases by treating Codex as a PTY-hosted black-box terminal application.
4. Give CodeGotchi a full interactive room with enough space for recognizable furniture, autonomous movement, idle animation, and care interactions.
5. Preserve the existing Rust simulation as the single authoritative source for needs, persistence, incidents, inventory, strict-mode decisions, and care validation.
6. Preserve the browser room as an optional second projection rather than deleting it.
7. Keep Codex usable when terminal space is constrained by automatically shrinking CodeGotchi through Full, Compact, and Minimal layouts.
8. Use a terminal-theme-adaptive monochrome pixel-art system with optional style presets.
9. Keep keyboard ownership with Codex and make CodeGotchi interactions mouse-first.

## Non-goals

- Do not recreate the Codex conversation UI, diff UI, approval UI, input editor, slash-command UI, model picker, or other Codex-owned controls.
- Do not parse Codex's visible terminal text to infer semantic activity.
- Do not fork or patch the Codex CLI.
- Do not move pet simulation authority into the terminal renderer.
- Do not let autonomous decorative behavior repair authoritative needs.
- Do not add fetch/toy minigames in this feature.
- Do not make the bed an autonomous animation: sleeping changes energy and therefore remains a user care action.
- Native Windows is not part of the first implementation. Initial support follows the current effective Unix launcher envelope: Linux, WSL, and macOS.

---

## Product Architecture

```text
physical terminal
        |
        v
+---------------- CodeGotchi terminal host ----------------+
|                                                           |
|  Codex pane                                               |
|   +-- PTY --> official `codex` executable                 |
|   |          + existing CodeGotchi additive profile       |
|   |          + existing hooks                             |
|   |          + ordinary Codex arguments                   |
|                                                           |
|-----------------------------------------------------------|
|  CodeGotchi room                                          |
|   +-- pixel room renderer                                 |
|   +-- autonomous presentation behavior                    |
|   +-- mouse interaction / hit testing                     |
|   +-- responsive layout                                   |
|                                                           |
+--------------------------+--------------------------------+
                           |
                           v
                 AuthoritativeRuntime
                   + PetSimulation
                   + SQLite persistence
                   + care validation
                   + strict-mode policy
                   + snapshot broadcast
                           |
                           +--> existing Axum/browser view
```

### Codex boundary

The Codex pane is a terminal surface, not a CodeGotchi-owned UI model.

CodeGotchi will:

- spawn the official Codex executable inside a PTY;
- pass the existing generated CodeGotchi profile, `CODEX_HOME`, `CODEGOTCHI_SESSION_FILE`, and ordinary trailing Codex arguments;
- read Codex ANSI/VT output into a virtual terminal screen;
- draw that screen into the upper rectangle;
- forward keyboard, paste, focus, and supported mouse input to the Codex PTY;
- propagate terminal resize events to the PTY;
- preserve Codex exit status and existing signal/cleanup semantics.

CodeGotchi must **not** inspect rendered text for strings such as `Thinking`, `Working`, tool names, command output, or labels. Activity comes from the existing hook/event pipeline and authoritative state.

### Authoritative pet boundary

The existing Rust runtime remains authoritative. The terminal room reuses the same `Arc<AuthoritativeRuntime>` already used by the server and relies on:

- `AuthoritativeRuntime::subscribe()` for initial and future snapshots;
- `feed(...)` for food drops;
- `clean(...)` for poop removal;
- `nap(...)` for the bed;
- `pet(...)` for validated petting gestures.

The terminal UI does not optimistically alter needs, inventory, demand queues, poop state, or enforcement state. It may animate immediate feedback, but authoritative visuals settle from the returned/broadcast snapshot.

The browser remains a second projection of the same pet.

---

## Visual Direction

### Art style

The approved direction is **mid-detail monochrome pixel art**:

- nostalgic virtual-pet/Tamagotchi influence;
- original fantasy creature rather than a literal cat or dog;
- rounded mascot-like silhouette;
- tiny ears/nubs, expressive eyes, stubby feet;
- more detail and cuteness than a classic one-bit handheld sprite;
- simple enough to remain legible in a terminal.

The room is a **side-view dollhouse bedroom**, not isometric.

### Bedroom vocabulary

Full mode should include:

- a window with simple day/night ambience;
- laptop desk;
- wall shelf with books/decorations;
- bed;
- wardrobe;
- flowers/plants;
- food/pantry area;
- open floor for roaming;
- authoritative poop objects;
- small decorative objects that support idle animation.

The vibe should be cozy, modern, childlike, and lived-in without becoming visually noisy.

### Pixel density

Use a logical pixel canvas and half-block characters such as `▀`, `▄`, and `█` so one terminal cell can represent two vertical logical pixels. This gives enough detail without consuming excessive terminal height.

The Full pet target is roughly 10-14 logical pixels high, about 5-7 terminal rows when half-block packed.

Required sprite/pose coverage should include at least:

- idle;
- blink;
- walk A/B;
- sit;
- curious/look;
- celebrate/happy;
- worried/upset;
- eating;
- petted reaction;
- sleeping in bed.

Compact and Minimal use purpose-built alternate compositions instead of shrinking the Full canvas.

---

## Theme System

Art is authored using four **semantic tones**, not fixed colors:

- `Tone0`: background;
- `Tone1`: dark/mid-low;
- `Tone2`: mid-high;
- `Tone3`: foreground/highlight.

### Auto theme

`Auto` is the default and must work with arbitrary terminal themes.

The robust baseline is:

- `Tone0` = terminal default background;
- `Tone3` = terminal default foreground;
- intermediate tones use ordered foreground/background dithering when exact colors cannot be inferred reliably.

This avoids assuming a dark terminal and should remain readable on light, dark, Solarized-like, custom, and accessibility themes.

### Optional presets

Initial rendering-only presets:

- `auto` — terminal default foreground/background;
- `mono` — neutral grayscale when truecolor is available;
- `soft-green` — retro handheld green;
- `amber` — warm CRT/terminal monochrome;
- `night` — subdued dark cozy palette.

Preset selection never changes pet state.

---

## Responsive Layout

Codex wins whenever vertical space becomes constrained.

The room has three layouts selected automatically with hysteresis.

### Full

Target approximately 12-16 terminal rows.

Contains:

- complete bedroom;
- open roaming area;
- full pet sprite;
- food/pantry drag sources with counts;
- physical poop objects;
- bed;
- Hunger, Energy, Happy, Clean status bars;
- affection/snack affordances integrated into the room.

### Compact

Target approximately 6-8 rows.

Contains:

- compact pet sprite;
- simplified furniture silhouettes;
- condensed bed, food, and poop affordances;
- condensed status bars;
- enough horizontal room for short autonomous movement.

Decorative furniture disappears before care functionality does.

### Minimal

Target approximately 2-3 rows.

Contains:

- pet;
- condensed need indicators;
- care affordances for food, bed, poop, and petting;
- no decorative bedroom background.

Minimal mode must not make strict-mode recovery impossible. Food can use a temporary one-line mouse-draggable tray when needed.

### Layout priority

1. Preserve a usable Codex pane.
2. Preserve core CodeGotchi care interactions.
3. Preserve the pet itself.
4. Preserve status information.
5. Preserve decorative room ambience last.

---

## Input Model

### Keyboard

Codex owns the keyboard. CodeGotchi does not introduce normal pet-care keyboard shortcuts in this feature.

Printable input, control chords, arrows, function keys, paste, focus events, and terminal editing sequences should be encoded back to the Codex PTY as faithfully as possible.

### Mouse routing

```text
mouse event
   |
   +-- inside Codex pane ------> encode / forward to Codex PTY
   |
   +-- inside room ------------> CodeGotchi hit testing
```

### Petting

- Pointer down over the pet starts a gesture.
- Movement accumulates path distance.
- Pointer up submits duration and accumulated distance through `AuthoritativeRuntime::pet`.
- Preserve existing backend minimums of `1_500 ms` and `120 px`-equivalent travel.
- Do not optimistically remove affection requests.

### Feeding

- Full/Compact expose draggable kibble/treat/fruit items with inventory counts.
- Mouse down begins a drag ghost.
- Dropping on the pet submits `feed` with a new action ID.
- Minimal exposes a temporary one-line food tray and uses the same drag-to-pet interaction.
- Energy drink semantics remain exactly whatever the authoritative domain defines.

### Cleaning poop

- Physical poop is rendered from authoritative poop IDs.
- Clicking a poop, or shovel-then-poop if that interaction is retained, submits `clean`.
- Poop remains visible until the authoritative snapshot removes it.

### Bed

- Clicking the bed submits `nap`.
- Sleeping presentation begins only after authoritative success.
- The pet never uses the energy-recovery bed autonomously.

---

## Living Behavior

The pet should feel like a creature sharing the workspace, not an animated Codex status indicator.

> **Autonomous behavior may express state, but never repair state.**

The presentation layer may autonomously:

- wander to reachable points;
- pause/look around;
- blink;
- sit;
- inspect plants/flowers;
- look out the window;
- sit at or inspect the laptop;
- inspect shelves;
- stretch/yawn;
- react to room objects;
- avoid/react to poop;
- linger near food when hungry;
- linger near the bed and yawn when tired;
- seek visual proximity to the room front when lonely.

It may **not** autonomously:

- eat inventory;
- resolve snack or affection requests;
- clean poop;
- sleep in the recovery bed;
- alter needs or enforcement mode.

### Codex activity influence

Use broad durable activity states only:

- `Calm` — normal room life;
- `Thinking` — occasional subtle look toward Codex, pause, or `...` bubble;
- `Working` — occasional laptop/watch/work reaction, then resume normal life;
- `Success` — short celebration;
- `Failure` — short worried/upset reaction;
- `WaitingOrBlocked` — occasional contextual waiting/attention reaction.

Thinking and Working are modifiers, not exclusive animation loops. During a long session the pet should still wander, sit, inspect furniture, look out the window, and otherwise have calm life moments.

Presentation movement does not need persistence and may use a session-local seeded PRNG so tests can be deterministic.

---

## Codex Update Compatibility

Supporting Codex updates is a primary constraint.

The compatibility surface is deliberately narrow:

1. Launch the official Codex binary through the existing resolver.
2. Preserve trailing Codex arguments in order.
3. Preserve the generated additive CodeGotchi profile and hook contract.
4. Treat Codex output only as VT/ANSI display operations.
5. Make no semantic decisions from Codex text layout, wording, labels, or colors.
6. New Codex TUI features should appear automatically if they use supported terminal semantics.
7. Unknown/unsupported escape sequences must fail visually benignly rather than crashing the host.
8. Browser mode remains a fallback if a future Codex release uses terminal behavior not yet supported by the compositor.

### UI selection

Add a CodeGotchi-owned option before the existing separator:

```text
codegotchi run [--ui auto|terminal|browser|both] -- codex [ordinary Codex arguments...]
```

Semantics:

- `auto` (default): use terminal room when stdin/stdout are interactive and terminal initialization succeeds; otherwise use the existing browser/inherited-terminal path.
- `terminal`: require the terminal host; return a clear error if initialization is impossible.
- `browser`: preserve today's inherited Codex stdio + browser behavior.
- `both`: run the terminal room and also launch the browser projection.

`CODEGOTCHI_BROWSER` remains the browser-helper control when browser launch is selected.

---

## Terminal Lifecycle and Safety

The compositor must:

- enter raw mode only after setup that can fail cleanly;
- enter the alternate screen;
- enable mouse capture while active;
- restore cursor, mouse mode, raw mode, and alternate screen on every exit/error path;
- forward `SIGINT`, `SIGTERM`, and resize behavior without leaving the terminal corrupted;
- resize the child PTY whenever the Codex rectangle changes;
- preserve Codex numeric exit status;
- never expose the bearer token on the composed terminal surface;
- preserve current runtime metadata/profile cleanup guarantees;
- fall back before child spawn in `auto` if terminal setup fails.

A small RAII `TerminalGuard` should own terminal restoration.

---

## Mock Renders

These are approved visual references. The Codex pane in the images is illustrative; the implementation must display the **actual official Codex TUI** from the PTY.

### Full room — dark adaptive theme

![Full dark terminal room mockup](../../mockups/terminal-room/full-dark.webp)

### Full room — light adaptive theme

![Full light terminal room mockup](../../mockups/terminal-room/full-light.webp)

### Compact room — Codex-priority layout

![Compact dark terminal room mockup](../../mockups/terminal-room/compact-dark.webp)

---

## Acceptance Criteria

### Codex fidelity

- `codegotchi run -- codex` in an interactive supported terminal displays the real Codex TUI in the upper pane.
- Codex prompt input, editing keys, paste, approvals, slash commands, scrolling, and tool output remain usable.
- Codex visual text/layout updates do not require CodeGotchi changes unless terminal-control behavior changes.
- Trailing Codex args and profile semantics are preserved.

### Room

- Full mode visually follows the approved side-view bedroom mockups.
- The fantasy pet has recognizable idle/walk/sit/reaction/sleep frames.
- Status bars show Hunger, Energy, Happy, Clean from authoritative snapshots.
- Poop and pending care state are authoritative.
- Light and dark terminal-default themes remain legible.

### Responsive behavior

- Full, Compact, and Minimal states are selected automatically.
- Resize hysteresis prevents rapid layout flicker.
- Codex receives a matching PTY resize whenever its rectangle changes.
- Core care remains possible in Minimal mode.

### Interaction

- Petting uses duration + path distance and backend validation.
- Food is dragged from inventory to the pet.
- Poop cleaning remains authoritative.
- Bed use is explicit and authoritative.
- Keyboard input is not stolen by the room.

### Living behavior

- During long Thinking/Working periods the pet still returns to calm autonomous behavior.
- Needs influence presentation but are never autonomously repaired.
- Bed, feeding, cleaning, and affection resolution never occur without user care actions.

### Regression safety

- Existing browser UI remains functional.
- Existing hook trust/profile behavior remains intact.
- Persistence, replay safety, strict mode, care pressure, motion/blinking, food, poop, and nap behavior remain green.

---

## Definition of Done

This feature is done when:

1. The official Codex TUI runs inside CodeGotchi's PTY-backed upper pane with no semantic UI recreation.
2. The lower room has Full, Compact, and Minimal responsive layouts matching the approved direction.
3. Auto theme and initial presets render readable semantic four-tone art.
4. The pet has autonomous non-authoritative room behavior and broad Calm/Thinking/Working/Success/Failure/WaitingOrBlocked reactions.
5. Mouse petting, food drag/drop, poop cleaning, and bed interaction call the existing authoritative runtime care methods.
6. Core care remains available in Minimal mode.
7. Browser mode remains functional and selectable.
8. Terminal state is restored after normal exit, errors, Ctrl-C/termination, and child spawn/wait failures.
9. Rust formatting, Clippy, workspace tests, web tests/lint/format/build, and production Playwright all pass.
10. A real Codex acceptance run on WSL/Linux demonstrates prompt entry, tool activity, approval/review interaction where available, resize Full -> Compact -> Minimal -> Full, petting, feeding, cleaning, nap, and clean exit without terminal corruption.
