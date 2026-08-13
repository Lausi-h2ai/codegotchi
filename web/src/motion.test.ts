import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
    createFreeTimeChoreography,
    createMotionController,
    isSafeRoomWaypoint,
    selectSafeRoomWaypoint,
    selectFreeTimeInteraction,
    SAFE_ROOM_WAYPOINTS,
    semanticModeForSnapshot,
    type MotionController,
} from "./motion";
import type {
    ActivityKind,
    AgentActivityState,
    PetBehavior,
    SimulationSnapshot,
} from "./protocol";

function snapshot(
    overrides: Partial<SimulationSnapshot> = {},
): SimulationSnapshot {
    return {
        schemaVersion: 1,
        petId: "00000000-0000-0000-0000-000000000001",
        name: "Mochi",
        species: "cat",
        needs: {
            hunger: 22,
            energy: 81,
            happiness: 94,
            cleanliness: 76,
        },
        behavior: "Working",
        activity: { Active: "editing" },
        recentOutcome: "None",
        workPoints: 4,
        digestionPoints: 12,
        lastUpdatedAt: "2026-08-05T12:00:00Z",
        pendingPoops: [],
        pendingDemands: [],
        attentionSequence: 0,
        nextIncidentAt: "2026-08-13T12:05:00Z",
        inventory: { kibble: 50, treat: 25, fruit: 25 },
        processedCareIds: [],
        poopSequence: 0,
        sessionActivities: {},
        processedEventIds: [],
        lastActivityAt: null,
        lastOutcomeAt: null,
        consecutiveFailures: 0,
        enforcementMode: "decorative",
        nappingUntil: null,
        ...overrides,
    };
}

describe("authoritative semantic mode projection", () => {
    it.each([
        ["editing", "desk"],
        ["testing", "desk"],
        ["building", "desk"],
        ["installing", "desk"],
        ["git_operation", "desk"],
        ["docker_operation", "desk"],
        ["unknown_work", "desk"],
        ["thinking", "thinking"],
        ["reading", "thinking"],
        ["searching", "thinking"],
        ["web_research", "thinking"],
        ["idle", "free_time"],
        ["waiting", "free_time"],
    ] as const)("maps active %s to %s", (activity, expected) => {
        expect(
            semanticModeForSnapshot(
                snapshot({ activity: { Active: activity as ActivityKind } }),
            ),
        ).toBe(expected);
    });

    it("maps the idle and waiting-for-user states to free time", () => {
        expect(semanticModeForSnapshot(snapshot({ activity: "Idle" }))).toBe(
            "free_time",
        );
        expect(
            semanticModeForSnapshot(snapshot({ activity: "WaitingForUser" })),
        ).toBe("free_time");
    });
});

