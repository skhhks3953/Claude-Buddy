import type { ClawdState } from "../state";

/** The corner badge. Colours live in CSS; only the glyph is chosen here. */
const GLYPHS: Partial<Record<ClawdState, string>> = {
  needsInput: "?",
  needsPermission: "!",
  success: "✓",
  failed: "×",
};

export function Badge({ state }: { state: ClawdState }) {
  if (state === "paused") {
    // Two bars rather than a typed glyph, so the pause mark stays on the grid.
    return (
      <div className="clawd__badge clawd__badge--bars" aria-hidden>
        <span className="clawd__bar" />
        <span className="clawd__bar" />
      </div>
    );
  }

  const glyph = GLYPHS[state];
  if (!glyph) return null;
  return (
    <div className="clawd__badge" aria-hidden>
      {glyph}
    </div>
  );
}
