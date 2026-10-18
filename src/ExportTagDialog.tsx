import { t as ui, tf } from "./i18n";
import { useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Download, FolderOpen, LoaderCircle } from "lucide-react";
import { api, message, Tag } from "./api";
import { Modal } from "./components";

type ExportResult = {
  path: string;
  copied: number;
  total: number;
  failed: number;
  failures: { path: string; error: string }[];
};
export function ExportTagDialog({
  tag,
  onClose,
  onComplete,
}: {
  tag: Tag;
  onClose: () => void;
  onComplete?: (result: ExportResult) => void;
}) {
  const [destination, setDestination] = useState("");
  const [busy, setBusy] = useState(false);
  const lock = useRef(false);
  const [error, setError] = useState("");
  const [result, setResult] = useState<ExportResult>();
  async function choose() {
    try {
      const path = await open({
        directory: true,
        multiple: false,
        title: ui("选择标签文件导出位置"),
      });
      if (typeof path === "string") {
        setDestination(path);
        setError("");
      }
    } catch (e) {
      setError(message(e));
    }
  }
  async function start() {
    if (lock.current || !destination.trim()) return;
    lock.current = true;
    setBusy(true);
    setError("");
    try {
      const completed = await api<ExportResult>("tags.export", {
        id: tag.id,
        destination,
      });
      setResult(completed);
      onComplete?.(completed);
    } catch (e) {
      setError(message(e));
    } finally {
      lock.current = false;
      setBusy(false);
    }
  }
  return (
    <Modal
      title={ui("导出标签文件")}
      dismissDisabled={busy}
      onClose={() => {
        if (!lock.current) onClose();
      }}
    >
      <div className="modal-content export-panel">
        <h3>{tag.name}</h3>
        {!result ? (
          <>
            <p className="muted">
              {tag.count} {ui("个关联文件 · 复制到新文件夹，原文件保留")}
            </p>
            <div className="export-destination">
              <input
                aria-label={ui("导出位置")}
                placeholder={ui("选择或粘贴文件夹路径")}
                value={destination}
                disabled={busy}
                onChange={(e) => setDestination(e.target.value)}
              />
              <button
                className="button"
                aria-label={ui("选择导出文件夹")}
                data-tooltip={ui("选择文件夹")}
                onClick={() => void choose()}
                disabled={busy}
              >
                <FolderOpen size={16} />
              </button>
            </div>
            <p className="subtle">
              {ui("同名文件自动编号，未能复制的文件列入导出清单。")}
            </p>
            <button
              className="button primary"
              onClick={() => void start()}
              disabled={busy || !destination.trim() || tag.count === 0}
            >
              {busy ? (
                <LoaderCircle size={16} className="spin" />
              ) : (
                <Download size={16} />
              )}{" "}
              {busy ? ui("正在复制，请稍候…") : ui("导出文件夹")}
            </button>
          </>
        ) : (
          <div role="status" className="export-result">
            <strong>
              {result.failed
                ? tf("已复制 {0} 个，失败 {1} 个", result.copied, result.failed)
                : tf("已导出 {0} 个文件", result.copied)}
            </strong>
            <p className="export-path">{result.path}</p>
            {result.failed > 0 && (
              <details>
                <summary>{ui("查看未导出的文件")}</summary>
                {result.failures.slice(0, 20).map((f, i) => (
                  <p key={i}>
                    {f.path}
                    <br />
                    {message(f.error)}
                  </p>
                ))}
                {result.failed > 20 && (
                  <p>{ui("完整记录见文件夹内的导出清单。")}</p>
                )}
              </details>
            )}
            <button
              className="button primary"
              onClick={() =>
                void api("export.reveal", { path: result.path }).catch((e) =>
                  setError(message(e)),
                )
              }
            >
              <FolderOpen size={16} />
              {ui("打开文件夹")}
            </button>
          </div>
        )}
        {error && (
          <p role="alert" className="tag-form-error">
            {error}
          </p>
        )}
      </div>
    </Modal>
  );
}
