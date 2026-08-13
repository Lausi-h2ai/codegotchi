# Terminal Room Visual Reference Manifest

**Status as of 2026-08-13 hardening pass:** the `docs/terminal-room-design` branch does not currently contain the binary room mockups or sprite-reference images. PR #2 explicitly notes that the binary mock renders were left out of the documentation PR.

This directory is the canonical repository location for visual references used by terminal-room implementation agents.

## Before Milestone B visual work

Add the supplied planning references to this directory, then replace this status note with an inventory table containing:

| File | Role | Normative details |
|---|---|---|
| `<full-room reference>` | Overall Full-room composition | Two-pane hierarchy, bedroom composition, pet scale, furniture balance, status-bar placement |
| `<sprite reference(s)>` | Pet visual language | Silhouette, face, proportions, pose/animation direction |
| `<compact/minimal reference(s)>` | Responsive direction, if supplied | What survives when vertical space shrinks |

Agents must inspect every listed reference before authoring room/sprite visuals. They must not invent replacement visual direction because an expected binary file is missing.

See:

- `docs/superpowers/specs/2026-08-13-terminal-room-design.md`
- `docs/superpowers/specs/2026-08-13-terminal-room-agent-hardening.md`
- `docs/superpowers/plans/2026-08-13-terminal-room-agent-hardening.md`

The visual hardening gate requires screenshot -> image inspection -> reference comparison -> correction -> recapture for every material visual change, with selected final evidence stored under `docs/verification/terminal-room/`.
