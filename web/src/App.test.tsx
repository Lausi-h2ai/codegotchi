import {
    act,
    cleanup,
    fireEvent,
    render,
    screen,
} from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import App from "./App";
import { useCodeGotchi } from "./useCodeGotchi";
import type { AgentActivityState, SimulationSnapshot } from "./protocol";

vi.mock("./useCodeGotchi", () => ({
    useCodeGotchi: vi.fn(),
}));

const mockedUseCodeGotchi = vi.mocked(useCodeGotchi);

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
        behavior: "Wandering",
        activity: "Idle",
        recentOutcome: "None",
        workPoints: 4,
        digestionPoints: 12,
        lastUpdatedAt: "2026-08-05T12:00:00Z",
        pendingPoops: [
            {
                id: "00000000-0000-0000-0000-000000000099",
                createdAt: "2026-08-05T12:00:00Z",
            },
        ],
        inventory: { kibble: 50, treat: 25, fruit: 25, energy_drink: 10 },
        processedCareIds: [],
        poopSequence: 1,
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

function renderApp(
    state: Partial<ReturnType<typeof useCodeGotchi>> = {},
): void {
    mockedUseCodeGotchi.mockReturnValue({
        snapshot: null,
        connectionStatus: "loading",
        error: null,
        feedback: null,
        debugEnabled: false,
        feed: vi.fn().mockResolvedValue(undefined),
        clean: vi.fn().mockResolvedValue(undefined),
        nap: vi.fn().mockResolvedValue(undefined),
        restock: vi.fn().mockResolvedValue(undefined),
        ...state,
    });
    render(<App launchToken="test-token" />);
}

