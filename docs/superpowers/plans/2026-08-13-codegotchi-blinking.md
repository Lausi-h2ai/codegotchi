# CodeGotchi Blinking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every awake CodeGotchi perform one quick close-and-open blink after a newly randomized wait of 5–10 seconds, while suppressing and resetting blinking during sleep.

**Architecture:** A focused React hook owns only the presentation timer and returns a closed-eye boolean. `App` enables the hook from the existing semantic motion mode and exposes the boolean as a pet data attribute; CSS reshapes the existing eyes without changing simulation, protocol, persistence, or motion choreography.

**Tech Stack:** React 19 hooks, TypeScript 5.9, Vitest fake timers, Testing Library, CSS.

## Global Constraints

- Each blink is a single close-and-open cycle with both eyes closed for 120 milliseconds.
- Each awake wait is independently randomized from 5,000 through 10,000 milliseconds.
- Entering `napping` cancels pending and active awake blinks; leaving it starts a fresh wait.
- Blinking must compose with every awake semantic mode and must not become a motion-controller action.
- No Rust, protocol, persistence, dependency, or sleeping-visual changes.
- Preserve all unrelated existing working-tree changes.

---

### Task 1: Presentation-only blink scheduler

**Files:**
- Create: `web/src/useBlink.ts`
- Create: `web/src/useBlink.test.tsx`

**Interfaces:**
- Consumes: `enabled: boolean` and an optional stable `random: () => number` dependency.
- Produces: `useBlink(enabled: boolean, random?: () => number): boolean`, where `true` means the awake eyes are in their 120-millisecond closed phase.

- [ ] **Step 1: Write failing minimum-delay, close/open, and cleanup tests**

Create `web/src/useBlink.test.tsx` with real hook behavior under fake timers:

```tsx
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useBlink } from "./useBlink";

describe("useBlink", () => {
    beforeEach(() => vi.useFakeTimers());

    afterEach(() => {
        cleanup();
        vi.useRealTimers();
    });

    it("closes once after the five-second minimum and reopens after 120ms", () => {
        const random = (): number => 0;
        const { result } = renderHook(() => useBlink(true, random));

        act(() => vi.advanceTimersByTime(4_999));
        expect(result.current).toBe(false);

        act(() => vi.advanceTimersByTime(1));
        expect(result.current).toBe(true);

        act(() => vi.advanceTimersByTime(119));
        expect(result.current).toBe(true);

        act(() => vi.advanceTimersByTime(1));
        expect(result.current).toBe(false);
    });

    it("clears every timer on unmount", () => {
        const random = (): number => 0.5;
        const { unmount } = renderHook(() => useBlink(true, random));
        expect(vi.getTimerCount()).toBe(1);
        unmount();
        expect(vi.getTimerCount()).toBe(0);
    });
});
```

- [ ] **Step 2: Run the focused test and verify RED**

Run: `corepack pnpm --filter @codegotchi/web test -- src/useBlink.test.tsx`

Expected: FAIL because `./useBlink` does not exist.

- [ ] **Step 3: Implement the minimum scheduler**

Create `web/src/useBlink.ts`:

```ts
import { useEffect, useState } from "react";

const MIN_BLINK_DELAY_MS = 5_000;
const BLINK_DELAY_RANGE_MS = 5_000;
const BLINK_DURATION_MS = 120;

function nextBlinkDelay(random: () => number): number {
    const sample = Math.min(1, Math.max(0, random()));
    return MIN_BLINK_DELAY_MS + Math.round(sample * BLINK_DELAY_RANGE_MS);
}

export function useBlink(
    enabled: boolean,
    random: () => number = Math.random,
): boolean {
    const [blinking, setBlinking] = useState(false);

    useEffect(() => {
        let waitTimer: number | undefined;
        let blinkTimer: number | undefined;

        if (enabled) {
            waitTimer = window.setTimeout(() => {
                setBlinking(true);
                blinkTimer = window.setTimeout(
                    () => setBlinking(false),
                    BLINK_DURATION_MS,
                );
            }, nextBlinkDelay(random));
        }

        return () => {
            window.clearTimeout(waitTimer);
            window.clearTimeout(blinkTimer);
        };
    }, [enabled, random]);

    return blinking;
}
```

- [ ] **Step 4: Run the focused test and verify GREEN**

Run: `corepack pnpm --filter @codegotchi/web test -- src/useBlink.test.tsx`

