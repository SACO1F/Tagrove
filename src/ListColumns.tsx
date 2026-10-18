import { t as ui, tf } from "./i18n";
import { useState, useRef, type CSSProperties, type PointerEvent } from "react";
type Columns = [number, number, number, number];
const initial: Columns = [1.4, 1, 90, 65];
const storageKey = "shiqian:list-columns:v1";
function saved(): Columns {
  try {
    const v = JSON.parse(localStorage.getItem(storageKey) || "null");
    if (
      Array.isArray(v) &&
      v.length === 4 &&
      v.every((n) => typeof n === "number" && Number.isFinite(n) && n > 0)
    )
      return [
        Math.min(10, v[0]),
        Math.min(10, v[1]),
        Math.max(80, Math.min(220, v[2])),
        Math.max(55, Math.min(140, v[3])),
      ];
  } catch {
    /* Use defaults when local preferences are unavailable. */
  }
  return initial;
}
export function useListColumns() {
  const [columns, setColumns] = useState<Columns>(saved);
  const drag = useRef<
    | {
        pointer: number;
        x: number;
        index: number;
        widths: Columns;
        start: Columns;
        latest: Columns;
      }
    | undefined
  >(undefined);
  const persist = (v: Columns) => {
    try {
      localStorage.setItem(storageKey, JSON.stringify(v));
    } catch {
      /* Optional UI preference. */
    }
  };
  const widths = (el: HTMLElement): Columns | undefined => {
    const row = el.closest(".files-layout-motion")?.querySelector(".file-list");
    if (!row) return;
    return [
      row.children[1],
      row.children[2],
      row.children[3],
      row.children[4],
    ].map((n) => n.getBoundingClientRect().width) as Columns;
  };
  const resize = (w: Columns, index: number, delta: number): Columns => {
    const next = [...w] as Columns;
    const right = index + 1;
    const minimum = [130, 110, 80, 55];
    const maximum = [Infinity, Infinity, 220, 140];
    const change = Math.max(
      Math.max(minimum[index] - w[index], w[right] - maximum[right]),
      Math.min(
        Math.min(w[right] - minimum[right], maximum[index] - w[index]),
        delta,
      ),
    );
    next[index] += change;
    next[right] -= change;
    const sum = next[0] + next[1];
    return [
      (next[0] / sum) * 2.4,
      (next[1] / sum) * 2.4,
      next[2] || columns[2],
      next[3],
    ];
  };
  const reset = () => {
    setColumns(initial);
    persist(initial);
  };
  const finish = (e: PointerEvent<HTMLSpanElement>, cancel = false) => {
    const current = drag.current;
    if (!current || current.pointer !== e.pointerId) return;
    if (cancel) setColumns(current.start);
    else persist(current.latest);
    drag.current = undefined;
    if (e.currentTarget.hasPointerCapture(e.pointerId))
      e.currentTarget.releasePointerCapture(e.pointerId);
  };
  const handle = (index: number, label: string) => (
    <span
      className={`list-column-resizer boundary-${index}`}
      role="separator"
      aria-label={tf("调整{0}列宽", label)}
      aria-orientation="vertical"
      aria-valuenow={columns[index]}
      aria-valuetext={
        index < 2
          ? tf("列宽比例 {0}", columns[index].toFixed(2))
          : tf("{0} 像素", columns[index])
      }
      tabIndex={0}
      data-tooltip={ui("拖动调整列宽；双击恢复默认")}
      onPointerDown={(e) => {
        if (e.button !== 0 || !e.isPrimary) return;
        const w = widths(e.currentTarget);
        if (!w) return;
        e.preventDefault();
        e.stopPropagation();
        drag.current = {
          pointer: e.pointerId,
          x: e.clientX,
          index,
          widths: w,
          start: columns,
          latest: columns,
        };
        e.currentTarget.setPointerCapture(e.pointerId);
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d || d.pointer !== e.pointerId) return;
        const next = resize(d.widths, d.index, e.clientX - d.x);
        d.latest = next;
        setColumns(next);
      }}
      onPointerUp={(e) => finish(e)}
      onPointerCancel={(e) => finish(e, true)}
      onLostPointerCapture={(e) => {
        if (drag.current) finish(e, true);
      }}
      onDoubleClick={reset}
      onKeyDown={(e) => {
        if (e.key === "Home") {
          e.preventDefault();
          reset();
        } else if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
          e.preventDefault();
          const w = widths(e.currentTarget);
          if (w) {
            const next = resize(w, index, e.key === "ArrowRight" ? 12 : -12);
            setColumns(next);
            persist(next);
          }
        }
      }}
    />
  );
  return {
    style: {
      "--list-name": `${columns[0]}fr`,
      "--list-tags": `${columns[1]}fr`,
      "--list-date": `${columns[2]}px`,
      "--list-size": `${columns[3]}px`,
    } as CSSProperties,
    heading: (
      <div className="file-list-heading">
        <span>
          {ui("文件名称")}
          {handle(0, ui("文件名称"))}
        </span>
        <span>
          {ui("标签")}
          {handle(1, ui("标签"))}
        </span>
        <span>
          <span data-tooltip={ui("修改／采集时间")}>{ui("时间")}</span>
          {handle(2, ui("修改时间"))}
        </span>
        <span>{ui("大小")}</span>
      </div>
    ),
  };
}
