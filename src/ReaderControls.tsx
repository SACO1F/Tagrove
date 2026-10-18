import { useEffect, useLayoutEffect, useRef, type HTMLAttributes } from "react";
import "./reader-controls.css";

/** A local, theme-aware segmented control; animation never resizes its labels. */
export function RubberSegment({
  index,
  children,
  className = "",
  ...props
}: HTMLAttributes<HTMLDivElement> & { index: number }) {
  const host = useRef<HTMLDivElement>(null);
  const thumb = useRef<HTMLSpanElement>(null);
  const initialized = useRef(false);
  const animation = useRef<Animation | undefined>(undefined);
  useEffect(() => () => animation.current?.cancel(), []);
  useLayoutEffect(() => {
    const el = host.current,
      indicator = thumb.current;
    if (!el || !indicator) return;
    const position = (animate: boolean) => {
      const button =
        el.querySelectorAll<HTMLButtonElement>(":scope > button")[index];
      if (!button) return;
      const parent = el.getBoundingClientRect(),
        old = indicator.getBoundingClientRect();
      const left = button.offsetLeft,
        width = button.offsetWidth;
      const from = old.left - parent.left;
      const fromWidth = old.width;
      animation.current?.cancel();
      indicator.style.left = `${left}px`;
      indicator.style.width = `${width}px`;
      if (
        animate &&
        initialized.current &&
        !matchMedia("(prefers-reduced-motion: reduce)").matches
      ) {
        animation.current = indicator.animate(
          [
            { left: `${from}px`, width: `${fromWidth}px` },
            {
              left: `${Math.min(from, left)}px`,
              width: `${Math.max(fromWidth, width) + Math.abs(left - from) * 0.7}px`,
              offset: 0.42,
            },
            { left: `${left}px`, width: `${width}px` },
          ],
          { duration: 360, easing: "cubic-bezier(.22,.72,.2,1)" },
        );
      }
      initialized.current = true;
    };
    position(true);
    let measuredWidth = el.clientWidth;
    const observer = new ResizeObserver(() => {
      if (el.clientWidth === measuredWidth) return;
      measuredWidth = el.clientWidth;
      position(false);
    });
    observer.observe(el);
    return () => {
      observer.disconnect();
    };
  }, [index]);
  return (
    <div {...props} ref={host} className={`reader-segment ${className}`}>
      <span ref={thumb} className="reader-segment-thumb" aria-hidden="true" />
      {children}
    </div>
  );
}
