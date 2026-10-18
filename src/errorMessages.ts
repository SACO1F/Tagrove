import { currentLanguage, t } from "./i18n";

// Error codes belong to the application. Raw service/OS details are retained verbatim.
const explanations: Record<string, string> = {
  OFFICE_ENGINE: "The installed Office engine could not create this preview. Select another engine, close busy Office windows, or install the optional component.",
  OFFICE_BUSY: "Another document is being converted. Retry after it finishes.",
  OFFICE_TIMEOUT: "Preview generation timed out. Check Office for a waiting dialog.",
  OFFICE_LIMIT: "The source or generated preview exceeds the allowed size.",
  BROWSER_ERROR: "The isolated website preview could not start. You can use the external browser.",
  INVALID_INPUT: "Invalid input. Check the values and try again.",
  INVALID_FILTER: "The filter is invalid. Review its conditions.",
  NOT_FOUND: "This record no longer exists. Refresh the library.",
  VERSION_CONFLICT:
    "This item changed in another window. Refresh before retrying.",
  TAG_CHANGED: "The tag changed. Refresh before retrying.",
  FILE_CHANGED:
    "The original file changed. Recheck its status before retrying.",
  FILE_UNAVAILABLE:
    "The original file is unavailable. Check its location or relink it.",
  FILE_MISSING:
    "The original file is missing. Check its location or relink it.",
  ACCESS_DENIED: "Access was denied. Check file permissions.",
  NOTHING_TO_UNDO: "There is no operation to undo.",
  SEMANTIC_SERVICE:
    "The vector service rejected the request. Check its URL, embedding model and key.",
  SEMANTIC_NETWORK:
    "Could not reach the vector service. Check the connection and service URL.",
  SEMANTIC_CONFIG:
    "Review the vector service URL and embedding model settings.",
  SEMANTIC_DISABLED: "Enable semantic search in Preferences first.",
  AI_NOT_ENABLED: "Enable AI tagging in Preferences first.",
  AI_CONFIG: "Review the AI service URL, model and key.",
  DATABASE_ERROR:
    "The library database could not complete this operation. Keep your backup and retry.",
};
const families: Record<string, string> = {
  SEMANTIC:
    "Semantic indexing or search could not complete. Review the details below.",
  AI: "AI analysis could not complete. Review its settings and the details below.",
  BACKUP:
    "The annotation backup could not be processed. Keep the original backup file.",
  TRANSFER:
    "The transfer task could not complete. Review the details before retrying.",
  PREVIEW: "This file could not be previewed. You can open the original file.",
  WATCH:
    "Folder monitoring needs attention. Review the directory and its permissions.",
  REPAIR:
    "Path repair could not complete. Review the candidates before confirming.",
};
export function errorMessage(error: unknown): string {
  const raw = String(error).replace(/^Error:\s*/, "");
  const match = /^([A-Z][A-Z_0-9]+):\s*([\s\S]*)$/.exec(raw);
  if (currentLanguage() !== "en") return match ? match[2] : raw;
  if (!match) return t(raw);
  const [, code, detail] = match;
  const explanation =
    explanations[code] ||
    families[code.split("_")[0]] ||
    "This operation could not complete. Review the original details below.";
  return `${explanation} [${code}]\nOriginal detail: ${detail}`;
}
