import { createTagDropTilt } from "./tagDropTilt";
import { t as ui, tf } from "./i18n";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { listen } from "@tauri-apps/api/event";
import { Check, LoaderCircle, Plus, Tag as TagIcon, X } from "lucide-react";
import { api, message, type Tag } from "./api";

type Point = {
  tagId: string;
  x: number;
  y: number;
  over: boolean;
  drop: boolean;
};
type Bounds = { left: number; top: number; width: number; height: number };
type Feedback = {
  phase: "drag" | "applying" | "success" | "error";
  tag: string;
  label: string;
  point: { x: number; y: number };
  bounds?: Bounds;
  clip?: Bounds;
  fileId?: string;
  tilt?: string;
};
type Annotation = { tagName: string; applied: number };
function flatBounds(card: HTMLElement | null | undefined) {
  if (!card) return;
  const wrapper = card.closest<HTMLElement>(".masonry-item, .virtual-row");
  const rect =
    wrapper && card.offsetParent === wrapper
      ? {
          left: wrapper.getBoundingClientRect().left + card.offsetLeft,
          top: wrapper.getBoundingClientRect().top + card.offsetTop,
          width: card.offsetWidth,
          height: card.offsetHeight,
        }
      : card.getBoundingClientRect();
  let left = 0;
  let top = 0;
  let right = innerWidth;
  let bottom = innerHeight;
  for (let parent = card.parentElement; parent; parent = parent.parentElement) {
    const style = getComputedStyle(parent);
    const r = parent.getBoundingClientRect();
    if (/(auto|scroll|hidden|clip)/.test(style.overflowX)) {
      left = Math.max(left, r.left + parent.clientLeft);
      right = Math.min(right, r.left + parent.clientLeft + parent.clientWidth);
    }
    if (/(auto|scroll|hidden|clip)/.test(style.overflowY)) {
      top = Math.max(top, r.top + parent.clientTop);
      bottom = Math.min(bottom, r.top + parent.clientTop + parent.clientHeight);
    }
  }
  if (
    Math.min(right, rect.left + rect.width) <= Math.max(left, rect.left) ||
    Math.min(bottom, rect.top + rect.height) <= Math.max(top, rect.top)
  )
    return;
  return {
    left: rect.left,
    top: rect.top,
    width: rect.width,
    height: rect.height,
    clip: { left, top, width: right - left, height: bottom - top },
  };
}

// Fade only the cropped edges; keep a full card's border and shadow intact.
function edgeMask(bounds: Bounds, clip: Bounds) {
  return (["vertical", "horizontal"] as const)
    .map((axis) => {
      const start = axis === "vertical" ? "top" : "left";
      const size = axis === "vertical" ? "height" : "width";
      const direction = axis === "vertical" ? "bottom" : "right";
      const end = clip[start] + clip[size];
      const fade = Math.min(12, clip[size] / 4);
      const fadeIn = bounds[start] < clip[start] ? fade : 0;
      const fadeOut = bounds[start] + bounds[size] > end ? fade : 0;
      return `linear-gradient(to ${direction}, transparent ${clip[start]}px, #000 ${clip[start] + fadeIn}px, #000 ${end - fadeOut}px, transparent ${end}px)`;
    })
    .join(", ");
}

