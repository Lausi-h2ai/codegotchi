export type FoodId = "kibble" | "treat" | "fruit" | "energy_drink";

export type PetSpecies = "cat";

export type PetBehavior =
    | "Wandering"
    | "Sleeping"
    | "Working"
    | "CriticalNeed"
    | "Blocked"
    | "RecentSuccess"
    | "RecentFailure";

export type ActivityKind =
    | "idle"
    | "thinking"
    | "reading"
    | "searching"
    | "editing"
    | "testing"
    | "building"
    | "installing"
    | "git_operation"
    | "docker_operation"
    | "web_research"
    | "waiting"
    | "celebrating"
    | "error"
    | "blocked"
    | "unknown_work";

export type AgentActivityState =
    "Idle" | "WaitingForUser" | "Blocked" | { Active: ActivityKind };

export type AgentOutcome = "None" | "Success" | "Failure";

export type EnforcementMode = "decorative" | "gentle" | "strict";

export interface PetNeeds {
    hunger: number;
    energy: number;
    happiness: number;
    cleanliness: number;
}

export interface Poop {
    id: string;
    createdAt: string;
}

export type FoodInventory = Partial<Record<FoodId, number>> &
    Record<string, number>;

export interface SessionActivity {
    activity: AgentActivityState;
    updatedAt: string;
}

/** The complete authoritative Task 2 state; the browser never derives it. */
export interface SimulationSnapshot {
    schemaVersion: number;
    petId: string;
    name: string;
    species: PetSpecies;
    needs: PetNeeds;
    behavior: PetBehavior;
    activity: AgentActivityState;
    recentOutcome: AgentOutcome;
    workPoints: number;
    digestionPoints: number;
    lastUpdatedAt: string;
    pendingPoops: Poop[];
    inventory: FoodInventory;
    processedCareIds: string[];
    poopSequence: number;
    sessionActivities: Record<string, SessionActivity>;
    processedEventIds: string[];
    lastActivityAt: string | null;
    lastOutcomeAt: string | null;
    consecutiveFailures: number;
    enforcementMode: EnforcementMode;
    /** Deadline of the current hammock nap, or null while awake. */
    nappingUntil: string | null;
}

export interface CareResponse extends SimulationSnapshot {
    duplicate: boolean;
}

export interface DebugStatusResponse {
    debugEnabled: boolean;
}

export interface ErrorEnvelope {
    error: {
        code: string;
        message: string;
    };
}

export function isFoodId(value: string): value is FoodId {
    return (
        value === "kibble" ||
        value === "treat" ||
        value === "fruit" ||
        value === "energy_drink"
    );
}

/** The fixed nap length the room uses; the domain enforces the same duration. */
export const NAP_DURATION_MS = 5_000;

/** Whether the pet is currently napping in the hammock at this instant. */
export function isNapping(
    snapshot: SimulationSnapshot,
    now = Date.now(),
): boolean {
    if (snapshot.nappingUntil === null) {
        return false;
    }
    const deadline = Date.parse(snapshot.nappingUntil);
    return Number.isFinite(deadline) && deadline > now;
}

export function activeActivity(
    activity: AgentActivityState,
): ActivityKind | null {
    return typeof activity === "object" ? activity.Active : null;
}
