import { t as ui, tf } from "./i18n";
import { useRef, type PointerEvent } from "react";

export const SIDEBAR_MIN = 180;
export const SIDEBAR_MAX = 360;
export const SIDEBAR_COLLAPSE_AT = 160;
export const SIDEBAR_CLOSED = 68;
export function sidebarWidth(value: unknown) {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.max(SIDEBAR_MIN, Math.min(SIDEBAR_MAX, Math.round(value)))
    : 216;
}
type Size = { width: number; collapsed: boolean };
export function SidebarResizeHandle({
  width,
  collapsed,
  onChange,
  onCommit,
  onResizing,
}: Size & {
  onChange: (size: Size) => void;
  onCommit: (size: Size) => void;
  onResizing: (active: boolean) => void;
}) {
  const drag = useRef<
    | {
        pointer: number;
        x: number;
        start: Size;
        size: Size;
      }
    | undefined
  >(undefined);
  const finish = (e: PointerEvent<HTMLDivElement>, cancel = false) => {
    const current = drag.current;
    if (!current || current.pointer !== e.pointerId) return;
    drag.current = undefined;
    onResizing(false);
    if (cancel) onChange(current.start);
    else onCommit(current.size);
    if (e.currentTarget.hasPointerCapture(e.pointerId))
      e.currentTarget.releasePointerCapture(e.pointerId);
  };
  const change = (raw: number, savedWidth = width): Size => ({
    collapsed: raw < SIDEBAR_COLLAPSE_AT,
    width: raw < SIDEBAR_COLLAPSE_AT ? savedWidth : sidebarWidth(raw),
  });
  return (
    <div
      className="sidebar-resize-handle"
      role="separator"
      tabIndex={0}
      aria-label={ui("调整左侧菜单宽度")}
      aria-orientation="vertical"
      aria-valuemin={SIDEBAR_CLOSED}
      aria-valuemax={SIDEBAR_MAX}
      aria-valuenow={collapsed ? SIDEBAR_CLOSED : width}
      aria-valuetext={collapsed ? ui("已收起") : tf("{0} 像素", width)}
      data-tooltip={ui("拖动调整宽度；向左缩小可收起")}
      onPointerDown={(e) => {
        if (e.button !== 0 || !e.isPrimary) return;
        e.preventDefault();
        e.currentTarget.focus({ preventScroll: true });
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = {
          pointer: e.pointerId,
          x: e.clientX,
          start: { width, collapsed },
          size: { width, collapsed },
        };
        onResizing(true);
      }}
      onPointerMove={(e) => {
        const current = drag.current;
        if (!current || current.pointer !== e.pointerId) return;
        const raw =
          (current.start.collapsed ? SIDEBAR_CLOSED : current.start.width) +
          e.clientX -
          current.x;
        current.size = change(raw, current.size.width);
        onChange(current.size);
      }}
      onPointerUp={(e) => finish(e)}
      onPointerCancel={(e) => finish(e, true)}
      onLostPointerCapture={(e) => finish(e, true)}
      onDoubleClick={() => {
        const next = { width: 216, collapsed: false };
        onChange(next);
        onCommit(next);
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape" && drag.current) {
          e.preventDefault();
          e.stopPropagation();
          const current = drag.current;
          drag.current = undefined;
          onChange(current.start);
          onResizing(false);
          if (e.currentTarget.hasPointerCapture(current.pointer))
            e.currentTarget.releasePointerCapture(current.pointer);
          return;
        }
        let next: Size;
        if (e.key === "ArrowLeft")
          next = collapsed ? { width, collapsed } : change(width - 24);
        else if (e.key === "ArrowRight")
          next = collapsed ? { width, collapsed: false } : change(width + 24);
        else if (e.key === "Home") next = { width, collapsed: true };
        else if (e.key === "End")
          next = { width: SIDEBAR_MAX, collapsed: false };
        else if (e.key === "Enter") next = { width, collapsed: !collapsed };
        else return;
        e.preventDefault();
        e.stopPropagation();
        onChange(next);
        onCommit(next);
      }}
    />
  );
}
