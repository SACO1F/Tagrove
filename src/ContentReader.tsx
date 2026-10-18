import { WebBrowser } from "./WebBrowser";
import { OfficeLayout } from "./OfficeLayout";
import { WebContent } from "./WebCapture";
import { webDrafts, webSaving, webUnsavedNames } from "./WebDrafts";
import { t as ui, tf } from "./i18n";
import { useEffect, useMemo, useRef, useState, useLayoutEffect } from "react";
import {
  AlertCircle,
  ChevronLeft,
  ChevronRight,
  Eye,
  Code,
  Save,
  LoaderCircle,
} from "lucide-react";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { api, type LocalFile, message } from "./api";
import { PdfReader } from "./PdfReader";
import { OfficeReader, isOfficePreview } from "./OfficeReader";
import { HtmlReader, isHtml } from "./HtmlReader";
import "./reader-controls.css";
import {
  FlowZoomReader,
  ReaderZoomControl,
  useZoomWheel,
  clampZoom,
  type ReaderZoomValue,
  type ZoomPoint,
} from "./ReaderZoom";
import { ZoomFrame } from "./ZoomFrame";

import {
  ReadingState,
  useReadingValue,
  useReadingScroll,
  useReadingPosition,
  keepReadingStateAfterSave,
} from "./ReadingState";

