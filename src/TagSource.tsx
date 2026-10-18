import { currentLanguage } from "./i18n";
import { t as ui, tf } from "./i18n";
import { Folder, Sparkles } from "lucide-react";
import type { Tag } from "./api";

export function TagSource({ tag, pool = false }: { tag: Tag; pool?: boolean }) {
  const source = pool ? tag.createdBy : tag.source;
  if (source === "ai" && (pool ? tag.accepted : tag.ai?.confirmed)) return null;
  if (source === "ai") {
    const detail = pool
      ? ui("由 AI 新建，可在标签管理中重命名或删除")
      : [
          ui("AI 标注，待确认"),
          tag.ai?.model,
          tag.ai?.reason,
          tag.ai?.updatedAt
            ? new Date(tag.ai.updatedAt).toLocaleString(currentLanguage())
            : "",
        ]
          .filter(Boolean)
          .join(" · ");
    return (
      <span
        className="source-badge source-ai"
        data-tooltip={detail}
        aria-label={pool ? ui("AI 新建标签") : ui("AI 待确认")}
      >
        <Sparkles size={10} />
        {pool ? ui("AI 新建") : "AI"}
      </span>
    );
  }
  if (!pool && source === "folder")
    return (
      <span
        className="source-badge source-folder"
        data-tooltip={ui("根据文件所在的直接父文件夹自动添加")}
      >
        <Folder size={10} />
        {ui("文件夹")}
      </span>
    );
  return null;
}
