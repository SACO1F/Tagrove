import { t as ui, tf } from "./i18n";
import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import { Minus, Plus } from "lucide-react";
import { useReadingValue } from "./ReadingState";
import { GlideSelect } from "./GlideSelect";
import "./reader-zoom.css";

export type ReaderZoomValue = number | "auto" | "page" | "width";
export const clampZoom = (value: number) =>
  Math.min(400, Math.max(25, Math.round(value)));
export type ZoomPoint = { x: number; y: number };
export function useZoomWheel(
  ref: RefObject<HTMLElement | null>,
  step: (direction: number, point?: ZoomPoint) => void,
  enabled = true,
) {
  const latest = useRef(step);
  latest.current = step;
  useEffect(() => {
    const el = ref.current;
    if (!el || !enabled) return;
    let accumulated = 0,
      last = 0;
    const wheel = (event: WheelEvent) => {
      if (!(event.ctrlKey || event.metaKey)) return;
      event.preventDefault();
      event.stopPropagation();
      const now = performance.now();
      if (now - last > 180) accumulated = 0;
      last = now;
      accumulated +=
        event.deltaY *
        (event.deltaMode === 1
          ? 16
          : event.deltaMode === 2
            ? el.clientHeight
            : 1);
      if (Math.abs(accumulated) >= 12) {
        latest.current(accumulated < 0 ? 1 : -1, {
          x: event.clientX,
          y: event.clientY,
        });
        accumulated = 0;
      }
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  }, [ref, enabled]);
}

export function ReaderZoomControl({
  value,
  scale,
  onChange,
  label = "内容",
  disabled = false,
  paged = true,
  onStep,
}: {
  value: ReaderZoomValue;
  scale: number;
  onChange: (value: ReaderZoomValue) => void;
  label?: string;
  disabled?: boolean;
  paged?: boolean;
  onStep?: (direction: number) => void;
}) {
  const options = [
    { value: "auto", label: ui("自动缩放") },
    { value: "actual", label: ui("实际大小") },
    ...(paged ? [{ value: "page", label: ui("适合页面") }] : []),
    { value: "width", label: ui("适合宽度") },
    ...[50, 75, 100, 125, 150, 200, 300, 400].map((n) => ({
      value: String(n),
      label: `${n}%`,
    })),
  ];
  if (
    typeof value === "number" &&
    !options.some((option) => option.value === String(value))
  )
    options.push({ value: String(value), label: `${value}%` });
  const step = (direction: number) =>
    onStep
      ? onStep(direction)
      : onChange(clampZoom(scale * 100 + direction * 25));
  return (
    <div
      className="reader-zoom-controls"
      role="group"
      aria-label={tf("{0}缩放工具", ui(label))}
    >
      <button
        className="icon-button"
        aria-label={tf("缩小{0}", ui(label))}
        data-tooltip={ui("缩小 · Ctrl＋滚轮")}
        disabled={disabled || (typeof value === "number" && value <= 25)}
        onClick={() => step(-1)}
      >
        <Minus size={16} />
      </button>
      <GlideSelect
        value={String(value)}
        options={options}
        ariaLabel={tf("{0}缩放比例", ui(label))}
        disabled={disabled}
        onChange={(next) =>
          onChange(
            next === "actual"
              ? 100
              : ["auto", "page", "width"].includes(next)
                ? (next as ReaderZoomValue)
                : Number(next),
          )
        }
      />
      <button
        className="icon-button"
        aria-label={tf("放大{0}", ui(label))}
        data-tooltip={ui("放大 · Ctrl＋滚轮")}
        disabled={disabled || (typeof value === "number" && value >= 400)}
        onClick={() => step(1)}
      >
        <Plus size={16} />
      </button>
    </div>
  );
}

export function useScrollZoomAnchor(
  ref: RefObject<HTMLElement | null>,
  scale: number,
) {
  const pending = useRef<
    | {
        el: HTMLElement;
        x: number;
        y: number;
        left: number;
        top: number;
        scale: number;
        target: Element | null;
        tx: number;
        ty: number;
        vx: number;
        vy: number;
      }
    | undefined
  >(undefined);
  const capture = (point?: ZoomPoint) => {
    if (!point) return;
    const root = ref.current;
    if (!root) return;
    const selectors =
      ".office-word-scroll,.office-sheet-scroll,.presentation-scroll,.html-source,.document-plain-text,.markdown-editor,.web-content";
    const hit = document.elementFromPoint(point.x, point.y);
    const el = (hit?.closest(selectors) ??
      root.querySelector(selectors)) as HTMLElement | null;
    if (!el || !root.contains(el)) return;
    const r = el.getBoundingClientRect();
    const target = hit?.closest("p,td,th,tr,h1,h2,h3,pre,textarea") ?? null;
    const t = target?.getBoundingClientRect();
    pending.current = {
      el,
      x: point.x - r.left,
      y: point.y - r.top,
      left: el.scrollLeft,
      top: el.scrollTop,
      scale,
      target,
      tx: t ? point.x - t.left : 0,
      ty: t ? point.y - t.top : 0,
      vx: point.x,
      vy: point.y,
    };
  };
  useLayoutEffect(() => {
    const p = pending.current;
    if (!p) return;
    pending.current = undefined;
    const ratio = scale / p.scale;
    if (p.target?.isConnected) {
      const r = p.target.getBoundingClientRect();
      p.el.scrollLeft += (r.left + p.tx * ratio - p.vx) / scale;
      p.el.scrollTop += (r.top + p.ty * ratio - p.vy) / scale;
    } else {
      p.el.scrollLeft = p.left + p.x / p.scale - p.x / scale;
      p.el.scrollTop = p.top + p.y / p.scale - p.y / scale;
    }
  }, [scale]);
  return capture;
}
const ZoomContext = createContext({
  scale: 1,
  step: (_direction: number) => {},
});
export const useReaderZoom = () => useContext(ZoomContext);
export function FlowZoomReader({ children }: { children: ReactNode }) {
  const [zoom, setZoom] = useReadingValue<ReaderZoomValue>("flowZoom", "page");
  // Flowing documents already reflow to their viewport at actual size.
  const scale = typeof zoom === "number" ? zoom / 100 : 1;
  const root = useRef<HTMLDivElement>(null);
  const anchor = useScrollZoomAnchor(root, scale);
  const step = (direction: number, point?: ZoomPoint) => {
    anchor(point);
    setZoom(clampZoom(scale * 100 + direction * 25));
  };
  useZoomWheel(root, step);
  return (
    <ZoomContext.Provider value={{ scale, step }}>
      <section
        className="flow-zoom-reader"
        style={{ "--reader-content-scale": scale } as React.CSSProperties}
      >
        <div className="reader-zoom-bar">
          <ReaderZoomControl value={zoom} scale={scale} onChange={setZoom} />
        </div>
        <div className="flow-zoom-body" ref={root}>
          {children}
        </div>
      </section>
    </ZoomContext.Provider>
  );
}
