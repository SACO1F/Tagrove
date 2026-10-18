import { flushReadingStates } from "./ReadingState";
import { WebSourceFilters } from "./WebCapture";
import { dateBoundary, localDateValue } from "./calendar";
import { DateFilter } from "./DateFilter";
import { applyAppearance } from "./appearance";
import { useLanguage } from "./i18n";
import { t as ui, tf, msg, type UiCopy } from "./i18n";
import {
  TagBranch,
  GroupManager,
  GroupDisclosure,
  groupedTags,
  emptyGroups,
} from "./TagGroups";
import { configureTagColors } from "./tagColors";
import { SemanticSearch, ImportSemanticPrompt } from "./Semantic";
import { WorkspaceFileColumn } from "./WorkspaceFileColumn";
import { InspectorPane, inspectorWidth } from "./InspectorPane";
import { useGalleryWheel } from "./useGalleryWheel";
import {
  ContentReader,
  documentsSaving,
  unsavedDocumentNames,
} from "./ContentReader";
import { WorkspaceToasts, type WorkspaceToast } from "./WorkspaceToasts";
import { WatchFolders } from "./WatchFolders";
import { PathRepair } from "./PathRepair";
import { TagAliases } from "./TagAliases";
import { MergeTagDialog, CollectionsBar } from "./TagOrganize";
import { WarmTooltip } from "./WarmTooltip";
import { FirstRunGuide } from "./FirstRunGuide";
import { StartupWindow, WindowControls } from "./WindowControls";
import { GlideSelect } from "./GlideSelect";
import { TransferDialog } from "./TransferDialog";
import { TaskPanel, useTasks } from "./TaskPanel";
import { DiagnosticPanel } from "./SupportPanel";
import { AiReview } from "./AiReview";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  Search,
  Sparkles,
  Plus,
  ChevronDown,
  FolderOpen,
  Files,
  Inbox,
  Star,
  Clock3,
  Unplug,
  Settings,
  Tags,
  LayoutGrid,
  List,
  PanelLeftClose,
  PanelLeftOpen,
  SlidersHorizontal,
  X,
  Undo2,
  RefreshCw,
  Trash2,
  PackageOpen,
  LoaderCircle,
  Check,
  ArrowDownUp,
  FolderInput,
  FilePlus2,
  Minus,
  ArrowUpRight,
  PanelsTopLeft,
  Edit3,
  ClipboardCheck,
} from "lucide-react";
import {
  api,
  tagMatches,
  tagSearch as searchTagNames,
  message,
  defaultQuery,
  versions,
  kindNames,
  LocalFile,
  Tag,
  Query,
  Results,
  Bootstrap,
  ImportJob,
} from "./api";
import {
  FilesView,
  Inspector,
  Modal,
  TagManager,
  SettingsPanel,
  ImportDetails,
  QuickTagPanel,
} from "./components";
import { Preview, clearPreviews } from "./preview";
import { isHtml } from "./HtmlReader";
import { isOfficePreview } from "./OfficeReader";
import { ExportTagDialog } from "./ExportTagDialog";
import { TagDropFeedback } from "./TagDropFeedback";
import { SidebarResizeHandle, sidebarWidth } from "./SidebarResizeHandle";
import "./workspace-glass.css";
import { useTagArrival } from "./microMotion";
import { NoteDrafts } from "./notes";

