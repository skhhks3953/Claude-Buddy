import type { Snapshot } from "./state";

/**
 * The webview side of the IPC (§3.1 `ipc`).
 *
 * The shell pushes state changes down; hover and drag go back up. Nothing here
 * derives anything — React is a pure render target (§3).
 *
 * Every call degrades to a no-op outside Tauri, so the gallery and the unit
 * tests can mount the same components in a plain browser.
 */

export const SNAPSHOT_EVENT = "clawd://snapshot";
export const LAYOUT_EVENT = "clawd://layout";
export const OCCLUSION_EVENT = "clawd://occlusion";

export interface Layout {
  /** Sprite edge in CSS pixels, already snapped to the display's grid (§6.4). */
  spriteSize: number;
}

export const inTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Unlisten = () => void;
const noop: Unlisten = () => {};

async function on<T>(event: string, handler: (payload: T) => void): Promise<Unlisten> {
  if (!inTauri()) return noop;
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, (e) => handler(e.payload));
}

async function call(command: string, args?: Record<string, unknown>): Promise<void> {
  if (!inTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke(command, args);
}

export const bridge = {
  onSnapshot: (handler: (s: Snapshot) => void) => on<Snapshot>(SNAPSHOT_EVENT, handler),
  onLayout: (handler: (l: Layout) => void) => on<Layout>(LAYOUT_EVENT, handler),
  onOcclusion: (handler: (occluded: boolean) => void) => on<boolean>(OCCLUSION_EVENT, handler),

  /** Ask the shell for the current snapshot and layout, on mount. */
  hello: () => call("hello"),

  /** Manual drag (§6.5): the shell owns the window, we own the tilt. */
  moveWindow: (dx: number, dy: number) => call("move_window", { dx, dy }),
  dragFinished: () => call("drag_finished"),

  /**
   * Dev harness only. Injects into MockSource, so the event flows through the
   * real state machine and the real IPC rather than straight to the view
   * (§8.1). Compiled out of release builds on both sides.
   */
  devInject: (action: string) => call("dev_inject", { action }),
};
