import { t as ui, tf } from "./i18n";
import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Download } from "lucide-react";
import { api, date, message, type SupportSnapshot } from "./api";

export function BackupStatus({ refreshKey }: { refreshKey: unknown }) {
  const [status, setStatus] = useState<SupportSnapshot>(),
    [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    setError("");
    api<SupportSnapshot>("support.status")
      .then((value) => {
        if (active) setStatus(value);
      })
      .catch((e) => {
        if (active) setError(message(e));
      });
    return () => {
      active = false;
    };
  }, [refreshKey]);
  return (
    <p className="subtle">
      {error
        ? tf("备份状态暂不可用：{0}", error)
        : !status
          ? ui("正在读取备份状态…")
          : status.lastBackup
            ? tf(
                "最近{0}备份：{1}",
                status.lastBackup.kind === "beforeRestore"
                  ? ui("保护")
                  : ui("标注"),
                date(status.lastBackup.createdAt),
              )
            : ui("尚未记录标注备份")}
    </p>
  );
}
export function DiagnosticPanel() {
  const [text, setText] = useState(""),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [saved, setSaved] = useState(false);
  useEffect(() => {
    let active = true;
    api<{ text: string }>("diagnostics.preview")
      .then((value) => {
        if (active) setText(value.text);
      })
      .catch((e) => {
        if (active) setError(message(e));
      });
    return () => {
      active = false;
    };
  }, []);
  async function exportReport() {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      const path = await save({
        title: ui("保存诊断报告"),
        defaultPath: `拾签诊断-${new Date().toLocaleDateString("sv-SE")}-${Date.now()}.md`,
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (path) {
        await api("diagnostics.export", { path });
        setSaved(true);
      }
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="modal-content diagnostic-panel">
      <p className="muted">
        {ui("报告仅包含版本、计数和任务状态，供你自行分享给开发者。")}
      </p>
      <pre aria-label={ui("诊断报告预览")} tabIndex={0}>
        {text || ui("正在生成…")}
      </pre>
      <button
        className="button primary"
        disabled={busy || !text}
        onClick={() => void exportReport()}
      >
        <Download size={15} />
        {busy ? ui("正在保存…") : ui("导出当前诊断")}
      </button>
      {saved && <p role="status">{ui("诊断报告已保存")}</p>}
      {error && (
        <p role="alert" className="tag-form-error">
          {error}
        </p>
      )}
    </div>
  );
}
