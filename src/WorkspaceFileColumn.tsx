import { useLayoutEffect, useRef, type ReactNode } from "react";

export function WorkspaceFileColumn({ children }: { children: ReactNode }) {
  const root = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const element = root.current;
    if (!element) return;
    const update = () => {
      const width = element.clientWidth;
      element.style.setProperty("--workspace-file-width", `${width}px`);
      element.dataset.size =
        width < 360 ? "narrow" : width < 660 ? "compact" : "wide";
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return (
    <div className="workspace-file-column" ref={root}>
      {children}
    </div>
  );
}
