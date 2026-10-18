import { t as ui, tf, type UiCopy } from "./i18n";
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type PointerEvent,
} from "react";
import { Check, X, AlertCircle, Undo2 } from "lucide-react";
import "./workspace-toasts.css";

export type WorkspaceToast = {
  id: string;
  text: UiCopy;
  error: boolean;
  time: number;
  undoId?: string;
  actionLabel?: UiCopy;
  onAction?: () => void;
};
export function WorkspaceToasts({
  items,
  onDismiss,
  undoId,
  busy,
  onUndo,
}: {
  items: WorkspaceToast[];
  onDismiss: (id: string) => void;
  undoId?: string;
  busy: boolean;
  onUndo: (item: WorkspaceToast) => void;
}) {
  const root = useRef<HTMLElement>(null);
  useLayoutEffect(() => {
    const el = root.current,
      workspace = el?.closest(".app-shell")?.querySelector(".workspace"),
      heading = workspace?.querySelector(".page-heading"),
      add = heading?.querySelector(".add-button");
    if (!el || !workspace || !heading || !add) return;
    const update = () => {
      const a = add.getBoundingClientRect();
      const column = heading.parentElement!.getBoundingClientRect();
      el.style.left = `${column.left + column.width / 2}px`;
      el.style.top = `${a.top + a.height / 2 - 27}px`;
      el.style.width = `${Math.min(640, column.width * 0.8)}px`;
    };
    update();
    const observer = new ResizeObserver(update);
    [workspace, heading, heading.parentElement!, add].forEach((node) =>
      observer.observe(node),
    );
    window.addEventListener("resize", update);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", update);
    };
  }, []);
  return (
    <aside
      ref={root}
      className="workspace-toasts"
      aria-label={ui("最近操作提示")}
    >
      {items.map((item) => (
        <Toast
          key={item.id}
          item={item}
          onDismiss={onDismiss}
          canUndo={!busy && item.undoId === undoId}
          onUndo={onUndo}
        />
      ))}
    </aside>
  );
}
function Toast({
  item,
  onDismiss,
  canUndo,
  onUndo,
}: {
  canUndo: boolean;
  onUndo: (item: WorkspaceToast) => void;
  item: WorkspaceToast;
  onDismiss: (id: string) => void;
}) {
  const [closing, setClosing] = useState(false);
  const [offset, setOffset] = useState(0);
  const start = useRef<number | undefined>(undefined);
  const removal = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;
  function close() {
    setClosing(true);
  }
  useEffect(() => {
    const timer = setTimeout(close, Math.max(0, item.time + 5000 - Date.now()));
    return () => clearTimeout(timer);
  }, [item.time]);
  useEffect(() => {
    if (closing)
      removal.current = setTimeout(() => dismiss.current(item.id), 160);
    return () => clearTimeout(removal.current);
  }, [closing, item.id]);
  function down(e: PointerEvent<HTMLElement>) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    start.current = e.clientY;
    e.currentTarget.setPointerCapture(e.pointerId);
  }
  function release() {
    start.current = undefined;
    if (offset > 40) close();
    setOffset(0);
  }
  return (
    <div
      className={`workspace-toast ${item.error ? "error" : ""} ${closing ? "is-closing" : ""}`}
      role={item.error ? "alert" : "status"}
      style={{ transform: offset ? `translateY(${offset}px)` : undefined }}
      onPointerDown={down}
      onPointerMove={(e) => {
        if (start.current !== undefined)
          setOffset(Math.max(0, e.clientY - start.current));
      }}
      onPointerUp={release}
      onPointerCancel={() => {
        start.current = undefined;
        setOffset(0);
      }}
    >
      {item.error ? <AlertCircle size={17} /> : <Check size={17} />}
      <span data-tooltip={ui(item.text)}>{ui(item.text)}</span>
      {item.onAction && (
        <button
          className="toast-undo"
          onClick={() => {
            item.onAction?.();
            close();
          }}
        >
          {ui(item.actionLabel || "查看")}
        </button>
      )}
      {item.undoId && (
        <button
          className="toast-undo"
          aria-label={tf("撤销：{0}", ui(item.text))}
          data-tooltip={canUndo ? ui("撤销此操作") : ui("请先撤销后续操作")}
          disabled={!canUndo}
          onClick={() => onUndo(item)}
        >
          <Undo2 size={14} />
          {ui("撤销")}
        </button>
      )}
      <button
        className="icon-button"
        aria-label={ui("关闭通知")}
        onClick={close}
      >
        {item.onAction ? ui("暂不") : <X size={14} />}
      </button>
      <span className="toast-fuse" aria-hidden="true" />
    </div>
  );
}
