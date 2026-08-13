import type {
    CareResponse,
    DebugStatusResponse,
    ErrorEnvelope,
    FoodId,
    SimulationSnapshot,
} from "./protocol";

export type ClientStatus =
    "loading" | "connecting" | "connected" | "reconnecting" | "disconnected";

export interface ClientError {
    code: string;
    message: string;
    status?: number;
}

export interface WebSocketLike {
    onopen: ((event: Event) => void) | null;
    onmessage: ((event: MessageEvent<string>) => void) | null;
    onerror: ((event: Event) => void) | null;
    onclose: ((event: CloseEvent) => void) | null;
    close(code?: number, reason?: string): void;
}

export type WebSocketFactory = new (
    url: string,
    protocols?: string | string[],
) => WebSocketLike;

export interface ClientOptions {
    fetch?: typeof globalThis.fetch;
    WebSocket?: WebSocketFactory;
    baseUrl?: string;
    reconnectDelaysMs?: number[];
}

export interface ClientObserver {
    onSnapshot?: (snapshot: SimulationSnapshot) => void;
    onStatus?: (status: ClientStatus) => void;
    onError?: (error: ClientError) => void;
}

const DEFAULT_RECONNECT_DELAYS_MS = [250, 500, 1_000, 2_000, 4_000];
const HISTORY_TOKEN_KEY = "__codeGotchiLaunchToken";

interface LaunchHistoryState extends Record<string, unknown> {
    [HISTORY_TOKEN_KEY]?: string;
}

/**
 * Reads the one-time launch fragment and removes it from the visible URL.
 * The token is retained in session history state so a same-tab reload can
 * authenticate again without putting the credential in a URL or localStorage.
 */
export function extractLaunchToken(
    location: Location = window.location,
    history: History = window.history,
): string | null {
    const hash = location.hash.startsWith("#")
        ? location.hash.slice(1)
        : location.hash;
    const token = new URLSearchParams(hash).get("token");
    if (token) {
        const currentState: LaunchHistoryState =
            history.state && typeof history.state === "object"
                ? { ...(history.state as Record<string, unknown>) }
                : {};
        history.replaceState(
            { ...currentState, [HISTORY_TOKEN_KEY]: token },
            "",
            `${location.pathname}${location.search}`,
        );
        return token;
    }

    const storedToken =
        history.state && typeof history.state === "object"
            ? (history.state as LaunchHistoryState)[HISTORY_TOKEN_KEY]
            : undefined;
    return typeof storedToken === "string" && storedToken.length > 0
        ? storedToken
        : null;
}

export class CodeGotchiClient {
    private readonly token: string;
    private readonly fetcher: typeof globalThis.fetch;
    private readonly webSocket: WebSocketFactory;
    private readonly baseUrl: string;
    private readonly reconnectDelaysMs: number[];
    private observer: ClientObserver = {};
    private socket: WebSocketLike | null = null;
    private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    private reconnectAttempt = 0;
    private stopped = true;
    private latestSnapshot: SimulationSnapshot | null = null;

    public constructor(token: string, options: ClientOptions = {}) {
        this.token = token;
        this.fetcher = options.fetch ?? globalThis.fetch.bind(globalThis);
        this.webSocket = options.WebSocket ?? WebSocket;
        this.baseUrl = options.baseUrl ?? window.location.origin;
        this.reconnectDelaysMs =
            options.reconnectDelaysMs ?? DEFAULT_RECONNECT_DELAYS_MS;
    }

    public start(observer: ClientObserver = {}): () => void {
        this.observer = observer;
        this.stopped = false;
        this.reconnectAttempt = 0;
        this.notifyStatus("loading");
        void this.loadInitialSnapshot();
        this.connectWebSocket();
        return () => this.close();
    }

    public close(): void {
        this.stopped = true;
        if (this.reconnectTimer) {
            clearTimeout(this.reconnectTimer);
            this.reconnectTimer = null;
        }
        const socket = this.socket;
        this.socket = null;
        socket?.close(1000, "client stopped");
    }

    /** Development-only recovery seam used by the focused browser test. */
    public disconnectForTest(): void {
        this.socket?.close(1000, "test disconnect");
    }

    public async feed(
        foodId: FoodId,
        actionId: string = createActionId(),
    ): Promise<CareResponse> {
        return this.care("feed", { actionId, foodId });
    }

    public async clean(
        poopId: string,
        actionId: string = createActionId(),
    ): Promise<CareResponse> {
        return this.care("clean", { actionId, poopId });
    }

    public async nap(
        actionId: string = createActionId(),
    ): Promise<CareResponse> {
        return this.care("nap", { actionId });
    }

    public async pet(
        interactionMs: number,
        pointerDistance: number,
        actionId: string = createActionId(),
    ): Promise<CareResponse> {
        return this.care("pet", {
            actionId,
            interactionMs,
            pointerDistance,
        });
    }

    /** Whether the owning runtime was launched with the guarded demo controls. */
    public async debugStatus(): Promise<DebugStatusResponse> {
        return this.request<DebugStatusResponse>("/api/v1/debug/status", {
            method: "GET",
        });
    }

