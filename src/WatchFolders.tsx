import { currentLanguage } from "./i18n";
import { t as ui, tf } from "./i18n";
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  FolderOpen,
  Plus,
  RefreshCw,
  Pause,
  Play,
  Pencil,
  Trash2,
} from "lucide-react";
import { api, message } from "./api";
import { Modal } from "./components";
import "./watch-folders.css";
export interface WatchRule {
  id: string;
  path: string;
  recursive: boolean;
  ignore: string[];
  ai: boolean;
  paused: boolean;
}
export interface WatchStatus {
  state: string;
  scanned: number;
  pending: number;
  added: number;
  moved: number;
  changed: number;
  missing: number;
  skipped: number;
  lastScan: number;
  errors: string[];
}
export interface WatchSnapshot {
  version: string;
  rules: { rule: WatchRule; status: WatchStatus }[];
}
const stateNames: Record<string, string> = {
  watching: "监听中",
  waiting: "等待文件稳定",
  scanning: "扫描中",
  paused: "已暂停",
  offline: "目录不可用",
  error: "需检查",
  warning: "部分需检查",
  deferred: "等待其他任务",
};
const emptyRule = (): WatchRule => ({
  id: "",
  path: "",
  recursive: true,
  ignore: ["*.tmp", "*.part", "*.crdownload", "~$*", ".git", "node_modules"],
  ai: false,
  paused: false,
});
export function WatchFolders({ onClose }: { onClose: () => void }) {
  const [data, setData] = useState<WatchSnapshot>();
  const [edit, setEdit] = useState<WatchRule>();
  const [editVersion, setEditVersion] = useState("");
  const [ignore, setIgnore] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true,
      polling = false;
    const poll = async () => {
      if (polling) return;
      polling = true;
      try {
        const next = await api<WatchSnapshot>("watch.list");
        if (live) setData(next);
      } catch (e) {
        if (live) setError(message(e));
      } finally {
        polling = false;
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 2000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, []);
  function startEdit(rule: WatchRule) {
    setEdit({ ...rule });
    setEditVersion(data?.version || "");
    setIgnore(rule.ignore.join("\n"));
    setError("");
  }
  async function run(action: string, payload: unknown, closeEdit = false) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await api(action, payload);
      setData(await api<WatchSnapshot>("watch.list"));
      if (closeEdit) setEdit(undefined);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function browse() {
    try {
      const path = await open({
        directory: true,
        multiple: false,
        title: ui("选择监听目录"),
      });
      if (typeof path === "string") setEdit((r) => (r ? { ...r, path } : r));
    } catch (e) {
      setError(message(e));
    }
  }
  return (
    <Modal
      title={ui("文件夹监听")}
      onClose={onClose}
      dismissDisabled={busy}
      wide
    >
      <div className="modal-content watch-folders">
        <p className="muted">
          {ui(
            "自动加入符合规则的现有及新增文件；文件删除或离线时保留标注。约每 5 秒检查，等待文件稳定后加入。",
          )}
        </p>
        <div className="watch-toolbar">
          <button
            className="button"
            disabled={busy || !data || data.rules.length >= 8}
            onClick={() => startEdit(emptyRule())}
          >
            <Plus size={16} />
            {ui("添加监听目录")}
          </button>
          <button
            className="button quiet"
            disabled={busy || !data?.rules.some((x) => !x.rule.paused)}
            onClick={() => void run("watch.scan", {})}
          >
            <RefreshCw size={15} />
            {ui("重新扫描")}
          </button>
        </div>
        {edit && (
          <form
            className="watch-editor"
            onSubmit={(e) => {
              e.preventDefault();
              void run(
                "watch.save",
                {
                  version: editVersion,
                  rule: {
                    ...edit,
                    ignore: ignore
                      .split(/\r?\n/)
                      .map((x) => x.trim())
                      .filter(Boolean),
                  },
                },
                true,
              );
            }}
          >
            <label>
              {ui("监听目录")}
              <div className="watch-path-input">
                <input
                  aria-label={ui("监听目录")}
                  required
                  value={edit.path}
                  disabled={busy}
                  placeholder={ui("选择或粘贴文件夹路径")}
                  onChange={(e) => setEdit({ ...edit, path: e.target.value })}
                />
                <button
                  className="icon-button"
                  type="button"
                  aria-label={ui("选择监听目录")}
                  disabled={busy}
                  onClick={() => void browse()}
                >
                  <FolderOpen size={17} />
                </button>
              </div>
            </label>
            <label className="watch-check">
              <input
                type="checkbox"
                aria-label={ui("包含子文件夹")}
                checked={edit.recursive}
                disabled={busy}
                onChange={(e) =>
                  setEdit({ ...edit, recursive: e.target.checked })
                }
              />
              {ui("包含子文件夹")}
            </label>
            <label>
              {ui("忽略规则")}
              <textarea
                aria-label={ui("忽略规则")}
                rows={4}
                disabled={busy}
                value={ignore}
                onChange={(e) => setIgnore(e.target.value)}
              />
              <small>
                {ui(
                  "* 匹配任意字符，? 匹配单个字符；每行一条，匹配相对路径或文件／目录名。",
                )}
              </small>
            </label>
            <label className="watch-check">
              <input
                type="checkbox"
                aria-label={ui("允许此目录自动 AI 标注")}
                checked={edit.ai}
                disabled={busy}
                onChange={(e) => setEdit({ ...edit, ai: e.target.checked })}
              />
              {ui("允许此目录自动 AI 标注")}
            </label>
            <p className="subtle">
              {ui(
                "默认关闭；勾选后仍需全局 AI 已启用。只对本规则新加入的文件应用，手工标注导入规则不变。",
              )}
            </p>
            <div className="watch-editor-actions">
              <button
                className="button"
                type="button"
                disabled={busy}
                onClick={() => setEdit(undefined)}
              >
                {ui("取消编辑")}
              </button>
              <button
                className="button primary"
                type="submit"
                disabled={busy || !edit.path.trim()}
              >
                {busy
                  ? ui("保存中…")
                  : edit.id
                    ? ui("保存规则")
                    : ui("开始监听")}
              </button>
            </div>
          </form>
        )}
        {!data && !error && <p role="status">{ui("正在读取监听规则…")}</p>}
        {data && !data.rules.length && !edit && (
          <p className="watch-empty">
            {ui("添加常用文件夹，新资料会自动进入工作台。")}
          </p>
        )}
        <div className="watch-list">
          {data?.rules.map(({ rule, status }) => (
            <article
              className="watch-card"
              key={rule.id}
              data-watch-id={rule.id}
            >
              <div className="watch-card-heading">
                <FolderOpen size={18} />
                <strong data-tooltip={rule.path}>{rule.path}</strong>
                <span className={`watch-state ${status.state}`}>
                  {ui(stateNames[status.state] || status.state)}
                </span>
              </div>
              <p className="watch-rule-summary">
                {rule.recursive ? ui("包含子文件夹") : ui("仅此文件夹")} ·{" "}
                {rule.ignore.length} {ui("条忽略规则 · AI")}{" "}
                {rule.ai ? ui("按全局设置") : ui("关闭")}
              </p>
              <p className="watch-counts">
                {ui("待处理")}
                {status.pending} {ui("· 已加入")}
                {status.added} {ui("· 改名")} {status.moved} {ui("· 内容更新")}
                {status.changed} {ui("· 缺失")} {status.missing}
              </p>
              {!!status.lastScan && (
                <small className="subtle">
                  {ui("最近检查")}{" "}
                  {new Date(status.lastScan).toLocaleTimeString(
                    currentLanguage(),
                  )}{" "}
                  · {status.scanned} {ui("个条目")}
                </small>
              )}
              {status.errors.length > 0 && (
                <details className="watch-errors">
                  <summary>
                    {status.errors.length} {ui("条提示")}
                  </summary>
                  {status.errors.map((e, i) => (
                    <p key={i}>{message(e)}</p>
                  ))}
                </details>
              )}
              <div className="watch-card-actions">
                <button
                  className="button quiet"
                  disabled={busy}
                  aria-label={tf(
                    "{0}监听 {1}",
                    rule.paused ? ui("恢复") : ui("暂停"),
                    rule.path,
                  )}
                  onClick={() =>
                    void run("watch.pause", {
                      id: rule.id,
                      paused: !rule.paused,
                      version: data.version,
                    })
                  }
                >
                  {rule.paused ? <Play size={14} /> : <Pause size={14} />}{" "}
                  {rule.paused ? ui("恢复") : ui("暂停")}
                </button>
                <button
                  className="button quiet"
                  disabled={busy}
                  aria-label={tf("编辑监听 {0}", rule.path)}
                  onClick={() => startEdit(rule)}
                >
                  <Pencil size={14} />
                  {ui("编辑")}
                </button>
                <button
                  className="button quiet"
                  disabled={busy}
                  aria-label={tf("移除监听 {0}", rule.path)}
                  onClick={() =>
                    void run("watch.remove", {
                      id: rule.id,
                      version: data.version,
                    })
                  }
                >
                  <Trash2 size={14} />
                  {ui("移除监听")}
                </button>
              </div>
            </article>
          ))}
        </div>
        {error && (
          <p className="tag-form-error" role="alert">
            {error}
          </p>
        )}
        <p className="subtle">
          {ui(
            "暂停与移除监听均保留已导入文件和标注。应用关闭后不监听，下次启动补扫；统计为本次运行。跨盘或不确定的移动请使用路径修复。",
          )}
        </p>
        <div className="watch-editor-actions">
          <button className="button" disabled={busy} onClick={onClose}>
            {ui("完成")}
          </button>
        </div>
      </div>
    </Modal>
  );
}
