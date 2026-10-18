import { t as ui } from "./i18n";
import { useEffect, useState, type ReactNode } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Minus, Square, Copy, X } from "lucide-react";
import { message } from "./api";

export function StartupWindow({ children }: { children: ReactNode }) {
  const [error, setError] = useState("");
  return (
    <div className="startup-window">
      <header className="topbar startup-topbar">
        <div
          className="window-drag-region"
          aria-label={ui("拖动窗口")}
          onMouseDown={(event) => {
            if (event.button === 0 && event.detail === 1)
              void getCurrentWebviewWindow()
                .startDragging()
                .catch((e) => setError(message(e)));
          }}
          onDoubleClick={() =>
            void getCurrentWebviewWindow()
              .toggleMaximize()
              .catch((e) => setError(message(e)))
          }
        />
        <WindowControls onError={setError} />
      </header>
      {children}
      {error && (
        <div className="toast error" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}

export function WindowControls({
  onError,
}: {
  onError: (text: string) => void;
}) {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    const win = getCurrentWebviewWindow();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const update = () =>
      win
        .isMaximized()
        .then((value) => {
          if (!disposed) setMaximized(value);
        })
        .catch(() => {});
    void update();
    void win
      .onResized(update)
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  const action = (task: () => Promise<void>) =>
    void task().catch((error) => onError(message(error)));
  return (
    <div className="window-controls" role="group" aria-label={ui("窗口控制")}>
      <button
        aria-label={ui("最小化窗口")}
        data-tooltip={ui("最小化")}
        onClick={() => action(() => getCurrentWebviewWindow().minimize())}
      >
        <Minus size={14} />
      </button>
      <button
        aria-label={maximized ? ui("还原窗口") : ui("最大化窗口")}
        data-tooltip={maximized ? ui("还原") : ui("最大化")}
        onClick={() => action(() => getCurrentWebviewWindow().toggleMaximize())}
      >
        {maximized ? <Copy size={12} /> : <Square size={12} />}
      </button>
      <button
        className="window-close"
        aria-label={ui("关闭窗口")}
        data-tooltip={ui("关闭")}
        onClick={() => action(() => getCurrentWebviewWindow().close())}
      >
        <X size={16} />
      </button>
      {!maximized &&
        (
          [
            "North",
            "NorthEast",
            "East",
            "SouthEast",
            "South",
            "SouthWest",
            "West",
            "NorthWest",
          ] as const
        ).map((direction) => (
          <div
            key={direction}
            className={`window-resize window-resize-${direction}`}
            onPointerDown={(event) => {
              if (event.button !== 0) return;
              event.preventDefault();
              event.stopPropagation();
              action(() =>
                getCurrentWebviewWindow().startResizeDragging(direction),
              );
            }}
          />
        ))}
    </div>
  );
}
