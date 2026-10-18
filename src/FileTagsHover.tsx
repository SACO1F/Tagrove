import { tf } from "./i18n";
import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type FocusEvent,
  type MouseEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { type LocalFile } from "./api";
import { TagSource } from "./TagSource";
import { tagTone, tagStyle } from "./tagColors";

function tagsTruncated(anchor: HTMLButtonElement) {
  const tags = anchor.querySelector<HTMLElement>(".list-tags");
  if (!tags || !tags.clientWidth) return false;
  const bounds = tags.getBoundingClientRect();
  return Array.from(tags.querySelectorAll<HTMLElement>(".tag-pill")).some(
    (pill) => {
      const rect = pill.getBoundingClientRect();
      return rect.right > bounds.right + 1 || rect.left < bounds.left - 1;
    },
  );
}

export function useFileTagsHover(file: LocalFile, onlyWhenTruncated = false) {
  const [anchor, setAnchor] = useState<HTMLButtonElement>();
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const id = useId();
  const keep = () => clearTimeout(timer.current);
  const close = () => {
    keep();
    timer.current = setTimeout(() => setAnchor(undefined), 150);
  };
  const show = (
    event: MouseEvent<HTMLButtonElement> | FocusEvent<HTMLButtonElement>,
  ) => {
    keep();
    if (
      file.tags.length &&
      (!onlyWhenTruncated || tagsTruncated(event.currentTarget))
    )
      setAnchor(event.currentTarget);
    else setAnchor(undefined);
  };
  useLayoutEffect(() => {
    if (!anchor || !onlyWhenTruncated) return;
    const check = () => {
      if (!tagsTruncated(anchor)) setAnchor(undefined);
    };
    check();
    const tags = anchor.querySelector(".list-tags");
    if (!tags) return;
    const observer = new ResizeObserver(check);
    observer.observe(tags);
    return () => observer.disconnect();
  }, [anchor, file.tags, onlyWhenTruncated]);
  useEffect(() => () => clearTimeout(timer.current), []);
  return {
    handlers: {
      onMouseEnter: show,
      onMouseLeave: close,
      onFocus: show,
      onBlur: close,
      "aria-describedby": anchor && file.tags.length ? id : undefined,
    },
    popover:
      anchor && file.tags.length ? (
        <FileTagsPopover
          file={file}
          anchor={anchor}
          id={id}
          onKeep={keep}
          onClose={close}
        />
      ) : null,
  };
}

export function FileTagsHover({
  file,
  onlyWhenTruncated = false,
  children,
}: {
  file: LocalFile;
  onlyWhenTruncated?: boolean;
  children: (
    handlers: ReturnType<typeof useFileTagsHover>["handlers"],
  ) => ReactNode;
}) {
  const { handlers, popover } = useFileTagsHover(file, onlyWhenTruncated);
  return (
    <>
      {children(handlers)}
      {popover}
    </>
  );
}

function FileTagsPopover({
  file,
  anchor,
  id,
  onKeep,
  onClose,
}: {
  file: LocalFile;
  anchor: HTMLButtonElement;
  id: string;
  onKeep: () => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: -10000, top: -10000 });
  useLayoutEffect(() => {
    const update = () => {
      if (!anchor.isConnected || !ref.current) return onClose();
      const card = anchor.getBoundingClientRect();
      const panel = ref.current.getBoundingClientRect();
      let left = Math.max(
        12,
        Math.min(card.left + 12, innerWidth - panel.width - 12),
      );
      const below = card.bottom + 8;
      const maxTop = innerHeight - panel.height - 12;
      const clampTop = (value: number) => Math.max(12, Math.min(value, maxTop));
      let top = below;
      if (below > maxTop) {
        const above = card.top - panel.height - 8;
        if (above >= 12) top = above;
        else if (card.right + 8 + panel.width <= innerWidth - 12) {
          left = card.right + 8;
          top = clampTop(card.top);
        } else if (card.left - panel.width - 8 >= 12) {
          left = card.left - panel.width - 8;
          top = clampTop(card.top);
        } else top = clampTop(below);
      }
      setPosition({ left, top });
    };
    update();
    const moved = (event: Event) => {
      // Scrolling this panel should not change its anchor or interrupt reading.
      if (event.target instanceof Node && ref.current?.contains(event.target))
        return;
      onClose();
    };
    window.addEventListener("resize", update);
    window.addEventListener("scroll", moved, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", moved, true);
    };
  }, [anchor, file.tags]);
  return createPortal(
    <div
      className="file-tag-hover"
      id={id}
      ref={ref}
      style={position}
      role="region"
      aria-label={tf("{0}的全部标签", file.name)}
      tabIndex={0}
      onMouseEnter={onKeep}
      onMouseLeave={onClose}
      onFocus={onKeep}
      onBlur={onClose}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          onClose();
        }
      }}
    >
      <div className="file-tag-hover-tags">
        {file.tags.map((tag) => (
          <span
            style={tagStyle(tag.id)}
            className={`file-tag-hover-tag tag-color tone-${tagTone(tag.id)}`}
            key={tag.id}
          >
            <span>{tag.name}</span>
            <TagSource tag={tag} />
          </span>
        ))}
      </div>
    </div>,
    document.body,
  );
}