describe("semantic mode override priority", () => {
    it("prioritizes napping over critical, outcome, and activity", () => {
        const napDeadline = new Date(1_000).toISOString();

        expect(
            semanticModeForSnapshot(
                snapshot({
                    behavior: "CriticalNeed",
                    activity: "Blocked",
                    recentOutcome: "Failure",
                    nappingUntil: napDeadline,
                }),
                0,
            ),
        ).toBe("napping");
    });

    it.each([
        ["critical behavior", { behavior: "CriticalNeed" as PetBehavior }],
        ["blocked behavior", { behavior: "Blocked" as PetBehavior }],
        ["blocked activity", { activity: "Blocked" as AgentActivityState }],
        [
            "blocked active activity",
            { activity: { Active: "blocked" } as AgentActivityState },
        ],
    ])("maps %s to critical", (_name, overrides) => {
        expect(semanticModeForSnapshot(snapshot(overrides))).toBe("critical");
    });

    it("prioritizes critical state over recent success and failure", () => {
        expect(
            semanticModeForSnapshot(
                snapshot({
                    behavior: "CriticalNeed",
                    recentOutcome: "Success",
                }),
            ),
        ).toBe("critical");
        expect(
            semanticModeForSnapshot(
                snapshot({
                    behavior: "Blocked",
                    recentOutcome: "Failure",
                }),
            ),
        ).toBe("critical");
    });

    it.each([
        [
            "success behavior",
            {
                behavior: "RecentSuccess" as PetBehavior,
                activity: "Idle" as AgentActivityState,
                lastOutcomeAt: new Date().toISOString(),
            },
        ],
        [
            "success outcome",
            {
                behavior: "Wandering" as PetBehavior,
                activity: "Idle" as AgentActivityState,
                recentOutcome: "Success" as const,
                lastOutcomeAt: new Date().toISOString(),
            },
        ],
    ])("maps %s to success", (_name, overrides) => {
        expect(semanticModeForSnapshot(snapshot(overrides))).toBe("success");
    });

    it.each([
        [
            "failure behavior",
            {
                behavior: "RecentFailure" as PetBehavior,
                activity: "Idle" as AgentActivityState,
                lastOutcomeAt: new Date().toISOString(),
            },
        ],
        [
            "failure outcome",
            {
                behavior: "Wandering" as PetBehavior,
                activity: "Idle" as AgentActivityState,
                recentOutcome: "Failure" as const,
                lastOutcomeAt: new Date().toISOString(),
            },
        ],
    ])("maps %s to failure", (_name, overrides) => {
        expect(semanticModeForSnapshot(snapshot(overrides))).toBe("failure");
    });

    it("treats an authoritative sleeping behavior as napping", () => {
        expect(
            semanticModeForSnapshot(snapshot({ behavior: "Sleeping" })),
        ).toBe("napping");
    });

    it("expires success and failure presentation after five minutes", () => {
        const outcomeAt = new Date(1_000_000).toISOString();

        expect(
            semanticModeForSnapshot(
                snapshot({
                    activity: "Idle",
                    recentOutcome: "Success",
                    lastOutcomeAt: outcomeAt,
                }),
                1_000_000 + 299_999,
            ),
        ).toBe("success");
        expect(
            semanticModeForSnapshot(
                snapshot({
                    activity: "Idle",
                    recentOutcome: "Failure",
                    lastOutcomeAt: outcomeAt,
                }),
                1_000_000 + 300_001,
            ),
        ).toBe("free_time");
    });

    it("does not present an outcome without a timestamp", () => {
        expect(
            semanticModeForSnapshot(
                snapshot({
                    behavior: "RecentSuccess",
                    activity: "Idle",
                    recentOutcome: "Success",
                    lastOutcomeAt: null,
                }),
            ),
        ).toBe("free_time");
    });

    it("lets a newer active behavior override an old outcome", () => {
        expect(
            semanticModeForSnapshot(
                snapshot({
                    behavior: "Working",
                    activity: { Active: "editing" },
                    recentOutcome: "Success",
                    lastOutcomeAt: new Date(1_000_000).toISOString(),
                }),
                1_000_001,
            ),
        ).toBe("desk");
        expect(
            semanticModeForSnapshot(
                snapshot({
                    behavior: "Working",
                    activity: { Active: "thinking" },
                    recentOutcome: "Failure",
                    lastOutcomeAt: new Date(1_000_000).toISOString(),
                }),
                1_000_001,
            ),
        ).toBe("thinking");
    });
});

