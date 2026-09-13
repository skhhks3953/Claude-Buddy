/**
 * A mirror of the Rust state (§3.2). Dumb by design: no derivation happens
 * here, because the state machine lives in Rust where it survives the webview
 * being occluded, throttled or suspended.
 */

export const STATES = [
  "idle",
  "thinking",
  "working",
  "needsInput",
  "needsPermission",
  "success",
  "failed",
  "paused",
  "longTask",
  "compacting",
] as const;

export type ClawdState = (typeof STATES)[number];

export interface Snapshot {
  state: ClawdState;
  label: string;
}

/** States that are waiting on the human. Their label must not go quiet (§5.2). */
export function isBlocking(state: ClawdState): boolean {
  return state === "needsInput" || state === "needsPermission" || state === "failed";
}

/** Human-readable name, used by the gallery and nowhere in the pet window. */
export const STATE_NAMES: Record<ClawdState, string> = {
  idle: "Idle / ready",
  thinking: "Thinking",
  working: "Running a tool",
  needsInput: "Waiting for input",
  needsPermission: "Needs permission",
  success: "Done",
  failed: "Error",
  paused: "Paused / stopped",
  longTask: "Long task",
  compacting: "Compacting context",
};
