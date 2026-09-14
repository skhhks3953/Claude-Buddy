import type { ClawdState } from "../state";

/**
 * Eye geometry, quoted as percentages of the sprite box exactly as the
 * prototype does. A table rather than ten CSS rules, because the difference
 * between these states *is* the numbers.
 */
interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

type Pose = { left: Rect; right: Rect; brows?: { top: number; height: number } };

const POSES: Record<ClawdState, Pose | "squint"> = {
  idle: pair(26.5, 64, 29.5, 9.5, 9.5),
  // Eyes drift upward: Claude is deciding what to do next.
  thinking: pair(26.5, 64, 24, 9.5, 9.5),
  // Narrowed to a working squint while the paws drum.
  working: pair(26.5, 64, 31, 9.5, 5),
  // Wide open.
  needsInput: pair(26.5, 64, 26, 9.5, 14),
  // Pushed apart.
  needsPermission: pair(20.5, 70, 29.5, 9.5, 9.5),
  success: "squint",
  // Sagging, under dropped brows.
  failed: { ...pair(26.5, 64, 35, 9.5, 5), brows: { top: 29, height: 3 } },
  paused: pair(26.5, 64, 31.5, 9.5, 4),
  longTask: pair(26.5, 64, 29.5, 9.5, 9.5),
  compacting: pair(26.5, 64, 31, 9.5, 4.5),
};

/** The held pose, from the prototype's drag illustration. */
const DRAGGING = pair(26.5, 64, 28, 9.5, 7.5);

function pair(l: number, r: number, top: number, width: number, height: number): Pose {
  return {
    left: { left: l, top, width, height },
    right: { left: r, top, width, height },
  };
}

/** Success turns the eyes into a squint: four pixels, two per eye. */
const SQUINT: Rect[] = [
  { left: 26.5, top: 28.5, width: 5, height: 5 },
  { left: 31.5, top: 33, width: 5, height: 5 },
  { left: 70, top: 28.5, width: 5, height: 5 },
  { left: 64, top: 33, width: 5, height: 5 },
];

const style = (r: Rect) => ({
  left: `${r.left}%`,
  top: `${r.top}%`,
  width: `${r.width}%`,
  height: `${r.height}%`,
});

export function Eyes({ state, dragging }: { state: ClawdState; dragging: boolean }) {
  const pose = dragging ? DRAGGING : POSES[state];

  if (pose === "squint") {
    return (
      <>
        {SQUINT.map((rect, i) => (
          <div key={i} className="clawd__eye" style={style(rect)} />
        ))}
      </>
    );
  }

  return (
    <>
      <div className="clawd__eye" style={style(pose.left)} />
      <div className="clawd__eye" style={style(pose.right)} />
      {pose.brows && (
        <>
          <div
            className="clawd__brow"
            style={style({ ...pose.left, top: pose.brows.top, height: pose.brows.height })}
          />
          <div
            className="clawd__brow"
            style={style({ ...pose.right, top: pose.brows.top, height: pose.brows.height })}
          />
        </>
      )}
    </>
  );
}