describe("motion controller", () => {
    let controller: MotionController;

    beforeEach(() => {
        vi.useFakeTimers();
    });

    afterEach(() => {
        controller?.dispose();
        vi.useRealTimers();
    });

    it("interrupts a cosmetic action immediately and travels to the new destination", () => {
        controller = createMotionController({ random: () => 0.75 });

        const desk = controller.update(
            snapshot({ activity: { Active: "editing" } }),
        );
        const thinking = controller.update(
            snapshot({ activity: { Active: "thinking" } }),
        );

        expect(desk.semanticMode).toBe("desk");
        expect(thinking.semanticMode).toBe("thinking");
        expect(thinking.destination).toBe("thinking");
        expect(thinking.phase).toBe("traveling");
        expect(thinking.action).toBe("roll");
        expect(thinking.facing).toBe("right");
        expect(thinking.generation).toBeGreaterThan(desk.generation);

        vi.advanceTimersByTime(999);

        expect(controller.getState().phase).toBe("acting");
        expect(controller.getState().action).toBe("think");
    });

    it("faces the desk destination toward the monitor", () => {
        controller = createMotionController({ random: () => 0 });

        const desk = controller.update(
            snapshot({ activity: { Active: "editing" } }),
        );

        expect(desk.semanticMode).toBe("desk");
        expect(desk.facing).toBe("right");
    });

    it("does not restart generation or choreography for an equivalent mode", () => {
        controller = createMotionController();

        const first = controller.update(
            snapshot({ activity: { Active: "editing" } }),
        );
        const timerCount = vi.getTimerCount();
        const equivalent = controller.update(
            snapshot({
                activity: { Active: "editing" },
                lastUpdatedAt: "2026-08-05T12:00:01Z",
                needs: {
                    hunger: 25,
                    energy: 80,
                    happiness: 90,
                    cleanliness: 70,
                },
            }),
        );

        expect(equivalent).toBe(first);
        expect(equivalent.generation).toBe(first.generation);
        expect(vi.getTimerCount()).toBe(timerCount);
    });

    it("cleans travel and looping timers on a mode change and dispose", () => {
        controller = createMotionController();
        controller.update(snapshot({ activity: { Active: "editing" } }));
        expect(vi.getTimerCount()).toBeGreaterThan(0);

        controller.update(snapshot({ activity: { Active: "thinking" } }));
        expect(vi.getTimerCount()).toBeGreaterThan(0);

        controller.dispose();

        expect(vi.getTimerCount()).toBe(0);
    });

    it("uses static semantic poses without scheduling under reduced motion", () => {
        controller = createMotionController({ reducedMotion: true });

        const state = controller.update(
            snapshot({ activity: { Active: "testing" } }),
        );

        expect(state.semanticMode).toBe("desk");
        expect(state.destination).toBe("desk");
        expect(state.phase).toBe("static");
        expect(state.action).toBeNull();
        expect(state.effect).toBe("none");
        expect(vi.getTimerCount()).toBe(0);
    });

    it("accepts deterministic clock, timer, and randomness ports", () => {
        let nextHandle = 0;
        const callbacks = new Map<number, () => void>();
        const timer = {
            setTimeout: vi.fn((callback: () => void) => {
                const handle = nextHandle++;
                callbacks.set(handle, callback);
                return handle;
            }),
            clearTimeout: vi.fn((handle: unknown) => {
                callbacks.delete(handle as number);
            }),
        };
        const clock = { now: vi.fn(() => 123) };
        const random = vi.fn(() => 0.75);

        controller = createMotionController({
            timer,
            clock,
            randomness: random,
        });
        const state = controller.update(
            snapshot({ activity: { Active: "thinking" } }),
        );

        expect(clock.now).toHaveBeenCalledTimes(1);
        expect(random).toHaveBeenCalled();
        expect(timer.setTimeout).toHaveBeenCalledWith(
            expect.any(Function),
            expect.any(Number),
        );
        expect(state.phase).toBe("traveling");
        expect([...callbacks.values()]).toHaveLength(1);
    });

    it("interrupts free-time choreography as soon as authoritative work starts", () => {
        controller = createMotionController({
            random: () => 0.5,
            travelDurationMs: 10,
        });

        const freeTime = controller.update(snapshot({ activity: "Idle" }));
        expect(freeTime.semanticMode).toBe("free_time");
        expect(freeTime.roomWaypoint).not.toBeNull();
        expect(vi.getTimerCount()).toBeGreaterThan(0);

        const desk = controller.update(
            snapshot({ activity: { Active: "editing" } }),
        );
        expect(desk.semanticMode).toBe("desk");
        expect(desk.roomWaypoint).toBeNull();

        vi.advanceTimersByTime(10);
        expect(controller.getState().semanticMode).toBe("desk");
        expect(controller.getState().action).toBe("type");
    });

    it("does not restart free-time choreography for equivalent snapshots", () => {
        controller = createMotionController({ random: () => 0.5 });

        const first = controller.update(snapshot({ activity: "Idle" }));
        const timerCount = vi.getTimerCount();
        const equivalent = controller.update(
            snapshot({
                activity: "Idle",
                lastUpdatedAt: "2026-08-05T12:00:01Z",
            }),
        );

        expect(equivalent).toBe(first);
        expect(vi.getTimerCount()).toBe(timerCount);
    });

    it("keeps reduced-motion free time static and unscheduled", () => {
        controller = createMotionController({
            reducedMotion: true,
            random: () => 0.5,
        });

        const state = controller.update(snapshot({ activity: "Idle" }));

        expect(state.semanticMode).toBe("free_time");
        expect(state.action).toBeNull();
        expect(state.phase).toBe("static");
        expect(vi.getTimerCount()).toBe(0);
    });

    it("responds to a dynamic reduced-motion change without restarting the mode", () => {
        let reducedMotion = false;
        controller = createMotionController({
            reducedMotion: () => reducedMotion,
            random: () => 0.5,
        });

        const running = controller.update(snapshot({ activity: "Idle" }));
        const generation = running.generation;
        expect(vi.getTimerCount()).toBeGreaterThan(0);

        reducedMotion = true;
        const staticState = controller.update(snapshot({ activity: "Idle" }));
        expect(staticState.generation).toBe(generation);
        expect(staticState.phase).toBe("static");
        expect(staticState.action).toBeNull();
        expect(vi.getTimerCount()).toBe(0);

        reducedMotion = false;
        const resumed = controller.update(snapshot({ activity: "Idle" }));
        expect(resumed.generation).toBe(generation);
        expect(resumed.phase).not.toBe("static");
        expect(vi.getTimerCount()).toBeGreaterThan(0);
    });

    it("does not schedule a timer after a listener disposes the controller", () => {
        controller = createMotionController({ travelDurationMs: 10 });
        controller.subscribe(() => controller.dispose());

        controller.update(snapshot({ activity: { Active: "editing" } }));

        expect(vi.getTimerCount()).toBe(0);
    });
});

