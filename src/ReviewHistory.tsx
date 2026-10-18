import { useEffect, useRef, useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";

// Keep native summary keyboard behavior, while delaying close until the branch
// has folded away. Interrupted clicks continue from the visible height.
export function ReviewHistory({
  title,
  meta,
  children,
}: {
  title: ReactNode;
  meta: ReactNode;
  children: ReactNode;
}) {
  const root = useRef<HTMLDetailsElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const motion = useRef<Animation[]>([]);
  const desired = useRef(false);
  const [expanded, setExpanded] = useState(false);
  useEffect(() => () => motion.current.forEach((a) => a.cancel()), []);

  function toggle() {
    const el = root.current,
      content = body.current;
    if (!el || !content) return;
    const height = el.open ? content.getBoundingClientRect().height : 0;
    const opacity = el.open ? getComputedStyle(content).opacity : "0";
    const rows = [...content.querySelectorAll("li")];
    const poses = rows.map((row) => ({
      opacity: getComputedStyle(row).opacity,
      transform: getComputedStyle(row).transform,
    }));
    motion.current.forEach((a) => a.cancel());
    desired.current = !desired.current;
    const opening = desired.current;
    setExpanded(opening);
    el.open = true;
    content.style.height = opening ? "auto" : "0px";
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) {
      el.open = opening;
      motion.current = [];
      return;
    }
    const animation = content.animate(
      [
        { height: `${height}px`, opacity },
        {
          height: opening ? `${content.scrollHeight}px` : "0px",
          opacity: opening ? 1 : 0,
        },
      ],
      { duration: opening ? 280 : 200, easing: "cubic-bezier(.4,0,.2,1)" },
    );
    motion.current = [animation];
    if (opening)
      rows.forEach((row, i) => {
        motion.current.push(
          row.animate(
            [
              {
                opacity: height ? poses[i].opacity : 0,
                transform: height ? poses[i].transform : "translateX(-8px)",
              },
              { opacity: 1, transform: "translateX(0)" },
            ],
            {
              duration: 200,
              delay: Math.min(i * 24, 96),
              fill: "backwards",
              easing: "cubic-bezier(.16,1,.3,1)",
            },
          ),
        );
      });
    animation.onfinish = () => {
      if (motion.current[0] === animation) el.open = desired.current;
    };
  }

  return (
    <details ref={root} className="review-history" data-expanded={expanded}>
      <summary
        aria-expanded={expanded}
        onClick={(e) => {
          e.preventDefault();
          toggle();
        }}
      >
        <strong>
          <ChevronRight size={15} aria-hidden="true" />
          {title}
        </strong>
        <span>{meta}</span>
      </summary>
      <div ref={body} className="review-history-branch" inert={!expanded}>
        {children}
      </div>
    </details>
  );
}
