import type {
    Effect,
    Facing,
    MotionClock,
    MotionPhase,
    MotionTimerPort,
    TransientVisualAction,
} from "./motion";

/** A room location expressed as a percentage-friendly 0..1 pair. */
export interface RelativeRoomPosition {
    x: number;
    y: number;
}

export type RoomRegion =
    | "window"
    | "shelf"
    | "floor-left"
    | "floor-center"
    | "floor-right"
    | "furniture";

export type FreeTimeInteraction =
    "watch_window" | "inspect_shelf" | "circle_furniture" | "sit";

export type IdleInteraction = FreeTimeInteraction;

/** A safe target the future DOM layer can turn into CSS coordinates. */
export interface RoomWaypoint {
    id: string;
    position: RelativeRoomPosition;
    /** Convenience aliases for integrations that do not want to unpack position. */
    x: number;
    y: number;
    region: RoomRegion;
    interaction: FreeTimeInteraction | null;
}

export type RoomHitZoneKind = "care-control" | "poop";

export interface RoomHitZone {
    id: string;
    kind: RoomHitZoneKind;
    left: number;
    top: number;
    width: number;
    height: number;
}

/**
 * Approximate normalized hit areas reserved by the room controls. Poop is
 * generated dynamically in this bounded floor area, so free-time targets keep
 * clear of the complete area rather than a particular poop instance.
 */
export const ROOM_FORBIDDEN_HIT_ZONES: readonly RoomHitZone[] = [
    {
        id: "feed-control",
        kind: "care-control",
        left: 0.26,
        top: 0.7,
        width: 0.27,
        height: 0.2,
    },
    {
        id: "poop-hit-area",
        kind: "poop",
        left: 0.57,
        top: 0.7,
        width: 0.25,
        height: 0.22,
    },
    {
        id: "shovel-control",
        kind: "care-control",
        left: 0.77,
        top: 0.69,
        width: 0.2,
        height: 0.23,
    },
    {
        id: "trash-control",
        kind: "care-control",
        left: 0.84,
        top: 0.81,
        width: 0.16,
        height: 0.19,
    },
] as const;

/** Stable, room-relative targets shared by every deterministic choreography. */
export const SAFE_ROOM_WAYPOINTS: readonly RoomWaypoint[] = [
    {
        id: "window-sill",
        position: { x: 0.14, y: 0.76 },
        x: 0.14,
        y: 0.76,
        region: "window",
        interaction: "watch_window",
    },
    {
        id: "shelf-front",
        position: { x: 0.55, y: 0.74 },
        x: 0.55,
        y: 0.74,
        region: "shelf",
        interaction: "inspect_shelf",
    },
    {
        id: "floor-left",
        position: { x: 0.18, y: 0.92 },
        x: 0.18,
        y: 0.92,
        region: "floor-left",
        interaction: null,
    },
    {
        id: "floor-center",
        position: { x: 0.55, y: 0.88 },
        x: 0.55,
        y: 0.88,
        region: "floor-center",
        interaction: null,
    },
    {
        id: "floor-right",
        position: { x: 0.72, y: 0.94 },
        x: 0.72,
        y: 0.94,
        region: "floor-right",
        interaction: null,
    },
    {
        id: "furniture-left",
        position: { x: 0.22, y: 0.84 },
        x: 0.22,
        y: 0.84,
        region: "furniture",
        interaction: "circle_furniture",
    },
    {
        id: "furniture-right",
        position: { x: 0.56, y: 0.94 },
        x: 0.56,
        y: 0.94,
        region: "furniture",
        interaction: "circle_furniture",
    },
] as const;

/** Aliases keep the room data discoverable for later adapters. */
export const ROOM_WAYPOINTS = SAFE_ROOM_WAYPOINTS;
export const FORBIDDEN_ROOM_HIT_ZONES = ROOM_FORBIDDEN_HIT_ZONES;

const FREE_TIME_INTERACTIONS: readonly FreeTimeInteraction[] = [
    "watch_window",
    "inspect_shelf",
    "circle_furniture",
    "sit",
];

