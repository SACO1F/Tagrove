import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { CalendarDays, ChevronLeft, ChevronRight } from "lucide-react";
import { t as ui, tf, currentLanguage } from "./i18n";
import { GlideSelect } from "./GlideSelect";
import { localDateValue, parseDateValue } from "./calendar";
import "./date-filter.css";

/** Application-owned calendar: WebView's native date widget ignores the UI locale. */
export function DateFilter({
  value,
  onChange,
  "aria-label": label,
}: {
  value: string;
  onChange: (value: string) => void;
  "aria-label": string;
}) {
  const id = useId(),
    host = useRef<HTMLDivElement>(null),
    panel = useRef<HTMLDivElement>(null),
    trigger = useRef<HTMLButtonElement>(null);
  const [draft, setDraft] = useState(value),
    [open, setOpen] = useState(false),
    [invalid, setInvalid] = useState(false);
  const [cursor, setCursor] = useState(
    () => parseDateValue(value) || new Date(),
  );
  const [position, setPosition] = useState({ left: 0, top: 0 });
  useEffect(() => {
    setDraft(value);
    setInvalid(false);
  }, [value]);
  useLayoutEffect(() => {
    if (!open) return;
    const update = () => {
      const a = host.current!.getBoundingClientRect(),
        h = panel.current?.offsetHeight || 350;
      setPosition({
        left: Math.max(8, Math.min(a.left, innerWidth - 296)),
        top:
          a.bottom + h + 8 <= innerHeight
            ? a.bottom + 6
            : Math.max(8, a.top - h - 6),
      });
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    const dismiss = (e: PointerEvent) => {
      const target = e.target as Node;
      if (
        !host.current?.contains(target) &&
        !panel.current?.contains(target) &&
        !(target instanceof Element && target.closest(".glide-select-menu"))
      )
        setOpen(false);
    };
    document.addEventListener("pointerdown", dismiss, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
      document.removeEventListener("pointerdown", dismiss, true);
    };
  }, [open]);
  useEffect(() => {
    if (!open) return;
    const frame = requestAnimationFrame(() =>
      panel.current
        ?.querySelector<HTMLButtonElement>('[role="gridcell"][tabindex="0"]')
        ?.focus(),
    );
    return () => cancelAnimationFrame(frame);
  }, [open]);
  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };
  const commit = (date: Date) => {
    onChange(localDateValue(date));
    setDraft(localDateValue(date));
    setInvalid(false);
    close();
  };
  const move = (amount: number) => {
    const next = new Date(cursor);
    next.setDate(next.getDate() + amount);
    if (next.getFullYear() >= 1 && next.getFullYear() <= 9999) {
      setCursor(next);
      requestAnimationFrame(() =>
        panel.current
          ?.querySelector<HTMLButtonElement>(
            `[data-date="${localDateValue(next)}"]`,
          )
          ?.focus(),
      );
    }
  };
  function keys(e: KeyboardEvent) {
    const offset: Record<string, number> = {
      ArrowLeft: -1,
      ArrowRight: 1,
      ArrowUp: -7,
      ArrowDown: 7,
      Home: -cursor.getDay(),
      End: 6 - cursor.getDay(),
    };
    if (e.key in offset) {
      e.preventDefault();
      move(offset[e.key]);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }
  const year = cursor.getFullYear(),
    month = cursor.getMonth(),
    first = new Date(cursor);
  first.setDate(1);
  first.setHours(12, 0, 0, 0);
  first.setDate(1 - first.getDay());
  const days = Array.from({ length: 42 }, (_, i) => {
    const d = new Date(first);
    d.setDate(d.getDate() + i);
    return d;
  });
  const locale = currentLanguage(),
    weekdays = Array.from({ length: 7 }, (_, i) =>
      new Intl.DateTimeFormat(locale, { weekday: "short" }).format(
        new Date(2024, 0, 7 + i),
      ),
    );
  const heading = new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "long",
  }).format(cursor);
  return (
    <div
      className="date-filter-field"
      ref={host}
      onKeyDown={(e) => {
        if (open && e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          close();
        }
      }}
    >
      <input
        aria-label={label}
        type="text"
        inputMode="numeric"
        autoComplete="off"
        placeholder={ui("年-月-日")}
        value={draft}
        aria-invalid={invalid || undefined}
        aria-describedby={invalid ? id + "-error" : undefined}
        onChange={(e) => {
          const next = e.target.value;
          setDraft(next);
          const valid = !next || !!parseDateValue(next);
          setInvalid(!valid);
          if (valid) onChange(next);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" && e.altKey) {
            e.preventDefault();
            setCursor(parseDateValue(value) || new Date());
            setOpen(true);
          }
          if (e.key === "Escape" && open) {
            e.stopPropagation();
            close();
          }
        }}
      />
      <button
        className="icon-button date-calendar-trigger"
        ref={trigger}
        aria-label={tf("选择{0}", label)}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => {
          setCursor(parseDateValue(value) || new Date());
          setOpen(!open);
        }}
      >
        <CalendarDays size={15} />
      </button>
      {invalid && (
        <span className="date-field-error" id={id + "-error"} role="status">
          {ui("请输入有效日期：YYYY-MM-DD")}
        </span>
      )}
      {open &&
        createPortal(
          <div
            className="date-calendar"
            ref={panel}
            id={id}
            role="dialog"
            aria-label={label}
            style={position}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                close();
              }
            }}
          >
            <div className="date-calendar-heading">
              <button
                className="icon-button"
                aria-label={ui("上个月")}
                disabled={year === 1 && month === 0}
                onClick={() => {
                  const next = new Date(cursor);
                  next.setDate(1);
                  next.setMonth(month - 1);
                  setCursor(next);
                }}
              >
                <ChevronLeft size={16} />
              </button>
              <strong aria-live="polite">{heading}</strong>
              <button
                className="icon-button"
                aria-label={ui("下个月")}
                disabled={year === 9999 && month === 11}
                onClick={() => {
                  const next = new Date(cursor);
                  next.setDate(1);
                  next.setMonth(month + 1);
                  setCursor(next);
                }}
              >
                <ChevronRight size={16} />
              </button>
            </div>
            <div className="date-calendar-selectors">
              <GlideSelect
                value={String(month)}
                ariaLabel={ui("月份")}
                options={Array.from({ length: 12 }, (_, i) => ({
                  value: String(i),
                  label: new Intl.DateTimeFormat(locale, {
                    month: "long",
                  }).format(new Date(2024, i, 1)),
                }))}
                onChange={(m) => {
                  const d = new Date(cursor);
                  d.setDate(1);
                  d.setMonth(Number(m));
                  setCursor(d);
                }}
              />
              <input
                type="number"
                aria-label={ui("年份")}
                min={1}
                max={9999}
                value={year}
                onChange={(e) => {
                  const y = Number(e.target.value);
                  if (Number.isInteger(y) && y >= 1 && y <= 9999) {
                    const d = new Date(cursor);
                    d.setDate(1);
                    d.setFullYear(y);
                    setCursor(d);
                  }
                }}
              />
            </div>
            <div
              className="date-calendar-grid"
              role="grid"
              aria-label={heading}
              onKeyDown={keys}
            >
              <div role="row">
                {weekdays.map((day, i) => (
                  <span key={i} role="columnheader">
                    {day}
                  </span>
                ))}
              </div>
              {Array.from({ length: 6 }, (_, week) => (
                <div role="row" key={week}>
                  {days.slice(week * 7, week * 7 + 7).map((day) => {
                    const iso = localDateValue(day),
                      valid =
                        day.getFullYear() >= 1 && day.getFullYear() <= 9999;
                    return (
                      <button
                        key={iso}
                        role="gridcell"
                        data-date={iso}
                        aria-label={new Intl.DateTimeFormat(locale, {
                          dateStyle: "full",
                        }).format(day)}
                        aria-selected={value === iso}
                        aria-current={
                          iso === localDateValue(new Date())
                            ? "date"
                            : undefined
                        }
                        tabIndex={iso === localDateValue(cursor) ? 0 : -1}
                        className={
                          day.getMonth() !== month ? "outside-month" : ""
                        }
                        disabled={!valid}
                        onClick={() => commit(day)}
                      >
                        {day.getDate()}
                      </button>
                    );
                  })}
                </div>
              ))}
            </div>
            <div className="date-calendar-footer">
              <button
                className="button quiet"
                onClick={() => {
                  onChange("");
                  setDraft("");
                  setInvalid(false);
                  close();
                }}
              >
                {ui("清除日期")}
              </button>
              <button
                className="button quiet"
                onClick={() => commit(new Date())}
              >
                {ui("今天")}
              </button>
            </div>
          </div>,
          document.body,
        )}
    </div>
  );
}
