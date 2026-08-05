import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useCodeGotchi } from "./useCodeGotchi";
import type { SimulationSnapshot } from "./protocol";

function snapshot(
    name: string,
    overrides: Partial<SimulationSnapshot> = {},
): SimulationSnapshot {
    return {
        schemaVersion: 1,
        petId: "00000000-0000-0000-0000-000000000001",
        name,
        species: "cat",
        needs: { hunger: 0, energy: 100, happiness: 100, cleanliness: 100 },
        behavior: "Wandering",
        activity: "Idle",
        recentOutcome: "None",
        workPoints: 0,
        digestionPoints: 0,
        lastUpdatedAt: "2026-08-05T12:00:00Z",
        pendingPoops: [],
        inventory: { kibble: 50, treat: 25, fruit: 25 },
        processedCareIds: [],
        poopSequence: 0,
        sessionActivities: {},
        processedEventIds: [],
        lastActivityAt: null,
        lastOutcomeAt: null,
        consecutiveFailures: 0,
        enforcementMode: "decorative",
        ...overrides,
    };
}

interface FakeResponse {
    ok: boolean;
    json: () => Promise<unknown>;
}

function responseFor(value: unknown): FakeResponse {
    return { ok: true, json: async () => value };
}

class FakeWebSocket {
    static instances: FakeWebSocket[] = [];
    onopen: (() => void) | null = null;
    onmessage: ((event: { data: string }) => void) | null = null;
    onerror: (() => void) | null = null;
    onclose: (() => void) | null = null;

    constructor() {
        FakeWebSocket.instances.push(this);
    }

    close(): void {
        this.onclose?.();
    }

    open(): void {
        this.onopen?.();
    }

    message(value: SimulationSnapshot): void {
        this.onmessage?.({ data: JSON.stringify(value) });
    }
}

describe("useCodeGotchi authoritative projection", () => {
    beforeEach(() => {
        FakeWebSocket.instances = [];
        vi.restoreAllMocks();
    });

    afterEach(() => {
        cleanup();
        vi.unstubAllGlobals();
    });

    it("does not let an older care response replace a newer stream snapshot", async () => {
        let resolveCare!: (response: FakeResponse) => void;
        const careRequest = new Promise<FakeResponse>((resolve) => {
            resolveCare = resolve;
        });
        const initial = snapshot("initial state");
        const streamed = snapshot("newer stream state", {
            lastUpdatedAt: "2026-08-05T12:00:02Z",
            processedEventIds: ["stream-event"],
        });
        const olderCare = snapshot("older care response", {
            lastUpdatedAt: "2026-08-05T12:00:01Z",
            processedCareIds: ["care-response"],
        });
        const fetch = vi.fn((input: unknown) => {
            if (String(input).endsWith("/api/v1/state")) {
                return Promise.resolve(responseFor(initial));
            }
            return careRequest;
        });
        vi.stubGlobal("fetch", fetch);
        vi.stubGlobal("WebSocket", FakeWebSocket);

        const { result, unmount } = renderHook(() =>
            useCodeGotchi("stream-secret"),
        );
        await act(async () => {
            await Promise.resolve();
            await Promise.resolve();
        });

        const socket = FakeWebSocket.instances[0];
        socket?.open();
        await act(async () => {
            socket?.message(streamed);
        });
        expect(result.current.snapshot).toEqual(streamed);

        let feedPromise: Promise<void>;
        await act(async () => {
            feedPromise = result.current.feed("kibble");
            await Promise.resolve();
        });
        resolveCare(responseFor({ ...olderCare, duplicate: false }));
        await act(async () => {
            await feedPromise;
        });

        expect(result.current.snapshot).toEqual(streamed);
        unmount();
    });
});