export const DEFAULT_FREE_TIME_ROAM_DURATION_MS = 8_000;
export const DEFAULT_FREE_TIME_PAUSE_DURATION_MS = 2_000;
export const DEFAULT_FREE_TIME_TRAVEL_DURATION_MS = 240;
export const DEFAULT_FREE_TIME_ROLL_INTERVAL_MIN_MS = 15_000;
export const DEFAULT_FREE_TIME_ROLL_INTERVAL_MAX_MS = 30_000;
export const DEFAULT_FREE_TIME_ROLL_DURATION_MS = 800;
const MAX_ROOM_RATIO = 1;

const defaultTimers: MotionTimerPort = {
    setTimeout(callback, delayMs) {
        return globalThis.setTimeout(callback, delayMs);
    },
    clearTimeout(handle) {
        globalThis.clearTimeout(
            handle as ReturnType<typeof globalThis.setTimeout>,
        );
    },
};

function readReducedMotion(
    reducedMotion: boolean | (() => boolean) | undefined,
): boolean {
    return typeof reducedMotion === "function"
        ? reducedMotion()
        : reducedMotion === true;
}

function boundedRandom(random: () => number): number {
    const value = random();
    if (!Number.isFinite(value)) {
        return 0;
    }
    return Math.max(0, Math.min(0.999999999, value));
}

function boundedDuration(value: number | undefined, fallback: number): number {
    if (value === undefined || !Number.isFinite(value)) {
        return fallback;
    }
    return Math.max(0, value);
}

function boundedInterval(value: number | undefined, fallback: number): number {
    if (value === undefined || !Number.isFinite(value)) {
        return fallback;
    }
    return Math.max(1, value);
}

function waypointPosition(waypoint: RoomWaypoint): RelativeRoomPosition {
    return {
        x: waypoint.position.x,
        y: waypoint.position.y,
    };
}

function pointInZone(
    position: RelativeRoomPosition,
    zone: RoomHitZone,
): boolean {
    return (
        position.x >= zone.left &&
        position.x <= zone.left + zone.width &&
        position.y >= zone.top &&
        position.y <= zone.top + zone.height
    );
}

/** Whether a waypoint is outside every care-control and dynamic-poop area. */
export function isSafeRoomWaypoint(
    waypoint: RoomWaypoint,
    forbiddenZones: readonly RoomHitZone[] = ROOM_FORBIDDEN_HIT_ZONES,
): boolean {
    const { x, y } = waypointPosition(waypoint);
    return (
        Number.isFinite(x) &&
        Number.isFinite(y) &&
        x >= 0 &&
        x <= MAX_ROOM_RATIO &&
        y >= 0 &&
        y <= MAX_ROOM_RATIO &&
        forbiddenZones.every((zone) => !pointInZone({ x, y }, zone))
    );
}

export const isSafeWaypoint = isSafeRoomWaypoint;

/**
 * Selects a safe target without coupling the choreography to layout or DOM
 * measurements. A previous target is avoided when there is another choice.
 */
export function selectSafeRoomWaypoint(
    random: () => number,
    waypoints: readonly RoomWaypoint[] = SAFE_ROOM_WAYPOINTS,
    previousId?: string,
): RoomWaypoint {
    const safe = waypoints.filter((waypoint) => isSafeRoomWaypoint(waypoint));
    const candidates =
        safe.length > 1 && previousId !== undefined
            ? safe.filter((waypoint) => waypoint.id !== previousId)
            : safe;
    if (candidates.length === 0) {
        throw new Error("free-time choreography requires a safe waypoint");
    }
    const index = Math.floor(boundedRandom(random) * candidates.length);
    return candidates[index] ?? candidates[0];
}

export const chooseSafeWaypoint = selectSafeRoomWaypoint;

/** Selects an interaction using one injected deterministic random sample. */
export function selectFreeTimeInteraction(
    random: () => number,
): FreeTimeInteraction {
    const index = Math.floor(
        boundedRandom(random) * FREE_TIME_INTERACTIONS.length,
    );
    return FREE_TIME_INTERACTIONS[index] ?? FREE_TIME_INTERACTIONS[0];
}

export const chooseFreeTimeInteraction = selectFreeTimeInteraction;

