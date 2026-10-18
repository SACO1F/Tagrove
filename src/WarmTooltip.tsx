import {
  cloneElement,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactElement,
} from "react";
import { createPortal } from "react-dom";
import "./warm-tooltip.css";

// Delegation keeps tooltips consistent without adding wrappers to grids,
// resizers, virtual rows or disabled controls in either application window.
export function GlobalTooltips() {
  const id = useId();
  const panel = useRef<HTMLDivElement>(null);
  const [tip, setTip] = useState<{ anchor: HTMLElement; text: string }>();
  const [visible, setVisible] = useState(false);
  const [position, setPosition] = useState({ left: -10000, top: -10000 });
  useEffect(() => {
    let anchor: HTMLElement | undefined;
    let timer: ReturnType<typeof setTimeout>;
    let removal: ReturnType<typeof setTimeout>;
    const clearDescription = () => {
      if (!anchor) return;
      const tokens = (anchor.getAttribute("aria-describedby") || "")
        .split(/\s+/)
        .filter((token) => token && token !== id);
      if (tokens.length)
        anchor.setAttribute("aria-describedby", tokens.join(" "));
      else anchor.removeAttribute("aria-describedby");
    };
    const close = () => {
      clearTimeout(timer);
      clearTimeout(removal);
      clearDescription();
      anchor = undefined;
      setVisible(false);
      removal = setTimeout(() => setTip(undefined), 120);
    };
    const show = (event: Event) => {
      const target = event.target instanceof Element ? event.target : null;
      const next = target?.closest<HTMLElement>("[data-tooltip]");
      if (
        !next ||
        next.closest(".warm-tooltip-anchor") ||
        !next.dataset.tooltip?.trim()
      ) {
        if (anchor) close();
        return;
      }
      if (anchor === next) return;
      close();
      clearTimeout(removal);
      setTip(undefined);
      anchor = next;
      timer = setTimeout(() => {
        if (!next.isConnected) return close();
        const description = next.getAttribute("aria-describedby");
        next.setAttribute(
          "aria-describedby",
          [description, id].filter(Boolean).join(" "),
        );
        setTip({ anchor: next, text: next.dataset.tooltip! });
        setVisible(true);
      }, 180);
    };
    const leave = (event: MouseEvent) => {
      if (
        anchor &&
        (!(event.relatedTarget instanceof Node) ||
          !anchor.contains(event.relatedTarget))
      )
        close();
    };
    const keydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    document.addEventListener("pointerover", show);
    document.addEventListener("pointerout", leave);
    document.addEventListener("focusin", show);
    document.addEventListener("focusout", close);
    document.addEventListener("pointerdown", close, true);
    document.addEventListener("keydown", keydown);
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    const observer = new MutationObserver(() => {
      if (anchor && (!anchor.isConnected || !anchor.dataset.tooltip)) close();
      else if (anchor)
        setTip((current) =>
          current && current.text !== anchor!.dataset.tooltip
            ? { anchor: anchor!, text: anchor!.dataset.tooltip! }
            : current,
        );
    });
    observer.observe(document.body, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["data-tooltip"],
    });
    return () => {
      clearTimeout(timer);
      clearTimeout(removal);
      clearDescription();
      observer.disconnect();
      document.removeEventListener("pointerover", show);
      document.removeEventListener("pointerout", leave);
      document.removeEventListener("focusin", show);
      document.removeEventListener("focusout", close);
      document.removeEventListener("pointerdown", close, true);
      document.removeEventListener("keydown", keydown);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
    };
  }, [id]);
  useLayoutEffect(() => {
    if (!tip || !panel.current) return;
    const a = tip.anchor.getBoundingClientRect(),
      p = panel.current.getBoundingClientRect();
    setPosition({
      left: Math.max(
        8,
        Math.min(a.left + (a.width - p.width) / 2, innerWidth - p.width - 8),
      ),
      top:
        a.bottom + 9 + p.height <= innerHeight - 8
          ? a.bottom + 9
          : Math.max(8, a.top - p.height - 9),
    });
  }, [tip]);
  return (
    tip &&
    createPortal(
      <div
        ref={panel}
        id={id}
        role="tooltip"
        className="warm-tooltip"
        data-visible={visible}
        style={position}
      >
        {tip.text}
      </div>,
      document.body,
    )
  );
}

export function WarmTooltip({
  text,
  children,
  detail,
  swatch,
}: {
  text: string;
  detail?: string;
  swatch?: string;
  children: ReactElement<{ "aria-describedby"?: string }>;
}) {
  const id = useId();
  const anchor = useRef<HTMLSpanElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const [visible, setVisible] = useState(false);
  const [mounted, setMounted] = useState(false);
  const [position, setPosition] = useState({ left: -10000, top: -10000 });
  const show = () => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      setMounted(true);
      setVisible(true);
    }, 180);
  };
  const close = () => {
    clearTimeout(timer.current);
    setVisible(false);
    timer.current = setTimeout(() => setMounted(false), 120);
  };
  useEffect(() => () => clearTimeout(timer.current), []);
  useLayoutEffect(() => {
    if (!mounted || !anchor.current || !panel.current) return;
    const a = anchor.current.getBoundingClientRect();
    const p = panel.current.getBoundingClientRect();
    const below = a.bottom + 9;
    setPosition({
      left: Math.max(
        8,
        Math.min(a.left + (a.width - p.width) / 2, innerWidth - p.width - 8),
      ),
      top:
        below + p.height <= innerHeight - 8
          ? below
          : Math.max(8, a.top - p.height - 9),
    });
    const dismiss = () => close();
    window.addEventListener("scroll", dismiss, true);
    window.addEventListener("resize", dismiss);
    return () => {
      window.removeEventListener("scroll", dismiss, true);
      window.removeEventListener("resize", dismiss);
    };
  }, [mounted, text]);
  return (
    <>
      <span
        ref={anchor}
        className="warm-tooltip-anchor"
        onMouseEnter={show}
        onMouseLeave={close}
        onFocus={show}
        onBlur={close}
        onClick={close}
        onKeyDown={(event) => {
          if (event.key === "Escape" && mounted) {
            event.stopPropagation();
            close();
          }
        }}
      >
        {cloneElement(children, {
          "aria-describedby": mounted ? id : undefined,
        })}
      </span>
      {mounted &&
        createPortal(
          <div
            ref={panel}
            id={id}
            role="tooltip"
            className={`warm-tooltip ${detail ? "warm-tooltip-color" : ""}`}
            data-visible={visible}
            style={position}
          >
            {detail ? (
              <>
                <span
                  className="warm-tooltip-swatch"
                  style={{ background: swatch }}
                />
                <span className="warm-tooltip-copy">
                  <strong>{text}</strong>
                  <small>{detail}</small>
                </span>
              </>
            ) : (
              text
            )}
          </div>,
          document.body,
        )}
    </>
  );
}
