import { bridge } from "../bridge";
import { STATES } from "../state";

/**
 * Number keys inject synthetic events into MockSource, which flow through the
 * real state machine, the real IPC and the real renderer (§8.1).
 *
 * The keys do not set states. Pressing `9` asks for the tool call that *earns*
 * the long-task pose; the pose arrives only once the timer says so. A dev
 * panel that wrote straight to the view would let the state machine be wrong
 * while every state still looked correct — this cannot.
 */
const KEYS: Record<string, string> = {
  // 1-9 then 0, matching the table in §8.1.
  ...Object.fromEntries(STATES.map((state, i) => [String((i + 1) % 10), state])),
  // Beyond single states (§8.2).
  s: "script",
  o: "second-session",
  t: "fast-timers",
};

export function bindDevKeys(): void {
  window.addEventListener("keydown", (e) => {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const action = KEYS[e.key.toLowerCase()];
    if (!action) return;
    e.preventDefault();
    void bridge.devInject(action);
  });

  // eslint-disable-next-line no-console
  console.info(
    "[clawd dev] 1-0 states · s scripted run · o second session · t fast timers",
  );
}
