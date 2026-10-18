import { errorMessage } from "./errorMessages";
import { t, currentLanguage } from "./i18n";
import { invoke } from "@tauri-apps/api/core";
export interface Tag {
  id: string;
  name: string;
  aliases?: string[];
  count: number;
  version: number;
  createdBy: "manual" | "folder" | "ai";
  accepted?: boolean;
  source: "manual" | "folder" | "ai" | "";
  ai?: { model: string; reason: string; updatedAt: number; confirmed: boolean };
}
export function tagMatches(tag: Tag, value: string) {
  const key = value.trim().normalize("NFC").toLocaleLowerCase();
  return [tag.name, ...(tag.aliases ?? [])].some(
    (n) => n.normalize("NFC").toLocaleLowerCase() === key,
  );
}
export function tagSearch(tag: Tag, value: string) {
  const key = value.trim().normalize("NFC").toLocaleLowerCase();
  return [tag.name, ...(tag.aliases ?? [])].some((n) =>
    n.normalize("NFC").toLocaleLowerCase().includes(key),
  );
}
export interface ContentLocation {
  kind: "word" | "sheet" | "pdf";
  page?: number;
  block?: number;
  tableRow?: number;
  tableColumn?: number;
  sheet?: number;
  row?: number;
  column?: number;
  label: string;
}
export interface LocalFile {
  id: string;
  path: string;
  parent: string;
  name: string;
  extension: string;
  kind: string;
  bytes: number;
  modified: number;
  added: number;
  addedIds?: string[];
  status: string;
  error: string;
  favorite: boolean;
  version: number;
  note: string;
  noteVersion: number;
  revision: string;
  identity: string;
  tags: Tag[];
  aiTask?: {
    status:
      "queued" | "running" | "done" | "failed" | "unsupported" | "cancelled";
    error: string;
    updatedAt: number;
  };
  webSource?: {
    format: number;
    kind: "web_page" | "web_excerpt" | "web_image";
    title: string;
    url: string;
    excerpt: string;
    capturedAt: number;
    reading?: { title: string; text: string; updatedAt: number };
    image?: {
      url: string;
      finalUrl: string;
      sha256: string;
      mime: string;
      width: number;
      height: number;
    };
  };
  searchMatch?: {
    queryText: string;
    sources: {
      kind: "filename" | "tag" | "alias" | "note" | "body" | "semantic" | "web";
      value?: string;
      exact?: boolean;
    }[];
  };
  contentMatch?: {
    semantic?: boolean;
    score?: number;
    snippet: string;
    matchedTerms?: string[];
    queryTerms?: number;
    queryText?: string;
    source: string;
    location?: ContentLocation | null;
    coverage?: string;
    encoding: string;
    truncated: boolean;
    indexedAt: number;
  };
}
export interface Query {
  scope: string;
  text: string;
  searchScope?: "all" | "name" | "body";
  searchMode?: "metadata" | "content" | "semantic" | "unified";
  semanticSession?: string;
  include: string[];
  exclude: string[];
  mode: string;
  kinds: string[];
  status: string;
  directory: string;
  sourceHost?: string;
  captureKind?: string;
  recursive: boolean;
  from?: number;
  to?: number;
  sort: string;
  direction: string;
  offset: number;
  limit: number;
}
export interface Results {
  searchWarning?: string;
  localSearch?: { contentEnabled: boolean };
  files: LocalFile[];
  total: number;
  offset: number;
  hasMore: boolean;
}
export interface SavedCollection {
  id: string;
  name: string;
  query: Query;
}
export interface Bootstrap {
  semanticEnabled: boolean;
  tags: Tag[];
  counts: Record<string, number>;
  settings: {
    tagGroups?: {
      version: number;
      groups: { id: string; name: string }[];
      members: Record<string, string>;
    };
    tagGroupsCollapsedMain?: string[];
    tagGroupsCollapsedFloating?: string[];
    tagColors?: Record<string, string>;
    theme?: string;
    accentColor?: keyof typeof import("./appearance").accentColors;
    language?: "zh-CN" | "en";
    uiFontSize?: number;
    views?: Record<string, string>;
    density?: string;
    details?: boolean;
    galleryColumns?: number;
    showColumnControls?: boolean;
    tagDropTilt?: boolean;
    sidebarCollapsed?: boolean;
    sidebarWidth?: number;
    inspectorWidth?: number;
    floatingSize?: { width: number; height: number };
    floatingAlwaysOnTop?: boolean;
    folderAutoTagging?: boolean;
    savedCollections?: SavedCollection[];
    ai?: AiConfig;
  };
  undoLabel?: string;
  undoId?: string;
  dataPath: string;
  version: string;
  notice: string;
}
export interface AiConfig {
  enabled: boolean;
  endpoint: string;
  model: string;
  allowNewTags: boolean;
}
export interface ImportJob {
  id: string;
  discovered: number;
  processed: number;
  added: number;
  addedIds?: string[];
  existing: number;
  skipped: number;
  failed: number;
  errors: string[];
  failedPaths: string[];
  done: boolean;
  cancelled: boolean;
}
export interface TransferTask {
  id: string;
  kind: string;
  state: "running" | "done" | "failed" | "cancelled";
  startedAt: number;
  finishedAt?: number;
  error?: string;
  result?: { path?: string; fileCount?: number; notice?: string };
}
export interface TransferStatus {
  busy: boolean;
  phase: string;
  bytes: number;
  totalBytes: number;
  completed: number;
  fileTotal: number;
  task?: TransferTask;
  history: TransferTask[];
}
export interface TaskSnapshot {
  index?: TextIndexStatus;
  watch?: import("./WatchFolders").WatchSnapshot;
  transfer: TransferStatus;
  import?: ImportJob;
  ai: Record<string, number>;
}
export interface TextIndexStatus {
  enabled: boolean;
  state: "active" | "paused" | "cancelled" | "disabled";
  eligible: number;
  counts: Record<string, number>;
  truncated: number;
  bytes: number;
  readLimit: number;
  textBudget: number;
  errors: { name: string; error: string }[];
}
export interface SupportSnapshot {
  lastBackup?: { createdAt: number; kind: string; path: string };
  notice: string;
}
export function api<T = Record<string, unknown>>(
  action: string,
  payload: unknown = {},
): Promise<T> {
  return invoke<T>("api", { action, payload });
}
export function message(error: unknown): string {
  return errorMessage(error);
}
export const defaultQuery = (): Query => ({
  scope: "all",
  searchMode: "unified",
  text: "",
  include: [],
  exclude: [],
  mode: "all",
  kinds: [],
  status: "",
  directory: "",
  recursive: true,
  sort: "relevance",
  direction: "desc",
  offset: 0,
  limit: 100,
});
export function versions(files: LocalFile[]) {
  return Object.fromEntries(files.map((f) => [f.id, f.version]));
}
export function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
}
export function date(ms: number): string {
  return ms
    ? new Intl.DateTimeFormat(currentLanguage(), {
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
      }).format(ms)
    : "—";
}
const statusLabels: Record<string, string> = {
  available: "可用",
  missing: "文件缺失",
  offline: "存储离线",
  inaccessible: "访问异常",
  unchecked: "等待检查",
};
const kindLabels: Record<string, string> = {
  image: "图片",
  pdf: "PDF",
  text: "文本",
  office: "Office",
  other: "其他",
  web: "网页资料",
};

export const statusNames = new Proxy(statusLabels, {
  get: (target, key: string) =>
    typeof target[key] === "string" ? t(target[key]) : target[key],
});
export const kindNames = new Proxy(kindLabels, {
  get: (target, key: string) =>
    typeof target[key] === "string" ? t(target[key]) : target[key],
});
