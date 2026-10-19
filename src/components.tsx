import { SettingsSection } from "./SettingsSection";
import { BrowserCaptureSettings } from "./WebCapture";
import { WarmTooltip } from "./WarmTooltip";
import { currentLanguage } from "./i18n";
import { tagTone, tagStyle } from "./tagColors";
import { accentColors, DEFAULT_ACCENT_COLOR } from "./appearance";
import { t as ui, tf } from "./i18n";
import SquishSwitch from "./SquishSwitch";
import { SemanticSettings } from "./Semantic";
import { GlideSelect } from "./GlideSelect";
import { RubberSegment } from "./ReaderControls";
import { ContentSnippet, TextIndexSettings } from "./TextIndex";
import { useListColumns } from "./ListColumns";
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useMemo,
  type ReactNode,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  X,
  Star,
  FileText,
  ImageIcon,
  FolderOpen,
  ArrowUpRight,
  ExternalLink,
  Copy,
  Link2,
  Plus,
  Check,
  Minus,
  LoaderCircle,
  AlertCircle,
  CheckCheck,
  Tags,
  Trash2,
  Edit3,
  Download,
  Upload,
  Sun,
  Moon,
  Monitor,
  HardDrive,
  RefreshCw,
  Maximize2,
  Sparkles,
} from "lucide-react";
import {
  LocalFile,
  Tag,
  Query,
  ImportJob,
  Bootstrap,
  size,
  date,
  statusNames,
  kindNames,
  message,
  tagMatches,
  tagSearch,
} from "./api";
import { Preview } from "./preview";
import { ContentReader } from "./ContentReader";
import { Draft } from "./notes";
import { MasonryGallery } from "./MasonryGallery";
import { TagSource } from "./TagSource";
import { FileTagsHover } from "./FileTagsHover";
import { AutoTagSettings } from "./AutoTagSettings";
import { SelectionCheck, reducedMotion, useTagArrival } from "./microMotion";
import { AiTaskStatus } from "./TaskStatus";
import { BackupStatus } from "./SupportPanel";

export function Modal({
  title,
  children,
  onClose,
  wide = false,
  dismissDisabled = false,
  className = "",
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  wide?: boolean;
  dismissDisabled?: boolean;
  className?: string;
}) {
  const [closing, setClosing] = useState(false);
  const closeTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const closeAction = useRef(onClose);
  closeAction.current = onClose;
  function dismiss() {
    if (dismissDisabled || closeTimer.current) return;
    if (reducedMotion()) {
      onClose();
      return;
    }
    setClosing(true);
    closeTimer.current = setTimeout(() => {
      closeTimer.current = undefined;
      setClosing(false);
      closeAction.current();
    }, 140);
  }
  useEffect(() => () => clearTimeout(closeTimer.current), []);
  const ref = useRef<HTMLDivElement>(null);
  const previousFocus = useRef(document.activeElement as HTMLElement | null);
  useEffect(() => {
    const previous = previousFocus.current;
    const el = ref.current;
    if (!el?.contains(document.activeElement))
      el?.querySelector<HTMLElement>("[autofocus],input,button")?.focus();
    return () => previous?.focus();
  }, []);
  return (
    <div
      className={`modal-backdrop ${closing ? "is-closing" : ""}`}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) dismiss();
      }}
    >
      <div
        ref={ref}
        className={`modal ${wide ? "modal-wide" : ""} ${className}`}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        inert={closing}
        aria-hidden={closing || undefined}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            dismiss();
          }
          if (e.key === "Tab") {
            const items = [
              ...(ref.current?.querySelectorAll<HTMLElement>(
                'button:not([disabled]),input:not([disabled]),textarea:not([disabled]),select:not([disabled]),summary,a[href],[tabindex="0"]',
              ) ?? []),
            ].filter((n) => n.offsetParent !== null);
            if (items.length) {
              if (e.shiftKey && document.activeElement === items[0]) {
                e.preventDefault();
                items.at(-1)?.focus();
              } else if (
                !e.shiftKey &&
                document.activeElement === items.at(-1)
              ) {
                e.preventDefault();
                items[0].focus();
              }
            }
          }
        }}
      >
        <header className="modal-header">
          <h2>{title}</h2>
          <button
            className="icon-button"
            onClick={dismiss}
            disabled={dismissDisabled}
            aria-label={ui("关闭对话框")}
          >
            <X size={18} />
          </button>
        </header>
        {children}
      </div>
    </div>
  );
}

