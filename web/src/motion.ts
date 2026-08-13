import {
    activeActivity,
    isNapping,
    type ActivityKind,
    type SimulationSnapshot,
} from "./protocol";
import {
    FreeTimeChoreography,
    type FreeTimeChoreographyOptions,
    type FreeTimeChoreographyState,
    type FreeTimeInteraction,
    type RoomRegion,
    type RoomWaypoint,
} from "./choreography";

/** A semantic presentation mode derived from authoritative state. */
export type SemanticMode =
    | "desk"
    | "thinking"
    | "free_time"
    | "napping"
    | "critical"
    | "success"
    | "failure";

/** Named presentation destinations. Concrete coordinates belong to the room layer. */
export type Waypoint =
    | "desk"
    | "thinking"
    | "free_time"
    | "hammock"
    | "critical"
    | "success"
    | "failure";

/** A short-lived visual action; it never represents authoritative domain state. */
export type TransientVisualAction =
    | "roll"
    | "type"
    | "think"
    | "wander"
    | "watch_window"
    | "inspect_shelf"
    | "circle_furniture"
    | "sit"
    | "sleep"
    | "shake"
    | "celebrate"
    | "flinch"
    | "pulse";

export type Facing = "left" | "right";

export type MotionPhase = "static" | "traveling" | "acting";

export type Effect = "none" | "sparkle" | "zzz" | "warning";

export interface MotionState {
    semanticMode: SemanticMode;
    /** The destination is semantic; the DOM/room layer resolves its coordinates. */
    destination: Waypoint;
    waypoint: Waypoint;
    facing: Facing;
    phase: MotionPhase;
    action: TransientVisualAction | null;
    effect: Effect;
    /** Room-relative presentation data; the DOM package resolves CSS later. */
    roomWaypoint: RoomWaypoint | null;
    roomRegion: RoomRegion | null;
    interaction: FreeTimeInteraction | null;
    paused: boolean;
    /** Increments only when the semantic mode changes. */
    generation: number;
}

export interface MotionTimerPort {
    setTimeout(callback: () => void, delayMs: number): unknown;
    clearTimeout(handle: unknown): void;
}

export interface MotionClockPort {
    now(): number;
}

export type MotionClock = (() => number) | MotionClockPort;

export type WaypointResolver = (
    mode: SemanticMode,
    random: () => number,
) => Waypoint;

export interface MotionControllerOptions {
    /** Singular alias for integrations that expose one scheduler port. */
    timer?: MotionTimerPort;
    timers?: MotionTimerPort;
    clock?: MotionClock;
    random?: () => number;
    randomness?: () => number;
    reducedMotion?: boolean | (() => boolean);
    waypointForMode?: WaypointResolver;
    /** Travel is capped below one second so an authority change cannot lag. */
    travelDurationMs?: number;
    /** Delay between cosmetic effect pulses while a mode remains active. */
    loopDelayMs?: number;
    /** Optional free-time choreography tuning; ports stay owned by this controller. */
    freeTime?: Omit<
        FreeTimeChoreographyOptions,
        | "timer"
        | "timers"
        | "clock"
        | "random"
        | "randomness"
        | "reducedMotion"
        | "onState"
    >;
    /** Alias for integrations that name the package-2 layer explicitly. */
    freeTimeChoreography?: MotionControllerOptions["freeTime"];
    onFreeTimeState?: (state: FreeTimeChoreographyState) => void;
}

export type MotionListener = (state: MotionState) => void;

const MAX_TRAVEL_DURATION_MS = 999;
const DEFAULT_TRAVEL_DURATION_MS = 240;
const DEFAULT_LOOP_DELAY_MS = 1_000;
export const RECENT_OUTCOME_WINDOW_MS = 5 * 60 * 1_000;

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

/**
 * Converts the complete authoritative snapshot into a small semantic mode.
 *
 * No need, care, persistence, or protocol field is changed here. The order is
 * deliberately explicit because a cosmetic activity must never hide a higher
 * priority authoritative state.
 */
