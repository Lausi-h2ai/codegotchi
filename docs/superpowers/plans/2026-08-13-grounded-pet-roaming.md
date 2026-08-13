# Grounded Pet Roaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep Pixel's feet in the room's floor band throughout free-time roaming while retaining window and shelf stops, walking bounce, rolls, and interaction actions.

**Architecture:** `SAFE_ROOM_WAYPOINTS` remains the source of truth for room-relative free-time destinations. Correct the waypoint coordinates at that source, preserve interaction metadata, and protect the floor contract at both the deterministic data layer and the rendered production-browser layer.

**Tech Stack:** TypeScript 5.9, React 19, Vitest, Playwright, CSS, Vite.

## Global Constraints

- Preserve all existing uncommitted workspace changes outside this focused repair.
- Do not create implementation commits: every owned implementation file already contains pre-existing uncommitted work, so committing whole files would silently absorb user changes.
- The room floor begins at 70% of `.room-illustration`; every free-time waypoint must place the pet's feet at or below that boundary.
- `window` and `shelf` remain regions with `watch_window` and `inspect_shelf` interactions.
- Walking bounce, rolls, pauses, semantic interruption, reduced motion, and care operations remain unchanged.
- No backend state, protocol, persistence, inventory, need, or care schema changes.

---

### Task 1: Ground the choreography waypoints

**Files:**
- Modify: `web/src/motion.test.ts`
- Modify: `web/src/choreography.ts`

**Interfaces:**
- Consumes: `SAFE_ROOM_WAYPOINTS: readonly RoomWaypoint[]` and `isSafeRoomWaypoint(waypoint): boolean`.
- Produces: the same waypoint IDs, regions, and interactions with normalized Y coordinates in the inclusive `0.7..1` floor band.

- [x] **Step 1: Write the failing unit regression test**

Add this test beside the existing safe-waypoint coverage in `web/src/motion.test.ts`:

```ts
it("keeps every free-time waypoint grounded in the floor band", () => {
    for (const waypoint of SAFE_ROOM_WAYPOINTS) {
        expect(waypoint.position.y, waypoint.id).toBeGreaterThanOrEqual(0.7);
        expect(waypoint.position.y, waypoint.id).toBeLessThanOrEqual(1);
    }
});
```

The production change this catches is any roaming target whose bottom-center anchor moves above the room's 70% floor boundary.

- [x] **Step 2: Run the focused test and verify RED**

Run: `corepack pnpm --filter @codegotchi/web exec vitest run src/motion.test.ts`

Expected: FAIL for `window-sill` at `0.27` (and the other current wall-band waypoints), proving the regression test detects the flying behavior.

- [x] **Step 3: Replace only the normalized waypoint positions**

Use grounded, safe observation and roaming points in `web/src/choreography.ts` while retaining every ID, region, and interaction:

```ts
window-sill:    { x: 0.14, y: 0.76 }
shelf-front:   { x: 0.55, y: 0.74 }
floor-left:    { x: 0.18, y: 0.92 }
floor-center:  { x: 0.55, y: 0.88 }
floor-right:   { x: 0.72, y: 0.94 }
furniture-left:{ x: 0.22, y: 0.84 }
furniture-right:{ x: 0.56, y: 0.94 }
```

For each waypoint, update both `position: { x, y }` and the compatibility aliases `x` and `y`. Do not change timer durations, action selection, region names, interaction names, or forbidden hit zones.

- [x] **Step 4: Run unit tests and verify GREEN**

Run: `corepack pnpm --filter @codegotchi/web exec vitest run src/motion.test.ts`

Expected: all motion tests pass, including the existing assertion that every waypoint remains outside forbidden care-control and poop zones.

- [x] **Step 5: Inspect the focused data-layer patch**

```bash
git diff -- web/src/choreography.ts web/src/motion.test.ts
```

Expected: only the regression test and the seven waypoint coordinate updates belong to this repair; leave the overlapping files uncommitted.

### Task 2: Protect grounded placement in the rendered room

**Files:**
- Modify: `web/e2e/mvp.spec.ts`

