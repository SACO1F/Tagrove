import { t as ui, tf } from "./i18n";
import { useEffect, useState } from "react";
import {
  Bookmark,
  GitMerge,
  Plus,
  X,
  Edit3,
  ArrowUp,
  ArrowDown,
  SlidersHorizontal,
} from "lucide-react";
import {
  api,
  message,
  Query,
  SavedCollection,
  Tag,
  kindNames,
  statusNames,
  date,
} from "./api";
import { Modal } from "./components";
import { GlideSelect } from "./GlideSelect";
import "./tag-organize.css";
type MergePreview = {
  source: Tag;
  target: Tag;
  affected: number;
  overlap: number;
  aliasCount: number;
  aliasConflicts: string[];
};
export function MergeTagDialog({
  source,
  tags,
  onClose,
  onMerged,
}: {
  source: Tag;
  tags: Tag[];
  onClose: () => void;
  onMerged: (source: string, target: string) => Promise<void>;
}) {
  const choices = tags.filter(
    (t) => t.id !== source.id && t.createdBy !== "folder",
  );
  const [target, setTarget] = useState(choices[0]?.id || "");
  const [preview, setPreview] = useState<MergePreview>();
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let alive = true;
    setPreview(undefined);
    setError("");
    if (!target) return;
    setLoading(true);
    void api<MergePreview>("tags.merge.preview", { source: source.id, target })
      .then((p) => {
        if (alive) setPreview(p);
      })
      .catch((e) => {
        if (alive) setError(message(e));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [source.id, target, attempt]);
  async function merge() {
    if (!preview || busy) return;
    setBusy(true);
    setError("");
    try {
      await api("tags.merge", { source: source.id, target, preview });
      await onMerged(source.id, target);
      onClose();
    } catch (e) {
      setError(message(e));
      setPreview(undefined);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal title={ui("合并标签")} onClose={onClose} dismissDisabled={busy}>
      <div className="modal-content organize-dialog">
        <div className="merge-flow">
          <span>{source.name}</span>
          <GitMerge size={18} />
          <GlideSelect
            ariaLabel={ui("合并到标签")}
            value={target}
            options={choices.map((t) => ({ value: t.id, label: t.name }))}
            onChange={setTarget}
            disabled={busy}
          />
        </div>
        {loading ? (
          <p role="status">{ui("正在核对关联…")}</p>
        ) : preview ? (
          <div className="merge-summary">
            <strong>
              {preview.affected} {ui("个文件受影响")}
            </strong>
            <span>
              {ui("合并后")}
              {preview.aliasCount} {ui("个别名")}
            </span>
            {preview.aliasConflicts.map((c) => (
              <p className="tag-form-error" role="alert" key={c}>
                {c}
              </p>
            ))}
            <span>
              {preview.overlap} {ui("个文件已有目标标签，将去重")}
            </span>
            <span>
              「{source.name}
              {ui("」及其")}
              {(source.aliases ?? []).length} {ui("个别名会归到「")}
              {preview.target.name}」。
            </span>
          </div>
        ) : !choices.length ? (
          <p>{ui("请先创建另一个普通标签。")}</p>
        ) : null}
        <p className="muted">
          {ui("合并后移除「")}
          {source.name}
          {ui(
            "」，保留目标标签。同步浮窗与保存的筛选；文件、备注和收藏保留。可在工作台撤销。",
          )}
        </p>
        {error && (
          <p role="alert" className="tag-form-error">
            {error}
          </p>
        )}
        <div className="organize-actions">
          <button className="button" disabled={busy} onClick={onClose}>
            {ui("取消")}
          </button>
          {!preview && !!target && !loading && (
            <button
              className="button"
              disabled={busy}
              onClick={() => setAttempt((v) => v + 1)}
            >
              {ui("重新核对")}
            </button>
          )}
          <button
            className="button primary"
            disabled={
              busy || !preview || loading || !!preview?.aliasConflicts.length
            }
            onClick={() => void merge()}
          >
            <GitMerge size={15} />
            {busy ? ui("正在合并…") : ui("确认合并")}
          </button>
        </div>
      </div>
    </Modal>
  );
}
function CollectionSummary({ query: q, tags }: { query: Query; tags: Tag[] }) {
  const tagNames = (ids: string[]) =>
    ids
      .map((id) => tags.find((t) => t.id === id)?.name || ui("已删除标签"))
      .join("、");
  const rows: [string, string][] = [
    [
      ui("文件范围"),
      {
        all: ui("全部文件"),
        inbox: ui("待整理"),
        starred: ui("我的收藏"),
        recent: ui("最近加入"),
        unavailable: ui("需要关注"),
      }[q.scope] || q.scope,
    ],
    [
      ui("搜索范围"),
      q.searchMode === "metadata"
        ? ui("名称、标签与备注")
        : q.searchMode === "content"
          ? ui("文件内容")
          : {
              all: ui("名称、标签、备注与正文"),
              name: ui("仅文件名"),
              body: ui("仅正文"),
            }[q.searchScope || "all"],
    ],
  ];
  if (q.text) rows.push([ui("搜索文字"), q.text]);
  if (q.sourceHost) rows.push([ui("来源网站域名"), q.sourceHost]);
  if (q.captureKind)
    rows.push([
      ui("采集类型"),
      (
        {
          web_page: ui("网页链接"),
          web_excerpt: ui("文字摘录"),
          web_image: ui("网页图片"),
        } as Record<string, string>
      )[q.captureKind] || q.captureKind,
    ]);
  if (q.include.length || q.exclude.length) {
    rows.push([
      ui("标签匹配"),
      q.mode === "any" ? ui("满足任一标签") : ui("满足全部标签"),
    ]);
    if (q.include.length) rows.push([ui("包含标签"), tagNames(q.include)]);
    if (q.exclude.length) rows.push([ui("排除标签"), tagNames(q.exclude)]);
  } else rows.push([ui("标签条件"), ui("不限标签")]);
  if (q.kinds.length)
    rows.push([
      ui("文件类型"),
      q.kinds.map((k) => kindNames[k] || k).join("、"),
    ]);
  if (q.status) rows.push([ui("文件状态"), statusNames[q.status] || q.status]);
  if (q.directory)
    rows.push([
      ui("文件目录"),
      tf(
        "{0}（{1}子文件夹）",
        q.directory,
        q.recursive ? ui("包含") : ui("不含"),
      ),
    ]);
  if (q.from || q.to)
    rows.push([
      ui("时间范围"),
      tf(
        "{0} 至 {1}",
        q.from ? date(q.from) : ui("不限"),
        q.to ? date(q.to) : ui("不限"),
      ),
    ]);
  rows.push([
    ui("排序"),
    tf(
      "{0}{1}",
      {
        relevance: ui("相关度"),
        added: ui("加入时间"),
        modified: ui("修改时间"),
        name: ui("名称"),
        size: ui("文件大小"),
      }[q.sort] || q.sort,
      q.sort === "relevance"
        ? ""
        : tf(" · {0}", q.direction === "asc" ? ui("升序") : ui("降序")),
    ),
  ]);
  return (
    <section className="collection-summary" aria-label={ui("筛选详情")}>
      <h3>{ui("筛选详情")}</h3>
      <dl>
        {rows.map(([label, value]) => (
          <div key={label}>
            <dt>{ui(label)}</dt>
            <dd>
              {[
                ui("搜索文字"),
                ui("包含标签"),
                ui("排除标签"),
                ui("文件目录"),
                ui("时间范围"),
              ].includes(label)
                ? value
                : ui(value)}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

export function CollectionsBar({
  collections,
  query,
  tags,
  showSave,
  onApply,
  onChanged,
}: {
  collections: SavedCollection[];
  query: Query;
  tags: Tag[];
  showSave: boolean;
  onApply: (q: Query, editConditions?: boolean) => void;
  onChanged: () => Promise<unknown>;
}) {
  const [saving, setSaving] = useState(false);
  const [managing, setManaging] = useState(false);
  const [editing, setEditing] = useState<SavedCollection>();
  const [draft, setDraft] = useState<Query>(query);
  const [directoryCheck, setDirectoryCheck] = useState<SavedCollection>();
  const [name, setName] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  function apply(c: SavedCollection) {
    if (
      [...c.query.include, ...c.query.exclude].some(
        (id) => !tags.some((t) => t.id === id),
      )
    ) {
      setError(ui("此集合引用的标签已删除，请重新保存筛选条件。"));
      return;
    }
    setError("");
    if (c.query.directory) {
      setDirectoryCheck(c);
      return;
    }
    onApply({ ...c.query, offset: 0, limit: 100 });
  }
  async function save() {
    if (busy || !name.trim()) return;
    setBusy(true);
    setError("");
    try {
      await api("collections.save", {
        name: name.trim(),
        query: draft,
        ...(editing ? { id: editing.id, expected: editing } : {}),
      });
      await onChanged();
      setSaving(false);
      setName("");
      setEditing(undefined);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function remove(id: string) {
    setBusy(true);
    setError("");
    try {
      await api("collections.delete", { id });
      await onChanged();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function move(index: number, delta: number) {
    setBusy(true);
    setError("");
    const ids = collections.map((c) => c.id);
    [ids[index], ids[index + delta]] = [ids[index + delta], ids[index]];
    try {
      await api("collections.reorder", { ids, expected: collections });
      await onChanged();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  function edit(c: SavedCollection) {
    setEditing(c);
    setName(c.name);
    setDraft({ ...c.query });
    setManaging(false);
    setSaving(true);
    setError("");
  }
  return (
    <>
      {(collections.length > 0 || showSave) && (
        <div className="collections-bar" aria-label={ui("保存的筛选集合")}>
          {!!collections.length && (
            <button
              className="button quiet"
              onClick={() => {
                setManaging(true);
                setError("");
              }}
            >
              <SlidersHorizontal size={13} />
              {ui("管理集合")}
            </button>
          )}
          <div className="collection-items">
            {collections.map((c) => (
              <span className="collection-chip" key={c.id}>
                <button
                  disabled={busy}
                  onClick={() => apply(c)}
                  aria-label={tf("应用筛选{0}", c.name)}
                >
                  <Bookmark size={13} />
                  {c.name}
                </button>
                <button
                  disabled={busy}
                  aria-label={tf("删除筛选{0}", c.name)}
                  onClick={() => void remove(c.id)}
                >
                  <X size={12} />
                </button>
              </span>
            ))}
            {showSave && (
              <button
                className="button"
                onClick={() => {
                  setEditing(undefined);
                  setDraft({ ...query });
                  setName("");
                  setSaving(true);
                  setError("");
                }}
              >
                <Plus size={13} />
                {ui("保存筛选")}
              </button>
            )}
          </div>
        </div>
      )}
      {directoryCheck && (
        <Modal
          title={ui("核对集合目录")}
          onClose={() => setDirectoryCheck(undefined)}
        >
          <div className="modal-content organize-dialog">
            <p>
              「{directoryCheck.name}
              {ui("」会在以下本机目录查找文件，请确认路径仍然适用。")}
            </p>
            <code className="collection-path">
              {directoryCheck.query.directory}
            </code>
            <div className="organize-actions">
              <button
                className="button"
                onClick={() => {
                  onApply(
                    { ...directoryCheck.query, offset: 0, limit: 100 },
                    true,
                  );
                  setDirectoryCheck(undefined);
                }}
              >
                {ui("修改条件")}
              </button>
              <button
                className="button primary"
                onClick={() => {
                  onApply({ ...directoryCheck.query, offset: 0, limit: 100 });
                  setDirectoryCheck(undefined);
                }}
              >
                {ui("应用筛选")}
              </button>
            </div>
          </div>
        </Modal>
      )}
      {managing && (
        <Modal
          title={ui("管理筛选集合")}
          onClose={() => setManaging(false)}
          dismissDisabled={busy}
        >
          <div className="modal-content organize-dialog collection-manager">
            {collections.map((c, index) => (
              <div className="collection-manager-row" key={c.id}>
                <span>{c.name}</span>
                <button
                  className="icon-button"
                  aria-label={tf("编辑集合{0}", c.name)}
                  disabled={busy}
                  onClick={() => edit(c)}
                >
                  <Edit3 size={15} />
                </button>
                <button
                  className="icon-button"
                  aria-label={tf("上移集合{0}", c.name)}
                  disabled={busy || index === 0}
                  onClick={() => void move(index, -1)}
                >
                  <ArrowUp size={15} />
                </button>
                <button
                  className="icon-button"
                  aria-label={tf("下移集合{0}", c.name)}
                  disabled={busy || index === collections.length - 1}
                  onClick={() => void move(index, 1)}
                >
                  <ArrowDown size={15} />
                </button>
                <button
                  className="icon-button"
                  aria-label={tf("删除集合{0}", c.name)}
                  disabled={busy}
                  onClick={() => void remove(c.id)}
                >
                  <X size={15} />
                </button>
              </div>
            ))}
            {!collections.length && <p className="muted">{ui("暂无集合")}</p>}
            {error && (
              <p role="alert" className="tag-form-error">
                {error}
              </p>
            )}
          </div>
        </Modal>
      )}
      {error && !saving && !managing && (
        <p className="collection-error" role="alert">
          {error}
        </p>
      )}
      {saving && (
        <Modal
          title={editing ? ui("编辑筛选集合") : ui("保存筛选集合")}
          onClose={() => setSaving(false)}
          dismissDisabled={busy}
        >
          <form
            className="modal-content organize-dialog"
            onSubmit={(e) => {
              e.preventDefault();
              void save();
            }}
          >
            <input
              autoFocus
              aria-label={ui("筛选集合名称")}
              placeholder={ui("例如：待处理的设计资料")}
              value={name}
              onChange={(e) => setName(e.target.value)}
              disabled={busy}
              onKeyDown={(e) => {
                if (e.key === "Enter" && e.nativeEvent.isComposing)
                  e.preventDefault();
              }}
            />
            <CollectionSummary query={draft} tags={tags} />
            {error && (
              <p role="alert" className="tag-form-error">
                {error}
              </p>
            )}
            <div className="organize-actions">
              <button
                type="button"
                className="button"
                onClick={() => setSaving(false)}
                disabled={busy}
              >
                {ui("取消")}
              </button>
              <button
                className="button primary"
                disabled={busy || !name.trim()}
              >
                {ui("保存集合")}
              </button>
            </div>
          </form>
        </Modal>
      )}
    </>
  );
}
