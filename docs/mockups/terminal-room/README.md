# Terminal Room Visual Reference Manifest

**Status as of 2026-08-20:** the supplied raster references are present and
were inspected directly. This directory is the canonical repository location
for the visual references used by terminal-room implementation agents. The
terminal captures under `docs/verification/terminal-room/` are implementation
evidence, not canonical art references.

## Audited reference inventory

| File | Primary role | Visual guidance |
|---|---|---|
| `9904EC37-0833-4E23-86D1-BBA5634D8EE6.jpeg` | Full composition; palette/theme | Two-pane Codex + framed room, large round pet, window/desk left, shelf, wardrobe, bed right, floor care objects, bottom status strip; strongest room-level soft-green reference. |
| `a4835b9b-53d0-467f-8189-a708eea397eb.png` | Compact direction; dark host/theme | Explicit Compact hierarchy with pet, vignette, status/care panel, and dark terminal chrome; use structure, not its historical pet art. |
| `ChatGPT Image 13. Aug. 2026, 22_43_21 (1).png` | Pet silhouette/poses; sleep/doze | Round mascot language, face/ears/feet, walk/reaction poses, and curled floor doze poses. Floor doze is distinct from bed sleep. |
| `ChatGPT Image 13. Aug. 2026, 22_43_21 (2).png` | Room surfaces; palette/theme | Green four-tone walls, shelves, windows, floors, and separable room layers for Full composition. |
| `ChatGPT Image 13. Aug. 2026, 22_43_21 (3).png` | Furniture balance; component states | Desk/laptop/lamp, shelves/books, plants, storage and relative scale. |
| `ChatGPT Image 13. Aug. 2026, 22_43_21 (4).png` | Bed/wardrobe states | Closed/open wardrobes, made/rumpled beds, and the authoritative pet-in-bed sleep state. |
| `ChatGPT Image 13. Aug. 2026, 22_43_21 (5).png` | Care/decor states; palette/theme | Food bowls, poop states, plants, rugs, lamps, and lived-in room vocabulary. |
| `ChatGPT Image 13. Aug. 2026, 22_43_21 (6).png` | Status/care readability | Title/banner, HUNGER/ENERGY/HAPPY/CLEAN bars, compact controls, and segmented fill states. |
| `ChatGPT Image 13. Aug. 2026, 22_43_21 (7).png` | Effects/affordances | Hearts, alerts, droplets, steam, Z particles, pointers, moons, foliage, and feedback cues. |

The current acceptance mapping is: Full light uses `9904...jpeg` plus `(2)–(6)`;
Full dark also uses `a483...png`; Compact uses `a483...png` plus `(1)` and
`(6)`; Minimal uses `(1)`, `(5)`, `(6)`, and `(7)` because no dedicated
Minimal bitmap exists; bed sleep uses `(4)` and `(7)`; floor doze uses `(1)` and
`(7)`. The complete role notes and dimensions are recorded in the SDD visual
audit at `.superpowers/sdd/2026-08-16-terminal-room-completion/visual-reference-audit.md`.

Agents must inspect every listed reference before authoring room/sprite visuals. They must not invent replacement visual direction because an expected binary file is missing.

See:

- `docs/superpowers/specs/2026-08-13-terminal-room-design.md`
- `docs/superpowers/specs/2026-08-13-terminal-room-agent-hardening.md`
- `docs/superpowers/plans/2026-08-13-terminal-room-agent-hardening.md`

The visual hardening gate requires screenshot -> image inspection -> reference comparison -> correction -> recapture for every material visual change, with selected final evidence stored under `docs/verification/terminal-room/`.

These images are references, not the final implementation. You should translate the visual language, not attempt a naïve one-image-pixel-per-terminal-cell reproduction. The terminal version will need aggressive simplification while preserving silhouette and proportions. For example, the tiny highlights and texture noise on the wardrobe are expendable; the wardrobe's outline, doors, handles, and top/bottom structure are not. The same is true for the cat: eyes, ears, round silhouette, paws, tail and mouth are load-bearing; tiny shading details aren't.

I also really like that the asset set contains component states, not merely finished screenshots. There are open and closed wardrobes, several beds including the cat sleeping in one, lamp on/off states, window variations, empty/fill food bowls, multiple poop states, furniture variants, UI fill levels, particles, etc. That's unusually useful for an implementation agent because it communicates intended behavior rather than forcing it to reverse-engineer states from a single screenshot.

One visual choice I'd preserve strongly is the large, almost absurdly round cat. The last mockup gets this right. Making the mascot substantially smaller or more anatomically cat-like would lose a lot of the charm. The furniture should feel like scenery around CodeGotchi, not like a detailed pixel-art room that happens to contain a cat.
