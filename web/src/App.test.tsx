import { cleanup, fireEvent, render, screen } from "@testing-library/react";
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
        inventory: { kibble: 50, treat: 25, fruit: 25 },
        processedCareIds: [],
        poopSequence: 1,
        sessionActivities: {},
        processedEventIds: [],
        lastActivityAt: null,
        lastOutcomeAt: null,
        consecutiveFailures: 0,
        enforcementMode: "decorative",
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
        feed: vi.fn().mockResolvedValue(undefined),
        clean: vi.fn().mockResolvedValue(undefined),
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
