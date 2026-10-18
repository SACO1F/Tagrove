import { t as ui, tf } from "./i18n";
import {
  useEffect,
  useId,
  useState,
  useLayoutEffect,
  useRef,
  type ReactNode,
  type MouseEventHandler,
} from "react";
import {
  ChevronRight,
  Folder,
  Plus,
  Save,
  Trash2,
  ArrowUp,
  ArrowDown,
} from "lucide-react";
import { api, message, type Tag } from "./api";
import { GlideSelect } from "./GlideSelect";
import { Modal } from "./components";
import "./tag-groups.css";
import { reducedMotion } from "./microMotion";
const animatingGroups = new WeakMap<HTMLElement, number>();
export type TagGroups = {
  version: number;
  groups: { id: string; name: string }[];
  members: Record<string, string>;
};
export const emptyGroups: TagGroups = { version: 0, groups: [], members: {} };
export function groupedTags(tags: Tag[], state: TagGroups = emptyGroups) {
  return [...state.groups, { id: "", name: ui("未分类") }].map((g) => ({
    ...g,
    tags: tags.filter((t) => (state.members[t.id] || "") === g.id),
  }));
}
export function TagBranch({
  name,
  count,
  id,
  closed,
  onToggle,
  children,
  floating = false,
  forcedOpen = false,
  onContextMenu,
  dropTarget = false,
}: {
  name: string;
  count: number;
  id: string;
  closed: boolean;
  onToggle: () => void;
  children: ReactNode;
  floating?: boolean;
  forcedOpen?: boolean;
  onContextMenu?: MouseEventHandler<HTMLButtonElement>;
  dropTarget?: boolean;
}) {
  const content = useId();
  const branch = useRef<HTMLElement>(null);
  const initialized = useRef(false);
  const shut = closed && !forcedOpen;
  useLayoutEffect(() => {
    if (!initialized.current) {
      initialized.current = true;
      return;
    }
    if (!floating || reducedMotion()) return;
    const host = branch.current?.closest<HTMLElement>(".palette-chips");
    if (!host) return;
    animatingGroups.set(host, (animatingGroups.get(host) ?? 0) + 1);
    host.dataset.groupAnimating = "true";
    let active = true;
    const finish = () => {
      if (!active) return;
      active = false;
      const count = Math.max(0, (animatingGroups.get(host) ?? 1) - 1);
      animatingGroups.set(host, count);
      if (!count) delete host.dataset.groupAnimating;
    };
    const timer = setTimeout(finish, 280);
    return () => {
      clearTimeout(timer);
      finish();
    };
  }, [shut, floating]);
  return (
    <section
      ref={branch}
      className={`tag-branch ${floating ? "floating-tag-branch" : ""}`}
      data-sort-group={id || "unclassified"}
      data-closed={shut}
      data-category-drop={dropTarget || undefined}
      aria-label={tf("分类{0}", name)}
    >
      <button
        className="tag-branch-heading"
        aria-expanded={!shut}
        aria-controls={content}
        disabled={forcedOpen}
        onClick={onToggle}
        onContextMenu={onContextMenu}
      >
        <ChevronRight size={14} />
        <Folder size={14} />
        <span>{name}</span>
        <small>{count}</small>
      </button>
      <div
        className="tag-branch-reveal"
        id={content}
        inert={shut}
        aria-hidden={shut}
      >
        <div className="tag-branch-clip">
          <div className="tag-branch-items">{children}</div>
        </div>
      </div>
    </section>
  );
}
export function GroupDisclosure({ children }: { children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const id = useId();
  return (
    <section className="group-manager-disclosure" data-open={open}>
      <button
        className="group-disclosure-toggle"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen(!open)}
      >
        <ChevronRight size={15} />
        <span>{ui("管理分类与标签归属")}</span>
      </button>
      <div
        className="group-disclosure-reveal"
        id={id}
        inert={!open}
        aria-hidden={!open}
      >
        <div className="group-disclosure-clip">{children}</div>
      </div>
    </section>
  );
}
export function GroupManager({
  tags,
  onUpdated,
}: {
  tags: Tag[];
  onUpdated: () => Promise<unknown>;
}) {
  const [state, setState] = useState<TagGroups>(emptyGroups),
    [name, setName] = useState(""),
    [names, setNames] = useState<Record<string, string>>({}),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [remove, setRemove] = useState(""),
    [query, setQuery] = useState("");
  const load = () => api<TagGroups>("tag-groups.get").then(setState);
  useEffect(() => {
    void load().catch((e) => setError(message(e)));
  }, []);
  async function update(payload: Record<string, unknown>) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      const next = await api<TagGroups>("tag-groups.update", {
        ...payload,
        version: state.version,
      });
      setState(next);
      await onUpdated();
      if (payload.op === "create") setName("");
      setRemove("");
    } catch (e) {
      setError(message(e));
      await load().catch(() => {});
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="group-manager">
      <h3>{ui("标签分类")}</h3>
      <p className="muted">
        {ui("分类只整理标签；删除分类后，标签回到未分类。")}
      </p>
      <form
        className="group-create"
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim()) void update({ op: "create", name });
        }}
      >
        <input
          aria-label={ui("新分类名称")}
          placeholder={ui("例如：学习")}
          value={name}
          maxLength={40}
          onChange={(e) => setName(e.target.value)}
          disabled={busy}
        />
        <button className="button" disabled={busy || !name.trim()}>
          <Plus size={14} />
          {ui("创建分类")}
        </button>
      </form>
      <div className="group-manager-list">
        {state.groups.map((g, i) => (
          <div className="group-manager-entry" key={g.id}>
            <div className="group-manager-row">
              <input
                aria-label={tf("分类名称{0}", g.name)}
                value={names[g.id] ?? g.name}
                maxLength={40}
                disabled={busy}
                onChange={(e) => setNames({ ...names, [g.id]: e.target.value })}
              />
              <button
                className="icon-button"
                aria-label={tf("保存分类{0}", g.name)}
                disabled={
                  busy || !names[g.id]?.trim() || names[g.id] === g.name
                }
                onClick={() =>
                  void update({ op: "rename", id: g.id, name: names[g.id] })
                }
              >
                <Save size={14} />
              </button>
              <button
                className="icon-button"
                aria-label={tf("前移分类{0}", g.name)}
                disabled={busy || i === 0}
                onClick={() => void update({ op: "move", id: g.id, delta: -1 })}
              >
                <ArrowUp size={14} />
              </button>
              <button
                className="icon-button"
                aria-label={tf("后移分类{0}", g.name)}
                disabled={busy || i === state.groups.length - 1}
                onClick={() => void update({ op: "move", id: g.id, delta: 1 })}
              >
                <ArrowDown size={14} />
              </button>
              <button
                className="icon-button"
                aria-label={tf("删除分类{0}", g.name)}
                disabled={busy}
                onClick={() => setRemove(g.id)}
              >
                <Trash2 size={14} />
              </button>
            </div>
            {remove === g.id && (
              <div className="group-delete-confirm">
                <span>
                  {ui("删除“")}
                  {g.name}
                  {ui("”？标签将回到未分类。")}
                </span>
                <button
                  className="button"
                  disabled={busy}
                  onClick={() => void update({ op: "delete", id: g.id })}
                >
                  {ui("确认删除分类")}
                </button>
                <button
                  className="button quiet"
                  disabled={busy}
                  onClick={() => setRemove("")}
                >
                  {ui("取消")}
                </button>
              </div>
            )}
          </div>
        ))}
      </div>
      <h4>{ui("标签归属")}</h4>
      <input
        aria-label={ui("搜索待分类标签")}
        placeholder={ui("搜索普通标签")}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <div className="group-assign-list">
        {tags
          .filter((t) => t.createdBy !== "folder" && t.name.includes(query))
          .map((t) => (
            <div className="group-assign-row" key={t.id}>
              <span data-tooltip={t.name}>{t.name}</span>
              <GlideSelect
                ariaLabel={tf("分类{0}", t.name)}
                value={state.members[t.id] || ""}
                disabled={busy}
                options={[
                  { value: "", label: ui("未分类") },
                  ...state.groups.map((g) => ({ value: g.id, label: g.name })),
                ]}
                onChange={(groupId) =>
                  void update({ op: "assign", tagId: t.id, groupId })
                }
              />
            </div>
          ))}
      </div>
      {error && (
        <p className="tag-form-error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
export function GroupAssignDialog({
  tag,
  onClose,
  onUpdated,
}: {
  tag: Tag;
  onClose: () => void;
  onUpdated: () => Promise<unknown>;
}) {
  const [state, setState] = useState<TagGroups>(),
    [group, setGroup] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function load() {
    try {
      const data = await api<TagGroups>("tag-groups.get");
      setState(data);
      setGroup(data.members[tag.id] || "");
    } catch (e) {
      setError(message(e));
    }
  }
  useEffect(() => {
    void load();
  }, [tag.id]);
  return (
    <Modal title={ui("标签分类")} onClose={onClose} dismissDisabled={busy}>
      <div className="modal-content group-assign-dialog">
        <strong>{tag.name}</strong>
        <GlideSelect
          value={group}
          ariaLabel={ui("所属分类")}
          disabled={busy || !state}
          options={[
            { value: "", label: ui("未分类") },
            ...(state?.groups || []).map((g) => ({
              value: g.id,
              label: g.name,
            })),
          ]}
          onChange={setGroup}
        />
        <p className="muted">{ui("仅调整标签归属，文件上的标签保持不变。")}</p>
        <button
          className="button primary"
          disabled={busy || !state}
          onClick={async () => {
            if (!state) return;
            setBusy(true);
            setError("");
            try {
              await api("tag-groups.update", {
                version: state.version,
                op: "assign",
                tagId: tag.id,
                groupId: group,
              });
              await onUpdated();
              onClose();
            } catch (e) {
              setError(message(e));
              await load();
            } finally {
              setBusy(false);
            }
          }}
        >
          {ui("保存分类")}
        </button>
        {error && (
          <p className="tag-form-error" role="alert">
            {error}
          </p>
        )}
      </div>
    </Modal>
  );
}
