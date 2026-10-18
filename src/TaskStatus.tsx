import { message } from "./api";
import { t as ui, tf } from "./i18n";
import { AlertCircle, Check, Clock3, Minus, Sparkles } from "lucide-react";
import type { LocalFile } from "./api";

export function TaskGlyph({
  status,
}: {
  status: "working" | "done" | "error" | "waiting" | "cancelled" | "review";
}) {
  if (status === "working")
    return (
      <span className="task-lattice" aria-hidden="true">
        {Array.from({ length: 9 }, (_, i) => (
          <i key={i} style={{ animationDelay: `${i * 75}ms` }} />
        ))}
      </span>
    );
  const Icon = {
    done: Check,
    error: AlertCircle,
    waiting: Clock3,
    cancelled: Minus,
    review: Sparkles,
  }[status];
  return <Icon className="task-glyph" size={14} aria-hidden="true" />;
}

export function AiTaskStatus({
  files,
  busy,
  onCancel,
}: {
  files: LocalFile[];
  busy: boolean;
  onCancel: () => void;
}) {
  const counts: Record<string, number> = {};
  for (const file of files)
    if (file.aiTask)
      counts[file.aiTask.status] = (counts[file.aiTask.status] || 0) + 1;
  const pending = files.reduce(
    (n, file) =>
      n + file.tags.filter((t) => t.source === "ai" && !t.ai?.confirmed).length,
    0,
  );
  const active = (counts.queued || 0) + (counts.running || 0);
  if (!Object.keys(counts).length && !pending) return null;
  const task = files.length === 1 ? files[0].aiTask : undefined;
  const status = active
    ? counts.running
      ? "working"
      : "waiting"
    : counts.failed
      ? "error"
      : pending
        ? "review"
        : counts.cancelled
          ? "cancelled"
          : counts.unsupported
            ? "waiting"
            : "done";
  const singleLabels = {
    queued: ui("等待 AI 分析"),
    running: ui("AI 正在分析"),
    done: pending ? tf("{0} 个 AI 标签待确认", pending) : ui("AI 标注已更新"),
    failed: ui("AI 分析失败，可重新识别"),
    unsupported: ui("暂不支持此文件内容"),
    cancelled: ui("AI 分析已取消"),
  };
  const text =
    files.length === 1
      ? task
        ? ui(singleLabels[task.status])
        : tf("{0} 个 AI 标签待确认", pending)
      : [
          counts.queued && tf("等待 {0}", counts.queued),
          counts.running && tf("处理中 {0}", counts.running),
          counts.done && tf("完成 {0}", counts.done),
          counts.failed && tf("失败 {0}", counts.failed),
          counts.unsupported && tf("未支持 {0}", counts.unsupported),
          counts.cancelled && tf("已取消 {0}", counts.cancelled),
          pending && tf("待确认标签 {0}", pending),
        ]
          .filter(Boolean)
          .join(" · ");
  return (
    <div
      className={`ai-task-status status-${task?.status || status}`}
      role="status"
      aria-live="polite"
      aria-atomic="true"
      data-task-state={status}
    >
      <span className="task-state-line" key={status}>
        <TaskGlyph status={status} />
        <span>{text}</span>
      </span>
      {active > 0 && (
        <button
          className="quiet-link task-cancel"
          disabled={busy}
          onClick={onCancel}
          data-tooltip={ui("仅取消所选文件的任务；已发送的请求结束后丢弃结果")}
        >
          {ui("取消识别")}
        </button>
      )}
      {task?.error && <small>{message(task.error)}</small>}
    </div>
  );
}
