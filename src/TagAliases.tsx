import { t as ui } from "./i18n";
import { useState } from "react";
import { api, message, type Tag } from "./api";
import { Modal } from "./components";
import "./tag-organize.css";
type Preview = {
  affected: number;
  collections: { name: string }[];
  conflicts: string[];
  name: string;
  aliases: string[];
};
export function TagAliases({
  tag,
  onClose,
  onSaved,
}: {
  tag: Tag;
  onClose: () => void;
  onSaved: (undoId?: string, name?: string) => Promise<void>;
}) {
  const [name, setName] = useState(tag.name),
    [text, setText] = useState((tag.aliases ?? []).join("\n"));
  const [preview, setPreview] = useState<Preview>(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const payload = () => ({
    id: tag.id,
    version: tag.version,
    name: name.trim(),
    aliases: text
      .split(/\r?\n/)
      .map((n) => n.trim())
      .filter(Boolean),
  });
  async function inspect() {
    setBusy(true);
    setError("");
    try {
      setPreview(await api<Preview>("tags.aliases.preview", payload()));
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function save() {
    if (!preview || busy) return;
    setBusy(true);
    setError("");
    try {
      const r = await api<{ undoId?: string }>("tags.aliases.update", {
        ...payload(),
        preview,
      });
      await onSaved(r.undoId, preview.name);
      onClose();
    } catch (e) {
      setError(message(e));
      setPreview(undefined);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={ui("规范名称与别名")}
      onClose={onClose}
      dismissDisabled={busy}
    >
      <form
        className="modal-content organize-dialog aliases-dialog"
        onSubmit={(e) => {
          e.preventDefault();
          void (preview ? save() : inspect());
        }}
      >
        <label>
          {ui("规范名称")}
          <input
            aria-label={ui("规范标签名称")}
            value={name}
            disabled={busy}
            onChange={(e) => {
              setName(e.target.value);
              setPreview(undefined);
            }}
          />
        </label>
        <label>
          {ui("别名")}
          <textarea
            aria-label={ui("标签别名")}
            rows={4}
            placeholder={ui(
              "每行一个别名，例如：\\n学习资料\\nLearning resources",
            )}
            value={text}
            disabled={busy}
            onChange={(e) => {
              setText(e.target.value);
              setPreview(undefined);
            }}
          />
        </label>
        <p className="muted">
          {ui(
            "最多 20 个别名。搜索或输入别名时使用同一个规范标签，文件上只显示规范名称。",
          )}
        </p>
        {preview && (
          <div className="merge-summary" aria-label={ui("别名变更预览")}>
            <strong>
              {preview.affected} {ui("个文件使用此标签")}
            </strong>
            <span>
              {preview.aliases.length} {ui("个别名 ·")}
              {preview.collections.length} {ui("个保存集合引用")}
            </span>
            <span>
              {ui("文件关联与集合条件保持原标签 ID，浮窗同步显示规范名称。")}
            </span>
            {preview.collections.length > 0 && (
              <span>{preview.collections.map((c) => c.name).join("、")}</span>
            )}
            {preview.conflicts.map((c) => (
              <p className="tag-form-error" role="alert" key={c}>
                {c}
              </p>
            ))}
          </div>
        )}
        {error && (
          <p role="alert" className="tag-form-error">
            {error}
          </p>
        )}
        <div className="organize-actions">
          <button
            type="button"
            className="button"
            disabled={busy}
            onClick={onClose}
          >
            {ui("取消")}
          </button>
          <button
            className="button primary"
            disabled={busy || !name.trim() || !!preview?.conflicts.length}
          >
            {busy ? ui("正在核对…") : preview ? ui("确认修改") : ui("预览变更")}
          </button>
        </div>
      </form>
    </Modal>
  );
}