**Interfaces:**
- Consumes: `.room-illustration`, `[data-testid="pet"]`, and the existing `data-motion-*` attributes.
- Produces: browser coverage proving observed free-time positions remain within the rendered floor band while multiple regions and rolls continue to occur.

- [x] **Step 1: Extend the existing idle-travel acceptance test**

Inside `keeps idle travel in safe regions and eventually performs a roll`, add a helper that reads real layout geometry:

```ts
const groundedInRenderedRoom = async (): Promise<boolean> => {
    const geometry = await page.evaluate(() => {
        const room = document.querySelector<HTMLElement>(".room-illustration");
        const pet = document.querySelector<HTMLElement>('[data-testid="pet"]');
        if (!room || !pet) return null;
        const roomRect = room.getBoundingClientRect();
        const petRect = pet.getBoundingClientRect();
        return {
            floorTop: roomRect.top + roomRect.height * 0.7,
            roomBottom: roomRect.bottom,
            petFeet: petRect.bottom,
        };
    });
    return (
        geometry !== null &&
        geometry.petFeet >= geometry.floorTop - 8 &&
        geometry.petFeet <= geometry.roomBottom + 1
    );
};
```

During the existing region/action polling, record `false` if any observed free-time frame violates this predicate, then assert no violation after multiple regions and a roll have been observed. The eight-pixel tolerance permits the intentional bounce/roll lift but rejects wall-height travel.

- [x] **Step 2: Prove the browser assertion catches the original coordinates**

Temporarily set every waypoint Y coordinate and alias to `0.27`, build/embed, and run the focused Playwright test. This removes random waypoint selection from the RED proof while reproducing the original wall-height placement:

`corepack pnpm --filter @codegotchi/web exec playwright test --config playwright.config.ts --grep "keeps idle travel in safe regions"`

Expected: FAIL because Pixel's feet are above the rendered floor boundary. Restore all seven grounded coordinates immediately after observing this failure.

- [x] **Step 3: Rebuild the embedded production UI**

Run: `corepack pnpm --filter @codegotchi/web build`

Run: `node web/scripts/embed-web.mjs`

Expected: Vite produces a new hashed CSS/JavaScript pair under `crates/codegotchi-cli/web-dist/assets`, and the old generated pair is removed by the embedding script.

- [x] **Step 4: Run the focused browser test and verify GREEN**

Run: `corepack pnpm --filter @codegotchi/web exec playwright test --config playwright.config.ts --grep "keeps idle travel in safe regions"`

Expected: PASS after observing at least two safe regions and a roll, with no grounded-placement violation.

- [x] **Step 5: Inspect browser coverage and generated bundle**

```bash
git diff -- web/e2e/mvp.spec.ts crates/codegotchi-cli/web-dist
```

Expected: the grounded-geometry assertion and rebuilt generated assets are present; leave the overlapping files uncommitted.

### Task 3: Verify the focused repair and surrounding behavior

**Files:**
- Verify only; modify Task 1 or Task 2 files only if a command exposes a defect caused by this repair.

**Interfaces:**
- Consumes: the grounded waypoint data, browser assertion, and rebuilt embedded UI.
- Produces: fresh evidence that the repair does not regress frontend behavior, formatting, linting, builds, or repository whitespace.

- [x] **Step 1: Run frontend unit tests**

Run: `corepack pnpm --filter @codegotchi/web test`

Expected: all Vitest files pass with zero failures.

- [x] **Step 2: Run static checks and production build**

Run: `corepack pnpm --filter @codegotchi/web lint`

Run: `corepack pnpm --filter @codegotchi/web format:check`

Run: `corepack pnpm --filter @codegotchi/web build`

Expected: all commands exit zero with no lint, format, TypeScript, or Vite errors.

- [x] **Step 3: Run the complete production-browser suite**

Run: `corepack pnpm playwright:test`

Expected: every Playwright scenario passes, including care operations, reduced motion, semantic interruption, multi-region idle travel, rolls, and grounded placement.

- [x] **Step 4: Check the final patch**

Run: `git diff --check`

Run: `git status --short`

Expected: no whitespace errors; only the pre-existing user changes plus the files explicitly owned by this plan are present.