function targetForInteraction(
    interaction: FreeTimeInteraction,
    random: () => number,
    waypoints: readonly RoomWaypoint[],
    previousId: string | undefined,
): RoomWaypoint {
    if (interaction === "sit") {
        const floor = waypoints.filter(
            (waypoint) => waypoint.interaction === null,
        );
        return selectSafeRoomWaypoint(
            random,
            floor.length > 0 ? floor : waypoints,
            previousId,
        );
    }

    const matching = waypoints.filter(
        (waypoint) => waypoint.interaction === interaction,
    );
    if (matching.length > 0) {
        return selectSafeRoomWaypoint(random, matching, previousId);
    }
    return selectSafeRoomWaypoint(random, waypoints, previousId);
}

function actionForInteraction(
    interaction: FreeTimeInteraction | null,
): TransientVisualAction {
    switch (interaction) {
        case "watch_window":
            return "watch_window";
        case "inspect_shelf":
            return "inspect_shelf";
        case "circle_furniture":
            return "circle_furniture";
        case "sit":
            return "sit";
        default:
            return "wander";
    }
}

export interface FreeTimeChoreographyState {
    waypoint: RoomWaypoint;
    facing: Facing;
    action: TransientVisualAction | null;
    phase: MotionPhase;
    effect: Effect;
    interaction: FreeTimeInteraction | null;
    /** True for the intentional 20% pause segment, not merely a sit action. */
    paused: boolean;
    rolling: boolean;
    sequence: number;
}

export interface FreeTimeChoreographyOptions {
    timer?: MotionTimerPort;
    timers?: MotionTimerPort;
    clock?: MotionClock;
    random?: () => number;
    randomness?: () => number;
    reducedMotion?: boolean | (() => boolean);
    waypoints?: readonly RoomWaypoint[];
    roamDurationMs?: number;
    pauseDurationMs?: number;
    travelDurationMs?: number;
    rollIntervalMinMs?: number;
    rollIntervalMaxMs?: number;
    rollDurationMs?: number;
    onState?: (state: FreeTimeChoreographyState) => void;
}

export type FreeTimeChoreographyListener = (
    state: FreeTimeChoreographyState,
) => void;

function initialChoreographyState(
    waypoints: readonly RoomWaypoint[],
): FreeTimeChoreographyState {
    const waypoint = waypoints[0] ?? SAFE_ROOM_WAYPOINTS[0];
    if (!waypoint) {
        throw new Error("free-time choreography requires a safe waypoint");
    }
    return {
        waypoint,
        facing: "right",
        action: null,
        phase: "static",
        effect: "none",
        interaction: null,
        paused: false,
        rolling: false,
        sequence: 0,
    };
}

/**
 * Timer-driven but DOM-free free-time presentation choreography. The class is
 * intentionally independent from snapshots; MotionController owns whether it
 * is currently allowed to run.
 */
export class FreeTimeChoreography {
    private readonly timers: MotionTimerPort;
    private readonly clock: MotionClock | undefined;
    private readonly random: () => number;
    private readonly reducedMotion: boolean | (() => boolean) | undefined;
    private readonly waypoints: readonly RoomWaypoint[];
    private readonly roamDurationMs: number;
    private readonly pauseDurationMs: number;
    private readonly travelDurationMs: number;
    private readonly rollIntervalMinMs: number;
    private readonly rollIntervalMaxMs: number;
    private readonly rollDurationMs: number;
    private readonly onState:
        ((state: FreeTimeChoreographyState) => void) | undefined;
    private readonly listeners = new Set<FreeTimeChoreographyListener>();
    private readonly scheduledTimers = new Set<unknown>();
    private state: FreeTimeChoreographyState;
    private baseState: FreeTimeChoreographyState;
    private running = false;
    private disposed = false;
    private reducedActive = false;
    private rolling = false;

