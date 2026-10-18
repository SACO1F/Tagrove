import type { CSSProperties } from "react";
const tones = ["sage", "blue", "rose", "amber", "violet", "teal"];

// A tag keeps its color when renamed, reordered or displayed in another window.
export function tagTone(id: string) {
  return tones[
    [...id].reduce((sum, character) => sum + character.charCodeAt(0), 0) %
      tones.length
  ];
}

let customColors: Record<string, string> = {};
export function configureTagColors(colors?: Record<string, string>) {
  customColors = colors ?? {};
}
export function tagCustomColor(id: string) {
  return customColors[id];
}
export function tagStyle(id: string): CSSProperties | undefined {
  const color = customColors[id];
  return color && /^#[0-9a-f]{6}$/i.test(color)
    ? ({ "--custom-tag-color": color } as CSSProperties)
    : undefined;
}