export function semanticModeForSnapshot(
    snapshot: SimulationSnapshot,
    now = Date.now(),
): SemanticMode {
    if (snapshot.behavior === "Sleeping" || isNapping(snapshot, now)) {
        return "napping";
    }

    const activityName = activeActivity(snapshot.activity) as string | null;
    if (
        snapshot.behavior === "CriticalNeed" ||
        snapshot.behavior === "Blocked" ||
        snapshot.activity === "Blocked" ||
        activityName === "blocked"
    ) {
        return "critical";
    }

    // A Working snapshot is authoritative over the cached outcome fields. The
    // server's behavior coordinator gives active work the same priority, so a
    // newly-started turn cannot keep showing an old success/failure pose.
    if (snapshot.behavior === "Working" && activityName !== null) {
        return semanticModeForActivity(activityName);
    }

    // Hook activity is also a stronger signal than a cosmetic outcome when it
    // is present. This keeps an active snapshot responsive even if behavior
    // was serialized just before the aggregate refresh.
    if (activityName !== null) {
        return semanticModeForActivity(activityName);
    }

    if (isRecentOutcome(snapshot, now)) {
        if (
            snapshot.behavior === "RecentSuccess" ||
            snapshot.recentOutcome === "Success"
        ) {
            return "success";
        }

        if (
            snapshot.behavior === "RecentFailure" ||
            snapshot.recentOutcome === "Failure"
        ) {
            return "failure";
        }
    }

    if (snapshot.activity === "WaitingForUser") {
        return "free_time";
    }
    return "free_time";
}

function semanticModeForActivity(activityName: string): SemanticMode {
    switch (activityName) {
        case "celebrating":
            return "success";
        case "error":
            return "failure";
        case "thinking":
        case "reading":
        case "searching":
        case "web_research":
            return "thinking";
        case "idle":
        case "waiting":
            return "free_time";
        case "unknown_work":
        case "editing":
        case "testing":
        case "building":
        case "installing":
        case "git_operation":
        case "docker_operation":
        case "blocked":
        default:
            return "desk";
    }
}

function isRecentOutcome(snapshot: SimulationSnapshot, now: number): boolean {
    if (
        snapshot.lastOutcomeAt === null ||
        (snapshot.recentOutcome === "None" &&
            snapshot.behavior !== "RecentSuccess" &&
            snapshot.behavior !== "RecentFailure")
    ) {
        return false;
    }
    const outcomeAt = Date.parse(snapshot.lastOutcomeAt);
    return (
        Number.isFinite(outcomeAt) &&
        outcomeAt <= now &&
        now - outcomeAt <= RECENT_OUTCOME_WINDOW_MS
    );
}

/** Alias phrased for consumers that prefer a mapper name. */
export const mapSnapshotToSemanticMode = semanticModeForSnapshot;

/** Alias phrased for consumers that prefer a `from` name. */
export const semanticModeFromSnapshot = semanticModeForSnapshot;

const defaultWaypointForMode: WaypointResolver = (mode) => {
    switch (mode) {
        case "napping":
            return "hammock";
        case "desk":
        case "thinking":
        case "free_time":
        case "critical":
        case "success":
        case "failure":
            return mode;
    }
};

const actionForMode: Record<SemanticMode, TransientVisualAction> = {
    desk: "type",
    thinking: "think",
    free_time: "wander",
    napping: "sleep",
    critical: "shake",
    success: "celebrate",
    failure: "flinch",
};

const effectForMode: Record<SemanticMode, Effect> = {
    desk: "none",
    thinking: "none",
    free_time: "none",
    napping: "zzz",
    critical: "warning",
    success: "sparkle",
    failure: "warning",
};

function initialState(): MotionState {
    return {
        semanticMode: "free_time",
        destination: "free_time",
        waypoint: "free_time",
        facing: "right",
        phase: "static",
        action: null,
        effect: "none",
        roomWaypoint: null,
        roomRegion: null,
        interaction: null,
        paused: false,
        generation: 0,
    };
}

function readClock(clock: MotionClock | undefined): number {
    if (!clock) {
        return Date.now();
    }
    return typeof clock === "function" ? clock() : clock.now();
}

function readReducedMotion(
    reducedMotion: boolean | (() => boolean) | undefined,
): boolean {
    return typeof reducedMotion === "function"
        ? reducedMotion()
        : reducedMotion === true;
}

function boundedDuration(value: number | undefined, fallback: number): number {
    if (value === undefined || !Number.isFinite(value)) {
        return fallback;
    }
    return Math.max(0, Math.min(MAX_TRAVEL_DURATION_MS, value));
}

