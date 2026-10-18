import { t as ui, tf } from "./i18n";
import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type MouseEvent,
  type RefObject,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Star, AlertCircle } from "lucide-react";
import { type LocalFile, statusNames } from "./api";
import { Preview, previewKey, previewRatio } from "./preview";
import { TagSource } from "./TagSource";
import { tagTone, tagStyle } from "./tagColors";
import { ContentSnippet } from "./TextIndex";
import { SelectionCheck, useTagArrival } from "./microMotion";

function GalleryCard({
  file,
  index,
  selected,
  onSizeChange,
  onSelect,
  onOpen,
}: {
  file: LocalFile;
  index: number;
  selected: boolean;
  onSizeChange: (index: number, height: number) => void;
  onSelect: (file: LocalFile, event: MouseEvent) => void;
  onOpen: (file: LocalFile) => void;
}) {
  const [ratio, setRatio] = useState(() => previewRatio(file));
  const card = useRef<HTMLButtonElement>(null);
  useTagArrival(card, file.id, file.tags);
  const visual = file.kind === "image" || file.kind === "pdf";
  const tags = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    // Loading a preview can change its ratio without re-rendering the gallery.
    // Update positions in the same commit, before the newly taller image paints.
    if (card.current) onSizeChange(index, card.current.offsetHeight);
  }, [ratio, index, onSizeChange, file.contentMatch?.snippet]);
  return (
    <>
      <button
        ref={card}
        data-file-id={file.id}
        className={`file-card masonry-card ${selected ? "is-selected" : ""} ${file.status !== "available" ? "is-unavailable" : ""}`}
        role="option"
        aria-label={file.name}
        aria-selected={selected}
        onClick={(event) => onSelect(file, event)}
        onDoubleClick={() => onOpen(file)}
        onKeyDown={(event) => {
          const list = tags.current;
          if (
            list &&
            list.scrollHeight > list.clientHeight &&
            !event.ctrlKey &&
            !event.metaKey &&
            !event.altKey &&
            (event.key === "ArrowDown" || event.key === "ArrowUp")
          ) {
            event.preventDefault();
            event.stopPropagation();
            list.scrollBy(0, event.key === "ArrowDown" ? 48 : -48);
          }
        }}
      >
        <span className="masonry-art" style={{ aspectRatio: ratio }}>
          <Preview
            file={file}
            onDimensions={(width, height) => {
              if (width > 0 && height > 0) setRatio(width / height);
            }}
          />
          <SelectionCheck checked={selected} />
          {file.tags.some((t) => t.source === "ai" && !t.ai?.confirmed) && (
            <span
              className="gallery-ai-mark"
              data-tooltip={ui("包含 AI 标注，选中文件可确认或重新识别")}
            >
              ✦ AI
            </span>
          )}
          {file.favorite && (
            <span className="favorite-mark">
              <Star size={13} fill="currentColor" />
            </span>
          )}
          {file.status !== "available" && (
            <span className="gallery-status">
              <AlertCircle size={13} />
              {statusNames[file.status]}
            </span>
          )}
          <span
            className="gallery-caption"
            role="region"
            aria-label={tf("{0}的全部标签", file.name)}
          >
            <span className="file-name">{file.name}</span>
            {file.tags.length > 0 && (
              <span className="gallery-tags" ref={tags}>
                {file.tags.map((tag) => (
                  <span
                    style={tagStyle(tag.id)}
                    className={`gallery-tag tag-color tone-${tagTone(tag.id)}`}
                    key={tag.id}
                    data-motion-tag={tag.id}
                  >
                    <span>{tag.name}</span>
                    <TagSource tag={tag} />
                  </span>
                ))}
              </span>
            )}
          </span>
        </span>
        {!visual && (
          <span className="gallery-document-name">
            <span className="gallery-document-title">{file.name}</span>
          </span>
        )}
        <ContentSnippet file={file} />
      </button>
    </>
  );
}

