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

These images are references, not the final implementation. You should translate the visual language, not attempt a naïve one-image-pixel-per-terminal-cell reproduction. The terminal version will need aggressive simplification while preserving silhouette and proportions. For example, the tiny highlights and texture noise on the wardrobe are expendable; the wardrobe's outline, doors, handles, and top/bottom structure are not. The same is true for the cat: eyes, ears, round silhouette, paws, tail and mouth are load-bearing; tiny shading details aren't.

I also really like that the asset set contains component states, not merely finished screenshots. There are open and closed wardrobes, several beds including the cat sleeping in one, lamp on/off states, window variations, empty/fill food bowls, multiple poop states, furniture variants, UI fill levels, particles, etc. That's unusually useful for an implementation agent because it communicates intended behavior rather than forcing it to reverse-engineer states from a single screenshot.

One visual choice I'd preserve strongly is the large, almost absurdly round cat. The last mockup gets this right. Making the mascot substantially smaller or more anatomically cat-like would lose a lot of the charm. The furniture should feel like scenery around CodeGotchi, not like a detailed pixel-art room that happens to contain a cat.