    /**
     * Fixed demo control: restore the starter pantry through the guarded
     * debug route. Only shown when the runtime reports debug enabled.
     */
    public async restock(): Promise<CareResponse> {
        const response = await this.request<CareResponse>(
            "/api/v1/debug/restock",
            {
                method: "POST",
                headers: {
                    "Content-Type": "application/json",
                    "X-CodeGotchi-Debug": "1",
                },
                body: "{}",
            },
        );
        this.publishSnapshot(response);
        return response;
    }

    private async loadInitialSnapshot(): Promise<void> {
        try {
            const snapshot = await this.request<SimulationSnapshot>(
                "/api/v1/state",
                { method: "GET" },
            );
            if (!this.stopped) {
                this.publishSnapshot(snapshot);
            }
        } catch (error) {
            if (!this.stopped) {
                this.notifyError(asClientError(error));
            }
        }
    }

    private connectWebSocket(): void {
        if (this.stopped) {
            return;
        }

        this.notifyStatus(
            this.reconnectAttempt === 0 ? "connecting" : "reconnecting",
        );
        let socket: WebSocketLike;
        try {
            socket = new this.webSocket(this.websocketUrl(), this.token);
        } catch (error) {
            this.notifyError({
                code: "stream_unavailable",
                message: asClientError(error).message,
            });
            this.scheduleReconnect();
            return;
        }

        this.socket = socket;
        socket.onopen = () => {
            if (socket !== this.socket || this.stopped) {
                return;
            }
            this.notifyStatus("connected");
        };
        socket.onmessage = (event) => {
            if (socket !== this.socket || this.stopped) {
                return;
            }
            try {
                const snapshot = JSON.parse(event.data) as SimulationSnapshot;
                this.reconnectAttempt = 0;
                this.publishSnapshot(snapshot);
                this.notifyStatus("connected");
            } catch (error) {
                this.notifyError({
                    code: "invalid_snapshot",
                    message: `The live room sent invalid state: ${asClientError(error).message}`,
                });
            }
        };
        socket.onerror = () => {
            if (socket === this.socket && !this.stopped) {
                this.notifyError({
                    code: "stream_unavailable",
                    message: "The live room stream is unavailable.",
                });
            }
        };
        socket.onclose = () => {
            if (socket !== this.socket || this.stopped) {
                return;
            }
            this.socket = null;
            this.scheduleReconnect();
        };
    }

    private scheduleReconnect(): void {
        if (this.stopped || this.reconnectTimer) {
            return;
        }
        const delay = this.reconnectDelaysMs[this.reconnectAttempt];
        if (delay === undefined) {
            this.notifyStatus("disconnected");
            return;
        }
        this.reconnectAttempt += 1;
        this.notifyStatus("reconnecting");
        this.reconnectTimer = setTimeout(() => {
            this.reconnectTimer = null;
            this.connectWebSocket();
        }, delay);
    }

    private async care(
        action: "feed" | "clean" | "nap" | "pet",
        body: Record<string, string | number>,
    ): Promise<CareResponse> {
        const response = await this.request<CareResponse>(
            `/api/v1/care/${action}`,
            {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(body),
            },
        );
        this.publishSnapshot(response);
        return response;
    }

    private async request<T>(path: string, init: RequestInit): Promise<T> {
        const response = await this.fetcher(this.httpUrl(path), {
            ...init,
            headers: {
                Authorization: `Bearer ${this.token}`,
                ...(init.headers ?? {}),
            },
        });
        const payload: unknown = await response.json().catch(() => null);
        if (!response.ok) {
            throw errorFromResponse(response.status, payload);
        }
        return payload as T;
    }

    private httpUrl(path: string): string {
        return new URL(path, this.baseUrl).toString();
    }

    private websocketUrl(): string {
        const url = new URL("/api/v1/stream", this.baseUrl);
        url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
        return url.toString();
    }

    private notifyStatus(status: ClientStatus): void {
        this.observer.onStatus?.(status);
    }

    private publishSnapshot(snapshot: SimulationSnapshot): boolean {
        if (!shouldAcceptSnapshot(this.latestSnapshot, snapshot)) {
            return false;
        }
        this.latestSnapshot = snapshot;
        this.observer.onSnapshot?.(snapshot);
        return true;
    }

    private notifyError(error: ClientError): void {
        this.observer.onError?.(error);
    }
}