export function MasonryGallery({
  files,
  scroll,
  width,
  columns,
  gap,
  selected,
  onSelect,
  onOpen,
  initialOffset,
  measurements,
  focusRequest,
  motionSnapshot,
  liveResize,
}: {
  files: LocalFile[];
  scroll: RefObject<HTMLDivElement | null>;
  width: number;
  columns: number;
  gap: number;
  selected: string[];
  onSelect: (file: LocalFile, event: MouseEvent) => void;
  onOpen: (file: LocalFile) => void;
  initialOffset: number;
  measurements: RefObject<Map<string, { width: number; height: number }>>;
  focusRequest: RefObject<string | undefined>;
  motionSnapshot: RefObject<Map<string, DOMRect>>;
  liveResize: RefObject<boolean>;
}) {
  const columnWidth = Math.max(1, (width - gap * (columns - 1)) / columns);
  const gallery = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: files.length,
    getScrollElement: () => scroll.current,
    initialOffset,
    getItemKey: (index) =>
      `${files[index].id}:${files[index].revision}:${files[index].identity}:${files[index].status}`,
    estimateSize: (index) => {
      const file = files[index];
      const cached = measurements.current.get(previewKey(file));
      if (cached && Math.abs(cached.width - columnWidth) < 1)
        return cached.height;
      return (
        columnWidth / previewRatio(file) +
        (file.kind === "image" || file.kind === "pdf" ? 0 : 44)
      );
    },
    lanes: columns,
    laneAssignmentMode: "estimate",
    gap,
    overscan: columns * 2,
  });
  // Width transitions remeasure many cards above the viewport. Automatic
  // per-card scroll compensation accumulates across open/close cycles.
  // Keep the user-controlled offset while the masonry geometry is rebuilt.
  virtual.shouldAdjustScrollPositionOnItemSizeChange = () => false;
  const onSizeChange = useCallback(
    (index: number, height: number) => {
      measurements.current.set(previewKey(files[index]), {
        width: columnWidth,
        height,
      });
      if (measurements.current.size > 1000)
        measurements.current.delete(measurements.current.keys().next().value!);
      virtual.resizeItem(index, height);
    },
    [virtual, files, columnWidth, measurements],
  );
  useLayoutEffect(() => {
    // Width changes update heights, not lane identity. Resetting measure()
    // each frame also cleared lane assignments and made cards jump sideways.
    // The virtualizer resets lanes itself when the column count changes.
    virtual.getTotalSize();
    files.forEach((file, index) => {
      const cached = measurements.current.get(previewKey(file));
      const height =
        cached && Math.abs(cached.width - columnWidth) < 1
          ? cached.height
          : Math.min(1200, Math.max(64, columnWidth / previewRatio(file))) +
            (file.kind === "image" || file.kind === "pdf" ? 0 : 44);
      virtual.resizeItem(index, height);
    });
    gallery.current
      ?.querySelectorAll<HTMLDivElement>(".masonry-item")
      .forEach((element) => {
        // Read directly even while scrolling; measureElement may use its cache
        // or defer to ResizeObserver during a user scroll.
        virtual.resizeItem(Number(element.dataset.index), element.offsetHeight);
      });
  }, [columnWidth, columns, gap, virtual, files, measurements]);
  useLayoutEffect(() => {
    const requested = focusRequest.current;
    focusRequest.current = undefined;
    if (requested) {
      const index = files.findIndex((file) => file.id === requested);
      if (index >= 0)
        virtual.scrollToIndex(index, { align: "center", behavior: "auto" });
    }
    const snapshot = new Map(motionSnapshot.current);
    motionSnapshot.current.clear();
    if (
      liveResize.current ||
      !snapshot.size ||
      matchMedia("(prefers-reduced-motion: reduce)").matches
    )
      return;
    const animations: Animation[] = [];
    const frame = requestAnimationFrame(() => {
      gallery.current
        ?.querySelectorAll<HTMLButtonElement>(".masonry-card")
        .forEach((card) => {
          const before = snapshot.get(card.dataset.fileId || "");
          if (!before) return;
          const after = card.getBoundingClientRect();
          if (after.width <= 0 || after.height <= 0) return;
          const sx = before.width / after.width;
          const sy = before.height / after.height;
          const timing = {
            duration: 300,
            easing: "cubic-bezier(0.42, 0, 0.58, 1)",
          };
          const start = document.timeline.currentTime;
          const motion = card.animate(
            [
              {
                transform: `translate(${before.left - after.left}px, ${before.top - after.top}px) scale(${sx}, ${sy})`,
              },
              { transform: "translate(0, 0) scale(1, 1)" },
            ],
            timing,
          );
          motion.startTime = start;
          animations.push(motion);
          // Image/frame geometry changes, but the UI inside must retain its
          // type size and icon proportions. A sampled reciprocal follows the
          // parent scale throughout the same eased timeline, not just at its
          // endpoints. Individual `scale` preserves hover/check transforms.
          const compensation = Array.from({ length: 65 }, (_, index) => {
            const progress = index / 64;
            return {
              offset: progress,
              scale: `${1 / (sx + (1 - sx) * progress)} ${1 / (sy + (1 - sy) * progress)}`,
            };
          });
          card
            .querySelectorAll<HTMLElement | SVGElement>(
              ".preview-placeholder > svg, .extension-stamp, .gallery-tag, .gallery-caption > .file-name, .gallery-document-title, .favorite-mark, .selection-check, .gallery-ai-mark, .gallery-status",
            )
            .forEach((element) => {
              const counter = element.animate(compensation, timing);
              counter.startTime = start;
              animations.push(counter);
            });
        });
    });
    return () => {
      cancelAnimationFrame(frame);
      animations.forEach((animation) => animation.cancel());
    };
  }, [
    columnWidth,
    columns,
    gap,
    virtual,
    focusRequest,
    motionSnapshot,
    liveResize,
  ]);
  return (
    <div
      ref={gallery}
      className="masonry-space"
      data-columns={columns}
      style={{ height: virtual.getTotalSize() }}
    >
      {virtual.getVirtualItems().map((item) => (
        <div
          key={item.key}
          ref={virtual.measureElement}
          data-index={item.index}
          data-lane={item.lane}
          className="masonry-item"
          style={{
            width: columnWidth,
            transform: `translate(${item.lane * (columnWidth + gap)}px, ${item.start}px)`,
          }}
        >
          <GalleryCard
            file={files[item.index]}
            index={item.index}
            onSizeChange={onSizeChange}
            selected={selected.includes(files[item.index].id)}
            onSelect={onSelect}
            onOpen={onOpen}
          />
        </div>
      ))}
    </div>
  );
}
