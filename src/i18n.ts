import { useSyncExternalStore } from "react";
import { english } from "./locale-en";
export type Language = "zh-CN" | "en";
let language: Language = "zh-CN";
const listeners = new Set<() => void>();
export function configureLanguage(value: unknown) {
  const next = value === "en" ? "en" : "zh-CN";
  document.documentElement.lang = next;
  if (next === language) return;
  language = next;
  listeners.forEach((listener) => listener());
}
export function useLanguage() {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    () => language,
  );
}
export type UiMessage = { kind: "ui-message"; key: string; values: unknown[] };
export type UiCopy = string | UiMessage;
export function msg(key: string, ...values: unknown[]): UiMessage {
  return { kind: "ui-message", key, values };
}
function isMessage(value: unknown): value is UiMessage {
  return (
    !!value &&
    typeof value === "object" &&
    (value as UiMessage).kind === "ui-message"
  );
}
/** Only call for interface copy. Never pass file names, tag names or document text. */
export function t(copy: UiCopy): string {
  if (isMessage(copy)) return tf(copy.key, ...copy.values);
  return language === "en" ? (english[copy] ?? copy) : copy;
}

export function tf(copy: string, ...values: unknown[]) {
  return t(copy).replace(/\{(\d+)\}/g, (_, index) =>
    isMessage(values[Number(index)])
      ? t(values[Number(index)] as UiMessage)
      : String(values[Number(index)] ?? ""),
  );
}

export function currentLanguage() {
  return language;
}