describe("CodeGotchi pet room", () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    afterEach(() => {
        cleanup();
    });

    it("renders a loading room while the authoritative snapshot is pending", () => {
        renderApp();

        expect(
            screen.getByRole("heading", { level: 1, name: "CodeGotchi" }),
        ).toBeInTheDocument();
        expect(screen.getByText("Loading authoritative room…")).toBeVisible();
    });

    it("renders disconnected state and backend errors", () => {
        renderApp({
            connectionStatus: "disconnected",
            error: {
                code: "unknown_food",
                message: "unknown food id: poison",
            },
        });

        expect(screen.getByText("Disconnected")).toBeVisible();
        expect(screen.getByRole("alert")).toHaveTextContent(
            "unknown food id: poison",
        );
    });

    it("projects the complete authoritative snapshot into an accessible room", () => {
        renderApp({
            snapshot: snapshot(),
            connectionStatus: "connected",
        });

        expect(
            screen.getByRole("region", { name: "CodeGotchi pet room" }),
        ).toBeInTheDocument();
        expect(
            screen.getByRole("heading", { level: 2, name: /Mochi/ }),
        ).toBeVisible();
        expect(screen.getByText("Connected")).toBeVisible();
        expect(screen.getByText("Hunger 22%")).toBeInTheDocument();
        expect(screen.getByText("Energy 81%")).toBeInTheDocument();
        expect(screen.getByText("Happiness 94%")).toBeInTheDocument();
        expect(screen.getByText("Cleanliness 76%")).toBeInTheDocument();
        expect(screen.getByTestId("food-kibble")).toHaveTextContent("50");
        expect(
            screen.getByTestId("poop-00000000-0000-0000-0000-000000000099"),
        ).toBeInTheDocument();
        expect(
            screen.getByRole("button", { name: /feed target/i }),
        ).toBeInTheDocument();
        expect(
            screen.getByRole("button", { name: /shovel/i }),
        ).toBeInTheDocument();
        expect(
            screen.getByRole("button", { name: /trash/i }),
        ).toBeInTheDocument();
        expect(
            screen.getByRole("button", { name: /hammock nap/i }),
        ).toBeInTheDocument();
        expect(screen.getByTestId("food-energy_drink")).toHaveTextContent("10");
    });

    it("sends one hammock nap when the hammock is clicked", () => {
        const nap = vi.fn().mockResolvedValue(undefined);
        renderApp({
            snapshot: snapshot(),
            connectionStatus: "connected",
            nap,
        });

        fireEvent.click(screen.getByRole("button", { name: /hammock nap/i }));

        expect(nap).toHaveBeenCalledTimes(1);
    });

    it("presents an active hammock nap with ZZZs and a sleeping label", () => {
        const napDeadline = new Date(Date.now() + 10_000).toISOString();
        renderApp({
            snapshot: snapshot({
                behavior: "Sleeping",
                nappingUntil: napDeadline,
            }),
            connectionStatus: "connected",
        });

        expect(screen.getByTestId("zzz")).toBeVisible();
        expect(screen.getByTestId("activity-label")).toHaveTextContent(
            "Sleeping",
        );
        expect(
            screen.getByRole("button", { name: /resting in hammock/i }),
        ).toBeDisabled();
    });

    it("does not start a second nap while the first is still active", () => {
        const napDeadline = new Date(Date.now() + 10_000).toISOString();
        const nap = vi.fn().mockResolvedValue(undefined);
        renderApp({
            snapshot: snapshot({
                nappingUntil: napDeadline,
            }),
            connectionStatus: "connected",
            nap,
        });

        fireEvent.click(
            screen.getByRole("button", { name: /resting in hammock/i }),
        );

        expect(nap).not.toHaveBeenCalled();
    });

    it("only offers the debug restock button when the runtime enables debug", () => {
        renderApp({
            snapshot: snapshot(),
            connectionStatus: "connected",
            debugEnabled: false,
        });

        expect(screen.queryByTestId("restock")).not.toBeInTheDocument();
    });

    it("restocks the pantry through the guarded debug button", () => {
        const restock = vi.fn().mockResolvedValue(undefined);
        renderApp({
            snapshot: snapshot(),
            connectionStatus: "connected",
            debugEnabled: true,
            restock,
        });

        fireEvent.click(screen.getByTestId("restock"));

        expect(restock).toHaveBeenCalledTimes(1);
    });

    it.each([
        ["idle", "Idle", "Wandering", "Idle"],
        ["wandering", "Idle", "Wandering", "Wandering / walking"],
        ["sleeping", "Idle", "Sleeping", "Sleeping"],
        ["thinking", { Active: "thinking" }, "Working", "Thinking"],
        ["reading", { Active: "reading" }, "Working", "Reading"],
        ["searching", { Active: "searching" }, "Working", "Searching"],
        ["typing", { Active: "editing" }, "Working", "Typing / editing"],
        ["testing", { Active: "testing" }, "Working", "Testing"],
        ["building", { Active: "building" }, "Working", "Building"],
        ["generic work", { Active: "installing" }, "Working", "Working"],
        ["celebrating", "Idle", "RecentSuccess", "Celebrating"],
        ["upset", "Idle", "RecentFailure", "Upset"],
        ["refusing", "Blocked", "Blocked", "Refusing"],
    ] as const)(
        "visibly distinguishes the %s presentation",
        (_name, activity, behavior, label) => {
            renderApp({
                snapshot: snapshot({
                    activity: activity as AgentActivityState,
                    behavior,
                    recentOutcome:
                        behavior === "RecentSuccess"
                            ? "Success"
                            : behavior === "RecentFailure"
                              ? "Failure"
                              : "None",
                }),
                connectionStatus: "connected",
            });

            expect(screen.getByTestId("activity-label")).toHaveTextContent(
                label,
            );
        },
    );

    it("shows authoritative eating feedback only after a returned care change", () => {
        renderApp({
            snapshot: snapshot(),
            connectionStatus: "connected",
            feedback: "Eating kibble",
        });

        expect(screen.getByText("Eating kibble")).toBeVisible();
    });

    it("does not send an invalid food drop", () => {
        const feed = vi.fn().mockResolvedValue(undefined);
        renderApp({
            snapshot: snapshot(),
            connectionStatus: "connected",
            feed,
        });

        fireEvent.drop(screen.getByRole("button", { name: /trash/i }), {
            dataTransfer: {
                getData: () => "food:kibble",
            },
        });

        expect(feed).not.toHaveBeenCalled();
    });

    it("does not clean a poop dragged directly to trash", () => {
        const clean = vi.fn().mockResolvedValue(undefined);
        renderApp({
            snapshot: snapshot(),
            connectionStatus: "connected",
            clean,
        });

        const poop = screen.getByTestId(
            "poop-00000000-0000-0000-0000-000000000099",
        );
        fireEvent.drop(screen.getByRole("button", { name: /trash/i }), {
            dataTransfer: {
                getData: () => `poop:${poop.dataset.poopId}`,
            },
        });

        expect(clean).not.toHaveBeenCalled();
        expect(poop).toBeInTheDocument();
    });

    it("sends cleaning only after shovel, poop, and trash", () => {
        const clean = vi.fn().mockResolvedValue(undefined);
        renderApp({
            snapshot: snapshot(),
            connectionStatus: "connected",
            clean,
        });

        const shovel = screen.getByRole("button", { name: /shovel/i });
        const poop = screen.getByTestId(
            "poop-00000000-0000-0000-0000-000000000099",
        );
        const trash = screen.getByRole("button", { name: /trash/i });

        fireEvent.click(shovel);
        fireEvent.click(poop);
        expect(clean).not.toHaveBeenCalled();
        fireEvent.click(trash);

        expect(clean).toHaveBeenCalledWith(
            "00000000-0000-0000-0000-000000000099",
        );
    });

    it("preserves the shovel, poop, and trash drag path", () => {
        const clean = vi.fn().mockResolvedValue(undefined);
        renderApp({
            snapshot: snapshot(),
            connectionStatus: "connected",
            clean,
        });

        const poop = screen.getByTestId(
            "poop-00000000-0000-0000-0000-000000000099",
        );
        const trash = screen.getByRole("button", { name: /trash/i });

        fireEvent.drop(poop, {
            dataTransfer: {
                getData: () => "shovel",
            },
        });
        fireEvent.drop(trash, {
            dataTransfer: {
                getData: () => "poop:00000000-0000-0000-0000-000000000099",
            },
        });

        expect(clean).toHaveBeenCalledWith(
            "00000000-0000-0000-0000-000000000099",
        );
    });
});