    constructor(options: FreeTimeChoreographyOptions = {}) {
        this.timers = options.timer ?? options.timers ?? defaultTimers;
        this.clock = options.clock;
        this.random = options.random ?? options.randomness ?? Math.random;
        this.reducedMotion = options.reducedMotion;
        this.waypoints = (options.waypoints ?? SAFE_ROOM_WAYPOINTS).filter(
            (waypoint) => isSafeRoomWaypoint(waypoint),
        );
        if (this.waypoints.length === 0) {
            throw new Error("free-time choreography requires a safe waypoint");
        }
        this.roamDurationMs = boundedDuration(
            options.roamDurationMs,
            DEFAULT_FREE_TIME_ROAM_DURATION_MS,
        );
        this.pauseDurationMs = boundedDuration(
            options.pauseDurationMs,
            DEFAULT_FREE_TIME_PAUSE_DURATION_MS,
        );
        this.travelDurationMs = boundedDuration(
            options.travelDurationMs,
            DEFAULT_FREE_TIME_TRAVEL_DURATION_MS,
        );
        this.rollIntervalMinMs = boundedInterval(
            options.rollIntervalMinMs,
            DEFAULT_FREE_TIME_ROLL_INTERVAL_MIN_MS,
        );
        this.rollIntervalMaxMs = Math.max(
            this.rollIntervalMinMs,
            boundedInterval(
                options.rollIntervalMaxMs,
                DEFAULT_FREE_TIME_ROLL_INTERVAL_MAX_MS,
            ),
        );
        this.rollDurationMs = boundedDuration(
            options.rollDurationMs,
            DEFAULT_FREE_TIME_ROLL_DURATION_MS,
        );
        this.onState = options.onState;
        this.state = initialChoreographyState(this.waypoints);
        this.baseState = this.state;
    }

    getState(): FreeTimeChoreographyState {
        return this.state;
    }

    isRunning(): boolean {
        return this.running;
    }

    subscribe(listener: FreeTimeChoreographyListener): () => void {
        if (this.disposed) {
            return () => undefined;
        }
        this.listeners.add(listener);
        return () => {
            this.listeners.delete(listener);
        };
    }

    start(): void {
        if (this.disposed || this.running) {
            return;
        }
        this.running = true;
        this.reducedActive = readReducedMotion(this.reducedMotion);
        this.rolling = false;
        this.clearScheduledTimers();

        if (this.reducedActive) {
            this.setBaseState({
                ...this.state,
                phase: "static",
                action: null,
                effect: "none",
                interaction: null,
                paused: false,
                rolling: false,
            });
            return;
        }

        this.beginRoam();
        if (this.disposed || !this.running) {
            return;
        }
        this.scheduleRoll();
    }

    /** Refreshes a dynamic prefers-reduced-motion source without a snapshot. */
    refresh(): void {
        if (this.disposed || !this.running) {
            return;
        }
        const reduced = readReducedMotion(this.reducedMotion);
        if (reduced === this.reducedActive) {
            return;
        }
        this.reducedActive = reduced;
        this.clearScheduledTimers();
        this.rolling = false;
        if (reduced) {
            this.setBaseState({
                ...this.state,
                phase: "static",
                action: null,
                effect: "none",
                paused: false,
                rolling: false,
            });
            return;
        }
        this.beginRoam();
        if (this.disposed || !this.running) {
            return;
        }
        this.scheduleRoll();
    }

    /** Stops and resets presentation timers; it can be started again. */
    stop(): void {
        if (this.disposed) {
            return;
        }
        this.running = false;
        this.rolling = false;
        this.clearScheduledTimers();
        this.setBaseState(
            {
                ...this.state,
                phase: "static",
                action: null,
                effect: "none",
                interaction: null,
                paused: false,
                rolling: false,
            },
            false,
        );
    }

    dispose(): void {
        if (this.disposed) {
            return;
        }
        this.disposed = true;
        this.running = false;
        this.rolling = false;
        this.clearScheduledTimers();
        this.listeners.clear();
    }

    private beginRoam(): void {
        if (this.disposed || !this.running || this.reducedActive) {
            return;
        }
        const interaction = selectFreeTimeInteraction(this.random);
        const previousId = this.baseState.waypoint.id;
        const waypoint = targetForInteraction(
            interaction,
            this.random,
            this.waypoints,
            previousId,
        );
        const facing: Facing =
            boundedRandom(this.random) < 0.5 ? "left" : "right";
        const sequence = this.baseState.sequence + 1;
        this.setBaseState({
            waypoint,
            facing,
            action: "wander",
            phase: "traveling",
            effect: "none",
            interaction: interaction === "sit" ? null : interaction,
            paused: false,
            rolling: false,
            sequence,
        });
        if (this.disposed || !this.running || this.reducedActive) {
            return;
        }
        this.scheduleTravel(sequence, waypoint, interaction);
        this.scheduleRoamEnd(sequence);
    }

