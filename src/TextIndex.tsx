import { SettingsSection } from "./SettingsSection";
import { t as ui, tf } from "./i18n";
import { useEffect, useState } from "react";
import { Search, Pause, Play, RefreshCw, Square, Trash2 } from "lucide-react";
import {
  api,
  message,
  size,
  type LocalFile,
  type TextIndexStatus,
} from "./api";
import "./text-index.css";

export function ContentSnippet({ file }: { file: LocalFile }) {
  if (!file.contentMatch && !file.searchMatch) return null;
  const labels = {
    filename: ui("文件名"),
    tag: ui("标签"),
    alias: ui("标签别名"),
    note: ui("备注"),
    body: ui("正文"),
    semantic: ui("语义补充"),
    web: ui("网页来源与摘录"),
  };
  const sources = file.searchMatch?.sources || [];
  const kinds = [...new Set(sources.map((s) => labels[s.kind]))];
  const evidence = kinds.length ? (
    <small
      className="search-explanation"
      data-tooltip={sources
        .map((s) => `${labels[s.kind]}${s.value ? ` · ${s.value}` : ""}`)
        .join("；")}
    >
      {ui("命中")} · {kinds.join(" · ")}
    </small>
  ) : null;
  if (!file.contentMatch) return evidence;
  const terms = (file.contentMatch.queryText || "")
    .trim()
    .split(/\s+/)
    .filter(Boolean);
  if (!terms.length) return null;
  const pattern = new RegExp(
    `(${terms.map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})`,
    "giu",
  );
  const pieces = file.contentMatch.semantic
    ? [file.contentMatch.snippet]
    : file.contentMatch.snippet.split(pattern);
  return (
    <span className="content-snippet">
      {evidence}
      <span>
        {pieces.map((piece, i) =>
          i % 2 ? <mark key={i}>{piece}</mark> : piece,
        )}
      </span>
      <small data-tooltip={file.contentMatch.coverage}>
        {file.contentMatch.source} ·{" "}
        {file.contentMatch.semantic
          ? tf("相似度 {0}（非置信度）", file.contentMatch.score?.toFixed(2))
          : file.contentMatch.encoding}
        {file.contentMatch.truncated ? ui(" · 部分内容") : ""}
      </small>
    </span>
  );
}

export function TextIndexSettings({ compact = false }: { compact?: boolean }) {
  const [status, setStatus] = useState<TextIndexStatus>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [confirm, setConfirm] = useState("");
  useEffect(() => {
    let active = true,
      polling = false;
    const poll = async () => {
      if (polling) return;
      polling = true;
      try {
        const s = await api<TextIndexStatus>("index.status");
        if (active) setStatus(s);
      } catch (e) {
        if (active) setError(message(e));
      } finally {
        polling = false;
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 1500);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, []);
  async function control(action: string) {
    setBusy(true);
    setError("");
    setConfirm("");
    try {
      await api("index.control", { action });
      setStatus(await api<TextIndexStatus>("index.status"));
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <SettingsSection
      className="text-index-settings"
      collapsible={!compact}
      title={
        <>
          <Search size={16} />
          {ui("正文索引")}
        </>
      }
    >
      <div className="setting-row">
        <div>
          {!compact && (
            <p>
              {ui("搜索已加入的文本、Office 和 PDF 正文，内容保存在本机。")}
            </p>
          )}
        </div>
        <button
          className="button"
          disabled={busy || !status}
          aria-pressed={status?.enabled === true}
          onClick={() => void control(status?.enabled ? "disable" : "enable")}
        >
          {status?.enabled ? ui("关闭索引") : ui("启用索引")}
        </button>
      </div>
      {status && (
        <>
          <p className="index-summary" role="status">
            {!status.enabled
              ? ui("未启用")
              : status.state === "paused"
                ? ui("已暂停")
                : status.state === "cancelled"
                  ? ui("已取消")
                  : status.counts.queued ||
                      status.counts.running ||
                      status.counts.stale
                    ? ui("正在建立索引")
                    : ui("索引已就绪")}{" "}
            {ui("· 完成")}
            {status.counts.done || 0} / {status.eligible} · {size(status.bytes)}
          </p>
          {status.enabled && (
            <>
              <div className="index-counts">
                <span>
                  {ui("等待")}{" "}
                  {(status.counts.queued || 0) + (status.counts.stale || 0)}
                </span>
                <span>
                  {ui("处理中")}
                  {status.counts.running || 0}
                </span>
                <span>
                  {ui("失败")}
                  {status.counts.failed || 0}
                </span>
                {status.truncated > 0 && (
                  <span>
                    {ui("部分内容")}
                    {status.truncated}
                  </span>
                )}
              </div>
              <div className="settings-buttons">
                <button
                  className="button"
                  disabled={busy}
                  onClick={() =>
                    void control(status.state === "active" ? "pause" : "resume")
                  }
                >
                  {status.state === "active" ? (
                    <Pause size={14} />
                  ) : (
                    <Play size={14} />
                  )}{" "}
                  {status.state === "active" ? ui("暂停索引") : ui("继续索引")}
                </button>
                <button
                  className="button quiet"
                  disabled={busy || status.state === "cancelled"}
                  onClick={() => void control("cancel")}
                >
                  <Square size={14} />
                  {ui("取消任务")}
                </button>
                <button
                  className="button quiet"
                  disabled={busy}
                  onClick={() => setConfirm("rebuild")}
                >
                  <RefreshCw size={14} />
                  {ui("重建索引")}
                </button>
              </div>
            </>
          )}
          {!compact && (
            <>
              <p className="subtle">
                {ui("每个文件读取前")}
                {size(status.readLimit)}
                {ui("；正文与规范化文本共")} {size(status.textBudget)}
                {ui(
                  "。Office 每文件最多提取 2 MiB 正文、20,000 个位置；DOCX／XLSX 源文件上限 32 MiB，CSV／TSV 上限 8 MiB。公式只读已保存的原始结果。PDF 源文件上限 32 MiB，最多前 500 页、单页 256 KiB、正文合计 2 MiB；扫描件暂不支持 OCR，加密 PDF 不索引。",
                )}
              </p>
              <button
                className="button quiet"
                disabled={busy || status.bytes === 0}
                onClick={() => setConfirm("clear")}
              >
                <Trash2 size={14} />
                {ui("清除正文索引")}
              </button>
            </>
          )}
          {confirm && (
            <div className="index-confirm">
              <p>
                {confirm === "clear"
                  ? ui("清除本机缓存的正文并关闭索引？")
                  : ui(
                      "重新读取已加入的文本、Office 和 PDF 文件并重建索引？",
                    )}{" "}
                {ui("标签、备注和原文件会保留。")}
              </p>
              <div className="settings-buttons">
                <button
                  className="button primary"
                  disabled={busy}
                  onClick={() => void control(confirm)}
                >
                  {ui("确认")}
                  {confirm === "clear" ? ui("清除") : ui("重建")}
                </button>
                <button className="button quiet" onClick={() => setConfirm("")}>
                  {ui("取消")}
                </button>
              </div>
            </div>
          )}
          {status.errors.length > 0 && (
            <details>
              <summary>{ui("查看失败原因（最近 5 项）")}</summary>
              {status.errors.map((item, i) => (
                <p key={i}>
                  {item.name}：{message(item.error)}
                </p>
              ))}
              <p className="subtle">{ui("修复文件后可重建索引重试。")}</p>
            </details>
          )}
        </>
      )}
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
    </SettingsSection>
  );
}
