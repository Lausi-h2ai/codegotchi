import { useState, type CSSProperties, type DragEvent } from "react";

import "./App.css";

import { extractLaunchToken } from "./client";
import type { MotionState } from "./motion";
import {
    activeActivity,
    isFoodId,
    isNapping,
    type ActivityKind,
    type FoodId,
    type SimulationSnapshot,
} from "./protocol";
import { useBlink } from "./useBlink";
import { useCodeGotchi } from "./useCodeGotchi";
import { usePetMotion } from "./usePetMotion";

const FOODS: { id: FoodId; label: string; icon: string }[] = [
    { id: "kibble", label: "Kibble", icon: "◈" },
    { id: "treat", label: "Treat", icon: "✦" },
    { id: "fruit", label: "Fruit", icon: "●" },
    { id: "energy_drink", label: "Energy drink", icon: "⚡" },
];

interface AppProps {
    launchToken?: string | null;
}

function App({ launchToken }: AppProps) {
    const [token] = useState<string | null>(() =>
        launchToken === undefined ? extractLaunchToken() : launchToken,
    );
    const {
        snapshot,
        connectionStatus,
        error,
        feedback,
        debugEnabled,
        feed,
        clean,
        nap,
        restock,
    } = useCodeGotchi(token);
    const [shovelArmed, setShovelArmed] = useState(false);
    const [cleaningPoopId, setCleaningPoopId] = useState<string | null>(null);
    const motionState = usePetMotion(snapshot);
    const blinking = useBlink(
        snapshot !== null && motionState.semanticMode !== "napping",
    );

    const activityLabel = snapshot ? presentationActivity(snapshot) : "Waiting";
    const behaviorLabel = snapshot ? presentationBehavior(snapshot) : "Waiting";
    const napping = snapshot ? isNapping(snapshot) : false;
    const motionWaypoint =
        motionState.roomWaypoint?.id ?? motionState.destination;
    const motionRegion = motionState.roomRegion ?? "";
    const motionStyle = roomMotionStyle(motionState);
    const showTypingMarks =
        motionState.semanticMode === "desk" &&
        (motionState.action === "type" || motionState.action === "pulse") &&
        motionState.phase !== "static";
    const showThoughtBubbles =
        motionState.semanticMode === "thinking" &&
        (motionState.action === "think" || motionState.action === "pulse") &&
        motionState.phase !== "static";
    const showSparkles = motionState.semanticMode === "success";

    function submitFeed(foodId: string): void {
        if (!isFoodId(foodId)) {
            return;
        }
        void feed(foodId);
    }

    function handleFeedDrop(event: DragEvent<HTMLButtonElement>): void {
        const data = event.dataTransfer.getData(
            "application/x-codegotchi-food",
        );
        if (!data.startsWith("food:")) {
            return;
        }
        const foodId = data.slice("food:".length);
        if (!isFoodId(foodId)) {
            return;
        }
        event.preventDefault();
        submitFeed(foodId);
    }

    function handlePoopDrop(event: DragEvent<HTMLButtonElement>): void {
        if (
            event.dataTransfer.getData("application/x-codegotchi-tool") !==
            "shovel"
        ) {
            return;
        }
        event.preventDefault();
        setCleaningPoopId(event.currentTarget.dataset.poopId ?? null);
        setShovelArmed(false);
    }

    function handlePoopClick(poopId: string): void {
        if (shovelArmed) {
            setCleaningPoopId(poopId);
            setShovelArmed(false);
        }
    }

    function handleTrashDrop(event: DragEvent<HTMLButtonElement>): void {
        const draggedPoopId = event.dataTransfer
            .getData("application/x-codegotchi-poop")
            .replace("poop:", "");
        if (
            !cleaningPoopId ||
            (draggedPoopId.length > 0 && draggedPoopId !== cleaningPoopId)
        ) {
            return;
        }
        const poopId = snapshot?.pendingPoops.some(
            (poop) => poop.id === cleaningPoopId,
        )
            ? cleaningPoopId
            : null;
        if (!poopId) {
            return;
        }
        event.preventDefault();
        void disposePoop(poopId);
    }

    async function disposePoop(poopId: string): Promise<void> {
        try {
            await clean(poopId);
            setCleaningPoopId(null);
            setShovelArmed(false);
        } catch {
            // The hook owns the backend error; retain the authoritative poop.
        }
    }

    return (
        <main className="app-shell">
            <header className="hero">
                <p className="eyebrow">Functional pet room · Task 3</p>
                <h1>CodeGotchi</h1>
                <p className="hero-copy">
                    A small, authoritative room for a coding companion. Every
                    need, food count, and poop below comes from the backend.
                </p>
            </header>

            <section className="room-card" aria-label="CodeGotchi pet room">
                <div className="room-card__header">
                    <div>
                        <p className="section-kicker">Current room</p>
                        <h2 id="room-title">
                            {snapshot ? `${snapshot.name}ʼs room` : "Pet room"}
                        </h2>
                    </div>
                    <span
                        className={`status-pill status-pill--${connectionStatus}`}
                        role="status"
                    >
                        {statusLabel(connectionStatus)}
                    </span>
                </div>

                {error ? (
                    <p className="error-banner" role="alert">
                        <strong>{error.code}:</strong> {error.message}
                    </p>
                ) : null}

                {!snapshot ? (
                    <div className="loading-room" role="status">
                        Loading authoritative room…
                    </div>
                ) : (
                    <div className="room-layout">
                        <section
                            className="room-illustration"
                            aria-label="Room with pet, desk, food, shovel, poop, and trash"
                        >
                            <div className="room-window" aria-hidden="true">
                                <span />
                                <span />
                            </div>
                            <div className="shelf" aria-hidden="true">
                                <span />
                                <span />
                                <span />
                            </div>

                            <div
                                className="desk-work-area"
                                role="region"
                                aria-label="Desk and work area"
                            >
                                <div className="monitor" aria-hidden="true">
                                    <span />
                                </div>
                                <div className="desk-top" aria-hidden="true" />
                                <div className="desk-leg desk-leg--left" />
                                <div className="desk-leg desk-leg--right" />
                            </div>

                            <button
                                className={`hammock ${napping ? "hammock--rocking" : ""}`}
                                type="button"
                                data-testid="hammock"
                                aria-label={
                                    napping
                                        ? "Resting in hammock"
                                        : "Hammock nap"
                                }
                                aria-pressed={napping}
                                disabled={napping}
                                onClick={() => {
                                    if (!napping) {
                                        void nap();
                                    }
                                }}
                            >
                                <span className="hammock__rope hammock__rope--left" />
                                <span className="hammock__rope hammock__rope--right" />
                                <span className="hammock__hook hammock__hook--left" />
                                <span className="hammock__hook hammock__hook--right" />
                                <span className="hammock__bed" />
                            </button>

                            <div
                                className={`pet pet--${poseClass(snapshot)}`}
                                role="img"
                                aria-label={`${snapshot.name}, ${activityLabel}`}
                                data-testid="pet"
                                data-motion-mode={motionState.semanticMode}
                                data-motion-action={
                                    motionState.action ?? "none"
                                }
                                data-motion-waypoint={motionWaypoint}
                                data-motion-facing={motionState.facing}
                                data-motion-phase={motionState.phase}
                                data-motion-region={motionRegion}
                                data-blinking={blinking ? "true" : undefined}
                                style={motionStyle}
                            >
                                <span className="pet-ear pet-ear--left" />
                                <span className="pet-ear pet-ear--right" />
                                <span className="pet-body">
                                    <span className="pet-eye pet-eye--left" />
                                    <span className="pet-eye pet-eye--right" />
                                    <span className="pet-mouth" />
                                </span>
                                {showTypingMarks ? (
                                    <span
                                        className="typing-marks"
                                        data-testid="typing-marks"
                                        aria-hidden="true"
                                    >
                                        <span />
                                        <span />
                                        <span />
                                    </span>
                                ) : null}
                                {showThoughtBubbles ? (
                                    <span
                                        className="thought-bubbles"
                                        data-testid="thought-bubbles"
                                        aria-hidden="true"
                                    >
                                        <span />
                                        <span />
                                        <span>?</span>
                                    </span>
                                ) : null}
                                {showSparkles ? (
                                    <span
                                        className="motion-sparkles"
                                        data-testid="motion-sparkles"
                                        aria-hidden="true"
                                    >
                                        <span>✦</span>
                                        <span>✧</span>
                                        <span>✦</span>
                                    </span>
                                ) : null}
                                {napping ? (
                                    <span
                                        className="zzz"
                                        data-testid="zzz"
                                        aria-hidden="true"
                                    >
                                        <span>z</span>
                                        <span>Z</span>
                                        <span>Z</span>
                                    </span>
                                ) : null}
                            </div>

                            <button
                                className="feed-target"
                                type="button"
                                aria-label={`Feed target for ${snapshot.name}`}
                                data-feed-target="true"
                                onDragOver={(event) => event.preventDefault()}
                                onDrop={handleFeedDrop}
                                onClick={() => submitFeed("kibble")}
                            >
                                <span aria-hidden="true">♡</span>
                                Feed target
                            </button>

                            {snapshot.pendingPoops.map((poop, index) => (
                                <button
                                    className={`poop ${cleaningPoopId === poop.id ? "poop--selected" : ""}`}
                                    style={{
                                        right: `${29 + (index % 6) * 6}%`,
                                        bottom: `${16 + Math.floor(index / 6) * 7}%`,
                                    }}
                                    data-testid={`poop-${poop.id}`}
                                    data-poop-id={poop.id}
                                    draggable
                                    key={poop.id}
                                    type="button"
                                    aria-label={`Poop ${poop.id}`}
                                    onClick={() => handlePoopClick(poop.id)}
                                    onDragStart={(event) => {
                                        event.dataTransfer.setData(
                                            "application/x-codegotchi-poop",
                                            `poop:${poop.id}`,
                                        );
                                    }}
                                    onDragOver={(event) =>
                                        event.preventDefault()
                                    }
                                    onDrop={handlePoopDrop}
                                >
                                    💩
                                </button>
                            ))}

                            <button
                                className={`shovel ${shovelArmed ? "shovel--armed" : ""}`}
                                type="button"
                                draggable
                                aria-pressed={shovelArmed}
                                aria-label={
                                    shovelArmed ? "Shovel armed" : "Shovel"
                                }
                                onClick={() =>
                                    setShovelArmed((armed) => !armed)
                                }
                                onDragStart={(event) => {
                                    event.dataTransfer.setData(
                                        "application/x-codegotchi-tool",
                                        "shovel",
                                    );
                                }}
                            >
                                🪣
                            </button>

                            <button
                                className="trash-target"
                                type="button"
                                aria-label="Trash"
                                data-trash-target="true"
                                onClick={() => {
                                    if (cleaningPoopId) {
                                        void disposePoop(cleaningPoopId);
                                    }
                                }}
                                onDragOver={(event) => event.preventDefault()}
                                onDrop={handleTrashDrop}
                            >
                                🗑️
                                <span>Trash</span>
                            </button>

                            <div
                                className="activity-sign"
                                data-testid="activity-label"
                                aria-live="polite"
                            >
                                <span
                                    className="activity-sign__icon"
                                    aria-hidden="true"
                                >
                                    {activityIcon(activityLabel)}
                                </span>
                                <span>{activityLabel}</span>
                                <small>{behaviorLabel}</small>
                            </div>

                            {feedback ? (
                                <p className="feedback" aria-live="polite">
                                    {feedback}
                                </p>
                            ) : null}
                        </section>

                        <aside className="room-sidebar">
                            <section
                                className="panel"
                                aria-labelledby="needs-title"
                            >
                                <p className="section-kicker">
                                    Authoritative needs
                                </p>
                                <h3 id="needs-title">
                                    How is {snapshot.name}?
                                </h3>
                                <div className="needs-grid">
                                    <Need
                                        label="Hunger"
                                        value={snapshot.needs.hunger}
                                    />
                                    <Need
                                        label="Energy"
                                        value={snapshot.needs.energy}
                                    />
                                    <Need
                                        label="Happiness"
                                        value={snapshot.needs.happiness}
                                    />
                                    <Need
                                        label="Cleanliness"
                                        value={snapshot.needs.cleanliness}
                                    />
                                </div>
                            </section>

                            <section
                                className="panel"
                                aria-labelledby="food-title"
                            >
                                <p className="section-kicker">
                                    Food & care items
                                </p>
                                <h3 id="food-title">Drag items to use</h3>
                                <div className="food-list">
                                    {FOODS.map((food) => (
                                        <button
                                            className="food-item"
                                            data-food-id={food.id}
                                            data-testid={`food-${food.id}`}
                                            draggable
                                            key={food.id}
                                            type="button"
                                            aria-label={`${food.label}, ${snapshot.inventory[food.id] ?? 0} available`}
                                            onClick={() => submitFeed(food.id)}
                                            onDragStart={(event) => {
                                                event.dataTransfer.setData(
                                                    "application/x-codegotchi-food",
                                                    `food:${food.id}`,
                                                );
                                            }}
                                        >
                                            <span aria-hidden="true">
                                                {food.icon}
                                            </span>
                                            <span>{food.label}</span>
                                            <strong>
                                                {snapshot.inventory[food.id] ??
                                                    0}
                                            </strong>
                                        </button>
                                    ))}
                                </div>
                                <p className="care-tip">
                                    Out of energy? The hammock nap restores the
                                    full meter in five seconds; energy drinks
                                    give an instant pick-me-up.
                                </p>
                                {debugEnabled ? (
                                    <button
                                        className="restock-button"
                                        type="button"
                                        data-testid="restock"
                                        onClick={() => void restock()}
                                    >
                                        ⟳ Restock pantry
                                    </button>
                                ) : null}
                            </section>

                            <p className="authoritative-note">
                                {snapshot.pendingPoops.length === 0
                                    ? "The floor is clean."
                                    : `${snapshot.pendingPoops.length} authoritative poop${snapshot.pendingPoops.length === 1 ? "" : "s"} waiting for the shovel.`}
                            </p>
                        </aside>
                    </div>
                )}
            </section>
        </main>
    );
}