export function FilesView({
  files: inputFiles,
  contentQuery,
  searchHint,
  total,
  view,
  density,
  galleryColumns,
  layoutCapture,
  onColumnCapacity,
  selected,
  focusOnResize,
  onSelect,
  onOpen,
  onMore,
  hasMore,
  loading,
  onAdd,
  filtered,
  onReset,
}: {
  files: LocalFile[];
  contentQuery: string;
  searchHint?: string;
  total: number;
  view: string;
  density: string;
  galleryColumns: number;
  layoutCapture: React.RefObject<(() => void) | undefined>;
  onColumnCapacity: (capacity: number) => void;
  selected: string[];
  focusOnResize: boolean;
  onSelect: (f: LocalFile, e: React.MouseEvent) => void;
  onOpen: (f: LocalFile) => void;
  onMore: () => void;
  hasMore: boolean;
  loading: boolean;
  onAdd: () => void;
  filtered: boolean;
  onReset: () => void;
}) {
  const files = useMemo(
    () =>
      inputFiles.map((file) => ({
        ...file,
        contentMatch:
          contentQuery.trim() && file.contentMatch?.queryText === contentQuery
            ? file.contentMatch
            : undefined,
        searchMatch:
          contentQuery.trim() && file.searchMatch?.queryText === contentQuery
            ? file.searchMatch
            : undefined,
      })),
    [inputFiles, contentQuery],
  );
  const listColumns = useListColumns();
  const scroll = useRef<HTMLDivElement>(null);
  const layout = useRef<HTMLDivElement>(null);
  const previousView = useRef(view);
  const offsets = useRef<Record<string, number>>({});
  const measurements = useRef(
    new Map<string, { width: number; height: number }>(),
  );
  const focusRequest = useRef<string | undefined>(undefined);
  const motionSnapshot = useRef(new Map<string, DOMRect>());
  const liveResize = useRef(false);
  const [width, setWidth] = useState(700);
  useLayoutEffect(() => {
    if (!scroll.current) return;
    const element = scroll.current;
    const style = getComputedStyle(element);
    setWidth(
      element.clientWidth -
        parseFloat(style.paddingLeft) -
        parseFloat(style.paddingRight),
    );
    element.scrollTop = offsets.current[view] ?? 0;
    const capture = () => {
      motionSnapshot.current.clear();
      element
        .querySelectorAll<HTMLButtonElement>(".masonry-card")
        .forEach((card) => {
          if (card.dataset.fileId)
            motionSnapshot.current.set(
              card.dataset.fileId,
              card.getBoundingClientRect(),
            );
        });
    };
    layoutCapture.current = capture;
    let endResize: ReturnType<typeof setTimeout>;
    const resizing = () => {
      liveResize.current = true;
      motionSnapshot.current.clear();
      clearTimeout(endResize);
      endResize = setTimeout(() => {
        liveResize.current = false;
      }, 160);
    };
    window.addEventListener("resize", resizing);
    window.addEventListener("shiqian-continuous-layout", resizing);
    const obs = new ResizeObserver((es) => {
      if (liveResize.current) resizing();
      else capture();
      setWidth(es[0].contentRect.width);
    });
    obs.observe(element);
    return () => {
      obs.disconnect();
      clearTimeout(endResize);
      window.removeEventListener("resize", resizing);
      window.removeEventListener("shiqian-continuous-layout", resizing);
      layoutCapture.current = undefined;
    };
  }, [view, layoutCapture]);
  const capacity = Math.max(2, Math.min(8, Math.floor((width + 14) / 134)));
  useEffect(() => onColumnCapacity(capacity), [capacity, onColumnCapacity]);
  const contentSearch = files.some((f) => f.contentMatch);
  const rowHeight = contentSearch ? 90 : density === "compact" ? 58 : 68;
  const virtual = useVirtualizer({
    count: files.length,
    getScrollElement: () => scroll.current,
    enabled: view === "list",
    initialOffset: () => offsets.current.list ?? 0,
    getItemKey: (index) => files[index].id,
    estimateSize: () => rowHeight,
    overscan: 2,
  });
  useLayoutEffect(() => {
    virtual.measure();
  }, [rowHeight]);
  useLayoutEffect(() => {
    const changed = previousView.current !== view;
    previousView.current = view;
    if (!changed || matchMedia("(prefers-reduced-motion: reduce)").matches)
      return;
    // Animate the content after its geometry is committed, keeping the scroller
    // and both layouts' remembered offsets outside the moving layer.
    const motion = layout.current?.animate(
      [
        {
          transform: `translateX(${view === "list" ? 16 : -16}px)`,
          opacity: 0.72,
        },
        { transform: "translateX(0)", opacity: 1 },
      ],
      { duration: 240, easing: "cubic-bezier(0.22, 1, 0.36, 1)" },
    );
    return () => motion?.cancel();
  }, [view]);
  return (
    <div
      key={view}
      className="file-area"
      ref={scroll}
      role="listbox"
      aria-label={ui("文件结果")}
      aria-multiselectable="true"
      tabIndex={0}
      onScroll={() => {
        const el = scroll.current;
        if (el) offsets.current[view] = el.scrollTop;
        if (
          el &&
          hasMore &&
          !loading &&
          el.scrollHeight - el.scrollTop - el.clientHeight < 400
        )
          onMore();
      }}
    >
      <div
        ref={layout}
        className="files-layout-motion"
        data-layout={view}
        style={
          {
            ...listColumns.style,
            "--list-min-height": `${rowHeight - 7}px`,
          } as React.CSSProperties
        }
      >
        {files.length === 0 ? (
          <div className="empty-state">
            <div className="empty-art">
              <div className="paper-one">
                <ImageIcon size={34} strokeWidth={1.4} />
              </div>
              <div className="paper-two">
                <FileText size={34} strokeWidth={1.4} />
              </div>
              <span className="floating-tag">
                <Tags size={14} /> {ui("有迹可寻")}
              </span>
            </div>
            <h2>
              {filtered
                ? ui("这里还没有符合条件的文件")
                : ui("给文件一个被找到的理由")}
            </h2>
            <p>
              {filtered ? (
                searchHint || ui("试试其他关键词，或调整筛选条件。")
              ) : (
                <>
                  {ui("把图片、文档和灵感留在原来的位置，")}
                  <br />
                  {ui("用标签串起它们的用途。")}
                </>
              )}
            </p>
            <button
              className="button primary"
              onClick={filtered ? onReset : onAdd}
            >
              {filtered ? <X size={16} /> : <Plus size={16} />}{" "}
              {filtered ? ui("清除筛选条件") : ui("加入第一份文件")}
            </button>
            <span className="subtle">{ui("也可以将文件或文件夹拖入窗口")}</span>
          </div>
        ) : (
          <>
            {view === "list" && listColumns.heading}
            {view === "grid" ? (
              <MasonryGallery
                files={files}
                scroll={scroll}
                width={width}
                columns={Math.min(galleryColumns, capacity)}
                gap={density === "compact" ? 10 : 18}
                selected={selected}
                onSelect={(file, event) => {
                  if (focusOnResize) focusRequest.current = file.id;
                  onSelect(file, event);
                }}
                onOpen={onOpen}
                initialOffset={offsets.current.grid ?? 0}
                measurements={measurements}
                focusRequest={focusRequest}
                motionSnapshot={motionSnapshot}
                liveResize={liveResize}
              />
            ) : (
              <div
                className="virtual-space"
                style={{ height: virtual.getTotalSize() }}
              >
                {virtual.getVirtualItems().map((row) => {
                  const f = files[row.index];
                  return (
                    <div
                      className="virtual-row list-row"
                      key={f.id}
                      data-index={row.index}
                      ref={virtual.measureElement}
                      style={{
                        transform: `translateY(${row.start}px)`,
                        minHeight: rowHeight,
                      }}
                    >
                      <FileTagsHover file={f} onlyWhenTruncated>
                        {(hoverHandlers) => (
                          <button
                            {...hoverHandlers}
                            data-file-id={f.id}
                            className={`file-card file-list ${selected.includes(f.id) ? "is-selected" : ""} ${f.status !== "available" ? "is-unavailable" : ""}`}
                            role="option"
                            aria-selected={selected.includes(f.id)}
                            aria-label={f.name}
                            onClick={(event) => onSelect(f, event)}
                            onDoubleClick={() => onOpen(f)}
                          >
                            <span
                              className={`file-art ${f.kind !== "image" && f.kind !== "pdf" ? "is-glyph" : ""}`}
                            >
                              <Preview file={f} compact />
                              <SelectionCheck
                                checked={selected.includes(f.id)}
                                small
                              />
                            </span>
                            <span className="file-description">
                              <span className="file-name" data-tooltip={f.name}>
                                {f.name}
                                {f.favorite && (
                                  <Star size={12} fill="currentColor" />
                                )}
                              </span>
                              <ContentSnippet file={f} />
                              <span
                                className="file-subline"
                                data-tooltip={f.webSource?.url || f.path}
                              >
                                {f.status !== "available"
                                  ? statusNames[f.status]
                                  : f.webSource?.url || f.parent}
                              </span>
                            </span>
                            <span className="list-tags">
                              {f.tags.map((t) => (
                                <span
                                  style={tagStyle(t.id)}
                                  className={`tag-pill tag-color tone-${tagTone(t.id)} ${t.source === "folder" || t.createdBy === "folder" ? "is-folder" : ""}`}
                                  key={t.id}
                                >
                                  <span className="tag-pill-name">
                                    {t.name}
                                  </span>
                                  <TagSource tag={t} />
                                </span>
                              ))}
                            </span>
                            <span
                              className="list-date"
                              data-tooltip={
                                f.webSource ? ui("采集时间") : ui("修改时间")
                              }
                            >
                              {date(f.webSource?.capturedAt || f.modified)}
                            </span>
                            <span className="list-size">
                              {f.kind === "web" ? "—" : size(f.bytes)}
                            </span>
                          </button>
                        )}
                      </FileTagsHover>
                    </div>
                  );
                })}
              </div>
            )}
            {hasMore && (
              <div className="load-more">
                <button
                  className="button quiet"
                  onClick={onMore}
                  disabled={loading}
                >
                  {loading ? <LoaderCircle className="spin" size={16} /> : null}
                  {ui("加载更多 · 已显示")}
                  {files.length} / {total}
                </button>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}

export function TagPicker({
  tags,
  onPick,
  onCreate,
  placeholder = ui("添加标签"),
  busy = false,
  appliedIds = [],
}: {
  tags: Tag[];
  appliedIds?: string[];
  onPick: (tag: Tag) => void;
  onCreate: (name: string) => void;
  placeholder?: string;
  busy?: boolean;
}) {
  const [text, setText] = useState("");
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const close = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, []);
  const applied = new Set(appliedIds);
  const choices = tags.filter(
    (t) =>
      t.createdBy !== "folder" &&
      !applied.has(t.id) &&
      [t.name, ...(t.aliases || [])].some((name) =>
        name.toLocaleLowerCase().includes(text.trim().toLocaleLowerCase()),
      ),
  );
  const exact = tags.find((t) =>
    [t.name, ...(t.aliases || [])].some(
      (name) => name.toLocaleLowerCase() === text.trim().toLocaleLowerCase(),
    ),
  );
  function submit() {
    if (!text.trim()) return;
    if (exact && (applied.has(exact.id) || exact.createdBy === "folder"))
      return;
    if (exact) onPick(exact);
    else onCreate(text.trim());
    setText("");
    setOpen(false);
  }
  return (
    <div className="tag-picker" ref={root}>
      <div className="tag-input">
        <Plus size={14} />
        <input
          aria-label={placeholder}
          placeholder={placeholder}
          value={text}
          disabled={busy}
          onFocus={() => setOpen(true)}
          onChange={(e) => {
            setText(e.target.value);
            setOpen(true);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.nativeEvent.isComposing) {
              e.preventDefault();
              submit();
            }
            if (e.key === "Escape") {
              setOpen(false);
              e.stopPropagation();
            }
          }}
        />
      </div>
      {open && (
        <div className="tag-options">
          {choices.map((t) => (
            <button
              key={t.id}
              onClick={() => {
                onPick(t);
                setText("");
                setOpen(false);
              }}
            >
              <span>
                # {t.name}
                <TagSource tag={t} pool />
              </span>
              <small>{t.count}</small>
            </button>
          ))}
          {text.trim() && !exact && (
            <button className="create-tag" onClick={submit}>
              <Plus size={14} />
              {ui("创建“")}
              {text.trim()}”
            </button>
          )}
          {!text.trim() && !choices.length && (
            <div className="subtle">{ui("输入标签名称，按 Enter 创建")}</div>
          )}
        </div>
      )}
    </div>
  );
}

export function Inspector({
  files,
  tags,
  draft,
  onNote,
  onFlush,
  onReloadNote,
  onTag,
  onCreateTag,
  onOpen,
  onReveal,
  onCopy,
  onRelink,
  onOpenSource,
  onPreview,
  onClose,
  onAnalyze,
  onCancelAI,
  onConfirmAI,
  onConfigureAI,
  onContentSaved,
  busy,
}: {
  files: LocalFile[];
  tags: Tag[];
  draft?: Draft;
  onNote: (text: string) => void;
  onFlush: () => void;
  onReloadNote: () => void;
  onTag: (tag: Tag, add: boolean) => void;
  onCreateTag: (name: string) => void;
  onOpen: () => void;
  onReveal: () => void;
  onCopy: () => void;
  onRelink: () => void;
  onOpenSource: () => void;
  onPreview: () => void;
  onClose: () => void;
  onAnalyze: () => void;
  onCancelAI: () => void;
  onConfirmAI: (tag: Tag) => void;
  onConfigureAI: () => void;
  onContentSaved: (undoId?: string) => void;
  busy: boolean;
}) {
  const f = files[0];
  const [tab, setTab] = useState("content");
  const tagsContainer = useRef<HTMLDivElement>(null);
  const metadata = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (metadata.current) metadata.current.scrollTop = 0;
  }, [tab, files.map((file) => file.id).join(":")]);
  const allTags = [
    ...new Map(files.flatMap((f) => f.tags).map((t) => [t.id, t])).values(),
  ];
  useTagArrival(tagsContainer, files.map((f) => f.id).join(":"), allTags);
  const analyzing = files.some(
    (file) =>
      file.aiTask?.status === "queued" || file.aiTask?.status === "running",
  );
  const retry = files.some(
    (file) =>
      file.aiTask?.status === "failed" || file.aiTask?.status === "cancelled",
  );
  if (!f)
    return (
      <aside className="inspector inspector-empty">
        <Tags size={32} strokeWidth={1} />
        <p>{ui("选择文件查看内容")}</p>
      </aside>
    );
  return (
    <aside
      className={`inspector inspector-content-layout ${tab === "content" && files.length === 1 ? "is-content-only" : ""}`}
    >
      {files.length > 1 && (
        <div className="inspector-heading">
          <span>{files.length > 1 ? ui("批量标注") : ui("文件详情")}</span>
          <div className="inspector-header-actions">
            <small>
              {files.length} {ui("个文件")}
            </small>
            <button
              className="icon-button"
              aria-label={ui("关闭文件详情")}
              data-tooltip={ui("关闭文件详情")}
              onClick={onClose}
            >
              <X size={16} />
            </button>
          </div>
        </div>
      )}
      {files.length === 1 && (
        <div className="inspector-tabs-bar">
          <RubberSegment
            index={["content", "annotation", "info"].indexOf(tab)}
            className="detail-tabs"
            role="tablist"
            aria-label={ui("文件详情")}
            onKeyDown={(e) => {
              const tabs = ["content", "annotation", "info"];
              const current = tabs.indexOf(tab);
              const index =
                e.key === "ArrowRight"
                  ? (current + 1) % 3
                  : e.key === "ArrowLeft"
                    ? (current + 2) % 3
                    : e.key === "Home"
                      ? 0
                      : e.key === "End"
                        ? 2
                        : -1;
              if (index < 0) return;
              e.preventDefault();
              e.stopPropagation();
              setTab(tabs[index]);
              e.currentTarget
                .querySelectorAll<HTMLButtonElement>("button")
                [index]?.focus();
            }}
          >
            <button
              role="tab"
              id="inspector-tab-content"
              tabIndex={tab === "content" ? 0 : -1}
              aria-controls="inspector-panel-content"
              aria-selected={tab === "content"}
              className={tab === "content" ? "active" : ""}
              onClick={() => setTab("content")}
            >
              {ui("内容")}
            </button>
            <button
              role="tab"
              id="inspector-tab-annotation"
              tabIndex={tab === "annotation" ? 0 : -1}
              aria-controls="inspector-panel-annotation"
              aria-selected={tab === "annotation"}
              className={tab === "annotation" ? "active" : ""}
              onClick={() => setTab("annotation")}
            >
              {ui("标注")}
            </button>
            <button
              role="tab"
              id="inspector-tab-info"
              tabIndex={tab === "info" ? 0 : -1}
              aria-controls="inspector-panel-info"
              aria-selected={tab === "info"}
              className={tab === "info" ? "active" : ""}
              onClick={() => setTab("info")}
            >
              {ui("信息")}
            </button>
          </RubberSegment>
          <button
            className="icon-button"
            aria-label={ui("打开文件")}
            data-tooltip={ui("打开文件")}
            onClick={onOpen}
            disabled={f.status !== "available"}
          >
            <ExternalLink size={16} />
          </button>
          <button
            className="icon-button"
            aria-label={ui("放大预览")}
            data-tooltip={ui("放大预览")}
            onClick={onPreview}
          >
            <Maximize2 size={16} />
          </button>
          <button
            className="icon-button"
            aria-label={ui("关闭文件详情")}
            data-tooltip={ui("关闭文件详情")}
            onClick={onClose}
          >
            <X size={16} />
          </button>
        </div>
      )}
      <div className="inspector-tab-body">
        <div
          ref={metadata}
          hidden={files.length === 1 && tab === "content"}
          role={files.length === 1 ? "tabpanel" : undefined}
          id={tab === "content" ? undefined : `inspector-panel-${tab}`}
          aria-labelledby={`inspector-tab-${tab}`}
          className={`inspector-metadata ${files.length > 1 ? "is-batch" : ""}`}
        >
          {files.length === 1 && tab !== "content" ? (
            <>
              <div className="detail-filename-row">
                <h2 className="detail-filename">{f.name}</h2>
                <div className="detail-actions">
                  <button
                    className="icon-button bordered"
                    aria-label={ui("在文件夹中显示")}
                    data-tooltip={ui("在文件夹中显示")}
                    onClick={onReveal}
                  >
                    <FolderOpen size={16} />
                  </button>
                  <button
                    className="icon-button bordered"
                    aria-label={ui("复制文件路径")}
                    data-tooltip={ui("复制路径")}
                    onClick={onCopy}
                  >
                    <Copy size={15} />
                  </button>
                </div>
              </div>
              <div className="detail-meta">
                {size(f.bytes)}
                <span>·</span>
                {date(f.modified)}
                <span
                  className={`availability ${f.status === "available" ? "good" : "bad"}`}
                >
                  {statusNames[f.status]}
                </span>
              </div>
              {f.status !== "available" && (
                <div className="inline-warning">
                  <AlertCircle size={16} />
                  <span>
                    {message(f.error)}
                    <button onClick={onRelink}>{ui("重新关联文件")}</button>
                  </span>
                </div>
              )}
            </>
          ) : files.length > 1 ? (
            <div className="batch-summary">
              <div className="batch-glyph">
                <CheckCheck size={30} />
              </div>
              <h2>
                {ui("已选择")}
                {files.length} {ui("个文件")}
              </h2>
              <p>{ui("标签将应用到全部所选文件")}</p>
            </div>
          ) : null}

          {tab === "annotation" || files.length > 1 ? (
            <>
              <div className="inspector-ai">
                <div className="inspector-ai-actions">
                  <button
                    className="button quiet"
                    disabled={busy || analyzing}
                    onClick={onAnalyze}
                    data-tooltip={ui(
                      "更新待确认的 AI 标签，保留手工、文件夹及已确认标签",
                    )}
                  >
                    <Sparkles size={14} />
                    AI{" "}
                    {analyzing
                      ? ui("识别中")
                      : retry
                        ? ui("重试识别")
                        : files.length > 1
                          ? ui("批量识别")
                          : f.aiTask
                            ? ui("重新识别")
                            : ui("识别")}
                  </button>
                  <button className="quiet-link" onClick={onConfigureAI}>
                    {ui("设置")}
                  </button>
                </div>
                <AiTaskStatus files={files} busy={busy} onCancel={onCancelAI} />
              </div>
              <div className="field-heading">
                <label>{ui("标签")}</label>
                <span>{allTags.length}</span>
              </div>
              <div className="editable-tags" ref={tagsContainer}>
                {allTags.map((t) => {
                  const count = files.filter((f) =>
                    f.tags.some((x) => x.id === t.id),
                  ).length;
                  return (
                    <span
                      className="editable-tag"
                      key={t.id}
                      data-motion-tag={t.id}
                    >
                      <span>{t.name}</span>
                      {files.length === 1 && <TagSource tag={t} />}
                      {files.length > 1 &&
                        files.some((f) =>
                          f.tags.some(
                            (x) =>
                              x.id === t.id &&
                              x.source === "ai" &&
                              !x.ai?.confirmed,
                          ),
                        ) && (
                          <span
                            className="source-badge source-ai"
                            data-tooltip={ui("部分文件由 AI 标注")}
                          >
                            AI
                          </span>
                        )}
                      {files.length === 1 &&
                        t.source === "ai" &&
                        !t.ai?.confirmed && (
                          <button
                            aria-label={tf("确认AI标签{0}", t.name)}
                            data-tooltip={ui("确认后，重新识别也会保留此标签")}
                            disabled={busy}
                            onClick={() => onConfirmAI(t)}
                          >
                            <Check size={12} />
                          </button>
                        )}
                      {files.length > 1 && count !== files.length && (
                        <small>
                          {count}/{files.length}
                        </small>
                      )}
                      {count !== files.length && (
                        <button
                          aria-label={tf("将{0}添加到全部", t.name)}
                          onClick={() => onTag(t, true)}
                          disabled={busy}
                        >
                          <Plus size={12} />
                        </button>
                      )}
                      <button
                        aria-label={tf("移除标签{0}", t.name)}
                        onClick={() => onTag(t, false)}
                        disabled={busy}
                      >
                        <X size={12} />
                      </button>
                    </span>
                  );
                })}
              </div>
              <TagPicker
                tags={tags}
                appliedIds={tags
                  .filter((tag) =>
                    files.every((file) =>
                      file.tags.some((t) => t.id === tag.id),
                    ),
                  )
                  .map((tag) => tag.id)}
                onPick={(t) => onTag(t, true)}
                onCreate={onCreateTag}
                busy={busy}
              />
              {files.length === 1 && draft && (
                <>
                  <div className="field-heading">
                    <label htmlFor="file-note">{ui("备注")}</label>
                    <span className={draft.error ? "error-label" : ""}>
                      {draft.saving
                        ? ui("保存中…")
                        : draft.error
                          ? ui("保存失败")
                          : draft.text !== draft.saved
                            ? ui("待保存")
                            : ui("已保存")}
                    </span>
                  </div>
                  <textarea
                    id="file-note"
                    className="note-input"
                    placeholder={ui(
                      "为什么保存这份文件？记下用途、想法或下一步。",
                    )}
                    value={draft.text}
                    onChange={(e) => onNote(e.target.value)}
                    onBlur={onFlush}
                    maxLength={20000}
                  />
                  <span className="note-caption">
                    {ui("仅在本机保存 · 自动保存备注")}
                  </span>
                  {draft.error && (
                    <div className="note-error" role="alert">
                      {message(draft.error)}
                      <div>
                        <button onClick={onFlush}>{ui("重试保存")}</button>
                        <button onClick={onReloadNote}>
                          {ui("重新读取备注")}
                        </button>
                      </div>
                    </div>
                  )}
                </>
              )}
            </>
          ) : tab === "info" ? (
            <dl className="file-properties">
              {f.webSource && (
                <>
                  <dt>{ui("来源链接")}</dt>
                  <dd className="full-path">
                    {f.webSource.url}

                    <button className="button quiet" onClick={onOpenSource}>
                      <ExternalLink size={14} />
                      {ui("打开来源网页")}
                    </button>
                  </dd>
                  {f.webSource.image && (
                    <>
                      <dt>{ui("图片地址")}</dt>
                      <dd className="full-path">{f.webSource.image.url}</dd>
                      {f.webSource.image.finalUrl !== f.webSource.image.url && (
                        <>
                          <dt>{ui("实际下载地址")}</dt>
                          <dd className="full-path">
                            {f.webSource.image.finalUrl}
                          </dd>
                        </>
                      )}
                      <dt>{ui("图片尺寸")}</dt>
                      <dd>
                        {f.webSource.image.width} × {f.webSource.image.height}
                      </dd>
                      <dt>{ui("图片校验")}</dt>
                      <dd className="full-path">{f.webSource.image.sha256}</dd>
                    </>
                  )}
                  <dt>{ui("采集时间")}</dt>
                  <dd>
                    {new Date(f.webSource.capturedAt).toLocaleString(
                      currentLanguage(),
                    )}
                  </dd>
                </>
              )}
              <dt>
                {f.kind === "web" ? ui("本机资料描述文件") : ui("完整路径")}
              </dt>
              <dd className="full-path">{f.path}</dd>
              <dt>{ui("文件类型")}</dt>
              <dd>
                {f.kind === "web"
                  ? ui("网页资料")
                  : f.extension.toUpperCase() || ui("未知类型")}
              </dd>
              <dt>
                {f.kind === "web" ? ui("资料描述文件大小") : ui("文件大小")}
              </dt>
              <dd>{size(f.bytes)}</dd>
              {f.kind !== "web" && (
                <>
                  <dt>{ui("修改时间")}</dt>
                  <dd>
                    {new Date(f.modified).toLocaleString(currentLanguage())}
                  </dd>
                </>
              )}
              <dt>{ui("加入时间")}</dt>
              <dd>{new Date(f.added).toLocaleString(currentLanguage())}</dd>
              <dt>{ui("文件关联")}</dt>
              <dd>
                {f.kind === "web" ? (
                  ui("网页资料通过来源链接访问")
                ) : (
                  <>
                    <button
                      className="button quiet"
                      onClick={onRelink}
                      disabled={f.kind === "web"}
                    >
                      <Link2 size={14} />
                      {ui("重新关联")}
                    </button>
                  </>
                )}
              </dd>
            </dl>
          ) : null}
        </div>
        {files.length === 1 && (
          <div
            className="inspector-document"
            aria-hidden={tab !== "content"}
            inert={tab !== "content"}
            data-active={tab === "content"}
            role="tabpanel"
            id="inspector-panel-content"
            aria-labelledby="inspector-tab-content"
          >
            <ContentReader key={f.id} file={f} onSaved={onContentSaved} />
          </div>
        )}
      </div>
    </aside>
  );
}

export function QuickTagPanel({
  tags,
  count,
  onApply,
  onManage,
}: {
  tags: Tag[];
  count: number;
  onApply: (name: string) => Promise<void>;
  onManage: () => void;
}) {
  const [name, setName] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const lock = useRef(false);
  const exact = tags.find((t) => tagMatches(t, name));
  async function apply(value: string) {
    if (!value.trim() || lock.current || (!count && exact)) return;
    lock.current = true;
    setPending(true);
    setError("");
    try {
      await onApply(value.trim());
    } catch (e) {
      setError(message(e));
    } finally {
      lock.current = false;
      setPending(false);
    }
  }
  return (
    <div className="modal-content quick-tag-panel">
      <p className="muted">
        {count ? tf("应用到 {0} 个所选文件", count) : ui("新建到标签库")}
      </p>
      <form
        className="manager-create"
        onSubmit={(e) => {
          e.preventDefault();
          void apply(name);
        }}
      >
        <input
          autoFocus
          aria-label={ui("标签名称")}
          placeholder={count ? ui("搜索或新建标签") : ui("新标签名称")}
          value={name}
          disabled={pending}
          onKeyDown={(e) => {
            if (e.key === "Enter" && e.nativeEvent.isComposing)
              e.preventDefault();
          }}
          onChange={(e) => setName(e.target.value)}
        />
        <button
          className="button primary"
          disabled={pending || !name.trim() || (!count && !!exact)}
        >
          {pending ? (
            <LoaderCircle className="spin" size={15} />
          ) : (
            <Plus size={15} />
          )}{" "}
          {count ? ui("添加") : ui("创建")}
        </button>
      </form>
      {error && (
        <p className="tag-form-error" role="alert">
          {error}
        </p>
      )}
      {count > 0 ? (
        <div className="quick-tag-choices">
          {tags
            .filter((t) => tagSearch(t, name))
            .map((t) => (
              <button
                key={t.id}
                disabled={pending}
                onClick={() => void apply(t.name)}
              >
                <Tags size={14} />
                <span>{t.name}</span>
              </button>
            ))}
          {!tags.length && (
            <p className="muted">{ui("输入名称即可新建并标注")}</p>
          )}
        </div>
      ) : (
        exact && (
          <p className="muted">
            {ui("已对应标签「")}
            {exact.name}」
          </p>
        )
      )}
      <button
        className="button quiet quick-tag-manage"
        disabled={pending}
        onClick={onManage}
      >
        <Edit3 size={14} />
        {ui("管理标签")}
      </button>
    </div>
  );
}

export function TagManager({
  tags,
  initialTag,
  onCreate,
  onRename,
  onDelete,
  onExport,
  onMerge,
  onAliases,
  busy,
}: {
  tags: Tag[];
  initialTag?: Tag;
  onCreate: (s: string) => Promise<void>;
  onRename: (t: Tag, s: string) => Promise<void>;
  onDelete: (t: Tag) => void;
  onExport: (t: Tag) => void;
  onMerge: (t: Tag) => void;
  onAliases: (t: Tag) => void;
  busy: boolean;
}) {
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState(initialTag?.id || "");
  const [value, setValue] = useState(initialTag?.name || "");
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  const lock = useRef(false);
  const disabled = busy || pending;
  async function submit(task: () => Promise<void>, done: () => void) {
    if (lock.current || busy) return;
    lock.current = true;
    setPending(true);
    setError("");
    try {
      await task();
      done();
    } catch (e) {
      setError(message(e));
    } finally {
      lock.current = false;
      setPending(false);
    }
  }
  return (
    <div className="modal-content">
      <p className="muted">{ui("重命名会同步到所有关联文件")}</p>
      <form
        className="manager-create"
        onSubmit={(e) => {
          e.preventDefault();
          if (query.trim() && !tags.some((t) => tagMatches(t, query)))
            void submit(
              () => onCreate(query.trim()),
              () => setQuery(""),
            );
        }}
      >
        <input
          aria-label={ui("新标签名称")}
          placeholder={ui("搜索或新建标签")}
          value={query}
          disabled={disabled}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && e.nativeEvent.isComposing)
              e.preventDefault();
          }}
        />
        <button
          className="button primary"
          disabled={
            disabled || !query.trim() || tags.some((t) => tagMatches(t, query))
          }
        >
          <Plus size={15} />
          {ui("创建")}
        </button>
      </form>
      {error && (
        <p className="tag-form-error" role="alert">
          {error}
        </p>
      )}
      <div className="tag-manager-list">
        {tags
          .filter((t) => t.id === editing || tagSearch(t, query))
          .map((t) => (
            <div className="tag-manager-row" key={t.id}>
              <Tags size={16} />
              {editing === t.id ? (
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    if (value.trim())
                      void submit(
                        () => onRename(t, value.trim()),
                        () => setEditing(""),
                      );
                  }}
                >
                  <input
                    autoFocus
                    aria-label={ui("标签新名称")}
                    value={value}
                    disabled={disabled}
                    onChange={(e) => setValue(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && e.nativeEvent.isComposing)
                        e.preventDefault();
                      if (e.key === "Escape") {
                        e.stopPropagation();
                        if (!disabled) {
                          setEditing("");
                          setError("");
                        }
                      }
                    }}
                  />
                  <button
                    className="icon-button"
                    aria-label={ui("确认重命名")}
                    data-tooltip={ui("保存")}
                    disabled={disabled || !value.trim()}
                  >
                    <Check size={16} />
                  </button>
                  <button
                    type="button"
                    className="icon-button"
                    aria-label={ui("取消重命名")}
                    data-tooltip={ui("取消")}
                    disabled={disabled}
                    onClick={() => {
                      setEditing("");
                      setError("");
                    }}
                  >
                    <X size={16} />
                  </button>
                </form>
              ) : (
                <>
                  <button
                    className="manager-tag-name"
                    data-tooltip={ui("重命名标签")}
                    aria-label={tf("重命名{0}", t.name)}
                    disabled={disabled}
                    onClick={() => {
                      setEditing(t.id);
                      setValue(t.name);
                      setError("");
                    }}
                  >
                    <span data-tooltip={(t.aliases ?? []).join("、")}>
                      {t.name}
                      {!!t.aliases?.length && (
                        <em className="alias-count">
                          {t.aliases.length} {ui("别名")}
                        </em>
                      )}
                    </span>
                    <TagSource tag={t} pool />
                    <Edit3 size={13} />
                  </button>
                  <small>
                    {t.count} {ui("个文件")}
                  </small>
                  <button
                    className="icon-button"
                    aria-label={tf("管理{0}的别名", t.name)}
                    data-tooltip={
                      t.createdBy === "folder"
                        ? ui("文件夹标签不设置别名")
                        : tf(
                            "规范名称与别名{0}",
                            t.aliases?.length
                              ? tf(" · {0} 个", t.aliases.length)
                              : "",
                          )
                    }
                    disabled={disabled || t.createdBy === "folder"}
                    onClick={() => onAliases(t)}
                  >
                    <Link2 size={14} />
                  </button>
                  <button
                    className="icon-button"
                    aria-label={tf("合并{0}", t.name)}
                    data-tooltip={
                      t.createdBy === "folder"
                        ? ui("文件夹标签不能合并")
                        : ui("合并到其他标签")
                    }
                    disabled={
                      disabled ||
                      t.createdBy === "folder" ||
                      !tags.some(
                        (other) =>
                          other.id !== t.id && other.createdBy !== "folder",
                      )
                    }
                    onClick={() => onMerge(t)}
                  >
                    <Tags size={14} />
                  </button>
                  <button
                    className="icon-button"
                    data-tooltip={ui("导出此标签的文件")}
                    aria-label={tf("导出标签{0}的文件", t.name)}
                    disabled={disabled || t.count === 0}
                    onClick={() => onExport(t)}
                  >
                    <Download size={14} />
                  </button>
                  <button
                    className="icon-button"
                    aria-label={tf("删除{0}", t.name)}
                    onClick={() => onDelete(t)}
                    disabled={disabled}
                  >
                    <Trash2 size={14} />
                  </button>
                </>
              )}
            </div>
          ))}
      </div>
      {tags.length === 0 && (
        <div className="simple-empty">{ui("暂无标签")}</div>
      )}
    </div>
  );
}

