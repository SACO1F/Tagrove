import { t as ui } from "./i18n";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalSize } from "@tauri-apps/api/dpi";

const directions = [
  "North",
  "NorthEast",
  "East",
  "SouthEast",
  "South",
  "SouthWest",
  "West",
  "NorthWest",
] as const;
export function FloatingResizeHandles({
  onError,
}: {
  onError: (error: unknown) => void;
}) {
  return (
    <>
      {directions.map((direction) => (
        <div
          key={direction}
          className={`float-resize resize-${direction}`}
          role={direction === "SouthEast" ? "button" : undefined}
          tabIndex={direction === "SouthEast" ? 0 : undefined}
          aria-label={
            direction === "SouthEast" ? ui("调整标签浮窗大小") : undefined
          }
          data-tooltip={ui("拖动调整浮窗大小")}
          onPointerDown={(e) => {
            if (e.button !== 0) return;
            e.preventDefault();
            e.stopPropagation();
            void getCurrentWebviewWindow()
              .startResizeDragging(direction)
              .catch(onError);
          }}
          onKeyDown={(e) => {
            const delta = {
              ArrowLeft: [-16, 0],
              ArrowRight: [16, 0],
              ArrowUp: [0, -16],
              ArrowDown: [0, 16],
            }[e.key];
            if (!delta) return;
            e.preventDefault();
            e.stopPropagation();
            const win = getCurrentWebviewWindow();
            void (async () => {
              const scale = await win.scaleFactor();
              const size = (await win.innerSize()).toLogical(scale);
              await win.setSize(
                new LogicalSize(
                  Math.max(280, Math.min(900, size.width + delta[0])),
                  Math.max(320, Math.min(1000, size.height + delta[1])),
                ),
              );
            })().catch(onError);
          }}
        />
      ))}
    </>
  );
}
