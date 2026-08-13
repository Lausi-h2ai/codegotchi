import { useEffect, useRef, useState } from "react";

import {
    createMotionController,
    type MotionController,
    type MotionState,
} from "./motion";
import type { SimulationSnapshot } from "./protocol";

const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

type LegacyMediaQueryList = MediaQueryList & {
    addListener?: (listener: (event: MediaQueryListEvent) => void) => void;
    removeListener?: (listener: (event: MediaQueryListEvent) => void) => void;
};

function readReducedMotionQuery(): MediaQueryList | null {
    if (
        typeof window === "undefined" ||
        typeof window.matchMedia !== "function"
    ) {
        return null;
    }
    return window.matchMedia(REDUCED_MOTION_QUERY);
}

/**
 * Owns the presentation controller for one mounted room. The authoritative
 * snapshot is only ever passed through to the controller; this hook does not
 * derive or mutate domain state.
 */
export function usePetMotion(snapshot: SimulationSnapshot | null): MotionState {
    const mediaQueryRef = useRef<MediaQueryList | null | undefined>(undefined);
    if (mediaQueryRef.current === undefined) {
        mediaQueryRef.current = readReducedMotionQuery();
    }

    const reducedMotionRef = useRef(mediaQueryRef.current?.matches ?? false);
    const controllerRef = useRef<MotionController | null>(null);
    if (controllerRef.current === null) {
        controllerRef.current = createMotionController({
            reducedMotion: () => reducedMotionRef.current,
        });
    }
    const controller = controllerRef.current;
    const latestSnapshotRef = useRef<SimulationSnapshot | null>(snapshot);
    latestSnapshotRef.current = snapshot;

    const [motionState, setMotionState] = useState<MotionState>(() =>
        controller.getState(),
    );

    useEffect(() => {
        let activeController = controllerRef.current;
        if (activeController === null) {
            activeController = createMotionController({
                reducedMotion: () => reducedMotionRef.current,
            });
            controllerRef.current = activeController;
        }

        const unsubscribe = activeController.subscribe((nextState) => {
            setMotionState(nextState);
        });
        const mediaQuery = mediaQueryRef.current ?? null;
        const onMediaQueryChange = (event: MediaQueryListEvent): void => {
            reducedMotionRef.current = event.matches;
            const latestSnapshot = latestSnapshotRef.current;
            if (latestSnapshot !== null) {
                activeController?.update(latestSnapshot);
            }
        };

        if (mediaQuery !== null) {
            const legacyMediaQuery = mediaQuery as LegacyMediaQueryList;
            if (typeof mediaQuery.addEventListener === "function") {
                mediaQuery.addEventListener("change", onMediaQueryChange);
            } else {
                legacyMediaQuery.addListener?.(onMediaQueryChange);
            }

            return () => {
                unsubscribe();
                if (typeof mediaQuery.removeEventListener === "function") {
                    mediaQuery.removeEventListener(
                        "change",
                        onMediaQueryChange,
                    );
                } else {
                    legacyMediaQuery.removeListener?.(onMediaQueryChange);
                }
                activeController?.dispose();
                if (controllerRef.current === activeController) {
                    controllerRef.current = null;
                }
            };
        }

        return () => {
            unsubscribe();
            activeController?.dispose();
            if (controllerRef.current === activeController) {
                controllerRef.current = null;
            }
        };
    }, []);

    useEffect(() => {
        const activeController = controllerRef.current;
        if (snapshot !== null && activeController !== null) {
            activeController.update(snapshot);
        }
    }, [snapshot]);

    return motionState;
}
