import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HOLD_MS, useLabelTiming } from "./useLabelTiming";

describe("chip timing", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const render = (props: Parameters<typeof useLabelTiming>[0]) =>
    renderHook((p: Parameters<typeof useLabelTiming>[0]) => useLabelTiming(p), {
      initialProps: props,
    });

  const base = { state: "working", label: "Editing App.tsx", hovered: false, reducedMotion: false } as const;

  it("holds for three seconds then fades", () => {
    const { result } = render({ ...base });
    expect(result.current).toBe(true);

    act(() => void vi.advanceTimersByTime(HOLD_MS - 1));
    expect(result.current).toBe(true);

    act(() => void vi.advanceTimersByTime(1));
    expect(result.current).toBe(false);
  });

  it("restores on hover", () => {
    const { result, rerender } = render({ ...base });
    act(() => void vi.advanceTimersByTime(HOLD_MS));
    expect(result.current).toBe(false);

    rerender({ ...base, hovered: true });
    expect(result.current).toBe(true);
  });

  it("holds indefinitely while blocking", () => {
    const { result } = render({ ...base, state: "needsPermission", label: "Allow edit?" });
    act(() => void vi.advanceTimersByTime(HOLD_MS * 20));
    expect(result.current).toBe(true);
  });

  it("holds indefinitely for every blocking state", () => {
    for (const state of ["needsInput", "needsPermission", "failed"] as const) {
      const { result } = render({ ...base, state });
      act(() => void vi.advanceTimersByTime(HOLD_MS * 5));
      expect(result.current, `${state} went quiet`).toBe(true);
    }
  });

  it("stays persistent under reduced motion, compensating for the lost cue", () => {
    const { result } = render({ ...base, reducedMotion: true });
    act(() => void vi.advanceTimersByTime(HOLD_MS * 10));
    expect(result.current).toBe(true);
  });

  it("gives a new tool within one state its own hold", () => {
    const { result, rerender } = render({ ...base });
    act(() => void vi.advanceTimersByTime(HOLD_MS));
    expect(result.current).toBe(false);

    rerender({ ...base, label: "Running npm test" });
    expect(result.current).toBe(true);

    act(() => void vi.advanceTimersByTime(HOLD_MS));
    expect(result.current).toBe(false);
  });
});