export function TagDropFeedback({
  tags,
  tiltEnabled,
  run,
  onNotify,
}: {
  tags: Tag[];
  tiltEnabled: boolean;
  run: (task: () => Promise<unknown>) => Promise<void>;
  onNotify: (text: string, error?: boolean) => void;
}) {
  const cursor = useRef<HTMLDivElement>(null);
  const [cursorSize, setCursorSize] = useState({ width: 0, height: 0 });
  const tagsRef = useRef(tags);
  tagsRef.current = tags;
  const [feedback, setFeedback] = useState<Feedback>();
  useLayoutEffect(() => {
    if (!cursor.current) return;
    const measure = () => {
      const r = cursor.current!.getBoundingClientRect();
      setCursorSize((old) =>
        old.width === r.width && old.height === r.height
          ? old
          : { width: r.width, height: r.height },
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(cursor.current);
    return () => observer.disconnect();
  }, [feedback?.phase, feedback?.tag]);
  useLayoutEffect(() => {
    if (!feedback?.fileId || feedback.phase === "drag") return;
    const card = document.querySelector<HTMLElement>(
      `[data-file-id="${CSS.escape(feedback.fileId)}"]`,
    );
    const r = flatBounds(card);
    const next = r
      ? { left: r.left, top: r.top, width: r.width, height: r.height }
      : undefined;
    const previous = feedback.bounds;
    if (
      next?.left !== previous?.left ||
      next?.top !== previous?.top ||
      next?.width !== previous?.width ||
      next?.height !== previous?.height ||
      r?.clip.left !== feedback.clip?.left ||
      r?.clip.top !== feedback.clip?.top ||
      r?.clip.width !== feedback.clip?.width ||
      r?.clip.height !== feedback.clip?.height
    )
      setFeedback((old) =>
        old ? { ...old, bounds: next, clip: r?.clip } : old,
      );
  });
  useEffect(() => {
    const tilt = createTagDropTilt(tiltEnabled);
    let target: HTMLElement | null = null;
    let targetBounds: ReturnType<typeof flatBounds>;
    let pending = false;
    let dragging = false;
    let disposed = false;
    let finishTimer: ReturnType<typeof setTimeout> | undefined;
    const clearTarget = () => {
      target?.classList.remove("tag-drop-target");
      tilt.reset();
      target = null;
      targetBounds = undefined;
    };
    const reset = () => {
      if (!pending) {
        clearTarget();
        setFeedback(undefined);
      }
    };
    const finish = (next: Feedback) => {
      pending = false;
      clearTarget();
      if (disposed) return;
      setFeedback(next);
      clearTimeout(finishTimer);
      finishTimer = setTimeout(
        () => {
          if (!disposed) setFeedback(undefined);
        },
        next.phase === "error" ? 1800 : 1100,
      );
    };
    const subscriptions = [
      listen<Point>("tag-drag-point", ({ payload: p }) => {
        if (pending || disposed) return;
        clearTimeout(finishTimer);
        dragging = !p.drop && p.over;
        const card = p.over
          ? document
              .elementFromPoint(p.x, p.y)
              ?.closest<HTMLElement>("[data-file-id]") || null
          : null;
        // Read the flat card before any transform write; never measure its tilted rectangle.
        const measured =
          card === target && targetBounds ? targetBounds : flatBounds(card);
        const bounds = measured
          ? {
              left: measured.left,
              top: measured.top,
              width: measured.width,
              height: measured.height,
              clip: measured.clip,
            }
          : undefined;
        if (card !== target) {
          clearTarget();
          target = card;
          targetBounds = bounds;
        }
        if (target && !target.classList.contains("tag-drop-target"))
          target.classList.add("tag-drop-target");
        const tag =
          tagsRef.current.find((t) => t.id === p.tagId)?.name || ui("标签");
        const r = bounds;
        const transform = tilt.update(card, p.x, p.y, r);
        const next: Feedback = {
          phase: "drag",
          tag,
          label: card ? tf("松手添加「{0}」", tag) : ui("移到文件上"),
          point: { x: p.x, y: p.y },
          bounds: r
            ? { left: r.left, top: r.top, width: r.width, height: r.height }
            : undefined,
          clip: r?.clip,
          fileId: card?.dataset.fileId,
          tilt: transform,
        };
        if (p.drop && p.over) {
          clearTarget();
          next.tilt = undefined;
          if (next.fileId) {
            pending = true;
            setFeedback({ ...next, phase: "applying", label: ui("正在标注…") });
            void run(async () => {
              try {
                const result = await api<Annotation>("annotation.apply", {
                  tagId: p.tagId,
                  ids: [next.fileId],
                });
                finish({
                  ...next,
                  phase: "success",
                  label: result.applied
                    ? tf("已添加「{0}」", result.tagName)
                    : ui("已有此标签"),
                });
              } catch (e) {
                finish({
                  ...next,
                  phase: "error",
                  label: ui("标注失败，请重试"),
                });
                throw e;
              }
            });
          } else {
            finish({ ...next, phase: "error", label: ui("请放到具体文件上") });
            void api("annotation.reject").catch((e) =>
              onNotify(message(e), true),
            );
          }
        } else if (p.over) setFeedback(next);
        else reset();
      }),
      listen("tag-drag-end", () => {
        dragging = false;
        clearTarget();
        setFeedback((old) => (old?.phase === "drag" ? undefined : old));
      }),
      listen<{ ok: boolean; result?: Annotation; error?: string }>(
        "annotation-result",
        ({ payload: p }) => {
          // App records successful changes with file names and an undo action.
          // Keep only no-op and error notices here, avoiding duplicate success toasts.
          if (!p.ok) onNotify(message(p.error), true);
          else if (!p.result?.applied) onNotify(ui("文件已有此标签"));
        },
      ),
    ];
    const onEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape" && dragging && !pending) {
        e.preventDefault();
        e.stopImmediatePropagation();
        dragging = false;
        reset();
        void api("floating.cancel").catch(() => {});
      }
    };
    const onGeometryChange = () => {
      if (dragging && !pending) reset();
    };
    document.addEventListener("scroll", onGeometryChange, true);
    window.addEventListener("resize", onGeometryChange);
    document.addEventListener("keydown", onEscape, true);
    return () => {
      disposed = true;
      clearTarget();
      tilt.dispose();
      clearTimeout(finishTimer);
      document.removeEventListener("scroll", onGeometryChange, true);
      window.removeEventListener("resize", onGeometryChange);
      document.removeEventListener("keydown", onEscape, true);
      subscriptions.forEach((p) => p.then((fn) => fn()));
    };
  }, [run, onNotify, tiltEnabled]);
  if (!feedback) return null;
  const icon =
    feedback.phase === "success" ? (
      <Check size={16} />
    ) : feedback.phase === "error" ? (
      <X size={16} />
    ) : feedback.phase === "applying" ? (
      <LoaderCircle size={16} className="spin" />
    ) : (
      <Plus size={16} />
    );
  const left = Math.max(
    8,
    Math.min(
      feedback.point.x + 20 + cursorSize.width <= innerWidth - 8
        ? feedback.point.x + 20
        : feedback.point.x - cursorSize.width - 20,
      innerWidth - cursorSize.width - 8,
    ),
  );
  const top = Math.max(
    8,
    Math.min(
      feedback.point.y + 20 + cursorSize.height <= innerHeight - 8
        ? feedback.point.y + 20
        : feedback.point.y - cursorSize.height - 20,
      innerHeight - cursorSize.height - 8,
    ),
  );
  return createPortal(
    <div
      className={`tag-drop-feedback phase-${feedback.phase} ${feedback.bounds ? "has-target" : "no-target"}`}
      aria-hidden="false"
    >
      {feedback.bounds && feedback.clip && (
        <div
          className="tag-drop-clip"
          style={{
            clipPath: `inset(${feedback.clip.top}px ${Math.max(0, innerWidth - feedback.clip.left - feedback.clip.width)}px ${Math.max(0, innerHeight - feedback.clip.top - feedback.clip.height)}px ${feedback.clip.left}px)`,
            maskImage: edgeMask(feedback.bounds, feedback.clip),
            maskComposite: "intersect",
          }}
        >
          <div
            className="tag-drop-frame"
            data-target-file={feedback.fileId}
            style={{ ...feedback.bounds, transform: feedback.tilt }}
          >
            <div
              className="tag-drop-target-label"
              style={{
                top: Math.max(10, feedback.clip.top - feedback.bounds.top + 10),
              }}
            >
              {icon}
              <span>{feedback.label}</span>
            </div>
          </div>
        </div>
      )}
      {feedback.phase === "drag" && (
        <div ref={cursor} className="tag-drag-cursor" style={{ left, top }}>
          <TagIcon size={14} />
          <span>{feedback.tag}</span>
          {feedback.bounds ? <Plus size={13} /> : <X size={13} />}
        </div>
      )}
      <div className="tag-drop-announcement" role="status" aria-live="polite">
        {feedback.label}
      </div>
      {!feedback.bounds && feedback.phase !== "drag" && (
        <div ref={cursor} className="tag-drag-cursor" style={{ left, top }}>
          {icon}
          {feedback.label}
        </div>
      )}
    </div>,
    document.body,
  );
}
