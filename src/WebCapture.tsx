import { SettingsSection } from "./SettingsSection";
import { useReadingScroll } from "./ReadingState";
import { useEffect, useRef, useState } from "react";
import { Globe, ExternalLink, Save, Eye, Edit3, RotateCcw } from "lucide-react";
import { webDirty, webDrafts, webSaving, type WebDraft } from "./WebDrafts";
import { GlideSelect } from "./GlideSelect";
import type { Query } from "./api";
import { api, message, type LocalFile } from "./api";
import { t as ui, currentLanguage } from "./i18n";
import "./web-capture.css";
export function BrowserCaptureSettings({
  refreshKey,
}: {
  refreshKey: unknown;
}) {
  const [status, setStatus] = useState<{
      enabled: boolean;
      extensionId: string;
      registered: boolean;
      removable: boolean;
      registrationState: string;
    }>(),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  useEffect(() => {
    let live = true;
    api<typeof status>("capture.bridge.status")
      .then((s) => {
        if (live) setStatus(s);
      })
      .catch((e) => {
        if (live) setError(message(e));
      });
    return () => {
      live = false;
    };
  }, [refreshKey]);
  async function change(action: string) {
    setBusy(true);
    setError("");
    try {
      await api(action);
      setStatus(await api("capture.bridge.status"));
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <SettingsSection title={ui("浏览器采集")}>
      <div className="setting-row">
        <div>
          <strong>{ui("Chrome 扩展连接")}</strong>
          <p>
            {ui("保存网页链接、摘录或所选图片；桌面未运行时留在浏览器队列。")}
          </p>
        </div>
        <button
          className="button"
          disabled={busy || !status}
          onClick={() =>
            void change(
              status?.enabled
                ? "capture.bridge.disable"
                : "capture.bridge.register",
            )
          }
        >
          <Globe size={16} />
          {busy
            ? ui("处理中…")
            : status?.enabled
              ? ui("断开连接")
              : ui("连接 Chrome 扩展")}
        </button>
      </div>
      <p className="subtle" role="status">
        {status?.enabled
          ? ui(
              "已允许本机扩展采集，不会自动发送给 AI。启动 Chrome 并加载配套扩展后即可使用。",
            )
          : ui("尚未启用；扩展安装步骤见本版本使用说明。")}
      </p>
      {status && (
        <div className="capture-connection-tools">
          <p className="subtle" role="status">
            {status.registrationState === "ready"
              ? ui("本机注册正常")
              : status.registrationState === "foreign"
                ? ui("本机连接指向其他位置，可修复为当前程序")
                : status.registrationState === "stale"
                  ? ui("程序或资料库位置已变化，请修复连接")
                  : ui("尚未注册本机连接")}
          </p>
          <div className="capture-actions">
            <button
              className="button quiet"
              disabled={busy}
              onClick={() => void change("capture.bridge.register")}
            >
              {ui("修复连接")}
            </button>
            <button
              className="button quiet"
              disabled={busy || !status.removable}
              onClick={() => void change("capture.bridge.unregister")}
            >
              {ui("移除本机连接")}
            </button>
          </div>
          <small className="subtle">
            {ui(
              "修复会连接当前程序和资料库；移除仅清理本机连接，不删除资料或浏览器队列。",
            )}
          </small>
        </div>
      )}
      {status && (
        <small className="capture-extension-id">
          {ui("扩展 ID")} · {status.extensionId}
        </small>
      )}
      {error && (
        <p role="alert" className="tag-form-error">
          {error}
        </p>
      )}
    </SettingsSection>
  );
}
function initialDraft(file: LocalFile): WebDraft {
  const source = file.webSource!;
  const title = source.reading?.title ?? source.title;
  const text = source.reading?.text ?? source.excerpt;
  return {
    title,
    text,
    savedTitle: title,
    savedText: text,
    version: file.version,
    name: file.name,
  };
}

export function WebSourceFilters({
  query,
  onChange,
}: {
  query: Query;
  onChange: (value: Partial<Query>) => void;
}) {
  const [host, setHost] = useState(query.sourceHost || "");
  useEffect(() => setHost(query.sourceHost || ""), [query.sourceHost]);
  return (
    <div className="filter-line web-source-filters">
      <strong>{ui("采集来源")}</strong>
      <GlideSelect
        ariaLabel={ui("采集类型")}
        value={query.captureKind || ""}
        onChange={(value) => onChange({ captureKind: value })}
        options={[
          { value: "", label: ui("全部采集类型") },
          { value: "web_page", label: ui("网页链接") },
          { value: "web_excerpt", label: ui("文字摘录") },
          { value: "web_image", label: ui("网页图片") },
        ]}
      />
      <input
        aria-label={ui("来源网站域名")}
        placeholder={ui("来源网站，如 example.com")}
        value={host}
        onChange={(e) => setHost(e.target.value)}
        onBlur={() => onChange({ sourceHost: host.trim() })}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            onChange({ sourceHost: host.trim() });
          }
        }}
      />
    </div>
  );
}

