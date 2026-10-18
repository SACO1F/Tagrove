import { t as ui, tf, msg, currentLanguage, type UiCopy } from "./i18n";
import { useEffect, useRef, useState } from "react";
import { Check, RefreshCw, Search, Sparkles, X } from "lucide-react";
import { api, message, type LocalFile } from "./api";
import { Modal } from "./components";
import { Preview } from "./preview";
import { ReviewHistory } from "./ReviewHistory";
import { ReviewTabs } from "./ReviewTabs";
import "./ai-review.css";

type Item = {
  fileId: string;
  tagId: string;
  version: number;
  tagVersion: number;
};
type Snapshot = Item & {
  name: string;
  tagName: string;
  ai: { model: string; reason: string; updatedAt: number };
};
type Conflict = { name: string; reason: string };
type Plan = {
  action: "accept" | "reject";
  items: Snapshot[];
  token: string;
  conflicts: Conflict[];
  count: number;
  fileCount: number;
};
type History = Plan & { id: string; createdAt: number };
type Results = {
  files: LocalFile[];
  total: number;
  suggestions: number;
  hasMore: boolean;
  summary: Record<string, number>;
  enabled: boolean;
};
const date = (n: number) =>
  new Date(n).toLocaleString(currentLanguage(), {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
const pending = (file: LocalFile) =>
  file.tags.filter((t) => t.source === "ai" && !t.ai?.confirmed);
const key = (i: Item) => `${i.fileId}:${i.tagId}`;

export function AiReview({
  onClose,
  onChanged,
  onConfigure,
}: {
  onClose: () => void;
  onChanged: () => Promise<void>;
  onConfigure: () => void;
}) {
  const [tab, setTab] = useState("pending"),
    [text, setText] = useState(""),
    [offset, setOffset] = useState(0);
  const [data, setData] = useState<Results>(),
    [history, setHistory] = useState<History[]>([]);
  const [selected, setSelected] = useState<Record<string, Item>>({}),
    [plan, setPlan] = useState<Plan>();
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState<UiCopy>("");
  const [conflicts, setConflicts] = useState<Conflict[]>([]),
    [retryPreview, setRetryPreview] = useState(false);
  const sequence = useRef(0),
    alive = useRef(true),
    working = useRef(false);
  async function load() {
    const request = ++sequence.current;
    const result = await api<Results>("ai.review.list", { text, offset });
    if (alive.current && request === sequence.current) {
      setData(result);
    }
  }
  useEffect(() => {
    if (tab !== "history") return;
    let active = true;
    void api<History[]>("ai.review.history")
      .then((records) => {
        if (active) setHistory(records);
      })
      .catch((e) => {
        if (active) setError(message(e));
      });
    return () => {
      active = false;
    };
  }, [tab]);
  useEffect(() => {
    alive.current = true;
    const refresh = () => {
      if (!working.current)
        void load().catch((e) => {
          if (alive.current) setError(message(e));
        });
    };
    const delay = setTimeout(refresh, 180),
      timer = setInterval(refresh, 1800);
    return () => {
      alive.current = false;
      sequence.current++;
      clearTimeout(delay);
      clearInterval(timer);
    };
  }, [text, offset]);
  async function run(task: () => Promise<void>) {
    if (working.current) return;
    working.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await task();
    } catch (e) {
      if (alive.current) setError(message(e));
    } finally {
      working.current = false;
      if (alive.current) setBusy(false);
    }
  }
  function toggle(item: Item) {
    setSelected((old) => {
      const next = { ...old };
      if (next[key(item)]) delete next[key(item)];
      else if (Object.keys(next).length < 200) next[key(item)] = item;
      return next;
    });
    setConflicts([]);
  }
  const pageItems = (data?.files || []).flatMap((file) =>
    pending(file).map((tag) => ({
      fileId: file.id,
      tagId: tag.id,
      version: file.version,
      tagVersion: tag.version,
    })),
  );
  const allPage =
    pageItems.length > 0 && pageItems.every((item) => selected[key(item)]);
  async function preview(action: "accept" | "reject") {
    await run(async () => {
      const next = await api<Plan>("ai.review.preview", {
        action,
        items: Object.values(selected),
      });
      setConflicts(next.conflicts);
      if (!next.conflicts.length) setPlan(next);
    });
  }
  async function commit() {
    if (!plan) return;
    await run(async () => {
      const result = await api<{ applied: number; conflicts: Conflict[] }>(
        "ai.review.commit",
        {
          action: plan.action,
          token: plan.token,
          items: Object.values(selected),
        },
      );
      setPlan(undefined);
      setConflicts(result.conflicts);
      if (result.applied) {
        setSelected({});
        setNotice(
          msg(
            "已{0} {1} 个建议",
            plan.action === "accept" ? msg("接受") : msg("拒绝"),
            result.applied,
          ),
        );
      }
      await load();
      await onChanged();
    });
  }
  return (
    <Modal
      title={ui("AI 建议审核")}
      wide
      className="ai-review-modal"
      onClose={onClose}
      dismissDisabled={busy}
    >
      <div className="modal-content ai-review">
        <ReviewTabs
          value={tab}
          disabled={busy || !!plan}
          onChange={(id) => {
            setTab(id);
            setRetryPreview(false);
          }}
        />
        <div
          key={tab}
          id="review-tab-panel"
          className="review-panel"
          role="tabpanel"
          aria-labelledby={`review-tab-${tab}`}
          tabIndex={0}
        >
          <div className="review-body">
            {error && (
              <p className="error-line" role="alert">
                {error}
              </p>
            )}
            {notice && (
              <p className="review-notice" role="status">
                <Check size={15} />
                {ui(notice)}
              </p>
            )}
            {!!conflicts.length && (
              <div className="review-conflicts" role="alert">
                <strong>{ui("未提交任何更改，请重新选择后预览")}</strong>
                {conflicts.map((c, n) => (
                  <p key={n}>
                    {c.name} · {c.reason}
                  </p>
                ))}
                <button
                  className="button quiet"
                  disabled={busy}
                  onClick={() => {
                    setSelected({});
                    setConflicts([]);
                    void run(load);
                  }}
                >
                  {ui("清除选择并刷新")}
                </button>
              </div>
            )}
            {plan ? (
              <section
                className="review-confirm"
                aria-label={ui("审核更改预览")}
              >
                <h3>
                  {plan.action === "accept" ? ui("接受") : ui("拒绝")}{" "}
                  {plan.count} {ui("个建议 ·")}
                  {plan.fileCount} {ui("个文件")}
                </h3>
                <p>
                  {plan.action === "accept"
                    ? ui("接受后移除 AI 标记，并将普通标签加入标签浮窗。")
                    : ui(
                        "仅移除所选待确认标签，没有其他关联的 AI 标签会从标签库清理。",
                      )}
                  {ui("手工、文件夹和已确认标签保留。")}
                </p>
                <ul className="review-change-list">
                  {plan.items.map((item) => (
                    <li key={key(item)}>
                      <span>{item.name}</span>
                      <strong>{item.tagName}</strong>
                    </li>
                  ))}
                </ul>
                <p className="review-caption">
                  {ui("提交时再次校验版本。批量审核没有撤销，操作记录会保留。")}
                </p>
                <div className="review-actions">
                  <button
                    className="button quiet"
                    disabled={busy}
                    onClick={() => setPlan(undefined)}
                  >
                    {ui("返回选择")}
                  </button>
                  <button
                    className={`button ${plan.action === "reject" ? "danger" : "primary"}`}
                    disabled={busy}
                    onClick={() => void commit()}
                  >
                    {busy ? ui("正在提交…") : ui("确认提交")}
                  </button>
                </div>
              </section>
            ) : tab === "pending" ? (
              <>
                <div className="review-toolbar">
                  <label className="review-search">
                    <Search size={16} />
                    <input
                      aria-label={ui("搜索待确认建议")}
                      placeholder={ui("搜索文件或建议标签")}
                      value={text}
                      disabled={busy}
                      onChange={(e) => {
                        setText(e.target.value);
                        setOffset(0);
                        setSelected({});
                        setConflicts([]);
                      }}
                    />
                  </label>
                  <button
                    className="icon-button"
                    aria-label={ui("刷新审核列表")}
                    disabled={busy}
                    onClick={() => void run(load)}
                  >
                    <RefreshCw size={16} />
                  </button>
                </div>
                <div className="review-selection">
                  <span>
                    {data
                      ? tf(
                          "{0} 个文件 · {1} 个建议",
                          data.total,
                          data.suggestions,
                        )
                      : ui("正在读取…")}
                  </span>
                  <button
                    className="quiet-link"
                    disabled={busy || !pageItems.length}
                    onClick={() => {
                      setSelected((old) =>
                        allPage
                          ? Object.fromEntries(
                              Object.entries(old).filter(
                                ([id]) => !pageItems.some((i) => key(i) === id),
                              ),
                            )
                          : Object.fromEntries(
                              [...Object.values(old), ...pageItems]
                                .slice(0, 200)
                                .map((i) => [key(i), i]),
                            ),
                      );
                      setConflicts([]);
                    }}
                  >
                    {allPage ? ui("取消本页") : ui("选择本页")}
                  </button>
                </div>
                <div className="review-files">
                  {data?.files.map((file) => (
                    <section className="review-file" key={file.id}>
                      <div className="review-file-heading">
                        <div className="review-thumbnail">
                          <Preview file={file} compact />
                        </div>
                        <div>
                          <h3>{file.name}</h3>
                          <p>{file.parent}</p>
                        </div>
                      </div>
                      <div className="review-suggestions">
                        {pending(file).map((tag) => {
                          const item = {
                            fileId: file.id,
                            tagId: tag.id,
                            version: file.version,
                            tagVersion: tag.version,
                          };
                          return (
                            <label className="review-suggestion" key={tag.id}>
                              <input
                                type="checkbox"
                                checked={!!selected[key(item)]}
                                disabled={
                                  busy ||
                                  (!selected[key(item)] &&
                                    Object.keys(selected).length >= 200)
                                }
                                onChange={() => toggle(item)}
                              />
                              <span>
                                <strong>
                                  <Sparkles size={13} />
                                  {tag.name}
                                </strong>
                                <span className="review-reason">
                                  {tag.ai?.reason || ui("未提供内容依据")}
                                </span>
                                <small>
                                  {tag.ai?.model || ui("未知模型")} ·{" "}
                                  {tag.ai && date(tag.ai.updatedAt)}
                                </small>
                              </span>
                            </label>
                          );
                        })}
                      </div>
                    </section>
                  ))}
                </div>
                {data && !data.files.length && (
                  <div className="review-empty">
                    <Check size={26} />
                    <h3>
                      {text ? ui("没有匹配的建议") : ui("没有待确认建议")}
                    </h3>
                    <p>
                      {data.enabled
                        ? ui("新生成的 AI 建议会出现在这里。")
                        : ui("可继续搜索和手工标注，也可在偏好设置中启用 AI。")}
                    </p>
                    {!data.enabled && (
                      <button className="button quiet" onClick={onConfigure}>
                        {ui("AI 设置")}
                      </button>
                    )}
                  </div>
                )}
                <div className="review-pagination">
                  <button
                    className="quiet-link"
                    disabled={busy || offset === 0}
                    onClick={() => setOffset(Math.max(0, offset - 40))}
                  >
                    {ui("上一页")}
                  </button>
                  <span>
                    {ui("第")}
                    {Math.floor(offset / 40) + 1} {ui("页")}
                  </span>
                  <button
                    className="quiet-link"
                    disabled={busy || !data?.hasMore}
                    onClick={() => setOffset(offset + 40)}
                  >
                    {ui("下一页")}
                  </button>
                </div>
              </>
            ) : tab === "history" ? (
              <section aria-label={ui("审核操作记录")}>
                <p className="review-caption">
                  {ui(
                    "本机保留最近 50 批，达到 6 MiB 时提前清理旧记录；随标注备份保存。记录反映操作时的名称，不提供跨会话撤销。",
                  )}
                </p>
                {!history.length && (
                  <p className="review-empty">{ui("暂无批量审核记录")}</p>
                )}
                {history.map((record) => (
                  <ReviewHistory
                    key={record.id}
                    title={tf(
                      "{0} {1} 个建议",
                      record.action === "accept" ? ui("接受") : ui("拒绝"),
                      record.count,
                    )}
                    meta={tf(
                      "{0} 个文件 · {1}",
                      record.fileCount,
                      date(record.createdAt),
                    )}
                  >
                    <ul>
                      {record.items.map((item) => (
                        <li key={key(item)}>
                          <strong>
                            {item.name} · {item.tagName}
                          </strong>
                          <p>{item.ai.reason}</p>
                          <small>
                            {item.ai.model} {ui("· 生成于")}
                            {date(item.ai.updatedAt)}
                          </small>
                        </li>
                      ))}
                    </ul>
                  </ReviewHistory>
                ))}
              </section>
            ) : (
              <section className="review-tasks" aria-label={ui("AI 识别任务")}>
                <h3>{ui("当前任务")}</h3>
                <p>
                  {ui("等待")}
                  {data?.summary.queued || 0} {ui("· 处理中")}{" "}
                  {data?.summary.running || 0} {ui("· 失败")}{" "}
                  {data?.summary.failed || 0} {ui("· 暂不支持")}{" "}
                  {data?.summary.unsupported || 0}
                </p>
                <p>
                  {ui(
                    "每个文件一次请求，串行处理。每次手动加入最多 1000 个文件，每个文件最多 5 个建议；费用由配置的模型服务计算。",
                  )}
                </p>
                <dl>
                  <dt>{ui("文本摘录")}</dt>
                  <dd>{ui("最多 12,000 字符")}</dd>
                  <dt>{ui("图片预览")}</dt>
                  <dd>{ui("编码后最多 12 MiB")}</dd>
                  <dt>{ui("PDF / Office 源文件")}</dt>
                  <dd>10 MiB / 30 MiB</dd>
                  <dt>{ui("请求超时")}</dt>
                  <dd>{ui("90 秒")}</dd>
                </dl>
                <p className="review-caption">
                  {ui(
                    "取消后不再接收任务结果；已经发送的请求可能仍计费。未启用 AI 时不会发送请求，已有建议仍可审核。",
                  )}
                </p>
                {retryPreview && (
                  <p className="review-caption">
                    {ui(
                      "重新提交全部失败任务，每个文件一次请求。不会重试已取消、不支持或已完成的任务。",
                    )}
                  </p>
                )}
              </section>
            )}
          </div>
          <div className="review-footer-slot">
            {!plan && tab === "pending" && (
              <div className="review-actions review-footer-bar">
                <span>
                  {ui("已选")}
                  {Object.keys(selected).length} / 200
                </span>
                <button
                  className="button quiet"
                  disabled={busy || !Object.keys(selected).length}
                  onClick={() => {
                    setSelected({});
                    setConflicts([]);
                  }}
                >
                  {ui("清除选择")}
                </button>
                <button
                  className="button quiet"
                  disabled={busy || !Object.keys(selected).length}
                  onClick={() => void preview("reject")}
                >
                  <X size={15} />
                  {ui("拒绝所选")}
                </button>
                <button
                  className="button primary"
                  disabled={busy || !Object.keys(selected).length}
                  onClick={() => void preview("accept")}
                >
                  <Check size={15} />
                  {ui("接受所选")}
                </button>
              </div>
            )}
            {!plan && tab === "tasks" && (
              <div className="review-task-footer">
                {retryPreview ? (
                  <>
                    <div className="review-actions">
                      <button
                        className="button quiet"
                        disabled={busy}
                        onClick={() => setRetryPreview(false)}
                      >
                        {ui("返回")}
                      </button>
                      <button
                        className="button primary"
                        disabled={busy || !data?.enabled}
                        onClick={() =>
                          void run(async () => {
                            const r = await api<{ queued: number }>(
                              "ai.retry_failed",
                            );
                            setRetryPreview(false);
                            setNotice(msg("已重新排队 {0} 个任务", r.queued));
                            await load();
                          })
                        }
                      >
                        {ui("确认重试失败任务")}
                      </button>
                    </div>
                  </>
                ) : (
                  <div className="review-actions">
                    <button
                      className="button quiet"
                      disabled={busy || !data?.enabled || !data.summary.failed}
                      onClick={() => setRetryPreview(true)}
                    >
                      {ui("重试失败任务 (")}
                      {data?.summary.failed || 0})
                    </button>
                    <button
                      className="button quiet"
                      disabled={
                        busy || !(data?.summary.queued || data?.summary.running)
                      }
                      onClick={() =>
                        void run(async () => {
                          await api("ai.cancel");
                          setNotice(msg("已取消等待和进行中的任务"));
                          await load();
                        })
                      }
                    >
                      {ui("取消全部识别")}
                    </button>
                    <button
                      className="button quiet"
                      disabled={busy}
                      onClick={onConfigure}
                    >
                      {ui("AI 设置")}
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      </div>
    </Modal>
  );
}
