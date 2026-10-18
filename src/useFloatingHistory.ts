import { tf } from "./i18n";
import { useCallback, useEffect, useState } from "react";

export type FloatingOperation = {
  id: string;
  text: string;
  time: number;
  undoId?: string;
};
export function useFloatingHistory(dataPath?: string) {
  const [operations, setOperations] = useState<FloatingOperation[]>([]);
  const [hydratedKey, setHydratedKey] = useState<string>();
  const key = dataPath ? `shiqian:floating-operations:${dataPath}` : undefined;
  useEffect(() => {
    if (!key) return;
    try {
      const saved: unknown = JSON.parse(localStorage.getItem(key) || "[]");
      setOperations(
        Array.isArray(saved)
          ? saved
              .filter(
                (item) =>
                  typeof item?.id === "string" &&
                  typeof item?.text === "string" &&
                  typeof item?.time === "number",
              )
              .slice(0, 3)
          : [],
      );
    } catch {
      setOperations([]);
    }
    setHydratedKey(key);
  }, [key]);
  const record = useCallback(
    (text: string, undoId?: string) => {
      const next = { id: crypto.randomUUID(), text, undoId, time: Date.now() };
      setOperations((old) => {
        const items = [next, ...old].slice(0, 3);
        if (key) {
          try {
            localStorage.setItem(key, JSON.stringify(items));
          } catch {
            /* History must not block annotation. */
          }
        }
        return items;
      });
    },
    [key],
  );
  const markUndone = useCallback(
    (id: string) => {
      setOperations((old) => {
        const items = old.map((item) =>
          item.id === id
            ? { ...item, text: tf("已撤销：{0}", item.text), undoId: undefined }
            : item,
        );
        if (key) {
          try {
            localStorage.setItem(key, JSON.stringify(items));
          } catch {}
        }
        return items;
      });
    },
    [key],
  );
  return {
    operations,
    record,
    markUndone,
    ready: !!key && hydratedKey === key,
  };
}