Expected: both the minimum-delay close/open and timer-cleanup tests PASS.

- [ ] **Step 5: Add a failing maximum and rerandomization test**

Add this case to `web/src/useBlink.test.tsx`:

```tsx
it("supports the ten-second maximum and chooses a fresh delay after reopening", () => {
    const random = vi.fn().mockReturnValueOnce(1).mockReturnValueOnce(0);
    const { result } = renderHook(() => useBlink(true, random));

    act(() => vi.advanceTimersByTime(9_999));
    expect(result.current).toBe(false);
    act(() => vi.advanceTimersByTime(1));
    expect(result.current).toBe(true);
    act(() => vi.advanceTimersByTime(120));
    expect(result.current).toBe(false);

    act(() => vi.advanceTimersByTime(4_999));
    expect(result.current).toBe(false);
    act(() => vi.advanceTimersByTime(1));
    expect(result.current).toBe(true);
    expect(random).toHaveBeenCalledTimes(2);
});

```

- [ ] **Step 6: Run the focused test and verify RED**

Run: `corepack pnpm --filter @codegotchi/web test -- src/useBlink.test.tsx`

Expected: FAIL because the second blink never starts and `random` is called only once.

- [ ] **Step 7: Reschedule with fresh randomness after each blink**

Replace the one-shot scheduling block in `useBlink.ts` with:

```ts
const schedule = (): void => {
    waitTimer = window.setTimeout(() => {
        setBlinking(true);
        blinkTimer = window.setTimeout(() => {
            setBlinking(false);
            schedule();
        }, BLINK_DURATION_MS);
    }, nextBlinkDelay(random));
};

if (enabled) {
    schedule();
}
```

- [ ] **Step 8: Run the focused test and verify GREEN**

Run: `corepack pnpm --filter @codegotchi/web test -- src/useBlink.test.tsx`

Expected: all three tests PASS, including the 10-second first wait and fresh 5-second second wait.

- [ ] **Step 9: Add a failing sleep reset and restart test**

Add to `web/src/useBlink.test.tsx`:

```tsx
it("cancels and clears blinking while disabled, then starts a fresh wait", () => {
    const random = vi.fn(() => 0);
    const { result, rerender } = renderHook(
        ({ enabled }) => useBlink(enabled, random),
        { initialProps: { enabled: true } },
    );

    act(() => vi.advanceTimersByTime(5_000));
    expect(result.current).toBe(true);
    rerender({ enabled: false });
    expect(result.current).toBe(false);
    expect(vi.getTimerCount()).toBe(0);

    rerender({ enabled: true });
    act(() => vi.advanceTimersByTime(4_999));
    expect(result.current).toBe(false);
    act(() => vi.advanceTimersByTime(1));
    expect(result.current).toBe(true);
});
```

- [ ] **Step 10: Run the focused test and verify RED**

Run: `corepack pnpm --filter @codegotchi/web test -- src/useBlink.test.tsx`

Expected: FAIL immediately after disabling because cleanup cancels the reopen timer but the closed-eye state remains `true`.

- [ ] **Step 11: Clear the blink state when disabled**

Extend the existing enable branch in `useBlink.ts`:

```ts
if (enabled) {
    schedule();
} else {
    setBlinking(false);
}
```

- [ ] **Step 12: Run the focused test and verify GREEN**

Run: `corepack pnpm --filter @codegotchi/web test -- src/useBlink.test.tsx`

Expected: all four scheduler tests PASS.

- [ ] **Step 13: Commit the scheduler**

```bash
git add web/src/useBlink.ts web/src/useBlink.test.tsx
git commit --only web/src/useBlink.ts web/src/useBlink.test.tsx -m "feat: schedule awake CodeGotchi blinks"
```

### Task 2: Pet presentation integration

**Files:**
- Modify: `web/src/App.tsx:1-55,228-249`
- Modify: `web/src/App.css:463-480,896-904`
- Modify: `web/src/App.test.tsx:361-650`

**Interfaces:**
- Consumes: `useBlink(enabled: boolean): boolean` from Task 1 and `motionState.semanticMode` from the existing motion adapter.
- Produces: `data-blinking="true"` on the pet only during an awake closed-eye phase; `.pet[data-blinking="true"] .pet-eye` supplies the visual.

- [ ] **Step 1: Write the failing awake and sleeping adapter tests**

