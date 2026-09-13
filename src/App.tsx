import { useEffect, useState } from "react";
import { bridge } from "./bridge";
import { useLabelTiming } from "./label/useLabelTiming";
import { Clawd } from "./renderer/Clawd";
import type { Snapshot } from "./state";
import { useDrag } from "./useDrag";
import { useReducedMotion } from "./useReducedMotion";
import "./app.css";

/**
 * The pet window.
 *
 * React is a pure render target: it receives a state and draws it (§3). The
 * only things decided here are the ones the shell cannot know — whether the
 * cursor is over the body, and whether a drag is in progress.
 */
export default function App() {
  const [snapshot, setSnapshot] = useState<Snapshot>({ state: "idle", label: "Ready" });
  const [spriteSize, setSpriteSize] = useState(80);
  // Two independent ways the window can stop being worth animating.
  const [windowOccluded, setWindowOccluded] = useState(false);
  const [documentHidden, setDocumentHidden] = useState(false);
  const occluded = windowOccluded || documentHidden;
  const [hovered, setHovered] = useState(false);

  const reducedMotion = useReducedMotion();
  const { dragging, handlers } = useDrag();
  const labelVisible = useLabelTiming({
    state: snapshot.state,
    label: snapshot.label,
    hovered,
    reducedMotion,
  });

  useEffect(() => {
    const subscriptions = [
      bridge.onSnapshot(setSnapshot),
      bridge.onLayout(({ spriteSize }) => setSpriteSize(spriteSize)),
      bridge.onOcclusion(setWindowOccluded),
    ];
    // Ask for the current state rather than waiting for the next change —
    // the window can be reloaded mid-session.
    void bridge.hello();
    return () => {
      for (const s of subscriptions) void s.then((off) => off());
    };
  }, []);

  // The webview can also be suspended or hidden by the OS without the shell
  // hearing about it; either signal is reason enough to stop animating (§7.4).
  useEffect(() => {
    const update = () => setDocumentHidden(document.hidden);
    document.addEventListener("visibilitychange", update);
    return () => document.removeEventListener("visibilitychange", update);
  }, []);

  const classes = [
    "clawd-root",
    occluded && "clawd-root--occluded",
    reducedMotion && "clawd-root--still",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div
      className={classes}
      style={{ ["--clawd-size" as string]: `${spriteSize}px` }}
      onPointerEnter={() => setHovered(true)}
      onPointerLeave={() => setHovered(false)}
      {...handlers}
    >
      <Clawd
        state={snapshot.state}
        label={snapshot.label}
        labelVisible={labelVisible}
        dragging={dragging}
      />
    </div>
  );
}
