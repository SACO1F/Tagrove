import { useEffect, useRef, type RefObject } from "react";

export function useGalleryWheel(
  root: RefObject<HTMLDivElement | null>,
  enabled: boolean,
  columns: number,
  capacity: number,
  onChange: (columns: number) => void,
  ready: boolean,
) {
  const latest = useRef({ enabled, columns, capacity, onChange });
  latest.current = { enabled, columns, capacity, onChange };
  useEffect(() => {
    const element = root.current;
    if (!element || !ready) return;
    let delta = 0,
      last = 0;
    const wheel = (event: WheelEvent) => {
      const state = latest.current;
      if (
        !state.enabled ||
        !event.ctrlKey ||
        !(event.target instanceof Element) ||
        !event.target.closest(".file-area")
      )
        return;
      event.preventDefault();
      if (!event.deltaY) return;
      const amount =
        event.deltaY *
        (event.deltaMode === 1 ? 40 : event.deltaMode === 2 ? 120 : 1);
      const now = performance.now();
      if (now - last > 180 || Math.sign(delta) !== Math.sign(amount)) delta = 0;
      last = now;
      delta += Math.max(-120, Math.min(120, amount));
      if (Math.abs(delta) < 60) return;
      // Up enlarges cards (fewer columns); down shows more columns.
      const next = Math.max(
        2,
        Math.min(state.capacity, state.columns + (delta > 0 ? 1 : -1)),
      );
      delta = 0;
      if (next !== state.columns) {
        state.columns = next;
        state.onChange(next);
      }
    };
    element.addEventListener("wheel", wheel, { passive: false });
    return () => element.removeEventListener("wheel", wheel);
  }, [root, ready]);
}
