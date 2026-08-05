# CodeGotchi backlog

This backlog separates non-blocking hardening from later product phases. The
items below do not change the approved Phase 1/2 behavior or make this task
incomplete.

## Non-blocking future hardening

These items come from the approved Task 4 review:

1. Add a direct Strict-mode test for the simultaneous critical hunger and
   cleanliness case, asserting the hunger-first tie and feed recovery action.
2. Add mismatched `CommandCategory`/`CommandPurpose` matrix cases and invoke a
   custom `WorkPermissionStrategy` in a test, protecting the fail-open purpose
   contract and replaceable strategy boundary.

The following Phase 1 frontend/toolchain notes are also non-blocking future
hardening, not current defects or MVP blockers:

3. Include `web/vite.config.ts` in the web `format:check` coverage.
4. Replace CI's moving Node `22` major alias with an exact, reproducible
   selector aligned with the documented Node `22.22.2` floor.

## Later product phases

The following are deliberately deferred future slices, not defects in the
current foundation:

| Future slice | Deliberately deferred work |
| --- | --- |
| Functional renderer and client workflows | PixiJS, Zustand, care controls, client projections, API clients, and WebSockets |
| Browser acceptance | Playwright workflows once there is a meaningful interactive client |
| Runtime authority | A daemon that hosts the live simulation and remains the authoritative boundary |
| Persistence | SQLite and persistence/replay integration |
| Process and command safety | Process wrapper, command proxy, hooks, and ingestion validation |
| External integrations | Agent/application adapters and MCP integration |
| Delivery | An embedded web bundle and its serving/runtime integration |
| Visual production | Polished art and final room/character assets |

Each later slice should preserve the rule that the browser is a projection and
the daemon is the future runtime authority. The current static HTML/CSS room is
enough to validate the accessible presentation foundation without pulling any
of those systems forward.