In `web/src/App.test.tsx`, mock `Math.random` to `0` within each test so the first blink occurs at exactly five seconds:

```tsx
it("exposes one awake blink after five seconds and reopens after 120ms", () => {
    vi.spyOn(Math, "random").mockReturnValue(0);
    renderApp({
        snapshot: snapshot({ behavior: "Wandering", activity: "Idle" }),
        connectionStatus: "connected",
    });
    const pet = screen.getByTestId("pet");

    expect(pet).not.toHaveAttribute("data-blinking");
    act(() => vi.advanceTimersByTime(5_000));
    expect(pet).toHaveAttribute("data-blinking", "true");
    act(() => vi.advanceTimersByTime(120));
    expect(pet).not.toHaveAttribute("data-blinking");
});

it("does not schedule awake blinking while napping", () => {
    vi.spyOn(Math, "random").mockReturnValue(0);
    renderApp({
        snapshot: snapshot({
            behavior: "Sleeping",
            nappingUntil: "2026-08-05T12:00:10Z",
        }),
        connectionStatus: "connected",
    });
    const pet = screen.getByTestId("pet");

    act(() => vi.advanceTimersByTime(10_000));
    expect(pet).not.toHaveAttribute("data-blinking");
});
```

Add `vi.restoreAllMocks()` to the motion adapter `afterEach` so the random spy cannot leak.

- [ ] **Step 2: Run the App tests and verify RED**

Run: `corepack pnpm --filter @codegotchi/web test -- src/App.test.tsx`

Expected: FAIL because the pet never receives `data-blinking`.

- [ ] **Step 3: Connect the hook to the semantic sleep mode**

In `web/src/App.tsx`, import the hook, call it after `usePetMotion`, and expose only the active value:

```tsx
import { useBlink } from "./useBlink";

const motionState = usePetMotion(snapshot);
const blinking = useBlink(
    snapshot !== null && motionState.semanticMode !== "napping",
);

// On the existing pet element:
data-blinking={blinking ? "true" : undefined}
```

- [ ] **Step 4: Add the eye-closing CSS**

Place this after the base `.pet-eye` rule in `web/src/App.css` so it composes with awake pose selectors:

```css
.pet[data-blinking="true"] .pet-eye {
    top: 31%;
    height: 0.22rem;
    border-radius: 0.15rem;
}
```

Keep the existing `.pet--napping .pet-eye` and semantic napping styles unchanged.

- [ ] **Step 5: Run focused tests and verify GREEN**

Run: `corepack pnpm --filter @codegotchi/web test -- src/useBlink.test.tsx src/App.test.tsx`

Expected: PASS for the hook lifecycle, awake attribute cycle, and napping suppression.

- [ ] **Step 6: Run all web quality gates**

Run:

```bash
corepack pnpm --filter @codegotchi/web test
corepack pnpm --filter @codegotchi/web lint
corepack pnpm --filter @codegotchi/web format:check
corepack pnpm --filter @codegotchi/web build
```

Expected: every command exits 0 with no test failures, lint errors, formatting differences, or TypeScript/build errors.

- [ ] **Step 7: Rebuild the embedded production UI**

Run: `node web/scripts/embed-web.mjs`

Expected: `crates/codegotchi-cli/web-dist/index.html` and hashed assets reflect the new web build, with obsolete generated assets removed by the script.

- [ ] **Step 8: Verify the embedded static asset contract**

Run: `cargo test -p codegotchi-cli --test static_assets`

Expected: PASS, proving the Rust binary serves a valid embedded SPA containing the rebuilt UI.

- [ ] **Step 9: Inspect scope and commit the completed feature**

Run `git diff --check` and `git status --short`. Confirm only the blink source/tests, generated embedded bundle, and pre-existing unrelated edits are present; stage and commit only blink-owned paths:

```bash
git add web/src/useBlink.ts web/src/useBlink.test.tsx web/src/App.tsx web/src/App.css web/src/App.test.tsx crates/codegotchi-cli/web-dist/index.html crates/codegotchi-cli/web-dist/assets
git commit --only web/src/useBlink.ts web/src/useBlink.test.tsx web/src/App.tsx web/src/App.css web/src/App.test.tsx crates/codegotchi-cli/web-dist/index.html crates/codegotchi-cli/web-dist/assets -m "feat: animate awake CodeGotchi blinks"
```