function Need({ label, value }: { label: string; value: number }) {
    const formatted = formatNeed(value);
    return (
        <div className="need">
            <div className="need__heading">
                <span>{label}</span>
                <strong>
                    {label} {formatted}
                </strong>
            </div>
            <div className="need__track" aria-hidden="true">
                <span
                    style={{ width: `${Math.max(0, Math.min(100, value))}%` }}
                />
            </div>
        </div>
    );
}

function formatNeed(value: number): string {
    return `${value.toFixed(0)}%`;
}

function statusLabel(status: string): string {
    switch (status) {
        case "connected":
            return "Connected";
        case "reconnecting":
            return "Reconnecting…";
        case "disconnected":
            return "Disconnected";
        case "connecting":
            return "Connecting…";
        default:
            return "Loading…";
    }
}

function presentationActivity(snapshot: SimulationSnapshot): string {
    if (snapshot.activity === "Blocked" || snapshot.behavior === "Blocked") {
        return "Refusing";
    }
    if (snapshot.behavior === "Sleeping") {
        return "Sleeping";
    }
    if (
        snapshot.behavior === "RecentSuccess" ||
        snapshot.recentOutcome === "Success"
    ) {
        return "Celebrating";
    }
    if (
        snapshot.behavior === "RecentFailure" ||
        snapshot.recentOutcome === "Failure"
    ) {
        return "Upset";
    }
    if (snapshot.behavior === "CriticalNeed") {
        return "Upset";
    }

    const activity = activeActivity(snapshot.activity);
    if (!activity) {
        return snapshot.activity === "WaitingForUser" ? "Waiting" : "Idle";
    }
    return activityLabel(activity);
}

