import type { ClawdState } from "../state";

/**
 * The label chip. Dumb: it is told what to say and whether to be visible.
 *
 * Chip *timing* — slide in, hold, fade, hover restores — lives in `label/`,
 * outside this boundary, because the renderer is not allowed to keep state of
 * its own (§7.1).
 */
export function Chip({
  state,
  text,
  visible,
}: {
  state: ClawdState;
  text: string;
  visible: boolean;
}) {
  return (
    <div
      className={`chip chip--${state} ${visible ? "chip--visible" : ""}`}
      // Kept in the tree while hidden so the fade has something to fade, but
      // taken out of the accessibility tree when it is not being shown.
      aria-hidden={!visible}
    >
      <span className="chip__dot" />
      {text}
    </div>
  );
}
