import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { reducedMotion } from "./microMotion";
type Drag = {
  id: string;
  group: string;
  pointer: number;
  startX: number;
  startY: number;
  x: number;
  y: number;
  width: number;
  height: number;
  dx: number;
  dy: number;
  started: boolean;
  original: string[];
  order: string[];
};
export function useFloatingSort(
  host: RefObject<HTMLDivElement | null>,
  ids: string[],
  enabled: boolean,
  onSave: (ids: string[]) => Promise<void>,
  onAssign: (id: string, group: string) => Promise<void>,
) {
  const [targetGroup, setTargetGroup] = useState<string>();
  const [preview, setPreview] = useState<string[]>(),
    [ghost, setGhost] = useState<{
      id: string;
      x: number;
      y: number;
      width: number;
      height: number;
    }>();
  const active = useRef<Drag>(undefined),
    save = useRef(onSave),
    assign = useRef(onAssign),
    source = useRef(ids),
    suppressed = useRef(false),
    frame = useRef(0),
    rects = useRef(new Map<string, DOMRect>()),
    animations = useRef(new Map<Element, Animation>());
  save.current = onSave;
  assign.current = onAssign;
  source.current = ids;
  const clear = () => {
    cancelAnimationFrame(frame.current);
    active.current = undefined;
    setGhost(undefined);
    setTargetGroup(undefined);
    setPreview(undefined);
  };
  const cancel = () => {
    if (active.current?.started) suppressed.current = true;
    clear();
  };
  const sourceGroup = (id: string) =>
    [...(host.current?.querySelectorAll<HTMLElement>("[data-preset-id]") ?? [])]
      .find((el) => el.dataset.presetId === id)
      ?.closest<HTMLElement>("[data-sort-group]");
  const inGroup = (id: string, x: number, y: number) => {
    const g = sourceGroup(id);
    if (!g) return true;
    const r = g.getBoundingClientRect();
    return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
  };
  const categoryAt = (x: number, y: number) => {
    const area = host.current?.getBoundingClientRect();
    if (
      !area ||
      x < area.left ||
      x > area.right ||
      y < area.top ||
      y > area.bottom
    )
      return;
    return [
      ...(host.current?.querySelectorAll<HTMLElement>(".tag-branch-heading") ??
        []),
    ]
      .find((heading) => {
        const r = heading.getBoundingClientRect();
        return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
      })
      ?.closest<HTMLElement>("[data-sort-group]")?.dataset.sortGroup;
  };
  const update = () => {
    const d = active.current,
      el = host.current;
    if (!d?.started || !el) return;
    setGhost({
      id: d.id,
      x: d.x - d.dx,
      y: d.y - d.dy,
      width: d.width,
      height: d.height,
    });
    const category = categoryAt(d.x, d.y);
    setTargetGroup(category && category !== d.group ? category : undefined);
    if (category && category !== d.group) return;
    const area = el.getBoundingClientRect();
    if (
      d.x < area.left ||
      d.x > area.right ||
      d.y < area.top ||
      d.y > area.bottom
    )
      return;
    if ((sourceGroup(d.id)?.dataset.sortGroup ?? "") !== d.group) {
      cancel();
      return;
    }
    if (!inGroup(d.id, d.x, d.y)) return;
    const group = sourceGroup(d.id);
    const chips = [...el.querySelectorAll<HTMLElement>("[data-preset-id]")];
    let target: HTMLElement | undefined,
      distance = Infinity;
    for (const chip of chips) {
      if (group && chip.closest("[data-sort-group]") !== group) continue;
      let left = chip.offsetLeft,
        top = chip.offsetTop,
        parent = chip.offsetParent as HTMLElement | null;
      while (parent && parent !== el) {
        left += parent.offsetLeft;
        top += parent.offsetTop;
        parent = parent.offsetParent as HTMLElement | null;
      }
      const cx = area.left + left + chip.offsetWidth / 2,
        cy = area.top + top - el.scrollTop + chip.offsetHeight / 2;
      const n = Math.hypot(d.x - cx, d.y - cy);
      if (n < distance) {
        distance = n;
        target = chip;
      }
    }
    const id = target?.dataset.presetId;
    if (!id || id === d.id) return;
    const from = d.order.indexOf(d.id),
      to = d.order.indexOf(id);
    if (from < 0 || to < 0 || from === to) return;
    rects.current = new Map(
      chips.map((c) => [c.dataset.presetId!, c.getBoundingClientRect()]),
    );
    const next = [...d.order];
    next.splice(from, 1);
    next.splice(to, 0, d.id);
    d.order = next;
    setPreview(next);
  };
  const scroll = () => {
    const d = active.current,
      el = host.current;
    if (!d?.started || !el) return;
    const r = el.getBoundingClientRect();
    if (d.x >= r.left && d.x <= r.right) {
      const top = d.y - r.top,
        bottom = r.bottom - d.y;
      const delta =
        top < 38
          ? -Math.min(8, Math.max(0, 38 - top) / 5)
          : bottom < 42
            ? Math.min(8, Math.max(0, 42 - bottom) / 5)
            : 0;
      if (delta) {
        el.scrollTop += delta;
        update();
      }
    }
    frame.current = requestAnimationFrame(scroll);
  };
  useLayoutEffect(() => {
    const el = host.current;
    if (!el || !preview) return;
    for (const chip of el.querySelectorAll<HTMLElement>("[data-preset-id]")) {
      const old = rects.current.get(chip.dataset.presetId!);
      animations.current.get(chip)?.cancel();
      const next = chip.getBoundingClientRect();
      if (
        old &&
        !reducedMotion() &&
        (Math.abs(old.left - next.left) > 1 || Math.abs(old.top - next.top) > 1)
      ) {
        animations.current.set(
          chip,
          chip.animate(
            [
              {
                transform: `translate(${old.left - next.left}px,${old.top - next.top}px)`,
              },
              { transform: "translate(0,0)" },
            ],
            { duration: 180, easing: "cubic-bezier(.22,.61,.36,1)" },
          ),
        );
      }
    }
    rects.current.clear();
  }, [preview, host]);
  useEffect(() => {
    if (
      active.current &&
      JSON.stringify(ids) !== JSON.stringify(active.current.original)
    )
      cancel();
  }, [ids]);
  useEffect(() => {
    if (!enabled && active.current) cancel();
  }, [enabled]);
  useEffect(() => {
    const move = (e: PointerEvent) => {
      const d = active.current;
      if (!d || e.pointerId !== d.pointer) return;
      d.x = e.clientX;
      d.y = e.clientY;
      if (!d.started) {
        if (Math.hypot(d.x - d.startX, d.y - d.startY) < 6) return;
        d.started = true;
        suppressed.current = true;
        setPreview(d.order);
        frame.current = requestAnimationFrame(scroll);
      }
      e.preventDefault();
      update();
    };
    const up = (e: PointerEvent) => {
      const d = active.current;
      if (!d || e.pointerId !== d.pointer) return;
      cancelAnimationFrame(frame.current);
      active.current = undefined;
      setGhost(undefined);
      setTargetGroup(undefined);
      if (!d.started) {
        setPreview(undefined);
        return;
      }
      suppressed.current = true;
      const category = categoryAt(e.clientX, e.clientY);
      if (category && category !== d.group) {
        setPreview(undefined);
        void assign.current(d.id, category === "unclassified" ? "" : category);
        return;
      }
      const r = host.current?.getBoundingClientRect();
      if (
        !r ||
        (sourceGroup(d.id)?.dataset.sortGroup ?? "") !== d.group ||
        !inGroup(d.id, e.clientX, e.clientY) ||
        e.clientX < r.left ||
        e.clientX > r.right ||
        e.clientY < r.top ||
        e.clientY > r.bottom ||
        JSON.stringify(d.order) === JSON.stringify(d.original)
      ) {
        setPreview(undefined);
        return;
      }
      void save.current(d.order).finally(() => setPreview(undefined));
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" && active.current) {
        e.preventDefault();
        e.stopPropagation();
        cancel();
      }
    };
    const stop = () => cancel();
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", stop);
    window.addEventListener("keydown", key, true);
    window.addEventListener("blur", stop);
    window.addEventListener("resize", stop);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", stop);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("blur", stop);
      window.removeEventListener("resize", stop);
      cancelAnimationFrame(frame.current);
      for (const animation of animations.current.values()) animation.cancel();
    };
  }, []);
  function down(e: ReactPointerEvent<HTMLButtonElement>, id: string) {
    e.stopPropagation();
    suppressed.current = false;
    if (!enabled || e.button !== 0) return;
    const el = e.currentTarget.closest<HTMLElement>("[data-preset-id]");
    if (!el) return;
    const r = el.getBoundingClientRect();
    e.currentTarget.setPointerCapture(e.pointerId);
    active.current = {
      id,
      group: sourceGroup(id)?.dataset.sortGroup ?? "",
      pointer: e.pointerId,
      startX: e.clientX,
      startY: e.clientY,
      x: e.clientX,
      y: e.clientY,
      width: r.width,
      height: r.height,
      dx: e.clientX - r.left,
      dy: e.clientY - r.top,
      started: false,
      original: [...source.current],
      order: [...source.current],
    };
  }
  return {
    ids: preview ?? ids,
    ghost,
    targetGroup,
    down,
    consumeClick: () => {
      if (suppressed.current) {
        suppressed.current = false;
        return true;
      }
      return false;
    },
  };
}