export function SettingsPanel({
  onHelp,
  boot,
  theme,
  setTheme,
  density,
  setDensity,
  showColumnControls,
  setShowColumnControls,
  onAppearanceChange,
  onRepair,
  onWatch,
  onBackup,
  onRestore,
  onPackageExport,
  onPackageImport,
  onClear,
  onData,
  onDiagnostics,
  busy,
}: {
  boot: Bootstrap;
  onHelp: () => void;
  theme: string;
  setTheme: (s: string) => void;
  density: string;
  setDensity: (s: string) => void;
  onAppearanceChange: (
    key: "language" | "accentColor" | "uiFontSize" | "tagDropTilt",
    value: string | number | boolean,
  ) => void;
  showColumnControls: boolean;
  setShowColumnControls: (value: boolean) => void;
  onRepair: () => void;
  onWatch: () => void;
  onBackup: () => void;
  onRestore: () => void;
  onPackageExport: () => void;
  onPackageImport: () => void;
  onClear: () => void;
  onData: () => void;
  onDiagnostics: () => void;
  busy: boolean;
}) {
  return (
    <div className="modal-content settings-content">
      <SettingsSection title={ui("使用说明")}>
        <div className="setting-row">
          <div>
            <strong>{ui("使用说明")}</strong>
            <p>{ui("查看文件导入、标签和资料交接的基本操作")}</p>
          </div>
          <button className="button" onClick={onHelp}>
            {ui("查看使用说明")}
          </button>
        </div>
      </SettingsSection>
      <SettingsSection title={ui("外观")}>
        <div className="setting-row">
          <div>
            <strong>{ui("外观")}</strong>
            <p>{ui("跟随系统，或选择固定主题")}</p>
          </div>
          <div className="segmented">
            {[
              ["system", Monitor, ui("系统")],
              ["light", Sun, ui("浅色")],
              ["dark", Moon, ui("深色")],
            ].map(([value, Icon, label]) => {
              const I = Icon as typeof Sun;
              return (
                <button
                  key={String(value)}
                  aria-pressed={theme === value}
                  onClick={() => setTheme(String(value))}
                >
                  <I size={14} />
                  {ui(String(label))}
                </button>
              );
            })}
          </div>
        </div>
        <div className="setting-row">
          <div>
            <strong>{ui("主题颜色")}</strong>
            <p>{ui("主题色同步到工作台与标签浮窗")}</p>
          </div>
          <div
            className="accent-choices"
            role="group"
            aria-label={ui("主题颜色")}
          >
            {Object.entries(accentColors).map(([id, color]) => (
              <WarmTooltip
                key={id}
                text={ui(color.name)}
                detail={color.light.toUpperCase()}
                swatch={color.light}
              >
                <button
                  data-accent-choice={id}
                  className="accent-choice"
                  aria-label={ui(color.name)}
                  aria-pressed={(boot.settings.accentColor || DEFAULT_ACCENT_COLOR) === id}
                  style={{ "--swatch": color.light } as React.CSSProperties}
                  onClick={() => onAppearanceChange("accentColor", id)}
                >
                  <span />
                </button>
              </WarmTooltip>
            ))}
          </div>
        </div>
        <div className="setting-row">
          <div>
            <strong>{ui("界面语言")}</strong>
            <p>{ui("不改变文件、标签名称或备注")}</p>
          </div>
          <GlideSelect
            ariaLabel={ui("界面语言")}
            value={boot.settings.language || "zh-CN"}
            onChange={(value) => onAppearanceChange("language", value)}
            options={[
              { value: "zh-CN", label: "简体中文" },
              { value: "en", label: "English" },
            ]}
          />
        </div>
        <div className="setting-row">
          <div>
            <strong>{ui("字体大小")}</strong>
            <p>{ui("调整工作台和标签浮窗的界面文字大小")}</p>
          </div>
          <GlideSelect
            ariaLabel={ui("字体大小")}
            value={String(boot.settings.uiFontSize || 13)}
            onChange={(value) =>
              onAppearanceChange("uiFontSize", Number(value))
            }
            options={[
              { value: "12", label: ui("较小") },
              { value: "13", label: ui("默认") },
              { value: "14", label: ui("较大") },
              { value: "15", label: ui("大") },
              { value: "16", label: ui("最大") },
            ]}
            disabled={busy}
          />
        </div>
        <div className="setting-row">
          <div>
            <strong>{ui("显示列数调节")}</strong>
            <p>{ui("显示瀑布流滑块和列数；隐藏后仍可用 Ctrl＋滚轮调节")}</p>
          </div>
          <SquishSwitch
            ariaLabel={ui("显示列数调节")}
            checked={showColumnControls}
            onChange={setShowColumnControls}
            disabled={busy}
          />
        </div>
        <div className="setting-row">
          <div>
            <strong>{ui("标签拖放立体动效")}</strong>
            <p>{ui("拖动标签经过文件时，卡片跟随鼠标轻微倾斜")}</p>
          </div>
          <SquishSwitch
            ariaLabel={ui("标签拖放立体动效")}
            checked={boot.settings.tagDropTilt !== false}
            disabled={busy}
            onChange={(value) => onAppearanceChange("tagDropTilt", value)}
          />
        </div>
        <div className="setting-row">
          <div>
            <strong>{ui("文件卡片密度")}</strong>
            <p>{ui("调整浏览时的留白")}</p>
          </div>
          <GlideSelect
            ariaLabel={ui("文件卡片密度")}
            value={density}
            onChange={setDensity}
            options={[
              { value: "comfortable", label: ui("舒展") },
              { value: "compact", label: ui("紧凑") },
            ]}
          />
        </div>
      </SettingsSection>
      <AutoTagSettings
        folderEnabled={boot.settings.folderAutoTagging !== false}
      />
      <TextIndexSettings />
      <SemanticSettings />
      <SettingsSection title={ui("资料交接")}>
        <p className="muted">
          {ui("携带原文件、标签、备注与收藏，接收后直接使用。")}
        </p>
        <div className="settings-buttons">
          <button className="button" onClick={onPackageExport} disabled={busy}>
            <Download size={16} />
            {ui("导出资料包")}
          </button>
          <button className="button" onClick={onPackageImport} disabled={busy}>
            <Upload size={16} />
            {ui("导入资料包")}
          </button>
        </div>
      </SettingsSection>
      <SettingsSection title={ui("文件位置")}>
        <div className="setting-row">
          <div>
            <strong>{ui("文件夹监听")}</strong>
            <p>{ui("按规则自动加入新文件，支持暂停与补扫")}</p>
          </div>
          <button className="button" onClick={onWatch} disabled={busy}>
            <FolderOpen size={16} />
            {ui("管理监听目录")}
          </button>
        </div>
        <div className="setting-row">
          <div>
            <strong>{ui("路径修复")}</strong>
            <p>{ui("文件夹移动、改名或换盘后重新定位文件")}</p>
          </div>
          <button className="button" onClick={onRepair} disabled={busy}>
            <FolderOpen size={16} />
            {ui("修复文件路径")}
          </button>
        </div>
      </SettingsSection>
      <BrowserCaptureSettings refreshKey={boot} library={boot.dataPath} />
      <SettingsSection title={ui("备份与恢复")}>
        <p className="muted">
          {ui("备份包含标签、备注和文件位置。原文件需另行备份。")}
        </p>
        <div className="settings-buttons">
          <button className="button" onClick={onBackup} disabled={busy}>
            <Download size={16} />
            {ui("导出标注备份")}
          </button>
          <button className="button" onClick={onRestore} disabled={busy}>
            <Upload size={16} />
            {ui("从备份恢复")}
          </button>
        </div>
        <p className="subtle">{ui("恢复前会自动保存当前库的备份。")}</p>
        <BackupStatus refreshKey={boot} />
      </SettingsSection>
      <SettingsSection title={ui("本地存储")}>
        <div className="setting-row">
          <div>
            <strong>{ui("预览缓存")}</strong>
            <p>{ui("图片 384 MiB · 文档摘要 128 MiB，自动清理较旧缓存")}</p>
            <p>{ui("清理后按需生成，标签和原文件不受影响")}</p>
          </div>
          <button className="button quiet" onClick={onClear} disabled={busy}>
            <RefreshCw size={14} />
            {ui("清理")}
          </button>
        </div>
        <div className="data-location">
          <HardDrive size={16} />
          <span>{boot.dataPath}</span>
          <button
            className="icon-button"
            onClick={onData}
            aria-label={ui("打开数据目录")}
          >
            <FolderOpen size={16} />
          </button>
        </div>
      </SettingsSection>
      <div className="about-line">
        <span>
          {ui("拾签")}
          {boot.version}
        </span>
        <span>{ui("本地优先 · 无需账号")}</span>
      </div>
      <button
        className="button quiet settings-diagnostics-action"
        onClick={onDiagnostics}
      >
        {ui("预览诊断报告")}
      </button>
    </div>
  );
}