export function WebContent({
  file,
  onSaved,
  editable = true,
}: {
  file: LocalFile;
  onSaved: (undoId?: string) => void;
  editable?: boolean;
}) {
  const scroll = useRef<HTMLElement>(null);
  useReadingScroll(scroll, "web");
  const [openError, setOpenError] = useState("");
  const [draft, setDraft] = useState(
    () => webDrafts.get(file.id) || initialDraft(file),
  );
  const [mode, setMode] = useState<"read" | "edit" | "original">(() =>
    webDrafts.get(file.id) && webDirty(webDrafts.get(file.id)!)
      ? "edit"
      : "read",
  );
  const [busy, setBusy] = useState(false);
  const [confirmReload, setConfirmReload] = useState(false);
  const matchIndex = useRef(-1);
  useEffect(() => {
    const sync = () => {
      const cached = webDrafts.get(file.id);
      if (cached && (webDirty(cached) || cached.version > file.version))
        setDraft(cached);
      else {
        webDrafts.delete(file.id);
        setDraft(initialDraft(file));
      }
    };
    sync();
    window.addEventListener("shiqian-web-updated", sync);
    return () => window.removeEventListener("shiqian-web-updated", sync);
  }, [file.id, file.version]);
  const source = file.webSource;
  if (!source) return null;
  const dirty = webDirty(draft);
  function edit(change: Partial<WebDraft>) {
    const next = { ...draft, ...change };
    webDrafts.set(file.id, next);
    setDraft(next);
    window.dispatchEvent(new Event("shiqian-web-updated"));
  }
  async function save(reset = false) {
    if (webSaving.has(file.id)) return;
    setBusy(true);
    webSaving.add(file.id);
    setOpenError("");
    try {
      const result = await api<{ file: LocalFile; undoId?: string }>(
        "capture.reading.save",
        {
          id: file.id,
          version: draft.version,
          title: draft.title,
          text: draft.text,
          reset,
        },
      );
      const latest = webDrafts.get(file.id);
      const fresh = initialDraft(result.file);
      if (
        !reset &&
        latest &&
        (latest.title !== draft.title || latest.text !== draft.text)
      ) {
        webDrafts.set(file.id, {
          ...fresh,
          title: latest.title,
          text: latest.text,
        });
      } else webDrafts.set(file.id, fresh);
      setDraft(webDrafts.get(file.id) || fresh);
      setConfirmReload(false);
      setMode("read");
      onSaved(result.undoId);
    } catch (e) {
      setOpenError(message(e));
    } finally {
      setBusy(false);
      webSaving.delete(file.id);
      window.dispatchEvent(new Event("shiqian-web-updated"));
    }
  }
  const original = mode === "original";
  const title = original ? source.title : draft.title;
  const text = original ? source.excerpt : draft.text;
  const query =
    file.contentMatch?.queryText || file.searchMatch?.queryText || "";
  const words = original
    ? []
    : query.trim().split(/\s+/).filter(Boolean).slice(0, 12);
  const matcher = words.length
    ? new RegExp(
        `(${words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})`,
        "giu",
      )
    : undefined;
  const highlighted = matcher
    ? text
        .split(matcher)
        .map((part, i) => (i % 2 ? <mark key={i}>{part}</mark> : part))
    : text;
  return (
    <article
      className="web-content"
      ref={scroll}
      onKeyDown={(e) => {
        if (
          (e.ctrlKey || e.metaKey) &&
          e.key.toLowerCase() === "s" &&
          mode === "edit"
        ) {
          e.preventDefault();
          e.stopPropagation();
          if (dirty && !busy) void save();
        }
      }}
    >
      <div className="reader-toolbar web-reading-tools">
        <div
          className="reader-control-group reader-mode-control"
          role="group"
          aria-label={ui("网页阅读模式")}
        >
          <button
            className={`button quiet ${mode === "read" ? "active" : ""}`}
            aria-pressed={mode === "read"}
            onClick={() => setMode("read")}
          >
            <Eye size={14} />
            {ui("阅读")}
          </button>
          {editable && (
            <button
              className={`button quiet ${mode === "edit" ? "active" : ""}`}
              aria-pressed={mode === "edit"}
              onClick={() => setMode("edit")}
            >
              <Edit3 size={14} />
              {ui("编辑")}
            </button>
          )}
          <button
            className={`button quiet ${original ? "active" : ""}`}
            aria-pressed={original}
            onClick={() => setMode("original")}
          >
            {ui("原始采集")}
          </button>
        </div>
        {editable && mode === "edit" && (
          <button
            className="button"
            disabled={!dirty || busy}
            onClick={() => void save()}
          >
            <Save size={14} />
            {busy ? ui("保存中…") : ui("保存")}
          </button>
        )}
      </div>
      <div className="web-source-kind">
        <Globe size={16} />
        {source.kind === "web_excerpt" ? ui("文字摘录") : ui("网页链接")}
      </div>
      {mode === "edit" && editable ? (
        <label className="web-edit-label">
          {ui("阅读标题")}
          <input
            aria-label={ui("阅读标题")}
            value={draft.title}
            maxLength={300}
            disabled={busy}
            onChange={(e) => edit({ title: e.target.value })}
          />
        </label>
      ) : (
        <h2>{title}</h2>
      )}
      <button
        className="button quiet web-source-link"
        onClick={() => {
          setOpenError("");
          void api("files.open", { id: file.id }).catch((e) =>
            setOpenError(message(e)),
          );
        }}
      >
        <ExternalLink size={14} />
        <span>{source.url}</span>
      </button>
      {openError && <p role="alert">{openError}</p>}
      {dirty && draft.version !== file.version && (
        <p className="tag-form-error" role="alert">
          {ui("资料已变化；草稿已保留，请重新读取后核对")}
        </p>
      )}
      <p className="subtle">
        {ui("采集时间")} ·{" "}
        {new Date(source.capturedAt).toLocaleString(currentLanguage())}
      </p>
      {dirty && mode !== "edit" && (
        <p role="status" className="subtle">
          {ui("未保存的阅读草稿")}
        </p>
      )}
      {words.length > 0 && mode === "read" && (
        <button
          className="button quiet"
          onClick={() => {
            const marks =
              scroll.current?.querySelectorAll<HTMLElement>(
                ".web-excerpt mark",
              );
            if (!marks?.length || !scroll.current) return;
            matchIndex.current = (matchIndex.current + 1) % marks.length;
            const mark = marks[matchIndex.current],
              host = scroll.current;
            host.scrollTo({
              top:
                host.scrollTop +
                mark.getBoundingClientRect().top -
                host.getBoundingClientRect().top -
                80,
              behavior: matchMedia("(prefers-reduced-motion: reduce)").matches
                ? "instant"
                : "smooth",
            });
          }}
        >
          {ui("下一处命中")}
        </button>
      )}
      {mode === "edit" && editable ? (
        <>
          <p className="subtle">
            {ui("仅修改阅读副本，原始摘录和来源保持不变。草稿在退出前请保存。")}
          </p>
          <textarea
            className="web-copy-editor"
            aria-label={ui("阅读正文")}
            value={draft.text}
            maxLength={20000}
            disabled={busy}
            onChange={(e) => edit({ text: e.target.value })}
          />
          <div className="web-copy-footer">
            <small className="subtle">
              {draft.text.length} / 20000 ·{" "}
              {dirty ? ui("未保存") : ui("已保存")}
            </small>
            <button
              className="button quiet"
              disabled={busy}
              onClick={() => setConfirmReload(true)}
            >
              <RotateCcw size={14} />
              {ui("重新读取")}
            </button>
          </div>
        </>
      ) : text ? (
        <div className="web-excerpt">{highlighted}</div>
      ) : (
        <p className="muted">{ui("已保存链接与来源信息，未下载网页正文。")}</p>
      )}
      {source.reading && !original && (
        <p className="subtle">
          {ui("阅读副本已修改")} ·{" "}
          {new Date(source.reading.updatedAt).toLocaleString(currentLanguage())}
        </p>
      )}
      {editable && source.reading && mode === "edit" && (
        <button
          className="button quiet"
          disabled={busy}
          onClick={() => setConfirmReload(true)}
        >
          {ui("恢复原始采集")}
        </button>
      )}
      {confirmReload && (
        <div
          className="web-copy-confirm"
          role="group"
          aria-label={ui("处理阅读副本")}
        >
          <p>
            {ui(
              "重新读取会放弃未保存草稿；恢复原始采集也会移除已保存的阅读副本。",
            )}
          </p>
          <button
            className="button quiet"
            disabled={busy}
            onClick={() => setConfirmReload(false)}
          >
            {ui("取消")}
          </button>
          <button
            className="button quiet"
            disabled={busy}
            onClick={async () => {
              try {
                const latest = await api<LocalFile>("file", { id: file.id });
                const fresh = initialDraft(latest);
                webDrafts.set(file.id, fresh);
                setDraft(fresh);
                setConfirmReload(false);
                setOpenError("");
                window.dispatchEvent(new Event("shiqian-web-updated"));
              } catch (e) {
                setOpenError(message(e));
              }
            }}
          >
            {ui("重新读取")}
          </button>
          {source.reading && (
            <button
              className="button"
              disabled={busy}
              onClick={() => void save(true)}
            >
              {ui("恢复原始采集")}
            </button>
          )}
        </div>
      )}
    </article>
  );
}
