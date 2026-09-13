import { useCallback, useRef, useState } from "react";
import { bridge } from "./bridge";

/**
 * Manual drag (§6.5).
 *
 * The OS drag handle is not used, because handing the drag to the OS gives
 * away the ability to apply the tilt. Pointer capture is what makes this work:
 * once captured, the webview keeps receiving moves after the cursor leaves the
 * window, so the pet does not get dropped the moment it overtakes the pointer.
 */
export function useDrag() {
  const [dragging, setDragging] = useState(false);
  const last = useRef({ x: 0, y: 0 });

  const onPointerDown = useCallback((e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.target as Element).setPointerCapture(e.pointerId);
    last.current = { x: e.screenX, y: e.screenY };
    setDragging(true);
  }, []);

  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!dragging) return;
      const dx = e.screenX - last.current.x;
      const dy = e.screenY - last.current.y;
      if (dx === 0 && dy === 0) return;
      last.current = { x: e.screenX, y: e.screenY };
      void bridge.moveWindow(dx, dy);
    },
    [dragging],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent) => {
      if (!dragging) return;
      (e.target as Element).releasePointerCapture?.(e.pointerId);
      setDragging(false);
      // The shell persists the position per display and re-runs the DPI snap,
      // in case the pet crossed onto a monitor with a different scale factor.
      void bridge.dragFinished();
    },
    [dragging],
  );

  return { dragging, handlers: { onPointerDown, onPointerMove, onPointerUp, onPointerCancel: onPointerUp } };
}