export function ImportDetails({
  job,
  onRetry,
  busy = false,
}: {
  job: ImportJob;
  onRetry?: () => void;
  busy?: boolean;
}) {
  return (
    <div className="modal-content">
      <div className="import-counts">
        <span>
          <strong>{job.added}</strong>
          {ui("新增")}
        </span>
        <span>
          <strong>{job.existing}</strong>
          {ui("已存在")}
        </span>
        <span>
          <strong>{job.skipped}</strong>
          {ui("跳过")}
        </span>
        <span>
          <strong>{job.failed}</strong>
          {ui("失败")}
        </span>
      </div>
      <p className="muted">
        {job.cancelled
          ? ui("任务已取消，已加入的文件保留。")
          : job.done
            ? ui("加入完成。原文件保持在原位置。")
            : ui("正在读取文件信息，缩略图会按需生成。")}
      </p>
      <p className="subtle">
        {ui("隐藏文件、系统文件、链接目录及应用自身数据会跳过。")}
      </p>
      {job.errors.length > 0 && (
        <ul className="import-errors">
          {job.errors.map((e, i) => (
            <li key={i}>{e}</li>
          ))}
        </ul>
      )}
      {job.done && job.failed > 0 && onRetry && !!job.failedPaths?.length && (
        <>
          <button className="button" disabled={busy} onClick={onRetry}>
            <RefreshCw size={14} />
            {ui("重试失败项")}
          </button>
          {job.failed > job.failedPaths.length && (
            <p className="subtle">
              {ui("本次最多重试已记录的")}
              {job.failedPaths.length}{" "}
              {ui("个路径，其余请重新选择来源文件夹。")}
            </p>
          )}
        </>
      )}
    </div>
  );
}
