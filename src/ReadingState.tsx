import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import { api, type LocalFile } from "./api";
import { t as ui } from "./i18n";

type Values = Record<string, string | number | boolean>;
type Entry = {
  file: LocalFile;
  values: Values;
  timer?: ReturnType<typeof setTimeout>;
  pending?: Promise<void>;
  dirty: boolean;
  writes: number;
  failed: boolean;
  listeners: Set<() => void>;
};
const entries = new Map<string, Entry>();
const savedRevisions = new Map<string, { revision: string; values: Values }>();
const stamp = (f: LocalFile) => `${f.id}:${f.revision}:${f.identity}`;
const Context = createContext<Entry | undefined>(undefined);
function persist(entry: Entry): Promise<void> {
  clearTimeout(entry.timer);
  if (!entry.dirty) return entry.pending || Promise.resolve();
  entry.dirty = false;
  entry.writes++;
  const payload = {
    id: entry.file.id,
    revision: entry.file.revision,
    identity: entry.file.identity,
    value: { ...entry.values },
  };
  entry.pending = (entry.pending || Promise.resolve()).then(async () => {
    try {
      await api("reader.save", payload);
      entry.failed = false;
    } catch {
      entry.failed = true;
      entry.dirty = true;
    }
    entry.writes--;
    entry.listeners.forEach((fn) => fn());
  });
  return entry.pending;
}
export function keepReadingStateAfterSave(
  file: LocalFile,
  revision: string,
  overrides: Values = {},
) {
  const entry = entries.get(stamp(file));
  if (entry) {
    const values = { ...entry.values, ...overrides };
    savedRevisions.set(file.id, {
      revision,
      values,
    });
    // The native library event may mount the new revision before save resolves.
    for (const current of entries.values())
      if (current.file.id === file.id && current.file.revision === revision) {
        Object.assign(current.values, values);
        current.dirty = true;
        current.listeners.forEach((fn) => fn());
        void persist(current);
        savedRevisions.delete(file.id);
      }
  }
}
export async function flushReadingStates() {
  await Promise.all([...entries.values()].map(persist));
}
function update(
  entry: Entry | undefined,
  key: string,
  value: string | number | boolean,
) {
  if (!entry || entry.values[key] === value) return;
  entry.values[key] = value;
  entry.dirty = true;
  clearTimeout(entry.timer);
  entry.timer = setTimeout(() => void persist(entry), 300);
}
export function ReadingState({
  file,
  children,
}: {
  file: LocalFile;
  children: ReactNode;
}) {
  const key = stamp(file);
  const [entry, setEntry] = useState<Entry | undefined>(() => entries.get(key));
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let alive = true;
    let current: Entry | undefined;
    const listen = (e: Entry) => {
      current = e;
      const notify = () => {
        if (alive) setFailed(e.failed);
      };
      e.listeners.add(notify);
      notify();
      return notify;
    };
    let off: (() => void) | undefined;
    const cached = entries.get(key);
    if (cached) {
      off = listen(cached);
      setEntry(cached);
    } else
      void api<{ revision: string; identity: string; value: Values }>(
        "reader.get",
        { id: file.id },
      )
        .then((result) => {
          if (!alive) return;
          const ownSave = savedRevisions.get(file.id);
          const carry = ownSave?.revision === file.revision;
          const e: Entry = entries.get(key) || {
            file,
            values: carry
              ? { ...ownSave!.values }
              : result.revision === file.revision &&
                  result.identity === file.identity
                ? result.value
                : {},
            dirty: carry,
            writes: 0,
            failed: false,
            listeners: new Set(),
          };
          entries.set(key, e);
          savedRevisions.delete(file.id);
          if (carry) void persist(e);
          off = listen(e);
          setEntry(e);
          // Only evict entries whose writes have completed. Their on-disk state stays available.
          if (entries.size > 100)
            for (const [oldKey, old] of entries) {
              if (entries.size <= 100) break;
              if (
                oldKey !== key &&
                old.listeners.size === 0 &&
                !old.dirty &&
                !old.writes
              )
                entries.delete(oldKey);
            }
        })
        .catch(() => {
          if (!alive) return;
          const e: Entry = {
            file,
            values: {},
            dirty: false,
            writes: 0,
            failed: true,
            listeners: new Set(),
          };
          entries.set(key, e);
          off = listen(e);
          setEntry(e);
        });
    return () => {
      alive = false;
      if (current) {
        if (off) current.listeners.delete(off);
        void persist(current);
      }
    };
  }, [key]);
  if (!entry)
    return (
      <div className="reader-message" role="status">
        {ui("正在恢复阅读状态…")}
      </div>
    );
  return (
    <Context.Provider value={entry}>
      {failed && (
        <div className="reader-inline-error" role="status">
          {ui("阅读位置暂未保存，仅本次有效")}
          <button
            className="button quiet"
            onClick={() => {
              entry.dirty = true;
              void persist(entry);
            }}
          >
            {ui("重试")}
          </button>
        </div>
      )}
      {children}
    </Context.Provider>
  );
}
export function useReadingValue<T extends string | number | boolean>(
  key: string,
  fallback: T,
): [T, (value: T | ((old: T) => T)) => void] {
  const entry = useContext(Context);
  const [value, setValue] = useState<T>(
    () => (entry?.values[key] ?? fallback) as T,
  );
  const current = useRef(value);
  current.current = value;
  useEffect(() => {
    if (!entry) return;
    const sync = () => {
      const next = entry.values[key];
      if (
        next !== undefined &&
        typeof next === typeof fallback &&
        next !== current.current
      ) {
        current.current = next as T;
        setValue(next as T);
      }
    };
    entry.listeners.add(sync);
    return () => {
      entry.listeners.delete(sync);
    };
  }, [entry, key]);
  return [
    value,
    (next) => {
      const value = typeof next === "function" ? next(current.current) : next;
      current.current = value;
      update(entry, key, value);
      setValue(value);
    },
  ];
}
export function useReadingPosition() {
  const entry = useContext(Context);
  return {
    get: (key: string, fallback = 0) =>
      typeof entry?.values[key] === "number"
        ? (entry.values[key] as number)
        : fallback,
    set: (key: string, value: number) => {
      if (Number.isFinite(value))
        update(entry, key, Math.round(value * 1000) / 1000);
    },
  };
}
export function useReadingScroll(
  ref: RefObject<HTMLElement | null>,
  key: string,
  enabled = true,
) {
  const position = useReadingPosition();
  const latest = useRef(position);
  latest.current = position;
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !enabled) return;
    const top = latest.current.get(key + "Top"),
      left = latest.current.get(key + "Left");
    el.scrollTop = top;
    el.scrollLeft = left;
    let frame = requestAnimationFrame(() => {
      el.scrollTop = top;
      el.scrollLeft = left;
    });
    const save = () => {
      latest.current.set(key + "Top", el.scrollTop);
      latest.current.set(key + "Left", el.scrollLeft);
    };
    el.addEventListener("scroll", save, { passive: true });
    return () => {
      cancelAnimationFrame(frame);
      save();
      el.removeEventListener("scroll", save);
    };
  }, [ref, key, enabled]);
}