const scopes = [
  {
    id: "all",
    name: "全部资料",
    icon: Files,
  },
  {
    id: "inbox",
    name: "待整理",
    icon: Inbox,
  },
  {
    id: "starred",
    name: "我的收藏",
    icon: Star,
  },
  {
    id: "recent",
    name: "最近加入",
    icon: Clock3,
  },
  {
    id: "unavailable",
    name: "需要关注",
    icon: Unplug,
  },
];
type Confirmation = {
  title: UiCopy;
  text: UiCopy;
  label: UiCopy;
  resolve: (ok: boolean) => void;
};
export default function App() {
  useLanguage();
  const [boot, setBoot] = useState<Bootstrap>();
  useEffect(() => {
    if (boot) applyAppearance(boot.settings);
  }, [
    boot?.settings.language,
    boot?.settings.accentColor,
    boot?.settings.uiFontSize,
  ]);
  const sidebarTagsMotion = useRef<HTMLDivElement>(null);
  useTagArrival(sidebarTagsMotion, boot?.dataPath, boot?.tags ?? []);
  const [mergeSource, setMergeSource] = useState<Tag>();
  const [aliasSource, setAliasSource] = useState<Tag>();
  const [fatal, setFatal] = useState("");
  const [query, setQuery] = useState<Query>(defaultQuery);
  const queryRef = useRef(query);
  queryRef.current = query;
  const [search, setSearch] = useState("");
  const [searchExpanded, setSearchExpanded] = useState(false);
  const composing = useRef(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const searchButton = useRef<HTMLButtonElement>(null);
  const openSearch = useCallback(() => {
    setSearchExpanded(true);
    requestAnimationFrame(() => {
      searchRef.current?.focus();
      searchRef.current?.select();
    });
  }, []);
  const [results, setResults] = useState<Results>({
    files: [],
    total: 0,
    offset: 0,
    hasMore: false,
  });
  const resultsRef = useRef(results);
  resultsRef.current = results;
  const [selected, setSelected] = useState<string[]>([]);
  const [retainedDetail, setRetainedDetail] = useState<LocalFile>();
  const preserveQuerySelection = useRef(false);
  const selectedRef = useRef(selected);
  selectedRef.current = selected;
  const anchor = useRef("");
  const galleryLayoutCapture = useRef<(() => void) | undefined>(undefined);
  const [loading, setLoading] = useState(false);
  const loadLock = useRef(false);
  const request = useRef(0);
  const [busy, setBusy] = useState(false);
  const [toasts, setToasts] = useState<WorkspaceToast[]>([]);
  const [importSemanticIds, setImportSemanticIds] = useState<string[]>([]);
  const promptedImports = useRef(new Set<string>());
  const [theme, setTheme] = useState("system");
  const [density, setDensity] = useState("comfortable");
  const [views, setViews] = useState<Record<string, string>>({});
  const [detailWidth, setDetailWidth] = useState(420);
  const contentBody = useRef<HTMLDivElement>(null);
  const [galleryColumns, setGalleryColumns] = useState(3);
  const [floatingVisible, setFloatingVisible] = useState(false);
  const [columnCapacity, setColumnCapacity] = useState(8);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [sidebarSize, setSidebarSize] = useState(216);
  const [sidebarResizing, setSidebarResizing] = useState(false);
  const settingsWrites = useRef<Promise<void>>(Promise.resolve());
  const [addMenu, setAddMenu] = useState(false);
  const [recursive, setRecursive] = useState(true);
  const [filters, setFilters] = useState(false);
  const [tagSearch, setTagSearch] = useState("");
  const [modal, setModal] = useState("");
  const [guideRequest, setGuideRequest] = useState(0);
  const [exportTag, setExportTag] = useState<Tag>();
  const [transferMode, setTransferMode] = useState<"export" | "import">();
  const [transferIds, setTransferIds] = useState<string[]>([]);
  const [editingTag, setEditingTag] = useState<Tag>();
  const [confirm, setConfirm] = useState<Confirmation>();
  const confirmRef = useRef<Confirmation | undefined>(undefined);
  confirmRef.current = confirm;
  const [quick, setQuick] = useState<LocalFile>();
  const [dragging, setDragging] = useState(false);
  const [job, setJob] = useState<ImportJob>();
  const tasks = useTasks();
  const [, redraw] = useState(0);
  const drafts = useRef<NoteDrafts | undefined>(undefined);
  const noteTimer = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  const initialized = useRef(false);
  const closeAllowed = useRef(false);
  const closeInProgress = useRef(false);
  const recursiveRef = useRef(recursive);
  recursiveRef.current = recursive;
  const currentScope = scopes.find((x) => x.id === query.scope)!;
  const visibleSelected = results.files.filter((f) => selected.includes(f.id));
  const selectedFiles =
    visibleSelected.length === 0 &&
    selected.length === 1 &&
    retainedDetail?.id === selected[0]
      ? [retainedDetail]
      : visibleSelected;
  const single = selectedFiles.length === 1 ? selectedFiles[0] : undefined;
  const view = views[query.scope] || "grid";
  const inspectorVisible = selectedFiles.length > 0;
  const lastInspected = useRef<LocalFile[]>([]);
  if (selectedFiles.length) lastInspected.current = selectedFiles;
  const inspectorFiles = selectedFiles.length
    ? selectedFiles
    : lastInspected.current;
  useLayoutEffect(() => {
    window.dispatchEvent(new Event("shiqian-continuous-layout"));
  }, [inspectorVisible]);
  const tell = useCallback(
    (text: UiCopy, error = false, undoId?: string) =>
      setToasts((old) =>
        [
          ...old,
          { id: crypto.randomUUID(), text, error, undoId, time: Date.now() },
        ].slice(-3),
      ),
    [],
  );
  const ask = useCallback(
    (title: UiCopy, text: UiCopy, label: UiCopy = msg("确认")) =>
      new Promise<boolean>((resolve) =>
        setConfirm({ title, text, label, resolve }),
      ),
    [],
  );
  const answer = (value: boolean) => {
    confirmRef.current?.resolve(value);
    setConfirm(undefined);
  };
  const reload = useCallback(async () => {
    const b = await api<Bootstrap>("bootstrap");
    configureTagColors(b.settings.tagColors);
    setBoot(b);
    return b;
  }, []);
  const fetchFiles = useCallback(
    async (q: Query, append = false, preserveLoaded = false) => {
      if (append && loadLock.current) return;
      const token = ++request.current;
      loadLock.current = true;
      setLoading(true);
      try {
        let r = await api<Results>("query", {
          ...q,
          offset: append ? resultsRef.current.files.length : 0,
        });
        if (token !== request.current) return;
        if (r.searchWarning) {
          const warning = r.searchWarning;
          r = await api<Results>("query", {
            ...q,
            semanticSession: "",
            offset: 0,
          });
          if (token !== request.current) return;
          append = false;
          preserveLoaded = false;
          tell(warning);
          setQuery((current) =>
            current.semanticSession === q.semanticSession
              ? { ...current, semanticSession: "" }
              : current,
          );
        }
        // Background tagging must keep the pages the user has already opened.
        const loaded = preserveLoaded ? resultsRef.current.files.length : 0;
        while (!append && r.hasMore && r.files.length < loaded) {
          const next = await api<Results>("query", {
            ...q,
            offset: r.files.length,
          });
          if (token !== request.current) return;
          if (!next.files.length) break;
          const files = [
            ...r.files,
            ...next.files.filter((f) => !r.files.some((x) => x.id === f.id)),
          ];
          if (files.length === r.files.length) break;
          r = { ...next, offset: 0, files };
        }
        let retained: LocalFile | undefined;
        if (
          preserveLoaded &&
          selectedRef.current.length === 1 &&
          !r.files.some((f) => f.id === selectedRef.current[0])
        ) {
          retained = await api<LocalFile>("file", {
            id: selectedRef.current[0],
          }).catch(() => undefined);
          if (token !== request.current) return;
        }
        if (!append) setRetainedDetail(retained);
        setResults((old) => ({
          ...r,
          files: append
            ? [
                ...old.files,
                ...r.files.filter((f) => !old.files.some((x) => x.id === f.id)),
              ]
            : r.files,
        }));
        if (!append)
          setSelected((old) =>
            old.filter(
              (id) => r.files.some((f) => f.id === id) || retained?.id === id,
            ),
          );
      } catch (e) {
        if (token === request.current) {
          if (
            q.searchMode === "content" ||
            q.searchMode === "semantic" ||
            q.searchMode === "unified"
          )
            setResults({ files: [], total: 0, offset: 0, hasMore: false });
          tell(message(e), true);
        }
      } finally {
        if (token === request.current) {
          loadLock.current = false;
          setLoading(false);
        }
      }
    },
    [tell],
  );
  const refresh = useCallback(async () => {
    const b = await reload();
    const q = queryRef.current;
    const include = q.include.filter((id) => b.tags.some((t) => t.id === id));
    const exclude = q.exclude.filter((id) => b.tags.some((t) => t.id === id));
    if (
      include.length !== q.include.length ||
      exclude.length !== q.exclude.length
    ) {
      preserveQuerySelection.current = true;
      setQuery({ ...q, include, exclude });
    } else await fetchFiles(q, false, true);
  }, [reload, fetchFiles]);
  const run = useCallback(
    async (task: () => Promise<unknown>, success?: UiCopy) => {
      setBusy(true);
      try {
        await task();
        if (success) tell(success);
      } catch (e) {
        tell(message(e), true);
      } finally {
        setBusy(false);
      }
    },
    [tell],
  );
  const flush = useCallback(async () => {
    clearTimeout(noteTimer.current);
    await drafts.current?.flushAll();
  }, []);
  const startImport = useCallback(async (paths: string[]) => {
    if (!paths.length) return;
    setAddMenu(false);
    await api("import", { paths, recursive: recursiveRef.current });
    const j = await api<ImportJob>("import.status");
    setJob(j);
  }, []);

  useEffect(() => {
    let alive = true;
    const sync = () =>
      void api<{ visible: boolean }>("floating.visibility")
        .then((s) => {
          if (alive) setFloatingVisible(s.visible);
        })
        .catch(() => {});
    const unlisten = listen<{ visible: boolean }>(
      "floating-visibility",
      (e) => {
        if (alive) setFloatingVisible(e.payload.visible);
      },
    );
    sync();
    window.addEventListener("focus", sync);
    return () => {
      alive = false;
      window.removeEventListener("focus", sync);
      void unlisten.then((off) => off());
    };
  }, []);

  useEffect(() => {
    let alive = true;
    reload()
      .then((b) => {
        if (!alive) return;
        if (!initialized.current) {
          initialized.current = true;
          setTheme(b.settings.theme || "system");
          setDensity(b.settings.density || "comfortable");
          setViews(b.settings.views || {});
          setDetailWidth(inspectorWidth(b.settings.inspectorWidth));
          setGalleryColumns(
            Number.isInteger(b.settings.galleryColumns)
              ? Math.max(2, Math.min(8, b.settings.galleryColumns!))
              : 3,
          );
          setSidebarCollapsed(b.settings.sidebarCollapsed === true);
          setSidebarSize(sidebarWidth(b.settings.sidebarWidth));
          drafts.current = new NoteDrafts(b.dataPath);
          drafts.current.onChange = () => redraw((x) => x + 1);
          if (b.notice) tell(b.notice);
        }
        return fetchFiles(queryRef.current);
      })
      .catch((e) => setFatal(message(e)));
    return () => {
      alive = false;
    };
  }, []);
  useEffect(() => {
    const timer = setTimeout(() => {
      if (!composing.current)
        setQuery((q) =>
          q.text === search
            ? q
            : {
                ...q,
                text: search,
                ...(q.semanticSession
                  ? { searchMode: "unified" as const, semanticSession: "" }
                  : {}),
              },
        );
    }, 250);
    return () => clearTimeout(timer);
  }, [search]);
  useEffect(() => {
    const preserve = preserveQuerySelection.current;
    preserveQuerySelection.current = false;
    if (!preserve) {
      setSelected([]);
      setRetainedDetail(undefined);
    }
    void fetchFiles(query, false, preserve);
  }, [query, fetchFiles]);
  useEffect(() => {
    const media = matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const effective =
        theme === "system" ? (media.matches ? "dark" : "light") : theme;
      document.documentElement.dataset.theme = effective;
      void api("theme.sync", { theme: effective }).catch((error) =>
        tell(message(error), true),
      );
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [];
    let timer: ReturnType<typeof setTimeout>;
    let disposed = false;
    unlisteners.push(
      listen<ImportJob>("import-progress", (e) => {
        setJob(e.payload);
        if (e.payload.done) {
          void refresh();
          if (
            e.payload.added > 0 &&
            e.payload.addedIds?.length &&
            !promptedImports.current.has(e.payload.id)
          ) {
            promptedImports.current.add(e.payload.id);
            const ids = e.payload.addedIds;
            setToasts((old) =>
              [
                ...old,
                {
                  id: crypto.randomUUID(),
                  time: Date.now(),
                  error: false,
                  text: msg(
                    "已加入 {0} 个文件，生成语义索引？",
                    e.payload.added,
                  ),
                  actionLabel: msg("生成语义"),
                  onAction: () => {
                    setImportSemanticIds(ids);
                    setModal("import-semantic");
                  },
                },
              ].slice(-3),
            );
          } else if (!promptedImports.current.has(e.payload.id)) {
            tell(
              e.payload.cancelled
                ? msg("加入已取消，已完成的内容已保留")
                : msg(
                    "已加入 {0} 个文件{1}",
                    e.payload.added,
                    e.payload.failed
                      ? msg("，{0} 个失败", e.payload.failed)
                      : "",
                  ),
            );
          }
        }
      }),
    );
    unlisteners.push(
      listen<{
        ok: boolean;
        result?: {
          applied: number;
          imported: number;
          names: string[];
          tagName: string;
          undoId?: string;
        };
      }>("annotation-result", ({ payload }) => {
        const r = payload.result;
        if (!payload.ok || !r || !r.applied) return;
        const names = r.names
          .slice(0, 2)
          .map((name) => `「${name}」`)
          .join("、");
        tell(
          msg(
            "已添加「{0}」 · {1}{2}{3}",
            r.tagName,
            names,
            r.names.length > 2 ? msg("等 {0} 个文件", r.names.length) : "",
            r.imported ? msg("，加入 {0} 个文件", r.imported) : "",
          ),
          false,
          r.undoId,
        );
      }),
    );
    unlisteners.push(
      listen("exit-blocked", () => {
        setModal("tasks");
        tell(msg("后台任务尚未结束，请等待完成，或取消后退出"));
      }),
    );
    unlisteners.push(
      listen("library-changed", () => {
        clearTimeout(timer);
        timer = setTimeout(
          () => void refresh().catch((e) => tell(message(e), true)),
          150,
        );
      }),
    );
    const win = getCurrentWebviewWindow();
    unlisteners.push(
      win.onDragDropEvent((e) => {
        if (e.payload.type === "enter" || e.payload.type === "over")
          setDragging(true);
        else setDragging(false);
        if (e.payload.type === "drop") {
          const paths = e.payload.paths;
          void run(() => startImport(paths));
        }
      }),
    );
    unlisteners.push(
      win.onCloseRequested(async (e) => {
        if (closeAllowed.current) return;
        e.preventDefault();
        if (closeInProgress.current) return;
        closeInProgress.current = true;
        try {
          if (documentsSaving()) {
            tell(msg("正在保存正文，请稍后关闭"));
            return;
          }
          if (unsavedDocumentNames().length) {
            if (
              !(await ask(
                msg("正文尚未保存"),
                msg(
                  "以下文件有未保存内容：{0}。关闭将放弃这些草稿。",
                  unsavedDocumentNames().join("、"),
                ),
                msg("放弃并关闭"),
              ))
            )
              return;
          }
          await flush();
          await settingsWrites.current;
          await flushReadingStates();
          const packageTask = await api<{ busy: boolean }>("package.status");
          if (packageTask.busy) {
            setModal("tasks");
            tell(msg("后台任务尚未结束，请等待完成，或在任务面板取消后退出"));
            return;
          }
          const active = await api<ImportJob | null>("import.status");
          if (active && !active.done) {
            if (
              !(await ask(
                msg("加入任务尚未完成"),
                msg("关闭后会停止继续加入，已经加入的文件会保留。"),
                msg("停止并退出"),
              ))
            )
              return;
            await api("import.cancel");
            for (let attempt = 0; attempt < 100; attempt++) {
              const pending = await api<ImportJob | null>("import.status");
              if (!pending || pending.done) break;
              await new Promise((resolve) => setTimeout(resolve, 50));
            }
          }
          closeAllowed.current = true;
          await api("main.close");
          closeAllowed.current = false;
        } catch (error) {
          closeAllowed.current = false;
          if (/^(TRANSFER_BUSY|IMPORT_BUSY):/.test(String(error))) {
            setModal("tasks");
            tell(message(error), true);
            return;
          }
          tell(msg("备注尚未保存：{0}", message(error)), true);
          if (
            await ask(
              msg("保留草稿并退出？"),
              msg(
                "数据库中的旧备注会保留，新内容已保存在本机草稿中，下次启动时可以继续保存。",
              ),
              msg("保留草稿并退出"),
            )
          ) {
            closeAllowed.current = true;
            await api("main.close");
            closeAllowed.current = false;
          }
        } finally {
          closeInProgress.current = false;
        }
      }),
    );
    return () => {
      disposed = true;
      clearTimeout(timer);
      for (const p of unlisteners) p.then((fn) => fn());
    };
  }, [refresh, run, startImport, tell, flush, ask]);
  useEffect(() => {
    if (!job || job.done) return;
    const timer = setInterval(
      () =>
        api<ImportJob>("import.status")
          .then((j) => {
            if (j) {
              setJob(j);
              if (j.done) void refresh();
            }
          })
          .catch(() => {}),
      700,
    );
    return () => clearInterval(timer);
  }, [job?.id, job?.done, refresh]);

  function operationText(
    action: string,
    value: Record<string, unknown>,
  ): UiCopy {
    const ids = Array.isArray(value.ids) ? (value.ids as string[]) : [];
    const files = results.files.filter((f) => ids.includes(f.id));
    const names = files
      .slice(0, 2)
      .map((f) => `「${f.name}」`)
      .join("、");
    const target = !names
      ? msg("{0} 个文件", ids.length)
      : ids.length > 2
        ? msg("{0}等 {1} 个文件", names, ids.length)
        : names;
    const tagNames = (key: string) =>
      (Array.isArray(value[key]) ? (value[key] as string[]) : [])
        .map((id) => `「${boot?.tags.find((t) => t.id === id)?.name || id}」`)
        .join("、");
    if (action === "files.tags") {
      const add = tagNames("add"),
        remove = tagNames("remove");
      const added = msg("为 {0} 添加标签 {1}", target, add);
      const removed = msg("从 {0} 移除标签 {1}", target, remove);
      return add && remove
        ? msg("{0}；{1}", added, removed)
        : add
          ? added
          : remove
            ? removed
            : "";
    }
    if (action === "files.favorite")
      return msg("{0} {1}", msg(value.value ? "收藏" : "取消收藏"), target);
    if (action === "files.remove")
      return msg("从资料库移除 {0}（原文件保留）", target);
    const tag = boot?.tags.find((t) => t.id === value.id);
    if (action === "tags.delete")
      return msg(
        "删除标签「{0}」，移除 {1} 个文件的对应标签",
        tag?.name || value.id,
        tag?.count || 0,
      );
    if (action === "tags.rename")
      return msg(
        "将标签「{0}」改为「{1}」，同步 {2} 个文件",
        tag?.name || value.id,
        value.name,
        tag?.count || 0,
      );
    if (action === "files.relink")
      return msg("修复文件「{0}」的路径", single?.name || value.id);
    return "";
  }
  const mutate = async (
    action: string,
    payload: unknown,
    description?: UiCopy,
  ) => {
    const text =
      description || operationText(action, payload as Record<string, unknown>);
    await flush();
    const result = await api<{ undoId?: string }>(action, payload);
    await refresh();
    if (text) tell(text, false, result.undoId);
  };
  const updateFiles = async (
    action: string,
    payload: Record<string, unknown>,
  ) =>
    mutate(action, {
      ids: selectedFiles.map((f) => f.id),
      versions: versions(selectedFiles),
      ...payload,
    });
  const addTag = async (tag: Tag, add = true) =>
    updateFiles("files.tags", {
      add: add ? [tag.id] : [],
      remove: add ? [] : [tag.id],
    });
  const createAndTag = async (name: string) => {
    const existing = boot?.tags.some((t) => tagMatches(t, name));
    const tag = await api<Tag>("tag.create", { name });
    if (!existing) tell(msg("创建标签「{0}」", tag.name));
    const target = selectedFiles
      .slice(0, 2)
      .map((f) => `「${f.name}」`)
      .join("、");
    await mutate(
      "files.tags",
      {
        ids: selectedFiles.map((f) => f.id),
        versions: versions(selectedFiles),
        add: [tag.id],
        remove: [],
      },
      tf(
        "为 {0}{1} 添加标签「{2}」",
        target,
        selectedFiles.length > 2
          ? tf("等 {0} 个文件", selectedFiles.length)
          : "",
        tag.name,
      ),
    );
  };
  const openFile = (f: LocalFile) => run(() => api("files.open", { id: f.id }));
  const undo = () =>
    run(async () => {
      await flush();
      await api("undo");
      await refresh();
    }, msg("已撤销上一步操作"));
  const chooseImport = (directory = false) =>
    run(async () => {
      setAddMenu(false);
      const paths = await open({
        directory,
        multiple: true,
        title: directory ? ui("选择要加入的文件夹") : ui("选择要加入的文件"),
      });
      if (paths) await startImport(Array.isArray(paths) ? paths : [paths]);
    });
  const chooseDirectory = () =>
    run(async () => {
      const path = await open({
        directory: true,
        multiple: false,
        title: ui("选择筛选目录"),
      });
      if (typeof path === "string")
        setQuery((q) => ({ ...q, directory: path }));
    });
  const relink = () => {
    if (!single) return;
    const file = single;
    void run(async () => {
      const path = await open({
        multiple: false,
        title: tf("重新关联 {0}", file.name),
      });
      if (typeof path !== "string") return;
      if (
        !(await ask(
          msg("重新关联文件"),
          msg(
            "当前：{0}\n\n关联到：{1}\n\n此文件的标签、备注和收藏会保留。请确认这是同一份资料。",
            file.path,
            path,
          ),
          msg("确认关联"),
        ))
      )
        return;
      await mutate("files.relink", {
        id: file.id,
        path,
        version: file.version,
      });
      tell(msg("已重新关联文件"));
    });
  };
  const remove = () =>
    run(async () => {
      if (
        !(await ask(
          msg("从文件库移除"),
          msg(
            "移除所选的 {0} 个文件记录。原文件保留在原来的位置，可用“撤销”恢复记录。",
            selectedFiles.length,
          ),
          msg("移除记录"),
        ))
      )
        return;
      await updateFiles("files.remove", {});
      setSelected([]);
    });
  const setSetting = (key: string, value: unknown) => {
    settingsWrites.current = settingsWrites.current
      .then(async () => {
        await api("settings.save", { key, value });
        setBoot((b) =>
          b ? { ...b, settings: { ...b.settings, [key]: value } } : b,
        );
      })
      .catch((error) => tell(msg("偏好保存失败：{0}", message(error)), true));
  };
  const changeColumns = (columns: number) => {
    galleryLayoutCapture.current?.();
    setGalleryColumns(columns);
    setSetting("galleryColumns", columns);
  };
  const toggleSidebar = () => {
    window.dispatchEvent(new Event("shiqian-continuous-layout"));
    setSidebarCollapsed(!sidebarCollapsed);
    setSetting("sidebarCollapsed", !sidebarCollapsed);
  };
  const visibleColumns = Math.min(galleryColumns, columnCapacity);
  useGalleryWheel(
    contentBody,
    view === "grid",
    visibleColumns,
    columnCapacity,
    changeColumns,
    !!boot,
  );
  const backup = () =>
    run(async () => {
      await flush();
      const path = await save({
        title: ui("导出标注备份"),
        defaultPath: `拾签标注-${new Date().toLocaleDateString("sv-SE")}.sqtagbackup`,
        filters: [{ name: ui("拾签标注备份"), extensions: ["sqtagbackup"] }],
      });
      if (path) {
        const result = await api<{ notice?: string }>("backup.export", {
          path,
        });
        await reload();
        tell(result.notice || msg("标注备份已导出"));
      }
    });
  const restore = () =>
    run(async () => {
      const path = await open({
        title: ui("选择标注备份"),
        multiple: false,
        filters: [{ name: ui("拾签标注备份"), extensions: ["sqtagbackup"] }],
      });
      if (typeof path !== "string") return;
      const info = await api<{
        fileCount: number;
        tagCount: number;
        createdAt: number;
      }>("backup.inspect", { path });
      if (
        !(await ask(
          msg("用备份恢复文件库"),
          msg(
            "已验证备份完整性：{0} 个文件记录，{1} 个标签。\n\n恢复将替换当前标注库，并自动备份当前内容。原文件需仍在记录的位置。",
            info.fileCount,
            info.tagCount,
          ),
          msg("备份当前库并恢复"),
        ))
      )
        return;
      await flush();
      const restored = await api<{ recoveryBackup: string }>("backup.restore", {
        path,
      });
      drafts.current?.clear();
      clearPreviews();
      setSelected([]);
      setSearch("");
      setQuery(defaultQuery());
      const b = await reload();
      setViews(b.settings.views || {});
      setTheme(b.settings.theme || "system");
      setDensity(b.settings.density || "comfortable");
      setDetailWidth(inspectorWidth(b.settings.inspectorWidth));
      setGalleryColumns(
        Number.isInteger(b.settings.galleryColumns)
          ? Math.max(2, Math.min(8, b.settings.galleryColumns!))
          : 3,
      );
      setSidebarCollapsed(b.settings.sidebarCollapsed === true);
      setSidebarSize(sidebarWidth(b.settings.sidebarWidth));
      await fetchFiles(defaultQuery());
      tell(msg("恢复完成。恢复前的标注已保存在 {0}", restored.recoveryBackup));
      await api("refresh");
    });
  const select = (f: LocalFile, e: React.MouseEvent) => {
    void flush().catch((error) => tell(message(error), true));
    setSelected((old) => {
      if (e.shiftKey && anchor.current) {
        const a = results.files.findIndex((x) => x.id === anchor.current),
          b = results.files.findIndex((x) => x.id === f.id);
        if (a >= 0)
          return [
            ...new Set([
              ...(e.ctrlKey ? old : []),
              ...results.files
                .slice(Math.min(a, b), Math.max(a, b) + 1)
                .map((x) => x.id),
            ]),
          ];
      }
      anchor.current = f.id;
      return e.ctrlKey || e.metaKey
        ? old.includes(f.id)
          ? old.filter((x) => x !== f.id)
          : [...old, f.id]
        : old.length === 1 && old[0] === f.id
          ? []
          : [f.id];
    });
  };
  const selectAllResults = () =>
    run(async () => {
      if (results.total > 1000) {
        tell(
          msg("批量标注每次最多支持 1,000 个文件，请缩小筛选范围后全选"),
          true,
        );
        return;
      }
      await flush();
      const snapshot = queryRef.current;
      const token = ++request.current;
      loadLock.current = true;
      try {
        const files: LocalFile[] = [];
        let page: Results;
        do {
          page = await api<Results>("query", {
            ...snapshot,
            offset: files.length,
            limit: 100,
          });
          if (
            token !== request.current ||
            JSON.stringify(snapshot) !== JSON.stringify(queryRef.current)
          )
            return;
          if (!page.files.length && page.hasMore)
            throw new Error(ui("文件列表已变化，请重新全选"));
          files.push(...page.files);
          if (files.length > 1000)
            throw new Error(
              ui("文件列表已变化，结果超过 1,000 项，请缩小筛选范围"),
            );
        } while (page.hasMore);
        const unique = [
          ...new Map(files.map((file) => [file.id, file])).values(),
        ];
        setResults({ ...page, offset: 0, files: unique, hasMore: false });
        setRetainedDetail(undefined);
        setSelected(unique.map((file) => file.id));
        anchor.current = unique[0]?.id || "";
      } finally {
        if (token === request.current) loadLock.current = false;
      }
    });
  const tagFilter = (id: string, exclude = false) =>
    setQuery((q) => {
      const field = exclude ? "exclude" : "include",
        other = exclude ? "include" : "exclude";
      return {
        ...q,
        [field]: q[field].includes(id)
          ? q[field].filter((x) => x !== id)
          : [...q[field], id],
        [other]: q[other].filter((x) => x !== id),
      };
    });
  const resetFilters = () => {
    setSearch("");
    setQuery((q) => ({ ...defaultQuery(), scope: q.scope }));
  };
  const selectSidebarTag = (id: string) =>
    setQuery((q) => ({
      ...q,
      include: q.include.length === 1 && q.include[0] === id ? [] : [id],
      exclude: [],
    }));
  const filterCount =
    Number(!!query.sourceHost) +
    Number(!!query.captureKind) +
    (query.searchScope && query.searchScope !== "all" ? 1 : 0) +
    query.include.length +
    query.exclude.length +
    query.kinds.length +
    Number(!!query.status) +
    Number(!!query.directory) +
    Number(!!query.from || !!query.to);
  const reloadNote = () =>
    run(async () => {
      if (!single) return;
      if (
        !(await ask(
          msg("重新读取备注"),
          msg("这会用数据库中的已保存内容替换当前草稿。"),
          msg("重新读取"),
        ))
      )
        return;
      const f = await api<LocalFile>("file", { id: single.id });
      drafts.current?.discard(f);
      setResults((r) => ({
        ...r,
        files: r.files.map((x) => (x.id === f.id ? f : x)),
      }));
    });
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      const typing = !!target.closest(
        'input,textarea,[contenteditable="true"],select',
      );
      if (quick && e.key === " " && !typing) {
        e.preventDefault();
        setQuick(undefined);
        return;
      }
      if (
        e.isComposing ||
        confirm ||
        modal ||
        quick ||
        exportTag ||
        transferMode
      )
        return;
      const inFiles = !!target.closest(".file-area");
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        openSearch();
        return;
      }
      if (typing) return;
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (!busy && !e.repeat) undo();
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
        e.preventDefault();
        if (!busy && !e.repeat) {
          if (results.total > 0 && selected.length === results.total)
            setSelected([]);
          else selectAllResults();
        }
        return;
      }
      if (e.key === "Escape") {
        setSelected([]);
        setAddMenu(false);
        setFilters(false);
      }
      if (e.key === " " && single && inFiles) {
        e.preventDefault();
        setQuick(single);
      }
      if (e.key === "Enter" && single && inFiles) {
        e.preventDefault();
        openFile(single);
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [
    boot,
    busy,
    results,
    selected,
    single,
    confirm,
    modal,
    quick,
    exportTag,
    transferMode,
    busy,
    selectAllResults,
  ]);

  if (fatal)
    return (
      <StartupWindow>
        <div className="startup">
          <img src="/icon.png" alt={ui("拾签")} />
          <h1>{ui("资料库暂时无法打开")}</h1>
          <p>{fatal}</p>
          <button className="button primary" onClick={() => location.reload()}>
            {ui("重新尝试")}
          </button>
        </div>
      </StartupWindow>
    );
  if (!boot)
    return (
      <StartupWindow>
        <div className="startup">
          <img src="/icon.png" alt={ui("拾签")} />
          <LoaderCircle className="spin" />
          <p>{ui("正在打开你的文件工作台…")}</p>
        </div>
      </StartupWindow>
    );
  return (
    <div
      className={`app-shell ${sidebarCollapsed ? "sidebar-is-collapsed" : ""} ${sidebarResizing ? "sidebar-is-resizing" : ""} ${view === "grid" ? "gallery-mode" : ""}`}
      style={{ "--sidebar-width": `${sidebarSize}px` } as CSSProperties}
      data-busy={busy}
    >
      <TagDropFeedback
        tags={boot.tags}
        tiltEnabled={boot.settings.tagDropTilt !== false}
        run={run}
        onNotify={tell}
      />
      <FirstRunGuide
        library={boot.dataPath}
        folderEnabled={boot.settings.folderAutoTagging !== false}
        aiConfigured={!!(boot.settings.ai?.endpoint && boot.settings.ai?.model)}
        helpRequest={guideRequest}
        onNotice={(text) => tell(text, true)}
      />
      <header className="topbar app-topbar">
        <div className="top-leading-actions">
          <button
            className="icon-button"
            data-tooltip={
              boot.undoLabel
                ? tf("撤销：{0}", boot.undoLabel)
                : ui("没有可撤销的操作")
            }
            aria-label={ui("撤销上一步")}
            disabled={!boot.undoLabel || busy}
            onClick={undo}
          >
            <Undo2 size={17} />
          </button>
          <button
            className="icon-button"
            aria-label={ui("刷新文件状态")}
            data-tooltip={ui("重新检查原文件状态")}
            disabled={busy}
            onClick={() =>
              run(async () => {
                clearPreviews();
                await api("refresh");
              }, ui("正在检查文件位置与状态"))
            }
          >
            <RefreshCw size={16} />
          </button>
          <button
            className="icon-button sidebar-toggle"
            aria-label={
              sidebarCollapsed ? ui("展开左侧菜单") : ui("收拢左侧菜单")
            }
            data-tooltip={
              sidebarCollapsed ? ui("展开左侧菜单") : ui("收拢左侧菜单")
            }
            aria-expanded={!sidebarCollapsed}
            aria-controls="workspace-sidebar"
            onClick={toggleSidebar}
          >
            {sidebarCollapsed ? (
              <PanelLeftOpen size={19} />
            ) : (
              <PanelLeftClose size={19} />
            )}
          </button>
        </div>
        <div
          className="window-drag-region"
          aria-label={ui("拖动窗口")}
          onMouseDown={(event) => {
            if (event.button === 0 && event.detail === 1)
              void getCurrentWebviewWindow()
                .startDragging()
                .catch((error) => tell(message(error), true));
          }}
          onDoubleClick={() =>
            void getCurrentWebviewWindow()
              .toggleMaximize()
              .catch((error) => tell(message(error), true))
          }
        >
          <span className="local-badge">
            <span /> {ui("本地资料库")}
          </span>
        </div>
        <WindowControls onError={(text) => tell(text, true)} />
      </header>
      <aside
        className="sidebar"
        id="workspace-sidebar"
        aria-label={ui("工作台导航")}
      >
        <SidebarResizeHandle
          width={sidebarSize}
          collapsed={sidebarCollapsed}
          onResizing={(active) => {
            window.dispatchEvent(new Event("shiqian-continuous-layout"));
            setSidebarResizing(active);
          }}
          onChange={({ width, collapsed }) => {
            window.dispatchEvent(new Event("shiqian-continuous-layout"));
            setSidebarSize(width);
            setSidebarCollapsed(collapsed);
          }}
          onCommit={({ width, collapsed }) => {
            setSetting("sidebarWidth", width);
            setSetting("sidebarCollapsed", collapsed);
          }}
        />
        <div className="brand">
          <img src="/icon.png" alt="" />
          <div>
            <strong>{ui("拾签")}</strong>
          </div>
        </div>
        <nav className="main-nav">
          {scopes.map((s) => {
            const Icon = s.icon;
            return (
              <button
                key={s.id}
                aria-label={ui(s.name)}
                data-tooltip={sidebarCollapsed ? ui(s.name) : undefined}
                className={query.scope === s.id ? "active" : ""}
                onClick={() => {
                  void flush().catch((e) => tell(message(e), true));
                  setSearch("");
                  setQuery({ ...defaultQuery(), scope: s.id });
                }}
              >
                <Icon size={18} />
                <span>{ui(s.name)}</span>
                <small>{boot.counts[s.id] || 0}</small>
              </button>
            );
          })}
        </nav>
        <div className="sidebar-index">
          <div className="sidebar-index-nav main-nav">
            <button
              className={
                modal === "tags" || query.include.length > 0 ? "active" : ""
              }
              aria-label={
                sidebarCollapsed ? ui("展开标签索引") : ui("标签索引")
              }
              data-tooltip={
                sidebarCollapsed ? ui("展开标签索引") : ui("管理标签")
              }
              onClick={() => {
                if (sidebarCollapsed) toggleSidebar();
                else {
                  setEditingTag(undefined);
                  setModal("tags");
                }
              }}
            >
              <Tags size={18} />
              <span>{ui("标签索引")}</span>
            </button>
            <button
              className="sidebar-index-gear icon-button"
              inert={sidebarCollapsed}
              aria-hidden={sidebarCollapsed}
              aria-label={ui("管理标签")}
              data-tooltip={ui("管理标签")}
              aria-haspopup="dialog"
              onClick={() => {
                setEditingTag(undefined);
                setModal("tags");
              }}
            >
              <Settings size={15} />
            </button>
          </div>
          <div
            className="sidebar-index-content"
            inert={sidebarCollapsed}
            aria-hidden={sidebarCollapsed}
          >
            <div className="sidebar-tags" ref={sidebarTagsMotion}>
              {boot.tags.length ? (
                [
                  { name: ui("文件夹"), label: ui("文件夹标签"), folder: true },
                  { name: ui("标签"), label: ui("普通标签"), folder: false },
                ].map((group) => {
                  const tags = boot.tags.filter(
                    (t) => (t.createdBy === "folder") === group.folder,
                  );
                  return (
                    tags.length > 0 && (
                      <section
                        className="sidebar-tag-group"
                        data-tag-kind={group.folder ? "folder" : "regular"}
                        key={group.label}
                        aria-label={ui(group.label)}
                      >
                        <div className="sidebar-tag-group-title">
                          <span>{ui(group.name)}</span>
                          <small>{tags.length}</small>
                        </div>
                        {group.folder
                          ? tags.map((t) => (
                              <div
                                className="sidebar-tag-row"
                                key={t.id}
                                data-motion-tag={t.id}
                              >
                                <button
                                  className={`sidebar-tag-filter ${query.include.includes(t.id) ? "active" : ""}`}
                                  onClick={() => selectSidebarTag(t.id)}
                                  aria-pressed={query.include.includes(t.id)}
                                  data-tooltip={t.name}
                                >
                                  {group.folder ? (
                                    <FolderOpen size={13} />
                                  ) : (
                                    <span className="tag-dot" />
                                  )}
                                  <span>{t.name}</span>
                                  <small>{t.count}</small>
                                </button>
                                <button
                                  className="icon-button sidebar-tag-edit"
                                  aria-label={tf("编辑标签{0}", t.name)}
                                  data-tooltip={ui("重命名标签")}
                                  onClick={() => {
                                    setEditingTag(t);
                                    setModal("tags");
                                  }}
                                >
                                  <Edit3 size={13} />
                                </button>
                              </div>
                            ))
                          : groupedTags(
                              tags,
                              boot.settings.tagGroups ?? emptyGroups,
                            ).map((category) => (
                              <TagBranch
                                key={category.id}
                                id={category.id}
                                name={
                                  category.id
                                    ? category.name
                                    : ui(category.name)
                                }
                                count={category.tags.length}
                                closed={(
                                  boot.settings.tagGroupsCollapsedMain ?? []
                                ).includes(category.id)}
                                onToggle={() => {
                                  const old =
                                    boot.settings.tagGroupsCollapsedMain ?? [];
                                  setSetting(
                                    "tagGroupsCollapsedMain",
                                    old.includes(category.id)
                                      ? old.filter((id) => id !== category.id)
                                      : [...old, category.id],
                                  );
                                }}
                              >
                                {category.tags.map((t) => (
                                  <div
                                    className="sidebar-tag-row"
                                    key={t.id}
                                    data-motion-tag={t.id}
                                  >
                                    <button
                                      className={`sidebar-tag-filter ${query.include.includes(t.id) ? "active" : ""}`}
                                      onClick={() => selectSidebarTag(t.id)}
                                      aria-pressed={query.include.includes(
                                        t.id,
                                      )}
                                      data-tooltip={t.name}
                                    >
                                      {group.folder ? (
                                        <FolderOpen size={13} />
                                      ) : (
                                        <span className="tag-dot" />
                                      )}
                                      <span>{t.name}</span>
                                      <small>{t.count}</small>
                                    </button>
                                    <button
                                      className="icon-button sidebar-tag-edit"
                                      aria-label={tf("编辑标签{0}", t.name)}
                                      data-tooltip={ui("重命名标签")}
                                      onClick={() => {
                                        setEditingTag(t);
                                        setModal("tags");
                                      }}
                                    >
                                      <Edit3 size={13} />
                                    </button>
                                  </div>
                                ))}
                              </TagBranch>
                            ))}
                      </section>
                    )
                  );
                })
              ) : (
                <p>{ui("暂无标签")}</p>
              )}
            </div>
          </div>
        </div>
        <div className="sidebar-bottom">
          <button
            aria-label={ui("AI 建议审核")}
            data-tooltip={sidebarCollapsed ? ui("AI 建议审核") : undefined}
            onClick={() => setModal("ai-review")}
          >
            <ClipboardCheck size={17} />
            <span>{ui("AI 建议审核")}</span>
          </button>
          <button
            className="floating-launch"
            aria-pressed={floatingVisible}
            aria-label={ui("标签浮窗")}
            data-tooltip={sidebarCollapsed ? ui("标签浮窗") : undefined}
            onClick={() => void run(() => api("floating.open"))}
          >
            <PanelsTopLeft size={17} />
            <span>{ui("标签浮窗")}</span>
          </button>
          <button
            onClick={() => setModal("settings")}
            aria-label={ui("偏好设置")}
            data-tooltip={sidebarCollapsed ? ui("偏好设置") : undefined}
          >
            <Settings size={17} />
            <span>{ui("偏好设置")}</span>
            <small>v{boot.version}</small>
          </button>
        </div>
      </aside>
      <main className="workspace">
        <div className="workspace-split">
          <WorkspaceFileColumn>
            <section className="page-heading">
              <div className="page-title-group">
                <h1>
                  {ui(currentScope.name)}
                  <span>{results.total}</span>
                </h1>
                <div
                  className={`heading-search ${searchExpanded ? "is-open" : ""}`}
                >
                  <button
                    ref={searchButton}
                    className={`icon-button search-toggle ${search ? "has-query" : ""}`}
                    aria-label={
                      searchExpanded ? ui("收起搜索栏") : ui("展开搜索栏")
                    }
                    data-tooltip={
                      searchExpanded
                        ? ui("收起搜索栏")
                        : ui("搜索文件 · Ctrl F")
                    }
                    aria-expanded={searchExpanded}
                    aria-controls="workspace-search"
                    onClick={() =>
                      searchExpanded ? setSearchExpanded(false) : openSearch()
                    }
                  >
                    <Search size={19} />
                  </button>
                  <div
                    id="workspace-search"
                    className="search-reveal"
                    inert={!searchExpanded}
                    aria-hidden={!searchExpanded}
                  >
                    <div className="search-box unified-search-box">
                      <input
                        ref={searchRef}
                        aria-label={ui("搜索文件")}
                        value={search}
                        onCompositionStart={() => {
                          composing.current = true;
                        }}
                        onCompositionEnd={(e) => {
                          composing.current = false;
                          const text = e.currentTarget.value;
                          setSearch(text);
                          setQuery((q) => ({
                            ...q,
                            text,
                            ...(q.semanticSession
                              ? {
                                  searchMode: "unified" as const,
                                  semanticSession: "",
                                }
                              : {}),
                          }));
                        }}
                        onChange={(e) => {
                          const text = e.target.value;
                          setSearch(text);
                          if (queryRef.current.semanticSession)
                            setQuery((q) => ({
                              ...q,
                              text,
                              searchMode: "unified",
                              semanticSession: "",
                              offset: 0,
                            }));
                          // Empty input must clear the query immediately, before the typing debounce.
                          if (!text.trim() && !composing.current)
                            setQuery((q) => ({
                              ...q,
                              text,
                              ...(q.semanticSession
                                ? {
                                    searchMode: "unified" as const,
                                    semanticSession: "",
                                  }
                                : {}),
                            }));
                        }}
                        onKeyDown={(e) => {
                          if (
                            e.key === "Escape" &&
                            !e.nativeEvent.isComposing
                          ) {
                            e.preventDefault();
                            e.stopPropagation();
                            setSearchExpanded(false);
                            searchButton.current?.focus();
                          }
                        }}
                        maxLength={256}
                        placeholder={ui("搜索名称、标签、备注或文件正文…")}
                      />
                      {search && (
                        <button
                          className="icon-button"
                          aria-label={ui("清空搜索")}
                          onClick={() => {
                            setSearch("");
                            setQuery((q) => ({
                              ...q,
                              text: "",
                              ...(q.semanticSession
                                ? {
                                    searchMode: "unified" as const,
                                    semanticSession: "",
                                  }
                                : {}),
                            }));
                            setResults((r) => ({
                              ...r,
                              files: r.files.map((f) =>
                                f.contentMatch
                                  ? { ...f, contentMatch: undefined }
                                  : f,
                              ),
                            }));
                            searchRef.current?.focus();
                          }}
                        >
                          <X size={14} />
                        </button>
                      )}
                      {boot.semanticEnabled && (
                        <button
                          className="search-supplement"
                          aria-label={ui("智能补充")}
                          disabled={query.searchScope === "name"}
                          data-tooltip={ui(
                            "智能补充 · 明确发送查询，或预览自然语言筛选",
                          )}
                          onClick={() => setModal("semantic")}
                        >
                          <Sparkles size={14} />
                          <span>{ui("智能补充")}</span>
                        </button>
                      )}
                    </div>
                  </div>
                </div>
              </div>
              <div className="add-wrapper">
                <button
                  className="button primary add-button"
                  aria-label={ui("加入文件")}
                  data-tooltip={ui("加入文件")}
                  onClick={() => setAddMenu((x) => !x)}
                  disabled={busy || (!!job && !job.done)}
                >
                  <Plus size={17} />
                  {ui("加入文件")}
                  <ChevronDown size={14} />
                </button>
                {addMenu && (
                  <>
                    <div
                      className="menu-dismiss"
                      onClick={() => setAddMenu(false)}
                    />
                    <div className="add-menu">
                      <button onClick={() => chooseImport()}>
                        <FilePlus2 size={16} />
                        {ui("选择文件")}
                      </button>
                      <button onClick={() => chooseImport(true)}>
                        <FolderInput size={16} />
                        {ui("选择文件夹")}
                      </button>
                      <label>
                        <input
                          type="checkbox"
                          checked={recursive}
                          onChange={(e) => setRecursive(e.target.checked)}
                        />
                        {ui("包含子文件夹")}
                      </label>
                    </div>
                  </>
                )}
              </div>
            </section>
            <section className="toolbar">
              <button
                className={`button filter-button ${filters || filterCount ? "engaged" : ""}`}
                aria-expanded={filters}
                aria-controls="workspace-filters"
                onClick={() => setFilters((v) => !v)}
              >
                <SlidersHorizontal size={15} />
                {ui("筛选")}
                {filterCount > 0 && <span>{filterCount}</span>}
              </button>
              <div className="toolbar-collections">
                <CollectionsBar
                  collections={boot.settings.savedCollections ?? []}
                  query={{ ...query, text: search }}
                  tags={boot.tags}
                  showSave={
                    filters &&
                    query.searchMode !== "semantic" &&
                    !query.semanticSession
                  }
                  onApply={(q, editConditions) => {
                    if (editConditions) setFilters(true);
                    setSearch(q.text);
                    setSearchExpanded(!!q.text);
                    setQuery({
                      ...q,
                      searchMode: "unified",
                      semanticSession: "",
                    });
                  }}
                  onChanged={reload}
                />
              </div>
              <div
                className="view-controls"
                data-view={view}
                role="group"
                aria-label={ui("文件布局")}
              >
                <WarmTooltip text={ui("瀑布流视图")}>
                  <button
                    className="icon-button"
                    aria-label={ui("瀑布流视图")}
                    aria-pressed={view === "grid"}
                    onClick={() => {
                      const v = { ...views, [query.scope]: "grid" };
                      setViews(v);
                      setSetting("views", v);
                    }}
                  >
                    <LayoutGrid size={17} />
                  </button>
                </WarmTooltip>
                <WarmTooltip text={ui("列表视图")}>
                  <button
                    className="icon-button"
                    aria-label={ui("列表视图")}
                    aria-pressed={view === "list"}
                    onClick={() => {
                      const v = { ...views, [query.scope]: "list" };
                      setViews(v);
                      setSetting("views", v);
                    }}
                  >
                    <List size={18} />
                  </button>
                </WarmTooltip>
              </div>
            </section>
            <div
              className={`filter-reveal ${filters ? "is-open" : ""}`}
              id="workspace-filters"
              inert={!filters}
              aria-hidden={!filters}
            >
              <div className="filter-clip">
                <section className="filter-panel" aria-label={ui("文件筛选")}>
                  <WebSourceFilters
                    query={query}
                    onChange={(value) =>
                      setQuery((q) => ({
                        ...q,
                        ...value,
                        offset: 0,
                        semanticSession: "",
                      }))
                    }
                  />
                  <div className="filter-line">
                    <strong>{ui("搜索范围")}</strong>
                    <GlideSelect
                      ariaLabel={ui("搜索范围")}
                      value={query.searchScope || "all"}
                      onChange={(value) =>
                        setQuery((q) => ({
                          ...q,
                          searchScope: value as "all" | "name" | "body",
                          semanticSession: "",
                          offset: 0,
                        }))
                      }
                      options={[
                        { value: "all", label: ui("名称、标签、备注与正文") },
                        { value: "name", label: ui("仅文件名") },
                        { value: "body", label: ui("仅正文") },
                      ]}
                    />
                  </div>
                  <div className="filter-line">
                    <strong>{ui("文件类型")}</strong>
                    <div className="filter-choices">
                      {Object.entries(kindNames).map(([id, name]) => (
                        <button
                          key={id}
                          aria-pressed={query.kinds.includes(id)}
                          onClick={() =>
                            setQuery((q) => ({
                              ...q,
                              kinds: q.kinds.includes(id)
                                ? q.kinds.filter((x) => x !== id)
                                : [...q.kinds, id],
                            }))
                          }
                        >
                          {name}
                        </button>
                      ))}
                    </div>
                    <GlideSelect
                      ariaLabel={ui("文件状态筛选")}
                      value={query.status}
                      onChange={(value) =>
                        setQuery((q) => ({ ...q, status: value }))
                      }
                      options={[
                        { value: "", label: ui("全部状态") },
                        { value: "available", label: ui("可用") },
                        { value: "missing", label: ui("文件缺失") },
                        { value: "offline", label: ui("存储离线") },
                        { value: "inaccessible", label: ui("访问异常") },
                      ]}
                    />
                  </div>
                  <div className="filter-line">
                    <strong>{ui("标签条件")}</strong>
                    <GlideSelect
                      ariaLabel={ui("标签匹配方式")}
                      value={query.mode}
                      onChange={(value) =>
                        setQuery((q) => ({ ...q, mode: value }))
                      }
                      options={[
                        { value: "all", label: ui("满足全部标签") },
                        { value: "any", label: ui("满足任一标签") },
                      ]}
                    />
                    <input
                      className="filter-tag-search"
                      aria-label={ui("查找筛选标签")}
                      placeholder={ui("查找标签")}
                      value={tagSearch}
                      onChange={(e) => setTagSearch(e.target.value)}
                    />
                    <small>{ui("点击名称包含，点击 − 排除")}</small>
                  </div>
                  <div className="filter-tag-list">
                    {boot.tags
                      .filter((t) => searchTagNames(t, tagSearch))
                      .map((t) => (
                        <span
                          key={t.id}
                          className={`filter-tag ${query.include.includes(t.id) ? "included" : ""} ${query.exclude.includes(t.id) ? "excluded" : ""}`}
                        >
                          <button onClick={() => tagFilter(t.id)}>
                            {query.include.includes(t.id) && (
                              <Check size={12} />
                            )}{" "}
                            {t.name}
                          </button>
                          <button
                            aria-label={tf("排除{0}", t.name)}
                            onClick={() => tagFilter(t.id, true)}
                          >
                            <Minus size={12} />
                          </button>
                        </span>
                      ))}
                  </div>
                  <div className="filter-line">
                    <strong>{ui("修改时间")}</strong>
                    <DateFilter
                      aria-label={ui("修改起始日期")}
                      value={
                        query.from !== undefined
                          ? localDateValue(new Date(query.from))
                          : ""
                      }
                      onChange={(value) =>
                        setQuery((q) => ({
                          ...q,
                          from: value ? dateBoundary(value) : undefined,
                        }))
                      }
                    />
                    <span>{ui("至")}</span>
                    <DateFilter
                      aria-label={ui("修改结束日期")}
                      value={
                        query.to !== undefined
                          ? localDateValue(new Date(query.to - 1))
                          : ""
                      }
                      onChange={(value) =>
                        setQuery((q) => ({
                          ...q,
                          to: value ? dateBoundary(value, true) : undefined,
                        }))
                      }
                    />
                    <button
                      className="button quiet directory-filter"
                      onClick={chooseDirectory}
                      data-tooltip={query.directory}
                    >
                      <FolderOpen size={14} />
                      {query.directory
                        ? query.directory.split(/[\\/]/).at(-1)
                        : ui("限定文件夹")}
                    </button>
                    {query.directory && (
                      <>
                        <button
                          className="icon-button"
                          aria-label={ui("清除目录筛选")}
                          onClick={() =>
                            setQuery((q) => ({ ...q, directory: "" }))
                          }
                        >
                          <X size={13} />
                        </button>
                        <label>
                          <input
                            type="checkbox"
                            checked={query.recursive}
                            onChange={(e) =>
                              setQuery((q) => ({
                                ...q,
                                recursive: e.target.checked,
                              }))
                            }
                          />
                          {ui("包含子目录")}
                        </label>
                      </>
                    )}
                  </div>
                </section>
              </div>
            </div>
            {(filterCount > 0 || query.text) && (
              <div className="active-filters">
                <span>{ui("正在筛选")}</span>
                {query.include.map((id) => (
                  <button key={id} onClick={() => tagFilter(id)}>
                    #{" "}
                    {boot.tags.find((t) => t.id === id)?.name ||
                      ui("已删除标签")}
                    <X size={11} />
                  </button>
                ))}
                {query.exclude.map((id) => (
                  <button
                    key={id}
                    className="negative"
                    onClick={() => tagFilter(id, true)}
                  >
                    {ui("排除")}
                    {boot.tags.find((t) => t.id === id)?.name}
                    <X size={11} />
                  </button>
                ))}
                <button className="reset-filters" onClick={resetFilters}>
                  {ui("清除条件")}
                </button>
              </div>
            )}
            <div className="content-meta">
              <div>
                <span className="result-count" role="status" aria-live="polite">
                  <span className="result-count-indicator" aria-hidden="true">
                    <span className="tiny-dot" />
                    <LoaderCircle
                      size={13}
                      className={`spin ${loading ? "is-loading" : ""}`}
                    />
                  </span>
                  {query.text &&
                    results.localSearch?.contentEnabled === false && (
                      <span className="local-search-status">
                        {ui("正文索引未开启，仅搜索文件信息")}
                      </span>
                    )}
                  {query.searchScope && query.searchScope !== "all" && (
                    <span className="local-search-status">
                      {query.searchScope === "name"
                        ? ui("仅文件名")
                        : ui("仅正文")}
                    </span>
                  )}
                  {query.semanticSession && (
                    <span className="local-search-status">
                      {ui("已含智能补充")}
                    </span>
                  )}
                  <span>
                    {selected.length
                      ? tf("已选 {0} 项", selected.length)
                      : tf("{0} 份资料", results.total)}
                  </span>
                </span>
              </div>
              {view === "grid" && boot.settings.showColumnControls === true && (
                <div
                  className="gallery-controls"
                  role="group"
                  aria-label={ui("图片列数")}
                >
                  <input
                    type="range"
                    aria-label={ui("瀑布流列数")}
                    aria-valuetext={tf("{0} 列", visibleColumns)}
                    min={2}
                    max={columnCapacity}
                    step={1}
                    value={visibleColumns}
                    onChange={(event) =>
                      changeColumns(Number(event.target.value))
                    }
                  />
                  <output aria-live="polite">
                    {visibleColumns} {ui("列")}
                  </output>
                </div>
              )}
              <div className="file-tools">
                <div className="sort-control">
                  <ArrowDownUp size={13} />
                  <GlideSelect
                    ariaLabel={ui("文件排序")}
                    value={query.sort}
                    disabled={query.searchMode === "semantic"}
                    onChange={(value) =>
                      setQuery((q) => ({ ...q, sort: value }))
                    }
                    options={
                      query.searchMode === "semantic"
                        ? [{ value: "relevance", label: ui("语义相关度") }]
                        : [
                            { value: "relevance", label: ui("相关度") },
                            { value: "added", label: ui("加入时间") },
                            { value: "modified", label: ui("修改时间") },
                            { value: "name", label: ui("名称") },
                            { value: "size", label: ui("文件大小") },
                          ]
                    }
                  />
                  <button
                    aria-label={ui("切换排序方向")}
                    disabled={
                      query.searchMode === "semantic" ||
                      query.sort === "relevance"
                    }
                    onClick={() =>
                      setQuery((q) => ({
                        ...q,
                        direction: q.direction === "asc" ? "desc" : "asc",
                      }))
                    }
                  >
                    {query.searchMode === "semantic" ||
                    query.sort === "relevance"
                      ? ui("相关度")
                      : query.direction === "asc"
                        ? ui("升序")
                        : ui("降序")}
                  </button>
                </div>
                <div className="selection-actions">
                  <WarmTooltip
                    text={
                      selectedFiles.length > 0 &&
                      selectedFiles.every((f) => f.favorite)
                        ? ui("取消收藏")
                        : ui("收藏")
                    }
                  >
                    <button
                      aria-label={
                        selectedFiles.length > 0 &&
                        selectedFiles.every((f) => f.favorite)
                          ? ui("取消收藏")
                          : ui("收藏")
                      }
                      disabled={busy || selected.length === 0}
                      onClick={() =>
                        run(() =>
                          updateFiles("files.favorite", {
                            value: !selectedFiles.every((f) => f.favorite),
                          }),
                        )
                      }
                    >
                      <Star
                        size={16}
                        fill={
                          selectedFiles.length > 0 &&
                          selectedFiles.every((f) => f.favorite)
                            ? "currentColor"
                            : "none"
                        }
                      />
                    </button>
                  </WarmTooltip>
                  <WarmTooltip text={ui("导出资料包")}>
                    <button
                      aria-label={ui("导出资料包")}
                      disabled={busy || selected.length === 0}
                      onClick={() => {
                        void run(async () => {
                          await flush();
                          setTransferIds(selectedFiles.map((f) => f.id));
                          setTransferMode("export");
                        });
                      }}
                    >
                      <PackageOpen size={16} />
                    </button>
                  </WarmTooltip>
                  <WarmTooltip text={ui("移除")}>
                    <button
                      aria-label={ui("移除")}
                      disabled={busy || selected.length === 0}
                      onClick={remove}
                    >
                      <Trash2 size={16} />
                    </button>
                  </WarmTooltip>
                </div>
              </div>
            </div>
            <div className="content-body" ref={contentBody}>
              <FilesView
                key={JSON.stringify(query)}
                files={results.files}
                searchHint={
                  query.text
                    ? query.searchScope === "name"
                      ? ui("当前仅搜索文件名，可在筛选中扩大范围。")
                      : query.searchScope === "body"
                        ? ui(
                            "仅搜索已完成索引的正文；扫描图片或超出索引范围的内容不参与。",
                          )
                        : ui(
                            "试试更短的关键词或标签；图片按名称、标签和备注查找，文档正文需要开启索引。",
                          )
                    : undefined
                }
                contentQuery={
                  query.searchMode === "content" ||
                  query.searchMode === "unified" ||
                  query.searchMode === "semantic"
                    ? search
                    : ""
                }
                total={results.total}
                view={view}
                density={density}
                galleryColumns={galleryColumns}
                layoutCapture={galleryLayoutCapture}
                onColumnCapacity={setColumnCapacity}
                selected={selected}
                focusOnResize={!inspectorVisible}
                onSelect={select}
                onOpen={openFile}
                onMore={() => fetchFiles(query, true)}
                hasMore={results.hasMore}
                loading={loading}
                onAdd={() => chooseImport()}
                filtered={
                  !!filterCount || !!query.text || query.scope !== "all"
                }
                onReset={resetFilters}
              />
            </div>
          </WorkspaceFileColumn>
          <InspectorPane
            open={inspectorVisible}
            width={detailWidth}
            onChange={setDetailWidth}
            onCommit={(width) => setSetting("inspectorWidth", width)}
          >
            <Inspector
              files={inspectorFiles}
              tags={boot.tags}
              draft={single ? drafts.current?.get(single) : undefined}
              onNote={(text) => {
                if (!single) return;
                drafts.current?.edit(single, text);
                clearTimeout(noteTimer.current);
                noteTimer.current = setTimeout(
                  () =>
                    void drafts.current
                      ?.flush(single.id)
                      .catch((e) => tell(message(e), true)),
                  500,
                );
              }}
              onFlush={() => void flush().catch((e) => tell(message(e), true))}
              onReloadNote={reloadNote}
              onTag={(t, add) => run(() => addTag(t, add))}
              onCreateTag={(name) => run(() => createAndTag(name))}
              onOpen={() => single && openFile(single)}
              onOpenSource={() =>
                single &&
                run(() => api("capture.source.open", { id: single.id }))
              }
              onReveal={() =>
                single && run(() => api("files.reveal", { id: single.id }))
              }
              onCopy={() =>
                single &&
                run(
                  () =>
                    navigator.clipboard.writeText(
                      single.kind === "web"
                        ? single.webSource?.url || single.path
                        : single.path,
                    ),
                  single.kind === "web" ? msg("链接已复制") : msg("路径已复制"),
                )
              }
              onRelink={relink}
              onContentSaved={(undoId) =>
                void refresh()
                  .then(() =>
                    tell(
                      single?.kind === "web"
                        ? msg("已保存网页阅读副本「{0}」", single?.name)
                        : msg("已保存 Markdown 文件「{0}」", single?.name),
                      false,
                      undoId,
                    ),
                  )
                  .catch((e) => tell(message(e), true))
              }
              onPreview={() => single && setQuick(single)}
              onClose={() => {
                void flush().catch((error) => tell(message(error), true));
                setSelected([]);
              }}
              onAnalyze={() =>
                run(async () => {
                  await api("ai.enqueue", {
                    ids: selectedFiles.map((f) => f.id),
                  });
                  await refresh();
                }, ui("已加入 AI 识别队列"))
              }
              onCancelAI={() =>
                run(async () => {
                  await api("ai.cancel", {
                    ids: selectedFiles.map((f) => f.id),
                  });
                  await refresh();
                }, ui("已取消所选文件的 AI 任务"))
              }
              onConfirmAI={(tag) =>
                single &&
                run(async () => {
                  await api("ai.confirm", {
                    id: single.id,
                    tagId: tag.id,
                    version: single.version,
                  });
                  await refresh();
                }, ui("标签已接受并加入浮窗"))
              }
              onConfigureAI={() => setModal("settings")}
              busy={busy}
            />
          </InspectorPane>
        </div>
        <footer className="statusbar">
          <button
            className="task-entry"
            onClick={() => setModal("tasks")}
            aria-label={ui("查看后台任务")}
          >
            {tasks?.transfer.busy ? (
              <LoaderCircle size={13} className="spin" />
            ) : (
              <Clock3 size={13} />
            )}
            {tasks?.transfer.busy ? tasks.transfer.phase : ui("任务")}
            {!!tasks?.ai.failed && (
              <small>
                {tasks.ai.failed} {ui("失败")}
              </small>
            )}
          </button>
          {boot.notice && (
            <button
              className="recovery-entry"
              onClick={() => setModal("recovery")}
            >
              {ui("恢复提示")}
            </button>
          )}
          <span>
            {job && !job.done ? (
              <>
                <LoaderCircle size={13} className="spin" />
                {ui("正在加入 · 已处理")}
                {job.processed} {ui("个")}
                <button onClick={() => run(() => api("import.cancel"))}>
                  {ui("取消")}
                </button>
              </>
            ) : job ? (
              <>
                <Check size={13} />
                {job.cancelled ? ui("已停止加入") : ui("加入完成")}{" "}
                {ui("· 新增")}
                {job.added} {ui("个")}
                <button onClick={() => setModal("import")}>
                  {ui("查看详情")}
                </button>
              </>
            ) : (
              <>{boot.settings.ai?.enabled ? ui("AI 自动标注已开启") : ""}</>
            )}
          </span>
          <span>
            {ui("Ctrl 多选")}
            <span className="separator">·</span>
            {ui("Space 预览")}
            <span className="separator">·</span>
            {ui("Ctrl A 全选 / 取消全选 · Ctrl Z 撤销")}
          </span>
        </footer>
      </main>
      <button
        className="tag-fab"
        aria-label={ui("添加标签悬浮按钮")}
        data-tooltip={
          selectedFiles.length
            ? tf("为 {0} 个文件添加标签", selectedFiles.length)
            : ui("新建标签")
        }
        onClick={() => setModal("quick-tag")}
        disabled={busy}
      >
        <Plus size={21} />
        <span>{ui("添加标签")}</span>
        {selectedFiles.length > 0 && <small>{selectedFiles.length}</small>}
      </button>
      {modal === "quick-tag" && (
        <Modal title={ui("添加标签")} onClose={() => setModal("")}>
          <QuickTagPanel
            tags={boot.tags}
            count={selectedFiles.length}
            onApply={async (name) => {
              if (selectedFiles.length) await createAndTag(name);
              else {
                await api("tag.create", { name });
                await refresh();
              }
              if (!selectedFiles.length) tell(msg("已创建标签「{0}」", name));
              setModal("");
            }}
            onManage={() => {
              setEditingTag(undefined);
              setModal("tags");
            }}
          />
        </Modal>
      )}
      {dragging && (
        <div className="drop-overlay">
          <div>
            <FolderInput size={48} />
            <h2>{ui("松手，加入文件库")}</h2>
            <p>{ui("原文件不移动，标注留在拾签")}</p>
          </div>
        </div>
      )}
      {modal === "tags" && (
        <Modal title={ui("管理标签")} onClose={() => setModal("")}>
          <div className="tag-management-body">
            <GroupDisclosure>
              <GroupManager tags={boot.tags} onUpdated={reload} />
            </GroupDisclosure>
            <TagManager
              tags={boot.tags}
              initialTag={editingTag}
              onAliases={(t) => {
                setModal("");
                setAliasSource(t);
              }}
              onMerge={(t) => {
                setModal("");
                setMergeSource(t);
              }}
              onExport={(tag) => {
                setModal("");
                setExportTag(tag);
              }}
              busy={busy}
              onCreate={async (name) => {
                const tag = await api<Tag>("tag.create", { name });
                await reload();
                tell(msg("已创建「{0}」", tag.name));
              }}
              onRename={async (t, name) => {
                if (t.createdBy !== "folder") {
                  const p = await api<{
                    conflicts: string[];
                    affected: number;
                    collections: { name: string }[];
                  }>("tags.aliases.preview", {
                    id: t.id,
                    version: t.version,
                    name,
                    aliases: (t.aliases ?? []).filter(
                      (a) =>
                        a.normalize("NFC").toLocaleLowerCase() !==
                        name.normalize("NFC").toLocaleLowerCase(),
                    ),
                  });
                  if (p.conflicts.length)
                    throw new Error(p.conflicts.join("；"));
                  if (
                    !(await ask(
                      msg("重命名标签"),
                      msg(
                        "「{0}」将改为「{1}」，同步 {2} 个文件、{3} 个集合及标签浮窗；其余别名保留。",
                        t.name,
                        name,
                        p.affected,
                        p.collections.length,
                      ),
                      msg("确认修改"),
                    ))
                  )
                    return;
                }
                await mutate("tags.rename", {
                  id: t.id,
                  name,
                  version: t.version,
                });
              }}
              onDelete={(t) =>
                run(async () => {
                  if (
                    await ask(
                      msg("删除标签"),
                      msg(
                        "“{0}”关联了 {1} 个文件。删除会移除这些关联及 {2} 个别名，文件记录和备注会保留。",
                        t.name,
                        t.count,
                        t.aliases?.length ?? 0,
                      ),
                      msg("删除标签"),
                    )
                  ) {
                    await mutate("tags.delete", {
                      id: t.id,
                      version: t.version,
                    });
                  }
                })
              }
            />
          </div>
        </Modal>
      )}
      {aliasSource && (
        <TagAliases
          tag={aliasSource}
          onClose={() => {
            setAliasSource(undefined);
            setModal("tags");
          }}
          onSaved={async (undoId) => {
            await refresh();
            tell(
              msg("已更新「{0}」的规范名称与别名", aliasSource.name),
              false,
              undoId,
            );
          }}
        />
      )}
      {mergeSource && (
        <MergeTagDialog
          source={mergeSource}
          tags={boot.tags}
          onClose={() => {
            setMergeSource(undefined);
            setModal("tags");
          }}
          onMerged={async (source, target) => {
            const q = queryRef.current;
            const next = {
              ...q,
              include: [
                ...new Set(
                  q.include.map((id) => (id === source ? target : id)),
                ),
              ],
              exclude: [
                ...new Set(
                  q.exclude.map((id) => (id === source ? target : id)),
                ),
              ],
            };
            queryRef.current = next;
            setQuery(next);
            await refresh();
            tell(msg("标签已合并，关联文件已同步"));
          }}
        />
      )}
      {exportTag && (
        <ExportTagDialog
          tag={exportTag}
          onClose={() => setExportTag(undefined)}
        />
      )}
      {transferMode && (
        <TransferDialog
          mode={transferMode}
          ids={transferIds}
          tags={boot.tags}
          onClose={() => setTransferMode(undefined)}
          onImported={() => {
            void refresh().catch((e) => tell(message(e), true));
          }}
        />
      )}
      {modal === "import-semantic" && (
        <ImportSemanticPrompt
          ids={importSemanticIds}
          onClose={() => {
            setImportSemanticIds([]);
            setModal("");
          }}
          onConfigure={() => setModal("settings")}
          onStarted={() => {
            setImportSemanticIds([]);
            setModal("");
            tell(msg("已开始为本次新文件生成语义索引"));
          }}
        />
      )}
      {modal === "semantic" && (
        <SemanticSearch
          query={{ ...query, text: search }}
          onClose={() => setModal("")}
          onConfigure={() => setModal("settings")}
          onApply={(q) => {
            setSearch(q.text);
            setQuery(q);
            setSearchExpanded(true);
          }}
        />
      )}
      {modal === "watch" && <WatchFolders onClose={() => setModal("")} />}
      {modal === "ai-review" && (
        <AiReview
          onClose={() => setModal("")}
          onChanged={refresh}
          onConfigure={() => setModal("settings")}
        />
      )}
      {modal === "repair" && (
        <PathRepair
          onClose={() => setModal("")}
          onRepaired={async (result) => {
            clearPreviews();
            if (result.mapCollections)
              setQuery((q) => {
                const directory = q.directory;
                const old = result.oldRoot.replace(/[\\/]+$/, "");
                if (
                  directory.toLowerCase() === old.toLowerCase() ||
                  directory.toLowerCase().startsWith(old.toLowerCase() + "\\")
                ) {
                  return {
                    ...q,
                    directory: result.newRoot + directory.slice(old.length),
                    offset: 0,
                  };
                }
                return q;
              });
            await refresh();
          }}
        />
      )}
      {modal === "settings" && (
        <Modal
          title={ui("偏好设置")}
          onClose={() =>
            setModal(importSemanticIds.length ? "import-semantic" : "")
          }
        >
          <SettingsPanel
            onHelp={() => {
              setModal("");
              setGuideRequest((value) => value + 1);
            }}
            boot={boot}
            theme={theme}
            setTheme={(s) => {
              setTheme(s);
              setSetting("theme", s);
            }}
            onAppearanceChange={(key, value) => setSetting(key, value)}
            showColumnControls={boot.settings.showColumnControls === true}
            setShowColumnControls={(value) =>
              setSetting("showColumnControls", value)
            }
            density={density}
            setDensity={(s) => {
              setDensity(s);
              setSetting("density", s);
            }}
            onPackageExport={() => {
              void run(async () => {
                await flush();
                setModal("");
                setTransferIds([]);
                setTransferMode("export");
              });
            }}
            onPackageImport={() => {
              setModal("");
              setTransferIds([]);
              setTransferMode("import");
            }}
            onWatch={() => setModal("watch")}
            onRepair={() => {
              setModal("repair");
            }}
            onBackup={backup}
            onRestore={restore}
            onClear={() =>
              run(async () => {
                await api("cache.clear");
                clearPreviews();
                redraw((x) => x + 1);
              }, ui("预览缓存已清理"))
            }
            onData={() => run(() => api("data.reveal"))}
            onDiagnostics={() => setModal("diagnostics")}
            busy={busy}
          />
        </Modal>
      )}
      {modal === "import" && job && (
        <Modal title={ui("文件加入详情")} onClose={() => setModal("")}>
          <ImportDetails
            job={job}
            busy={busy}
            onRetry={() => void run(() => startImport(job.failedPaths || []))}
          />
        </Modal>
      )}
      {modal === "tasks" && (
        <Modal title={ui("后台任务")} onClose={() => setModal("")}>
          <TaskPanel
            snapshot={tasks}
            onSemantic={(ids) => {
              setImportSemanticIds(ids);
              setModal("import-semantic");
            }}
            onWatch={() => setModal("watch")}
            onRetryImport={async (paths) => {
              await startImport(paths);
            }}
          />
        </Modal>
      )}
      {modal === "diagnostics" && (
        <Modal title={ui("诊断报告")} onClose={() => setModal("")}>
          <DiagnosticPanel />
        </Modal>
      )}
      {modal === "recovery" && (
        <Modal title={ui("恢复提示")} onClose={() => setModal("")}>
          <div className="modal-content">
            <p className="confirm-text">{boot.notice}</p>
            <div className="settings-buttons">
              <button
                className="button"
                onClick={() => void run(() => api("data.reveal"))}
              >
                <FolderOpen size={15} />
                {ui("打开数据目录")}
              </button>
              <button
                className="button primary"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await api("recovery.dismiss");
                    await reload();
                    setModal("");
                  })
                }
              >
                {ui("已了解")}
              </button>
            </div>
          </div>
        </Modal>
      )}
      {quick && (
        <Modal title={quick.name} onClose={() => setQuick(undefined)} wide>
          {quick.kind === "web" ||
          quick.kind === "image" ||
          quick.kind === "pdf" ||
          isOfficePreview(quick) ||
          isHtml(quick) ||
          /^(pptx|md|markdown|txt|log)$/i.test(quick.extension) ? (
            <div className="quick-content-reader">
              <ContentReader
                key={quick.id}
                file={quick}
                editable={false}
                onSaved={() => {}}
              />
            </div>
          ) : (
            <div className="quick-preview">
              <Preview file={quick} large />
            </div>
          )}
          <div className="quick-footer">
            <span>
              {quick.kind === "pdf" ? ui("PDF · 本地只读预览") : quick.path}
            </span>
            <button className="button primary" onClick={() => openFile(quick)}>
              <ArrowUpRight size={15} />
              {ui("打开原文件")}
            </button>
          </div>
        </Modal>
      )}
      {confirm && (
        <Modal title={ui(confirm.title)} onClose={() => answer(false)}>
          <div className="modal-content">
            <p className="confirm-text">{ui(confirm.text)}</p>
            <div className="confirm-actions">
              <button className="button" onClick={() => answer(false)}>
                {ui("取消")}
              </button>
              <button className="button primary" onClick={() => answer(true)}>
                {ui(confirm.label)}
              </button>
            </div>
          </div>
        </Modal>
      )}
      <WorkspaceToasts
        items={toasts}
        undoId={boot.undoId}
        busy={busy}
        onUndo={(item) =>
          void run(async () => {
            await flush();
            await api("undo", { expectedId: item.undoId });
            await refresh();
            setToasts((old) => old.filter((t) => t.id !== item.id));
            tell(msg("已撤销：{0}", item.text));
          })
        }
        onDismiss={(id) =>
          setToasts((old) => old.filter((item) => item.id !== id))
        }
      />
    </div>
  );
}
