import type { Bootstrap } from "./api";
import { configureLanguage } from "./i18n";
import { getCurrentWindow } from "@tauri-apps/api/window";
export const DEFAULT_ACCENT_COLOR = "blue";
export const accentColors = {
  green: { light: "#2c6654", dark: "#a5cbb1", name: "荷叶绿" },
  blue: { light: "#315c9c", dark: "#a8c7f1", name: "群青（默认）" },
  purple: { light: "#6c4898", dark: "#cab6ed", name: "桔梗紫" },
  rose: { light: "#984661", dark: "#ebb3c7", name: "海棠红" },
  amber: { light: "#805716", dark: "#dfc18b", name: "金叶黄" },
  graphite: { light: "#4e5965", dark: "#bdc8d4", name: "青灰" },
  teal: { light: "#286f73", dark: "#99cdd0", name: "青碧" },
  terracotta: { light: "#94523a", dark: "#e9b59d", name: "赭石" },
  indigo: { light: "#475781", dark: "#b8c5ea", name: "黛蓝" },
  vermilion: { light: "#ac352b", dark: "#f3afa2", name: "朱红" },
  moss: { light: "#626d20", dark: "#c7d28b", name: "苔黄绿" },
  cocoa: { light: "#715044", dark: "#d7baab", name: "咖啡棕" },
} as const;
export function applyAppearance(settings: Bootstrap["settings"]) {
  configureLanguage(settings.language);
  const english = settings.language === "en";
  const floating = new URLSearchParams(location.search).has("floating");
  const title = english
    ? floating
      ? "Tagrove · Tags"
      : "Tagrove"
    : floating
      ? "拾签 · 标签浮窗"
      : "拾签";
  if (document.title !== title) {
    document.title = title;
    if ("__TAURI_INTERNALS__" in window)
      void getCurrentWindow().setTitle(title).catch(console.error);
  }
  const root = document.documentElement;
  const size = settings.uiFontSize;
  const fontSize =
    typeof size === "number" &&
    Number.isInteger(size) &&
    size >= 12 &&
    size <= 16
      ? size
      : 13;
  root.style.setProperty("--ui-font-scale", String(fontSize / 13));
  root.dataset.accent =
    settings.accentColor && settings.accentColor in accentColors
      ? settings.accentColor
      : DEFAULT_ACCENT_COLOR;
}
