import { t as ui } from "./i18n";
import { type KeyboardEvent } from "react";
import { RubberSegment } from "./ReaderControls";

const tabs = [
  ["pending", "待确认"],
  ["history", "操作记录"],
  ["tasks", "识别任务"],
] as const;

export function ReviewTabs({
  value,
  disabled,
  onChange,
}: {
  value: string;
  disabled: boolean;
  onChange: (value: string) => void;
}) {
  function keyboard(e: KeyboardEvent<HTMLButtonElement>, index: number) {
    const next =
      e.key === "ArrowRight"
        ? (index + 1) % 3
        : e.key === "ArrowLeft"
          ? (index + 2) % 3
          : e.key === "Home"
            ? 0
            : e.key === "End"
              ? 2
              : -1;
    if (disabled || next < 0) return;
    e.preventDefault();
    onChange(tabs[next][0]);
    e.currentTarget.parentElement
      ?.querySelector<HTMLButtonElement>(`[data-tab="${tabs[next][0]}"]`)
      ?.focus();
  }
  return (
    <RubberSegment
      index={tabs.findIndex(([id]) => id === value)}
      className="review-tabs"
      role="tablist"
      aria-label={ui("AI 审核内容")}
    >
      {tabs.map(([id, label], index) => (
        <button
          key={id}
          id={`review-tab-${id}`}
          data-tab={id}
          role="tab"
          aria-selected={value === id}
          aria-controls="review-tab-panel"
          tabIndex={value === id ? 0 : -1}
          disabled={disabled}
          onKeyDown={(e) => keyboard(e, index)}
          onClick={() => onChange(id)}
        >
          {ui(label)}
        </button>
      ))}
    </RubberSegment>
  );
}