function boundedLoopDelay(value: number | undefined): number {
    if (value === undefined || !Number.isFinite(value)) {
        return DEFAULT_LOOP_DELAY_MS;
    }
    return Math.max(1, value);
}

/**
 * Presentation-only motion state machine. It accepts snapshots and emits
 * semantic state; it has no write path to the authoritative client or domain.
 */
export class MotionController {
    private readonly timers: MotionTimerPort;
    private readonly clock: MotionClock | undefined;
    private readonly random: () => number;
    private readonly waypointForMode: WaypointResolver;
    private readonly travelDurationMs: number;
    private readonly loopDelayMs: number;
    private readonly reducedMotion: boolean | (() => boolean) | undefined;
    private readonly freeTimeOptions: MotionControllerOptions["freeTime"];
    private readonly onFreeTimeState:
        ((state: FreeTimeChoreographyState) => void) | undefined;
    private readonly listeners = new Set<MotionListener>();
    private readonly scheduledTimers = new Set<unknown>();
    private state = initialState();
    private currentMode: SemanticMode | null = null;
    private reducedMotionActive = false;
    private freeTimeChoreography: FreeTimeChoreography | null = null;
    private disposed = false;

    constructor(options: MotionControllerOptions = {}) {
        this.timers = options.timer ?? options.timers ?? defaultTimers;
        this.clock = options.clock;
        this.random = options.random ?? options.randomness ?? Math.random;
        this.waypointForMode =
            options.waypointForMode ?? defaultWaypointForMode;
        this.travelDurationMs = boundedDuration(
            options.travelDurationMs,
            DEFAULT_TRAVEL_DURATION_MS,
        );
        this.loopDelayMs = boundedLoopDelay(options.loopDelayMs);
        this.reducedMotion = options.reducedMotion;
        this.freeTimeOptions =
            options.freeTime ?? options.freeTimeChoreography ?? {};
        this.onFreeTimeState = options.onFreeTimeState;
    }

    getState(): MotionState {
        return this.state;
    }

    /** Optional room-layer hook for package 3 adapters and diagnostics. */
    getFreeTimeChoreography(): FreeTimeChoreography | null {
        return this.freeTimeChoreography;
    }

    /**
     * Applies an authoritative snapshot. Equivalent semantic modes return the
     * same state object and leave all timers untouched.
     */
    update(snapshot: SimulationSnapshot): MotionState {
        if (this.disposed) {
            return this.state;
        }

        const nextMode = semanticModeForSnapshot(
            snapshot,
            readClock(this.clock),
        );
        if (nextMode === this.currentMode) {
            this.refreshReducedMotion();
            return this.state;
        }

        this.currentMode = nextMode;
        this.clearScheduledTimers();
        this.stopFreeTimeChoreography();

        const generation = this.state.generation + 1;
        const destination = this.waypointForMode(nextMode, this.random);
        const facing: Facing =
            nextMode === "desk"
                ? "right"
                : this.random() < 0.5
                  ? "left"
                  : "right";
        const isReduced = readReducedMotion(this.reducedMotion);
        this.reducedMotionActive = isReduced;

        this.state = {
            semanticMode: nextMode,
            destination,
            waypoint: destination,
            facing,
            phase:
                isReduced || nextMode === "free_time" ? "static" : "traveling",
            action: isReduced || nextMode === "free_time" ? null : "roll",
            effect: "none",
            roomWaypoint: null,
            roomRegion: null,
            interaction: null,
            paused: false,
            generation,
        };
        this.notify();
        if (this.disposed) {
            return this.state;
        }

        if (nextMode === "free_time") {
            this.startFreeTimeChoreography(generation);
        } else if (!isReduced) {
            this.scheduleTravel(generation, nextMode);
        }

        return this.state;
    }

    /** Explicit alias for adapters that receive snapshots as events. */
    updateSnapshot(snapshot: SimulationSnapshot): MotionState {
        return this.update(snapshot);
    }

    subscribe(listener: MotionListener): () => void {
        if (this.disposed) {
            return () => undefined;
        }
        this.listeners.add(listener);
        return () => {
            this.listeners.delete(listener);
        };
    }

    dispose(): void {
        if (this.disposed) {
            return;
        }
        this.disposed = true;
        this.clearScheduledTimers();
        this.stopFreeTimeChoreography();
        this.listeners.clear();
    }