    private scheduleTravel(
        sequence: number,
        waypoint: RoomWaypoint,
        interaction: FreeTimeInteraction,
    ): void {
        const delay = Math.min(this.travelDurationMs, this.roamDurationMs);
        this.schedule(delay, () => {
            if (
                this.disposed ||
                !this.running ||
                this.reducedActive ||
                this.baseState.sequence !== sequence ||
                this.baseState.waypoint.id !== waypoint.id
            ) {
                return;
            }
            this.setBaseState({
                ...this.baseState,
                action: actionForInteraction(interaction),
                phase: "acting",
            });
        });
    }

    private scheduleRoamEnd(sequence: number): void {
        this.schedule(this.roamDurationMs, () => {
            if (
                this.disposed ||
                !this.running ||
                this.reducedActive ||
                this.baseState.sequence !== sequence
            ) {
                return;
            }
            this.enterPause(sequence);
        });
    }

    private enterPause(sequence: number): void {
        const pausedState: FreeTimeChoreographyState = {
            ...this.baseState,
            action: "sit",
            phase: "acting",
            effect: "none",
            interaction: "sit",
            paused: true,
            rolling: false,
            sequence,
        };
        this.setBaseState(pausedState);
        if (this.disposed || !this.running || this.reducedActive) {
            return;
        }
        this.schedule(this.pauseDurationMs, () => {
            if (
                this.disposed ||
                !this.running ||
                this.reducedActive ||
                this.baseState.sequence !== sequence
            ) {
                return;
            }
            this.beginRoam();
        });
    }

    private scheduleRoll(): void {
        const interval = Math.round(
            this.rollIntervalMinMs +
                boundedRandom(this.random) *
                    (this.rollIntervalMaxMs - this.rollIntervalMinMs),
        );
        this.schedule(interval, () => {
            if (this.disposed || !this.running || this.reducedActive) {
                return;
            }
            this.beginRoll();
            this.scheduleRoll();
        });
    }

    private beginRoll(): void {
        if (this.disposed || !this.running || this.reducedActive) {
            return;
        }
        this.rolling = true;
        this.state = {
            ...this.baseState,
            action: "roll",
            phase: "acting",
            effect: "none",
            rolling: true,
        };
        this.notify();
        if (this.disposed || !this.running || this.reducedActive) {
            return;
        }
        this.schedule(this.rollDurationMs, () => {
            if (this.disposed || !this.running || this.reducedActive) {
                return;
            }
            this.rolling = false;
            this.state = {
                ...this.baseState,
                rolling: false,
            };
            this.notify();
        });
    }

    private setBaseState(
        nextState: FreeTimeChoreographyState,
        notify = true,
    ): void {
        this.baseState = nextState;
        if (!this.rolling) {
            this.state = nextState;
            if (notify) {
                this.notify();
            }
        }
    }

    private schedule(delayMs: number, callback: () => void): void {
        const handle = this.timers.setTimeout(() => {
            this.scheduledTimers.delete(handle);
            callback();
        }, delayMs);
        this.scheduledTimers.add(handle);
    }

    private clearScheduledTimers(): void {
        for (const handle of this.scheduledTimers) {
            this.timers.clearTimeout(handle);
        }
        this.scheduledTimers.clear();
    }

    private notify(): void {
        if (this.disposed) {
            return;
        }
        this.onState?.(this.state);
        if (this.disposed) {
            return;
        }
        for (const listener of [...this.listeners]) {
            if (this.disposed) {
                return;
            }
            listener(this.state);
        }
    }
}

export function createFreeTimeChoreography(
    options: FreeTimeChoreographyOptions = {},
): FreeTimeChoreography {
    return new FreeTimeChoreography(options);
}

export const createIdleChoreography = createFreeTimeChoreography;

/** CSS-ready values, leaving actual DOM/style ownership to package 3. */
export function cssPositionForWaypoint(waypoint: RoomWaypoint): {
    left: string;
    top: string;
} {
    return {
        left: `${waypoint.position.x * 100}%`,
        top: `${waypoint.position.y * 100}%`,
    };
}

export const roomWaypointToCssPosition = cssPositionForWaypoint;
