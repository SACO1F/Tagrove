import { t as ui, tf } from "./i18n";
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";

export function inspectorWidth(value: unknown) {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.max(300, Math.min(900, Math.round(value)))
    : 420;
}
export function InspectorPane({
  open,
  width,
  onChange,
  onCommit,
  children,
}: {
  open: boolean;
  width: number;
  onChange: (width: number) => void;
  onCommit: (width: number) => void;
  children: ReactNode;
}) {
  const pane = useRef<HTMLDivElement>(null);
  const [available, setAvailable] = useState(1000),
    [resizing, setResizing] = useState(false),
    [fitting, setFitting] = useState(false);
  const drag = useRef<
    | {
        pointer: number;
        x: number;
        start: number;
        preferred: number;
        current: number;
      }
    | undefined
  >(undefined);
  useEffect(() => {
    const parent = pane.current?.parentElement;
    if (!parent) return;
    let timer: ReturnType<typeof setTimeout>;
    let previous = parent.clientWidth;
    const observer = new ResizeObserver(() => {
      const next = parent.clientWidth;
      if (next === previous) return;
      previous = next;
      setFitting(true);
      clearTimeout(timer);
      timer = setTimeout(() => setFitting(false), 180);
      setAvailable(next);
    });
    observer.observe(parent);
    setAvailable(parent.clientWidth);
    return () => {
      observer.disconnect();
      clearTimeout(timer);
    };
  }, []);
  const maximum = Math.max(300, Math.min(900, available - 240));
  const actual = Math.min(width, maximum);
  useLayoutEffect(() => {
    const shell = pane.current?.closest<HTMLElement>(".app-shell");
    shell?.style.setProperty(
      "--active-inspector-width",
      `${open ? actual : 0}px`,
    );
    return () => {
      shell?.style.removeProperty("--active-inspector-width");
    };
  }, [actual, open]);
  useEffect(() => {
    if (!resizing) return;
    const oldCursor = document.body.style.cursor,
      oldSelect = document.body.style.userSelect;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    return () => {
      document.body.style.cursor = oldCursor;
      document.body.style.userSelect = oldSelect;
    };
  }, [resizing]);
  const continuous = () =>
    window.dispatchEvent(new Event("shiqian-continuous-layout"));
  const change = (next: number) => {
    continuous();
    onChange(next);
  };
  const finish = (cancel = false) => {
    const d = drag.current;
    if (!d) return;
    drag.current = undefined;
    setResizing(false);
    continuous();
    if (cancel) onChange(d.preferred);
    else onCommit(d.current);
  };
  return (
    <div
      ref={pane}
      id="workspace-inspector"
      className={`inspector-reveal ${open ? "is-open" : ""} ${resizing ? "is-resizing" : ""} ${fitting ? "is-fitting" : ""}`}
      style={{ "--inspector-width": `${actual}px` } as CSSProperties}
      inert={!open}
      aria-hidden={!open}
    >
      <div
        className="inspector-resize-handle"
        role="separator"
        tabIndex={0}
        aria-label={ui("调整文件详情宽度")}
        aria-orientation="vertical"
        aria-valuemin={300}
        aria-valuemax={maximum}
        aria-valuenow={actual}
        aria-valuetext={tf("{0} 像素", actual)}
        data-tooltip={ui("拖动调整详情宽度；双击恢复默认")}
        onPointerDown={(e) => {
          if (e.button !== 0 || !e.isPrimary) return;
          e.preventDefault();
          e.currentTarget.focus({ preventScroll: true });
          e.currentTarget.setPointerCapture(e.pointerId);
          drag.current = {
            pointer: e.pointerId,
            x: e.clientX,
            start: actual,
            preferred: width,
            current: actual,
          };
          setResizing(true);
          continuous();
        }}
        onPointerMove={(e) => {
          const d = drag.current;
          if (!d || d.pointer !== e.pointerId) return;
          d.current = Math.round(
            Math.max(300, Math.min(maximum, d.start + d.x - e.clientX)),
          );
          change(d.current);
        }}
        onPointerUp={(e) => {
          if (drag.current?.pointer !== e.pointerId) return;
          finish();
          if (e.currentTarget.hasPointerCapture(e.pointerId))
            e.currentTarget.releasePointerCapture(e.pointerId);
        }}
        onPointerCancel={() => finish(true)}
        onLostPointerCapture={() => finish(true)}
        onDoubleClick={() => {
          const next = 420;
          change(next);
          onCommit(next);
        }}
        onKeyDown={(e) => {
          if (e.key === "Escape" && drag.current) {
            e.preventDefault();
            e.stopPropagation();
            const pointer = drag.current.pointer;
            finish(true);
            if (e.currentTarget.hasPointerCapture(pointer))
              e.currentTarget.releasePointerCapture(pointer);
            return;
          }
          const raw =
            e.key === "ArrowLeft"
              ? actual + 24
              : e.key === "ArrowRight"
                ? actual - 24
                : e.key === "Home"
                  ? 300
                  : e.key === "End"
                    ? maximum
                    : undefined;
          if (raw === undefined) return;
          e.preventDefault();
          e.stopPropagation();
          const next = Math.max(300, Math.min(maximum, raw));
          change(next);
          onCommit(next);
        }}
      />
      {children}
    </div>
  );
}