describe("free-time room choreography", () => {
    beforeEach(() => {
        vi.useFakeTimers();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    it("defines relative waypoints in several safe room regions", () => {
        expect(
            new Set(SAFE_ROOM_WAYPOINTS.map(({ region }) => region)).size,
        ).toBeGreaterThanOrEqual(4);

        for (const waypoint of SAFE_ROOM_WAYPOINTS) {
            expect(waypoint.position.x).toBeGreaterThanOrEqual(0);
            expect(waypoint.position.x).toBeLessThanOrEqual(1);
            expect(waypoint.position.y).toBeGreaterThanOrEqual(0);
            expect(waypoint.position.y).toBeLessThanOrEqual(1);
            expect(isSafeRoomWaypoint(waypoint)).toBe(true);
        }
    });

    it("keeps every free-time waypoint grounded in the floor band", () => {
        for (const waypoint of SAFE_ROOM_WAYPOINTS) {
            expect(waypoint.position.y, waypoint.id).toBeGreaterThanOrEqual(
                0.7,
            );
            expect(waypoint.position.y, waypoint.id).toBeLessThanOrEqual(1);
        }
    });

    it("selects only safe targets from the injected random sequence", () => {
        const randomValues = [0, 0.2, 0.4, 0.6, 0.8];
        const selected = randomValues.map((value) =>
            selectSafeRoomWaypoint(() => value),
        );

        expect(selected.every((waypoint) => isSafeRoomWaypoint(waypoint))).toBe(
            true,
        );
        expect(
            new Set(selected.map(({ region }) => region)).size,
        ).toBeGreaterThanOrEqual(3);
    });

    it("selects each interaction deterministically from the injected random value", () => {
        expect(selectFreeTimeInteraction(() => 0)).toBe("watch_window");
        expect(selectFreeTimeInteraction(() => 0.3)).toBe("inspect_shelf");
        expect(selectFreeTimeInteraction(() => 0.55)).toBe("circle_furniture");
        expect(selectFreeTimeInteraction(() => 0.8)).toBe("sit");
    });

    it("keeps roughly 80% of free time active and rolls every 15 to 30 seconds", () => {
        const events: Array<{
            at: number;
            paused: boolean;
            action: string | null;
            waypoint: string;
        }> = [];
        const randomValues = [0, 0.3, 0.55, 0.8, 0.1, 0.4, 0.6, 0.9];
        let randomIndex = 0;
        const choreography = createFreeTimeChoreography({
            random: () => {
                const value =
                    randomValues[randomIndex % randomValues.length] ?? 0;
                randomIndex += 1;
                return value;
            },
            roamDurationMs: 8_000,
            pauseDurationMs: 2_000,
            travelDurationMs: 0,
            rollIntervalMinMs: 15_000,
            rollIntervalMaxMs: 30_000,
            rollDurationMs: 500,
        });
        choreography.subscribe((state) => {
            events.push({
                at: Date.now(),
                paused: state.paused,
                action: state.action,
                waypoint: state.waypoint.id,
            });
        });

        choreography.start();
        vi.advanceTimersByTime(60_000);

        let pausedMs = 0;
        for (let index = 0; index < events.length; index += 1) {
            const current = events[index];
            const nextAt = events[index + 1]?.at ?? 60_000;
            if (current.paused) {
                pausedMs += Math.max(0, nextAt - current.at);
            }
        }
        const pausedRatio = pausedMs / 60_000;
        expect(pausedRatio).toBeGreaterThan(0.1);
        expect(pausedRatio).toBeLessThan(0.3);

        const rolls = events
            .filter(({ action }) => action === "roll")
            .map(({ at }) => at);
        expect(rolls.length).toBeGreaterThanOrEqual(2);
        for (let index = 1; index < rolls.length; index += 1) {
            expect(rolls[index] - rolls[index - 1]).toBeGreaterThanOrEqual(
                15_000,
            );
            expect(rolls[index] - rolls[index - 1]).toBeLessThanOrEqual(30_000);
        }

        expect(
            new Set(events.map(({ waypoint }) => waypoint)).size,
        ).toBeGreaterThanOrEqual(3);
        expect(
            events.every(({ waypoint }) =>
                isSafeRoomWaypoint(
                    SAFE_ROOM_WAYPOINTS.find(({ id }) => id === waypoint) ??
                        SAFE_ROOM_WAYPOINTS[0],
                ),
            ),
        ).toBe(true);
    });

    it("cleans every choreography timer when interrupted", () => {
        const choreography = createFreeTimeChoreography({
            random: () => 0.5,
        });

        choreography.start();
        expect(vi.getTimerCount()).toBeGreaterThan(0);
        choreography.stop();
        expect(vi.getTimerCount()).toBe(0);

        const stopped = choreography.getState();
        vi.advanceTimersByTime(60_000);
        expect(choreography.getState()).toBe(stopped);
    });

    it("does not schedule after a choreography listener disposes during start", () => {
        const choreography = createFreeTimeChoreography({
            random: () => 0.5,
        });
        choreography.subscribe(() => choreography.dispose());

        choreography.start();

        expect(vi.getTimerCount()).toBe(0);
    });
});