type TextDocument = { text: string; revision: string; digest: string };
type DocumentDraft = TextDocument & { saved: string; name: string };
const drafts = new Map<string, DocumentDraft>();
let savingDocuments = 0;
const savingFiles = new Set<string>();
export function documentsSaving() {
  return savingDocuments > 0 || webSaving.size > 0;
}
export function unsavedDocumentNames() {
  return [
    ...webUnsavedNames(),
    ...[...drafts.values()]
      .filter((d) => d.text !== d.saved)
      .map((d) => d.name),
  ];
}
export function discardDocumentDrafts() {
  drafts.clear();
  webDrafts.clear();
}
function ReaderMessage({
  error,
  loading,
}: {
  error?: string;
  loading?: boolean;
}) {
  return (
    <div className="reader-message" role={error ? "alert" : "status"}>
      {loading ? (
        <LoaderCircle className="spin" size={22} />
      ) : (
        <AlertCircle size={22} />
      )}
      <p>{error || ui("正在读取文件…")}</p>
    </div>
  );
}
function TextReader({
  file,
  onSaved,
  editable = true,
}: {
  file: LocalFile;
  onSaved: (undoId?: string) => void;
  editable?: boolean;
}) {
  const markdown = /^(md|markdown)$/i.test(file.extension);
  const [document, setDocument] = useState<DocumentDraft>();
  const [editing, setEditing] = useReadingValue(
    "textEditing",
    !!drafts.get(file.id) &&
      drafts.get(file.id)!.text !== drafts.get(file.id)!.saved,
  );
  const textScroll = useRef<HTMLTextAreaElement>(null);
  const plainScroll = useRef<HTMLPreElement>(null);
  const caret = useReadingPosition();
  useLayoutEffect(() => {
    const el = textScroll.current;
    if (!el || !editing || !editable || !document) return;
    const start = Math.min(
      document.text.length,
      Math.max(0, caret.get("textCaretStart")),
    );
    const end = Math.min(
      document.text.length,
      Math.max(start, caret.get("textCaretEnd", start)),
    );
    el.setSelectionRange(start, end);
  }, [editing, editable, !!document]);

  useReadingScroll(textScroll, "textEdit", editing && editable && !!document);
  useReadingScroll(
    plainScroll,
    "textRead",
    (!editing || !editable) && !!document,
  );
  const [error, setError] = useState(""),
    [saving, setSaving] = useState(() => savingFiles.has(file.id)),
    [attempt, setAttempt] = useState(0),
    [confirmRead, setConfirmRead] = useState(false);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  useEffect(() => {
    const update = (event: Event) => {
      if ((event as CustomEvent<string>).detail !== file.id) return;
      const draft = drafts.get(file.id);
      if (draft) setDocument(draft);
      setSaving(savingFiles.has(file.id));
    };
    window.addEventListener("shiqian-document-updated", update);
    return () => window.removeEventListener("shiqian-document-updated", update);
  }, [file.id]);
  useEffect(() => {
    let mounted = true;
    const draft = drafts.get(file.id);
    if (draft && draft.text !== draft.saved) {
      setDocument(draft);
      if (draft.revision !== file.revision)
        setError(ui("原文件已变化；草稿已保留，请核对后保存或重新读取"));
      return;
    }
    setDocument(undefined);
    setError("");
    void api<TextDocument>("text.document", { id: file.id })
      .then((d) => {
        if (!mounted) return;
        const value = { ...d, saved: d.text, name: file.name };
        setDocument(value);
        for (const [id, d] of drafts) if (d.text === d.saved) drafts.delete(id);
        drafts.set(file.id, value);
      })
      .catch((e) => {
        if (mounted) setError(message(e));
      });
    return () => {
      mounted = false;
    };
  }, [file.id, file.revision, file.identity, attempt]);
  const rendered = useMemo(() => {
    if (!markdown || !document) return "";
    const body = DOMPurify.sanitize(
      marked.parse(document.text, { async: false, gfm: true }),
      {
        USE_PROFILES: { html: true },
        FORBID_TAGS: [
          "style",
          "iframe",
          "form",
          "input",
          "meta",
          "base",
          "object",
          "embed",
        ],
        FORBID_ATTR: ["style", "srcset"],
        // CSP also blocks remote assets; strip their URLs before frame creation
        // so the browser need not attempt a blocked request for Markdown images.
        ALLOWED_URI_REGEXP:
          /^(?:data:image\/(?:png|jpe?g|gif|webp|svg\+xml);|#)/i,
      },
    );
    return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'"><style>body{margin:24px;color:#2c3037;background:white;font:15px/1.75 system-ui,sans-serif;overflow-wrap:anywhere}h1,h2,h3{line-height:1.3}h1{font-size:26px}h2{font-size:21px}pre{overflow:auto;background:#f3f4f5;padding:12px;border-radius:8px}code{font-family:Consolas,monospace}table{border-collapse:collapse;max-width:100%;display:block;overflow:auto}td,th{padding:6px 10px;border:1px solid #dce0e5}img{max-width:100%}blockquote{margin-left:0;padding-left:16px;border-left:2px solid var(--reader-accent,#2c6654)}a{color:var(--reader-accent,#2c6654);pointer-events:none}</style></head><body>${body}</body></html>`;
  }, [document?.text, markdown]);
  const dirty = !!document && document.text !== document.saved;
  async function save() {
    if (!document || savingFiles.has(file.id)) return;
    setSaving(true);
    savingDocuments++;
    savingFiles.add(file.id);
    setError("");
    try {
      const result = await api<TextDocument>("text.save", {
        id: file.id,
        text: document.text,
        revision: document.revision,
        digest: document.digest,
      });
      const latest = drafts.get(file.id);
      const value = {
        ...result,
        text:
          latest && latest.text !== document.text ? latest.text : result.text,
        saved: result.text,
        name: file.name,
      };
      drafts.set(file.id, value);
      keepReadingStateAfterSave(file, result.revision, {
        textEditing: editing,
      });
      if (alive.current) {
        setDocument(value);
        onSaved();
      }
    } catch (e) {
      if (alive.current) setError(message(e));
    } finally {
      savingDocuments--;
      savingFiles.delete(file.id);
      window.dispatchEvent(
        new CustomEvent("shiqian-document-updated", { detail: file.id }),
      );
      if (alive.current) setSaving(false);
    }
  }
  return (
    <section
      className="document-text-reader"
      aria-label={markdown ? ui("Markdown 内容") : ui("文本内容")}
    >
      <div className="reader-toolbar">
        {markdown && editable ? (
          <div
            className="reader-control-group reader-mode-control"
            role="group"
            aria-label={ui("Markdown 阅读模式")}
          >
            <button
              className="button quiet"
              aria-pressed={!editing}
              onClick={() => setEditing(false)}
            >
              <Eye size={14} />
              {ui("阅读")}
            </button>
            <button
              className="button quiet"
              aria-pressed={editing}
              onClick={() => setEditing(true)}
            >
              <Code size={14} />
              {ui("编辑")}
            </button>
          </div>
        ) : (
          <span>{markdown ? ui("Markdown · 阅读") : ui("文本 · UTF-8")}</span>
        )}
        {markdown && editable && (
          <button
            className="button quiet"
            disabled={!dirty || saving}
            onClick={() => void save()}
          >
            <Save size={14} />
            {saving ? ui("保存中") : ui("保存")}
          </button>
        )}
        <span className="reader-save-state" role="status">
          {dirty ? ui("未保存 · 修改原文件") : ui("已保存")}
        </span>
      </div>
      {error && (
        <div className="reader-inline-error" role="alert">
          {error}
          {document && (
            <button
              className="button quiet"
              disabled={saving}
              onClick={() => {
                if (dirty) setConfirmRead(true);
                else {
                  drafts.delete(file.id);
                  setAttempt((n) => n + 1);
                }
              }}
            >
              {ui("重新读取")}
            </button>
          )}
        </div>
      )}
      {confirmRead && (
        <div className="reader-inline-error">
          {ui("重新读取会放弃当前草稿。")}
          <button
            className="button quiet"
            onClick={() => {
              drafts.delete(file.id);
              setConfirmRead(false);
              setAttempt((n) => n + 1);
            }}
          >
            {ui("放弃草稿并重新读取")}
          </button>
          <button
            className="button quiet"
            onClick={() => setConfirmRead(false)}
          >
            {ui("保留草稿")}
          </button>
        </div>
      )}
      {!document ? (
        error ? (
          <ReaderMessage error={error} />
        ) : (
          <ReaderMessage loading />
        )
      ) : editing && markdown && editable ? (
        <textarea
          ref={textScroll}
          aria-label={ui("Markdown 源码")}
          className="markdown-editor"
          value={document.text}
          spellCheck={false}
          autoFocus
          onSelect={(e) => {
            caret.set("textCaretStart", e.currentTarget.selectionStart);
            caret.set("textCaretEnd", e.currentTarget.selectionEnd);
          }}
          onBlur={(e) => {
            caret.set("textCaretStart", e.currentTarget.selectionStart);
            caret.set("textCaretEnd", e.currentTarget.selectionEnd);
          }}
          disabled={saving}
          onChange={(e) => {
            const value = { ...document, text: e.target.value };
            drafts.set(file.id, value);
            setDocument(value);
            window.dispatchEvent(
              new CustomEvent("shiqian-document-updated", { detail: file.id }),
            );
          }}
          onKeyDown={(e) => {
            if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
              e.preventDefault();
              void save();
            }
          }}
        />
      ) : markdown ? (
        <ZoomFrame
          title={ui("Markdown 阅读内容")}
          document={rendered}
          stateKey="markdown"
        />
      ) : (
        <pre ref={plainScroll} className="document-plain-text">
          {document.text || ui("（空文件）")}
        </pre>
      )}
    </section>
  );
}
function ImageReader({ file }: { file: LocalFile }) {
  const [data, setData] = useState<string>(),
    [error, setError] = useState(""),
    [zoom, setZoom] = useReadingValue<ReaderZoomValue>("imageZoom", "page"),
    [panX, setPanX] = useReadingValue<number>("imagePanX", 0),
    [panY, setPanY] = useReadingValue<number>("imagePanY", 0),
    [natural, setNatural] = useState({ w: 1, h: 1 }),
    [size, setSize] = useState({ w: 300, h: 300 });
  const pan = { x: panX, y: panY };
  const setPan = (
    value:
      | { x: number; y: number }
      | ((old: { x: number; y: number }) => { x: number; y: number }),
  ) => {
    const next = typeof value === "function" ? value(pan) : value;
    setPanX(next.x);
    setPanY(next.y);
  };
  const viewport = useRef<HTMLDivElement>(null),
    drag = useRef<
      | {
          x: number;
          y: number;
          left: number;
          top: number;
          panX: number;
          panY: number;
        }
      | undefined
    >(undefined);
  useEffect(() => {
    let alive = true;
    setData(undefined);
    setError("");
    void api<{ data: string }>("preview", { id: file.id, large: true })
      .then((r) => {
        if (alive) setData(r.data);
      })
      .catch((e) => {
        if (alive) setError(message(e));
      });
    return () => {
      alive = false;
    };
  }, [file.id, file.revision, file.identity, file.status]);
  useEffect(() => {
    const el = viewport.current;
    if (!el) return;
    const update = () =>
      setSize({
        w: Math.max(1, el.clientWidth - 24),
        h: Math.max(1, el.clientHeight - 24),
      });
    const observer = new ResizeObserver(update);
    observer.observe(el);
    update();
    return () => observer.disconnect();
  }, []);
  useReadingScroll(viewport, "image", !!data && natural.w > 1);
  const fit = Math.min(size.w / natural.w, size.h / natural.h);
  const scale =
    typeof zoom === "number"
      ? zoom / 100
      : zoom === "width"
        ? size.w / natural.w
        : zoom === "page"
          ? fit
          : Math.min(fit, 1);
  const zoomAnchor = useRef<
    { x: number; y: number; ix: number; iy: number } | undefined
  >(undefined);
  const anchorTimer = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  useEffect(() => () => clearTimeout(anchorTimer.current), []);
  const step = (direction: number, point?: ZoomPoint) => {
    const img = viewport.current?.querySelector("img");
    if (point && img) {
      const r = img.getBoundingClientRect();
      clearTimeout(anchorTimer.current);
      anchorTimer.current = setTimeout(() => {
        zoomAnchor.current = undefined;
      }, 180);
      zoomAnchor.current = {
        x: point.x,
        y: point.y,
        ix: (point.x - r.left) / scale,
        iy: (point.y - r.top) / scale,
      };
    }
    setZoom(clampZoom(scale * 100 + direction * 25));
  };
  useLayoutEffect(() => {
    const a = zoomAnchor.current,
      el = viewport.current,
      img = el?.querySelector("img");
    if (!a || !el || !img) return;
    const r = img.getBoundingClientRect();
    const dx = r.left + a.ix * scale - a.x,
      dy = r.top + a.iy * scale - a.y;
    const left = el.scrollLeft,
      top = el.scrollTop;
    el.scrollLeft += dx;
    el.scrollTop += dy;
    const extraX = dx - (el.scrollLeft - left),
      extraY = dy - (el.scrollTop - top);
    if (Math.abs(extraX) > 0.5 || Math.abs(extraY) > 0.5)
      setPan((p) => ({ x: p.x - extraX, y: p.y - extraY }));
  }, [scale, size.w, size.h, pan.x, pan.y]);
  useZoomWheel(viewport, step, !!data);
  return (
    <section className="image-content-reader" aria-label={ui("图片内容")}>
      <div className="reader-toolbar">
        <ReaderZoomControl
          value={zoom}
          scale={scale}
          onChange={(value) => {
            zoomAnchor.current = undefined;
            setPan({ x: 0, y: 0 });
            setZoom(value);
          }}
          label={ui("图片")}
          disabled={!data}
        />
      </div>
      <div
        className="image-content-viewport"
        ref={viewport}
        tabIndex={0}
        aria-label={ui("图片缩放画布")}
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          const el = e.currentTarget;
          zoomAnchor.current = undefined;
          drag.current = {
            x: e.clientX,
            y: e.clientY,
            left: el.scrollLeft,
            top: el.scrollTop,
            panX: pan.x,
            panY: pan.y,
          };
          el.setPointerCapture(e.pointerId);
        }}
        onPointerMove={(e) => {
          if (drag.current) {
            const d = drag.current,
              el = e.currentTarget,
              left = d.left - e.clientX + d.x,
              top = d.top - e.clientY + d.y;
            el.scrollLeft = left;
            el.scrollTop = top;
            setPan({
              x: d.panX + el.scrollLeft - left,
              y: d.panY + el.scrollTop - top,
            });
          }
        }}
        onPointerUp={() => {
          drag.current = undefined;
        }}
        onPointerCancel={() => {
          drag.current = undefined;
        }}
      >
        {error ? (
          <ReaderMessage error={error} />
        ) : !data ? (
          <ReaderMessage loading />
        ) : (
          <div
            className="image-content-stage"
            style={{
              width: Math.max(size.w + 24, natural.w * scale + 24),
              height: Math.max(size.h + 24, natural.h * scale + 24),
            }}
          >
            <img
              src={`data:image/png;base64,${data}`}
              alt={file.name}
              draggable={false}
              style={{
                width: natural.w * scale,
                height: natural.h * scale,
                transform: `translate(${pan.x}px,${pan.y}px)`,
              }}
              onDoubleClick={() => {
                zoomAnchor.current = undefined;
                setPan({ x: 0, y: 0 });
                setZoom((z) => (typeof z === "number" ? "auto" : 100));
              }}
              onLoad={(e) =>
                setNatural({
                  w: e.currentTarget.naturalWidth,
                  h: e.currentTarget.naturalHeight,
                })
              }
            />
          </div>
        )}
      </div>
    </section>
  );
}
type Slides = {
  slides: {
    number: number;
    paragraphs: string[];
    images: string[];
    omittedImages: number;
  }[];
  images: Record<string, string>;
  total: number;
  truncated: boolean;
};
function PresentationReader({
  file,
  engine,
}: {
  file: LocalFile;
  engine?: string;
}) {
  const [data, setData] = useState<Slides>(),
    [error, setError] = useState(""),
    [page, setPage] = useReadingValue<number>("slide", 0);
  const slideScroll = useRef<HTMLDivElement>(null);
  useReadingScroll(slideScroll, "slide" + page, !!data);
  useEffect(() => {
    let alive = true;
    setData(undefined);
    setError("");
    void api<Slides>(engine ? "office.normalized" : "office.document", {
      id: file.id,
      engine,
    })
      .then((r) => {
        if (alive) {
          setData(r);
          setPage((n) => Math.min(Math.max(0, r.slides.length - 1), n));
        }
      })
      .catch((e) => {
        if (alive) setError(message(e));
      });
    return () => {
      alive = false;
    };
  }, [file.id, file.revision, file.identity, engine]);
  const slide = data?.slides[page];
  return (
    <section className="presentation-reader" aria-label={ui("PowerPoint 内容")}>
      <div className="reader-toolbar">
        <button
          className="icon-button"
          aria-label={ui("上一张幻灯片")}
          disabled={page === 0}
          onClick={() => setPage((n) => n - 1)}
        >
          <ChevronLeft size={16} />
        </button>
        <span>
          {data
            ? `${data.slides.length ? page + 1 : 0} / ${data.total}`
            : "PowerPoint"}
        </span>
        <button
          className="icon-button"
          aria-label={ui("下一张幻灯片")}
          disabled={!data || page >= data.slides.length - 1}
          onClick={() => setPage((n) => n + 1)}
        >
          <ChevronRight size={16} />
        </button>
      </div>
      {error ? (
        <ReaderMessage error={error} />
      ) : !data ? (
        <ReaderMessage loading />
      ) : (
        <div ref={slideScroll} className="presentation-scroll">
          <article className="presentation-page" key={page}>
            {slide?.paragraphs.map((text, i) =>
              i === 0 ? <h3 key={i}>{text}</h3> : <p key={i}>{text}</p>,
            )}
            {slide?.images.map((key, i) => (
              <img
                key={i}
                src={`data:image/png;base64,${data.images[key]}`}
                alt={tf("幻灯片 {0} 图片 {1}", page + 1, i + 1)}
              />
            ))}
            {!slide?.paragraphs.length && !slide?.images.length && (
              <p>{ui("此页没有可显示的文字或图片")}</p>
            )}
            {!!slide?.omittedImages && (
              <p className="subtle">{ui("部分图片暂不支持预览")}</p>
            )}
          </article>
        </div>
      )}
      <small className="reader-note">
        {ui("内容预览 · 不还原动画、图表及原始排版")}
        {data?.truncated ? ui(" · 仅显示前 200 页") : ""}
      </small>
    </section>
  );
}
function ContentReaderBody({
  file,
  onSaved,
  editable = true,
}: {
  file: LocalFile;
  onSaved: (undoId?: string) => void;
  editable?: boolean;
}) {
  if (file.status !== "available")
    return <ReaderMessage error={file.error || ui("文件不可用，请重新关联")} />;
  if (file.kind === "image") return <ImageReader file={file} />;
  if (file.kind === "pdf") return <PdfReader file={file} />;
  if (file.extension.toLowerCase() === "pptx")
    return (
      <OfficeLayout file={file}>
        <FlowZoomReader key={file.id}>
          <PresentationReader file={file} />
        </FlowZoomReader>
      </OfficeLayout>
    );
  if (/^(csv|tsv)$/i.test(file.extension))
    return (
      <FlowZoomReader>
        <OfficeReader file={file} />
      </FlowZoomReader>
    );
  if (isOfficePreview(file))
    return (
      <OfficeLayout file={file}>
        <FlowZoomReader key={file.id}>
          <OfficeReader file={file} />
        </FlowZoomReader>
      </OfficeLayout>
    );
  if (/^(doc|xls|ppt|docm|xlsm|pptm|rtf)$/i.test(file.extension))
    return (
      <OfficeLayout
        file={file}
        legacyContent={(engine) => (
          <FlowZoomReader>
            {/^(ppt|pptm)$/i.test(file.extension) ? (
              <PresentationReader file={file} engine={engine} />
            ) : (
              <OfficeReader file={file} engine={engine} />
            )}
          </FlowZoomReader>
        )}
      />
    );
  if (isHtml(file))
    return (
      <FlowZoomReader key={file.id}>
        <HtmlReader file={file} />
      </FlowZoomReader>
    );
  if (/^(md|markdown|txt|log)$/i.test(file.extension))
    return (
      <FlowZoomReader key={file.id}>
        <TextReader file={file} onSaved={onSaved} editable={editable} />
      </FlowZoomReader>
    );
  return (
    <ReaderMessage
      error={ui("此格式暂不支持内置预览，可在文件信息中打开原文件")}
    />
  );
}

export function ContentReader(props: {
  file: LocalFile;
  onSaved: (undoId?: string) => void;
  editable?: boolean;
}) {
  const f = props.file;
  if (f.status !== "available")
    return <ReaderMessage error={f.error || ui("文件不可用，请重新关联")} />;
  // Keep the chosen live/copy mode while saving a new reading-copy revision.
  // Selecting a different file still mounts a fresh WebBrowser with the online default.
  if (f.webSource && f.kind === "web")
    return (
      <WebBrowser key={f.id} file={f}>
        <ReadingState key={`${f.id}:${f.revision}:${f.identity}`} file={f}>
          <FlowZoomReader>
            <WebContent
              file={f}
              onSaved={props.onSaved}
              editable={props.editable ?? true}
            />
          </FlowZoomReader>
        </ReadingState>
      </WebBrowser>
    );
  return (
    <ReadingState key={`${f.id}:${f.revision}:${f.identity}`} file={f}>
      <ContentReaderBody {...props} />
    </ReadingState>
  );
}
