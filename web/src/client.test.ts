import { beforeEach, describe, expect, it, vi } from "vitest";

import {
    CodeGotchiClient,
    extractLaunchToken,
    type ClientStatus,
} from "./client";
import type { SimulationSnapshot } from "./protocol";

function snapshot(name: string): SimulationSnapshot {
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
    };
}

class FakeWebSocket {
    static instances: FakeWebSocket[] = [];
    readonly url: string;
    readonly protocol: string;
    onopen: (() => void) | null = null;
    onmessage: ((event: { data: string }) => void) | null = null;
    onerror: (() => void) | null = null;
    onclose: (() => void) | null = null;

    constructor(url: string, protocol?: string | string[]) {
        this.url = url;
        this.protocol = typeof protocol === "string" ? protocol : "";
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

describe("CodeGotchi browser client", () => {
    beforeEach(() => {
        vi.restoreAllMocks();
        vi.useRealTimers();
        FakeWebSocket.instances = [];
        window.history.replaceState({}, "", "/");
    });

    it("extracts the launch token and removes the fragment immediately", () => {
        window.history.replaceState({}, "", "/#token=fragment-secret");
        const replaceState = vi.spyOn(window.history, "replaceState");

        expect(extractLaunchToken()).toBe("fragment-secret");
        expect(window.location.hash).toBe("");
        expect(replaceState).toHaveBeenCalledWith(expect.anything(), "", "/");
    });

    it("uses bearer headers and one UUID action id for feed and clean", async () => {
        const fetch = vi.fn().mockResolvedValue({
            ok: true,
            json: async () => ({ ...snapshot("Mochi"), duplicate: false }),
        });
        const randomUUID = vi
            .spyOn(crypto, "randomUUID")
            .mockReturnValue("00000000-0000-0000-0000-000000000123");
        const client = new CodeGotchiClient("care-secret", {
            fetch,
            WebSocket: FakeWebSocket,
            baseUrl: "http://127.0.0.1:4242",
        });

        await client.feed("kibble");
        await client.clean("00000000-0000-0000-0000-000000000099");

        expect(randomUUID).toHaveBeenCalledTimes(2);
        expect(fetch).toHaveBeenNthCalledWith(
            1,
            "http://127.0.0.1:4242/api/v1/care/feed",
            expect.objectContaining({
                method: "POST",
                headers: expect.objectContaining({
                    Authorization: "Bearer care-secret",
                    "Content-Type": "application/json",
                }),
                body: JSON.stringify({
                    actionId: "00000000-0000-0000-0000-000000000123",
                    foodId: "kibble",
                }),
            }),
        );
        expect(fetch).toHaveBeenNthCalledWith(
            2,
            "http://127.0.0.1:4242/api/v1/care/clean",
            expect.objectContaining({
                headers: expect.objectContaining({
                    Authorization: "Bearer care-secret",
                }),
                body: JSON.stringify({
                    actionId: "00000000-0000-0000-0000-000000000123",
                    poopId: "00000000-0000-0000-0000-000000000099",
                }),
            }),
        );
    });

    it("loads a complete snapshot, reconnects with bounded retry, and replaces state", async () => {
        vi.useFakeTimers();
        const initial = snapshot("HTTP state");
        const reconnected = snapshot("authoritative reconnect");
        const fetch = vi.fn().mockResolvedValue({
            ok: true,
            json: async () => initial,
        });
        const statuses: ClientStatus[] = [];
        const snapshots: SimulationSnapshot[] = [];
        const client = new CodeGotchiClient("stream-secret", {
            fetch,
            WebSocket: FakeWebSocket,
            baseUrl: "http://127.0.0.1:4242",
            reconnectDelaysMs: [100, 200],
        });

        client.start({
            onStatus: (status) => statuses.push(status),
            onSnapshot: (value) => snapshots.push(value),
        });
        await vi.runAllTicks();

        expect(fetch).toHaveBeenCalledWith(
            "http://127.0.0.1:4242/api/v1/state",
            expect.objectContaining({
                headers: { Authorization: "Bearer stream-secret" },
            }),
        );
        expect(FakeWebSocket.instances).toHaveLength(1);
        expect(FakeWebSocket.instances[0]?.protocol).toBe("stream-secret");

        FakeWebSocket.instances[0]?.open();
        FakeWebSocket.instances[0]?.message(initial);
        FakeWebSocket.instances[0]?.close();
        expect(statuses).toContain("reconnecting");

        await vi.advanceTimersByTimeAsync(100);
        expect(FakeWebSocket.instances).toHaveLength(2);
        FakeWebSocket.instances[1]?.open();
        FakeWebSocket.instances[1]?.message(reconnected);

        expect(snapshots.at(-1)).toEqual(reconnected);
        expect(statuses.at(-1)).toBe("connected");
        client.close();
    });

    it("ends in disconnected after the bounded retry budget", async () => {
        vi.useFakeTimers();
        const fetch = vi.fn().mockResolvedValue({
            ok: true,
            json: async () => snapshot("Mochi"),
        });
        const statuses: ClientStatus[] = [];
        const client = new CodeGotchiClient("stream-secret", {
            fetch,
            WebSocket: FakeWebSocket,
            baseUrl: "http://127.0.0.1:4242",
            reconnectDelaysMs: [10, 20],
        });
        client.start({ onStatus: (status) => statuses.push(status) });
        await vi.runAllTicks();

        for (let attempt = 0; attempt < 3; attempt += 1) {
            FakeWebSocket.instances.at(-1)?.close();
            await vi.runAllTimersAsync();
        }

        expect(statuses.at(-1)).toBe("disconnected");
        client.close();
    });
});