function shouldAcceptSnapshot(
    current: SimulationSnapshot | null,
    incoming: SimulationSnapshot,
): boolean {
    if (!current) {
        return true;
    }

    const currentTime = snapshotTimestamp(current.lastUpdatedAt);
    const incomingTime = snapshotTimestamp(incoming.lastUpdatedAt);
    if (currentTime === null || incomingTime === null) {
        return false;
    }
    if (incomingTime > currentTime) {
        return true;
    }
    if (incomingTime < currentTime) {
        return false;
    }

    // The runtime serializes mutations under one lock. At this precision,
    // only replay sets, scheduler counters/deadline, and pending-object
    // continuation are ordered monotonically; care IDs are the explicit
    // authority for removals. Accept an equal-time snapshot only when it is
    // a strict extension of that server ordering, and reject incomparable
    // states instead of inventing an arbitrary payload ordering.
    const careIdsAdvance = continuationSetAdvances(
        current.processedCareIds,
        incoming.processedCareIds,
    );
    const eventIdsAdvance = continuationSetAdvances(
        current.processedEventIds,
        incoming.processedEventIds,
    );
    const continuationSetsContainCurrent =
        containsAll(current.processedCareIds, incoming.processedCareIds) &&
        containsAll(current.processedEventIds, incoming.processedEventIds);
    if (
        !continuationSetsContainCurrent ||
        incoming.poopSequence < current.poopSequence ||
        incoming.attentionSequence < current.attentionSequence
    ) {
        return false;
    }

    const schedulerOrder = compareSnapshotTimestamps(
        current.nextIncidentAt,
        incoming.nextIncidentAt,
    );
    if (schedulerOrder === null || schedulerOrder < 0) {
        return false;
    }

    const currentDemandIds = current.pendingDemands.map((demand) => demand.id);
    const incomingDemandIds = incoming.pendingDemands.map(
        (demand) => demand.id,
    );
    const currentPoopIds = current.pendingPoops.map((poop) => poop.id);
    const incomingPoopIds = incoming.pendingPoops.map((poop) => poop.id);
    const demandsContainCurrent = containsAll(
        currentDemandIds,
        incomingDemandIds,
    );
    const poopsContainCurrent = containsAll(currentPoopIds, incomingPoopIds);
    if (
        (!careIdsAdvance && !demandsContainCurrent) ||
        (!careIdsAdvance && !poopsContainCurrent)
    ) {
        return false;
    }

    const demandIdsAdvance = continuationSetAdvances(
        currentDemandIds,
        incomingDemandIds,
    );
    const poopIdsAdvance = continuationSetAdvances(
        currentPoopIds,
        incomingPoopIds,
    );
    const poopSequenceAdvance = incoming.poopSequence > current.poopSequence;
    const attentionSequenceAdvance =
        incoming.attentionSequence > current.attentionSequence;
    const schedulerAdvances = schedulerOrder > 0;

    return (
        careIdsAdvance ||
        eventIdsAdvance ||
        poopSequenceAdvance ||
        attentionSequenceAdvance ||
        schedulerAdvances ||
        demandIdsAdvance ||
        poopIdsAdvance
    );
}

function continuationSetAdvances(
    current: string[],
    incoming: string[],
): boolean {
    const currentIds = new Set(current);
    const incomingIds = new Set(incoming);
    return incomingIds.size > currentIds.size && containsAll(current, incoming);
}

function containsAll(current: string[], incoming: string[]): boolean {
    const incomingIds = new Set(incoming);
    return current.every((id) => incomingIds.has(id));
}

function compareSnapshotTimestamps(
    current: string,
    incoming: string,
): number | null {
    const currentTime = snapshotTimestamp(current);
    const incomingTime = snapshotTimestamp(incoming);
    if (currentTime === null || incomingTime === null) {
        return null;
    }
    return incomingTime > currentTime ? 1 : incomingTime < currentTime ? -1 : 0;
}

/**
 * Date.parse intentionally exposes only milliseconds. Rust snapshots retain
 * sub-millisecond precision, so preserve their fractional RFC3339 component
 * before comparing snapshots that land in the same browser millisecond.
 */
function snapshotTimestamp(value: string): bigint | null {
    if (typeof value !== "string") {
        return null;
    }
    const fractionMatch = value.match(/\.(\d+)(?=(?:Z|[+-]\d{2}:\d{2})$)/);
    const base = fractionMatch
        ? value.slice(0, fractionMatch.index) +
          value.slice(fractionMatch.index! + fractionMatch[0].length)
        : value;
    const milliseconds = Date.parse(base);
    if (!Number.isFinite(milliseconds)) {
        return null;
    }
    const fraction = fractionMatch?.[1] ?? "";
    const nanoseconds = BigInt(fraction.slice(0, 9).padEnd(9, "0") || "0");
    return BigInt(milliseconds) * 1_000_000n + nanoseconds;
}

export function createActionId(): string {
    if (globalThis.crypto?.randomUUID) {
        return globalThis.crypto.randomUUID();
    }
    throw new Error("This browser does not provide crypto.randomUUID().");
}

function errorFromResponse(status: number, payload: unknown): ClientError {
    const envelope = payload as Partial<ErrorEnvelope> | null;
    return {
        code: envelope?.error?.code ?? "http_error",
        message:
            envelope?.error?.message ?? `The backend returned HTTP ${status}.`,
        status,
    };
}

function asClientError(error: unknown): ClientError {
    if (isClientError(error)) {
        return error;
    }
    return {
        code: "client_error",
        message: error instanceof Error ? error.message : String(error),
    };
}

function isClientError(error: unknown): error is ClientError {
    return (
        typeof error === "object" &&
        error !== null &&
        "code" in error &&
        "message" in error &&
        typeof error.code === "string" &&
        typeof error.message === "string"
    );
}
