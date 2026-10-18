export interface WebDraft {
  title: string;
  text: string;
  version: number;
  savedTitle: string;
  savedText: string;
  name: string;
}
export const webDrafts = new Map<string, WebDraft>();
export const webSaving = new Set<string>();
export const webDirty = (d: WebDraft) =>
  d.title !== d.savedTitle || d.text !== d.savedText;
export function webUnsavedNames() {
  return [...webDrafts.values()].filter(webDirty).map((d) => d.name);
}