function presentationBehavior(snapshot: SimulationSnapshot): string {
    switch (snapshot.behavior) {
        case "Wandering":
            return "Wandering / walking";
        case "Sleeping":
            return "Sleeping";
        case "Working":
            return "Working";
        case "CriticalNeed":
            return "Upset";
        case "Blocked":
            return "Refusing";
        case "RecentSuccess":
            return "Celebrating";
        case "RecentFailure":
            return "Upset";
    }
}

function activityLabel(activity: ActivityKind): string {
    switch (activity) {
        case "thinking":
            return "Thinking";
        case "reading":
            return "Reading";
        case "searching":
        case "web_research":
            return "Searching";
        case "editing":
            return "Typing / editing";
        case "testing":
            return "Testing";
        case "building":
            return "Building";
        case "celebrating":
            return "Celebrating";
        case "error":
            return "Upset";
        case "blocked":
            return "Refusing";
        case "waiting":
            return "Waiting";
        case "idle":
            return "Idle";
        default:
            return "Working";
    }
}

function activityIcon(label: string): string {
    if (label === "Sleeping") return "☾";
    if (label === "Celebrating") return "✦";
    if (label === "Upset" || label === "Refusing") return "!";
    if (label === "Idle" || label === "Wandering / walking") return "•";
    return "⌁";
}

function roomMotionStyle(state: MotionState): CSSProperties {
    if (state.roomWaypoint === null) {
        return {};
    }

    return {
        "--pet-x": `${state.roomWaypoint.position.x * 100}%`,
        "--pet-y": `${state.roomWaypoint.position.y * 100}%`,
    } as CSSProperties;
}

function poseClass(snapshot: SimulationSnapshot): string {
    if (isNapping(snapshot)) {
        return "napping";
    }
    return presentationActivity(snapshot)
        .toLowerCase()
        .replaceAll(" / ", "-")
        .replaceAll(" ", "-")
        .replaceAll("/", "-");
}

export default App;