    private startFreeTimeChoreography(generation: number): void {
        const choreography = new FreeTimeChoreography({
            ...this.freeTimeOptions,
            timer: this.timers,
            clock: this.clock,
            random: this.random,
            reducedMotion: this.reducedMotion,
            onState: (nextState) => {
                this.onFreeTimeState?.(nextState);
                if (
                    this.disposed ||
                    this.currentMode !== "free_time" ||
                    this.state.generation !== generation
                ) {
                    return;
                }
                this.state = {
                    ...this.state,
                    roomWaypoint: nextState.waypoint,
                    roomRegion: nextState.waypoint.region,
                    interaction: nextState.interaction,
                    facing: nextState.facing,
                    phase: nextState.phase,
                    action: nextState.action,
                    effect: nextState.effect,
                    paused: nextState.paused,
                };
                this.notify();
            },
        });
        this.freeTimeChoreography = choreography;
        choreography.start();
    }

    private stopFreeTimeChoreography(): void {
        if (!this.freeTimeChoreography) {
            return;
        }
        this.freeTimeChoreography.dispose();
        this.freeTimeChoreography = null;
    }

    private refreshReducedMotion(): void {
        const nextReduced = readReducedMotion(this.reducedMotion);
        if (nextReduced === this.reducedMotionActive) {
            this.freeTimeChoreography?.refresh();
            return;
        }

        this.reducedMotionActive = nextReduced;
        if (this.freeTimeChoreography) {
            this.freeTimeChoreography.refresh();
            return;
        }

        this.clearScheduledTimers();
        if (nextReduced) {
            this.state = {
                ...this.state,
                phase: "static",
                action: null,
                effect: "none",
            };
            this.notify();
            return;
        }

        this.state = {
            ...this.state,
            phase: "traveling",
            action: "roll",
            effect: "none",
        };
        this.notify();
        if (this.disposed) {
            return;
        }
        this.scheduleTravel(this.state.generation, this.state.semanticMode);
    }

    private scheduleTravel(generation: number, mode: SemanticMode): void {
        const handle = this.timers.setTimeout(() => {
            this.scheduledTimers.delete(handle);
            const reduced = readReducedMotion(this.reducedMotion);
            if (
                this.disposed ||
                this.state.generation !== generation ||
                reduced
            ) {
                if (!this.disposed && reduced) {
                    this.reducedMotionActive = true;
                    this.state = {
                        ...this.state,
                        phase: "static",
                        action: null,
                        effect: "none",
                    };
                    this.notify();
                }
                return;
            }

            this.state = {
                ...this.state,
                phase: "acting",
                action: actionForMode[mode],
                effect: effectForMode[mode],
            };
            this.notify();
            if (!this.disposed) {
                this.scheduleLoop(generation, mode);
            }
        }, this.travelDurationMs);
        this.scheduledTimers.add(handle);
    }

    private scheduleLoop(generation: number, mode: SemanticMode): void {
        const handle = this.timers.setTimeout(() => {
            this.scheduledTimers.delete(handle);
            const reduced = readReducedMotion(this.reducedMotion);
            if (
                this.disposed ||
                this.state.generation !== generation ||
                reduced
            ) {
                if (!this.disposed && reduced) {
                    this.reducedMotionActive = true;
                    this.clearScheduledTimers();
                    this.state = {
                        ...this.state,
                        phase: "static",
                        action: null,
                        effect: "none",
                    };
                    this.notify();
                }
                return;
            }

            this.state = {
                ...this.state,
                action: "pulse",
                effect: effectForMode[mode],
            };
            this.notify();
            if (!this.disposed) {
                this.scheduleLoop(generation, mode);
            }
        }, this.loopDelayMs);
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
        for (const listener of [...this.listeners]) {
            if (this.disposed) {
                return;
            }
            listener(this.state);
        }
    }
}

export function createMotionController(
    options: MotionControllerOptions = {},
): MotionController {
    return new MotionController(options);
}

/** Naming alias for consumers that call this the presentation controller. */
export const createPresentationMotionController = createMotionController;

/** Class alias for DOM packages that name the state machine by its boundary. */
export { MotionController as PresentationMotionController };

export type PresentationState = MotionState;

/** Keep the wire activity type discoverable beside the mapping implementation. */
export type AuthoritativeActivity = ActivityKind;

export * from "./choreography";
