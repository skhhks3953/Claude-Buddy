import { useEffect, useRef, useState } from "react";
import { isBlocking, type ClawdState } from "../state";

/** How long the chip holds after a state change before fading (§5.2). */
export const HOLD_MS = 3000;

export interface LabelTimingInput {
  state: ClawdState;
  label: string;
  hovered: boolean;
  /** When motion is reduced, the chip compensates by never going quiet (§7.5). */
  reducedMotion: boolean;
}

/**
 * Chip timing: slide in on a change, hold ~3s, fade. Hover restores it.
 *
 * This lives outside `renderer/` on purpose. The renderer takes a state and
 * draws it, and nothing more (§7.1) — so the one piece of the label that is
 * genuinely temporal is kept here, where it can hold state honestly.
 */
export function useLabelTiming({
  state,
  label,
  hovered,
  reducedMotion,
}: LabelTimingInput): boolean {
  const [held, setHeld] = useState(true);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => {
    clearTimeout(timer.current);

    // The two states that need the user are the two that must not go quiet.
    // Same for reduced motion, where the label is carrying the signal that the
    // pose no longer can.
    if (isBlocking(state) || reducedMotion) {
      setHeld(true);
      return;
    }

    setHeld(true);
    timer.current = setTimeout(() => setHeld(false), HOLD_MS);
    return () => clearTimeout(timer.current);
    // Re-running on `label` is deliberate: a new tool within one state is a
    // new thing to say, and deserves its own hold.
  }, [state, label, reducedMotion]);

  return held || hovered;
}
