import { useState } from "react";
import { Clawd } from "../renderer/Clawd";
import { STATE_NAMES, STATES, type ClawdState } from "../state";
import "./gallery.css";

/**
 * A browser-only board of all ten states, laid out like the prototype's
 * "Ten states" section.
 *
 * This is the §9.3 judgement call made cheap: do these states read at 80px,
 * side by side, before anything is wired to a desktop. It drives the renderer
 * directly and is *only* a visual check — it deliberately does not stand in
 * for the pipeline tests, which is the whole point of §8.1.
 */

const LABELS: Record<ClawdState, string> = {
  idle: "Ready",
  thinking: "Thinking…",
  working: "Editing App.tsx",
  needsInput: "Your turn",
  needsPermission: "Allow edit?",
  success: "Done in 42s",
  failed: "Rate limited",
  paused: "Paused",
  // The prototype's "Test suite 63%" cannot be derived from a hook payload,
  // so the long-task chip keeps naming the tool and the meter carries the
  // elapsed read instead (§5.3).
  longTask: "Running npm test",
  compacting: "Compacting…",
};

const NOTES: Record<ClawdState, string> = {
  idle: "Slow float, blinks every few seconds. No label unless you hover.",
  thinking: "Eyes drift upward, three pixels cycle overhead.",
  working: "Paws drum on the desktop edge. Label names the tool.",
  needsInput: "Eyes go wide, body brightens, one pulse ring every two seconds.",
  needsPermission: "Amber ring, eyes pushed apart, a shake every two seconds.",
  success: "Eyes squint, one celebratory hop, green ring flashes once.",
  failed: "Deep red, brows drop, eyes sag. Label stays put.",
  paused: "All colour drains out and motion stops completely.",
  longTask: "A slower bob, and a six-block elapsed meter under the body.",
  compacting: "Squashes and stretches while two pixels fold inward.",
};

export function Gallery() {
  const [size, setSize] = useState(80);
  const [still, setStill] = useState(false);
  const [occluded, setOccluded] = useState(false);
  const [dragging, setDragging] = useState(false);
  // Clawd sits on whatever desktop the user has. Colour has to carry the
  // urgency read on both (§7.3), so both are one click away here.
  const [ground, setGround] = useState<"dark" | "light">("dark");

  const rootClass = [
    "clawd-root",
    "gallery__stage",
    still && "clawd-root--still",
    occluded && "clawd-root--occluded",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div className="gallery">
      <header className="gallery__header">
        <h1>Clawd — ten states</h1>
        <p>
          Pose, motion and label for every thing the agent can be doing. Rendered by the
          same components the pet window uses.
        </p>
        <div className="gallery__controls">
          <label>
            Sprite
            <input
              type="range"
              min={48}
              max={160}
              step={16}
              value={size}
              onChange={(e) => setSize(Number(e.target.value))}
            />
            <b>{size}px</b>
          </label>
          <label>
            <input type="checkbox" checked={still} onChange={(e) => setStill(e.target.checked)} />
            Reduced motion
          </label>
          <label>
            <input
              type="checkbox"
              checked={occluded}
              onChange={(e) => setOccluded(e.target.checked)}
            />
            Occluded
          </label>
          <label>
            <input
              type="checkbox"
              checked={dragging}
              onChange={(e) => setDragging(e.target.checked)}
            />
            Dragging
          </label>
          <label>
            <input
              type="checkbox"
              checked={ground === "light"}
              onChange={(e) => setGround(e.target.checked ? "light" : "dark")}
            />
            Light desktop
          </label>
        </div>
      </header>

      <div className={rootClass} style={{ ["--clawd-size" as string]: `${size}px` }}>
        {STATES.map((state, i) => (
          <section className={`gallery__cell gallery__cell--${ground}`} key={state}>
            <div className="gallery__index">
              <span>{String(i + 1).padStart(2, "0")}</span>
              <span className="gallery__key">{state}</span>
            </div>
            <div className="gallery__pet">
              <Clawd state={state} label={LABELS[state]} labelVisible dragging={dragging} />
            </div>
            <h3>{STATE_NAMES[state]}</h3>
            <p>{NOTES[state]}</p>
          </section>
        ))}
      </div>
    </div>
  );
}
