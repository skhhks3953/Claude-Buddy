import type { ClawdState } from "../state";
import { Badge } from "./Badge";
import { Chip } from "./Chip";
import { Eyes } from "./Eyes";
import { Folds, Meter, Paws, Ring, Thought } from "./Parts";
import "./clawd.css";

/**
 * The isolated renderer boundary (§7.1).
 *
 * It takes a state plus label text and draws. No timers, no derivation, no
 * knowledge of sessions or hooks. Be strict about this: every tempting
 * shortcut involves the renderer keeping a little state of its own, and this
 * single rule is what keeps a swap to canvas reachable if WebView2's footprint
 * misses budget (§11).
 *
 * `dragging` and `labelVisible` are decided outside and passed in; drawing
 * them is still drawing.
 */
export interface ClawdProps {
  state: ClawdState;
  label: string;
  labelVisible: boolean;
  dragging: boolean;
}

const RINGED: ClawdState[] = ["needsInput", "needsPermission", "success"];

export function Clawd({ state, label, labelVisible, dragging }: ClawdProps) {
  return (
    <div className="clawd-figure">
      <div
        className={`clawd clawd--${state}${dragging ? " clawd--dragging" : ""}`}
        data-state={state}
        data-drag-handle="true"
      >
        <div className="clawd__pose">
          {RINGED.includes(state) && <Ring />}
          <div className="clawd__body" />
          <Eyes state={state} dragging={dragging} />
          {state === "working" && <Paws />}
          {state === "thinking" && <Thought />}
          {state === "compacting" && <Folds />}
          {state === "longTask" && <Meter />}
          <Badge state={state} />
        </div>
      </div>
      <Chip state={state} text={label} visible={labelVisible} />
    </div>
  );
}
