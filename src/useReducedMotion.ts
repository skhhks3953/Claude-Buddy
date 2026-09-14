import { useEffect, useState } from "react";

/**
 * The OS reduced-motion setting.
 *
 * Honoured, but not by silently disabling animation — motion *is* the signal
 * here, and a still pet says nothing (§7.5). States collapse to their
 * distinguishing pose and colour, and the label stops auto-fading.
 */
export function useReducedMotion(): boolean {
  const query = "(prefers-reduced-motion: reduce)";
  const [reduced, setReduced] = useState(
    () => typeof window !== "undefined" && window.matchMedia?.(query).matches === true,
  );

  useEffect(() => {
    const mq = window.matchMedia?.(query);
    if (!mq) return;
    const update = () => setReduced(mq.matches);
    mq.addEventListener("change", update);
    return () => mq.removeEventListener("change", update);
  }, []);

  return reduced;
}