describe("CodeGotchi motion presentation adapter", () => {
    beforeEach(() => {
        vi.useFakeTimers();
        vi.setSystemTime(new Date("2026-08-05T12:00:00Z"));
    });

    afterEach(() => {
        cleanup();
        vi.restoreAllMocks();
        vi.useRealTimers();
    });

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

    it.each([
        [
            "editing",
            {
                behavior: "Working" as const,
                activity: { Active: "editing" } as const,
                expectedMode: "desk",
                expectedWaypoint: "desk",
            },
        ],
        [
            "thinking",
            {
                behavior: "Working" as const,
                activity: { Active: "thinking" } as const,
                expectedMode: "thinking",
                expectedWaypoint: "thinking",
            },
        ],
        [
            "idle",
            {
                behavior: "Wandering" as const,
                activity: "Idle" as const,
                expectedMode: "free_time",
                expectedWaypoint: "room waypoint",
            },
        ],
        [
            "napping",
            {
                behavior: "Sleeping" as const,
                nappingUntil: "2026-08-05T12:00:10Z",
                expectedMode: "napping",
                expectedWaypoint: "hammock",
            },
        ],
        [
            "critical",
            {
                behavior: "CriticalNeed" as const,
                expectedMode: "critical",
                expectedWaypoint: "critical",
            },
        ],
        [
            "success",
            {
                behavior: "RecentSuccess" as const,
                recentOutcome: "Success" as const,
                lastOutcomeAt: "2026-08-05T12:00:00Z",
                expectedMode: "success",
                expectedWaypoint: "success",
            },
        ],
        [
            "failure",
            {
                behavior: "RecentFailure" as const,
                recentOutcome: "Failure" as const,
                lastOutcomeAt: "2026-08-05T12:00:00Z",
                expectedMode: "failure",
                expectedWaypoint: "failure",
            },
        ],
    ] as const)(
        "exposes the %s semantic mode and destination on the pet node",
        (_name, state) => {
            renderApp({
                snapshot: snapshot(state),
                connectionStatus: "connected",
            });

            const pet = screen.getByTestId("pet");
            expect(pet).toHaveAttribute("data-motion-mode", state.expectedMode);
            if (state.expectedWaypoint === "room waypoint") {
                expect(pet).not.toHaveAttribute(
                    "data-motion-waypoint",
                    "free_time",
                );
            } else {
                expect(pet).toHaveAttribute(
                    "data-motion-waypoint",
                    state.expectedWaypoint,
                );
            }
        },
    );

    it("settles editing and thinking actions within 999ms", () => {
        renderApp({
            snapshot: snapshot({
                behavior: "Working",
                activity: { Active: "editing" },
            }),
            connectionStatus: "connected",
        });

        const pet = screen.getByTestId("pet");
        expect(pet).toHaveAttribute("data-motion-action", "roll");
        act(() => vi.advanceTimersByTime(999));
        expect(pet).toHaveAttribute("data-motion-action", "type");

        cleanup();
        renderApp({
            snapshot: snapshot({
                behavior: "Working",
                activity: { Active: "thinking" },
            }),
            connectionStatus: "connected",
        });

        const thinkingPet = screen.getByTestId("pet");
        act(() => vi.advanceTimersByTime(999));
        expect(thinkingPet).toHaveAttribute("data-motion-action", "think");
    });

    it("interrupts desk motion when an authoritative thinking snapshot arrives", () => {
        const editingState = {
            snapshot: snapshot({
                behavior: "Working" as const,
                activity: { Active: "editing" } as const,
            }),
            connectionStatus: "connected" as const,
        };
        mockedUseCodeGotchi.mockReturnValue({
            snapshot: editingState.snapshot,
            connectionStatus: editingState.connectionStatus,
            error: null,
            feedback: null,
            debugEnabled: false,
            feed: vi.fn().mockResolvedValue(undefined),
            clean: vi.fn().mockResolvedValue(undefined),
            nap: vi.fn().mockResolvedValue(undefined),
            restock: vi.fn().mockResolvedValue(undefined),
        });
        const view = render(<App launchToken="test-token" />);
        const pet = screen.getByTestId("pet");

        mockedUseCodeGotchi.mockReturnValue({
            snapshot: snapshot({
                behavior: "Working",
                activity: { Active: "thinking" },
            }),
            connectionStatus: "connected",
            error: null,
            feedback: null,
            debugEnabled: false,
            feed: vi.fn().mockResolvedValue(undefined),
            clean: vi.fn().mockResolvedValue(undefined),
            nap: vi.fn().mockResolvedValue(undefined),
            restock: vi.fn().mockResolvedValue(undefined),
        });
        view.rerender(<App launchToken="test-token" />);

        expect(pet).toHaveAttribute("data-motion-mode", "thinking");
        expect(pet).toHaveAttribute("data-motion-action", "roll");
        act(() => vi.advanceTimersByTime(999));
        expect(pet).toHaveAttribute("data-motion-action", "think");
    });

    it("reconnects motion after StrictMode replays before the first snapshot", () => {
        mockedUseCodeGotchi.mockReturnValue({
            snapshot: null,
            connectionStatus: "loading",
            error: null,
            feedback: null,
            debugEnabled: false,
            feed: vi.fn().mockResolvedValue(undefined),
            clean: vi.fn().mockResolvedValue(undefined),
            nap: vi.fn().mockResolvedValue(undefined),
            restock: vi.fn().mockResolvedValue(undefined),
        });
        const view = render(
            <StrictMode>
                <App launchToken="test-token" />
            </StrictMode>,
        );

        mockedUseCodeGotchi.mockReturnValue({
            snapshot: snapshot({
                behavior: "Working",
                activity: { Active: "editing" },
            }),
            connectionStatus: "connected",
            error: null,
            feedback: null,
            debugEnabled: false,
            feed: vi.fn().mockResolvedValue(undefined),
            clean: vi.fn().mockResolvedValue(undefined),
            nap: vi.fn().mockResolvedValue(undefined),
            restock: vi.fn().mockResolvedValue(undefined),
        });
        view.rerender(
            <StrictMode>
                <App launchToken="test-token" />
            </StrictMode>,
        );

        const pet = screen.getByTestId("pet");
        expect(pet).toHaveAttribute("data-motion-mode", "desk");
        expect(pet).toHaveAttribute("data-motion-action", "roll");
        act(() => vi.advanceTimersByTime(999));
        expect(pet).toHaveAttribute("data-motion-action", "type");
    });

    it("keeps equivalent editing snapshots on the same generation and timers", () => {
        const first = snapshot({
            behavior: "Working",
            activity: { Active: "editing" },
        });
        mockedUseCodeGotchi.mockReturnValue({
            snapshot: first,
            connectionStatus: "connected",
            error: null,
            feedback: null,
            debugEnabled: false,
            feed: vi.fn().mockResolvedValue(undefined),
            clean: vi.fn().mockResolvedValue(undefined),
            nap: vi.fn().mockResolvedValue(undefined),
            restock: vi.fn().mockResolvedValue(undefined),
        });
        const view = render(<App launchToken="test-token" />);
        const pet = screen.getByTestId("pet");
        const facing = pet.getAttribute("data-motion-facing");
        const timerCount = vi.getTimerCount();

        mockedUseCodeGotchi.mockReturnValue({
            snapshot: snapshot({
                behavior: "Working",
                activity: { Active: "editing" },
                lastUpdatedAt: "2026-08-05T12:00:01Z",
                needs: {
                    hunger: 25,
                    energy: 80,
                    happiness: 90,
                    cleanliness: 70,
                },
            }),
            connectionStatus: "connected",
            error: null,
            feedback: null,
            debugEnabled: false,
            feed: vi.fn().mockResolvedValue(undefined),
            clean: vi.fn().mockResolvedValue(undefined),
            nap: vi.fn().mockResolvedValue(undefined),
            restock: vi.fn().mockResolvedValue(undefined),
        });
        view.rerender(<App launchToken="test-token" />);

        expect(pet).toHaveAttribute("data-motion-facing", facing ?? "");
        expect(vi.getTimerCount()).toBe(timerCount);
    });

    it("disposes the mounted controller and clears timers on unmount", () => {
        renderApp({
            snapshot: snapshot({
                behavior: "Working",
                activity: { Active: "editing" },
            }),
            connectionStatus: "connected",
        });

        expect(vi.getTimerCount()).toBeGreaterThan(0);
        cleanup();
        expect(vi.getTimerCount()).toBe(0);
    });

    it("gates decorative effects on the active motion action", () => {
        renderApp({
            snapshot: snapshot({
                behavior: "Working",
                activity: { Active: "editing" },
            }),
            connectionStatus: "connected",
        });
        const editingPet = screen.getByTestId("pet");
        expect(screen.queryByTestId("typing-marks")).not.toBeInTheDocument();
        act(() => vi.advanceTimersByTime(999));
        expect(editingPet).toHaveAttribute("data-motion-action", "type");
        expect(screen.getByTestId("typing-marks")).toHaveAttribute(
            "aria-hidden",
            "true",
        );

        cleanup();
        renderApp({
            snapshot: snapshot({
                behavior: "Working",
                activity: { Active: "thinking" },
            }),
            connectionStatus: "connected",
        });
        expect(screen.queryByTestId("thought-bubbles")).not.toBeInTheDocument();
        act(() => vi.advanceTimersByTime(999));
        expect(screen.getByTestId("thought-bubbles")).toHaveAttribute(
            "aria-hidden",
            "true",
        );

        cleanup();
        renderApp({
            snapshot: snapshot({
                behavior: "RecentSuccess",
                recentOutcome: "Success",
                lastOutcomeAt: "2026-08-05T12:00:00Z",
            }),
            connectionStatus: "connected",
        });
        vi.advanceTimersByTime(999);
        expect(screen.getByTestId("motion-sparkles")).toHaveAttribute(
            "aria-hidden",
            "true",
        );
    });
});
