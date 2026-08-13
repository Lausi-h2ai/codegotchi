import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useBlink } from "./useBlink";

describe("useBlink", () => {
    beforeEach(() => vi.useFakeTimers());

    afterEach(() => {
        cleanup();
        vi.useRealTimers();
    });

    it("closes once after the five-second minimum and reopens after 120ms", () => {
        const random = (): number => 0;
        const { result } = renderHook(() => useBlink(true, random));

        act(() => vi.advanceTimersByTime(4_999));
        expect(result.current).toBe(false);

        act(() => vi.advanceTimersByTime(1));
        expect(result.current).toBe(true);

        act(() => vi.advanceTimersByTime(119));
        expect(result.current).toBe(true);

        act(() => vi.advanceTimersByTime(1));
        expect(result.current).toBe(false);
    });

    it("clears every timer on unmount", () => {
        const random = (): number => 0.5;
        const { unmount } = renderHook(() => useBlink(true, random));

        expect(vi.getTimerCount()).toBe(1);
        unmount();
        expect(vi.getTimerCount()).toBe(0);
    });

    it("supports the ten-second maximum and chooses a fresh delay after reopening", () => {
        const random = vi.fn().mockReturnValueOnce(1).mockReturnValueOnce(0);
        const { result } = renderHook(() => useBlink(true, random));

        act(() => vi.advanceTimersByTime(9_999));
        expect(result.current).toBe(false);
        act(() => vi.advanceTimersByTime(1));
        expect(result.current).toBe(true);
        act(() => vi.advanceTimersByTime(120));
        expect(result.current).toBe(false);

        act(() => vi.advanceTimersByTime(4_999));
        expect(result.current).toBe(false);
        act(() => vi.advanceTimersByTime(1));
        expect(result.current).toBe(true);
        expect(random).toHaveBeenCalledTimes(2);
    });

    it("cancels and clears blinking while disabled, then starts a fresh wait", () => {
        const random = vi.fn(() => 0);
        const { result, rerender } = renderHook(
            ({ enabled }) => useBlink(enabled, random),
            { initialProps: { enabled: true } },
        );

        act(() => vi.advanceTimersByTime(5_000));
        expect(result.current).toBe(true);
        rerender({ enabled: false });
        expect(result.current).toBe(false);
        expect(vi.getTimerCount()).toBe(0);

        rerender({ enabled: true });
        act(() => vi.advanceTimersByTime(4_999));
        expect(result.current).toBe(false);
        act(() => vi.advanceTimersByTime(1));
        expect(result.current).toBe(true);
    });
});
