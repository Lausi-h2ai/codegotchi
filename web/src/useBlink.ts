import { useEffect, useState } from "react";

const MIN_BLINK_DELAY_MS = 5_000;
const BLINK_DELAY_RANGE_MS = 5_000;
const BLINK_DURATION_MS = 120;

function nextBlinkDelay(random: () => number): number {
    const sample = Math.min(1, Math.max(0, random()));
    return MIN_BLINK_DELAY_MS + Math.round(sample * BLINK_DELAY_RANGE_MS);
}

export function useBlink(
    enabled: boolean,
    random: () => number = Math.random,
): boolean {
    const [blinking, setBlinking] = useState(false);

    useEffect(() => {
        let waitTimer: number | undefined;
        let blinkTimer: number | undefined;

        const schedule = (): void => {
            waitTimer = window.setTimeout(() => {
                setBlinking(true);
                blinkTimer = window.setTimeout(() => {
                    setBlinking(false);
                    schedule();
                }, BLINK_DURATION_MS);
            }, nextBlinkDelay(random));
        };

        if (enabled) {
            schedule();
        } else {
            setBlinking(false);
        }

        return () => {
            window.clearTimeout(waitTimer);
            window.clearTimeout(blinkTimer);
        };
    }, [enabled, random]);

    return blinking;
}
