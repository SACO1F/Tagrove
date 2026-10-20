import { applyAppearance } from "./appearance";
import { useLanguage } from "./i18n";
import { t as ui, tf, msg, type UiCopy } from "./i18n";
import {
  TagBranch,
  GroupManager,
  GroupAssignDialog,
  groupedTags,
  emptyGroups,
} from "./TagGroups";
import { useFloatingSort } from "./useFloatingSort";
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowUpRight,
  Check,
  ChevronDown,
  ChevronUp,
  Download,
  Edit3,
  Trash2,
  ImagePlus,
  LoaderCircle,
  Plus,
  Palette,
  FolderOpen,
  Pin,
  PinOff,
  Tag as TagIcon,
  X,
} from "lucide-react";
import { api, Bootstrap, message, Tag, tagSearch } from "./api";
import { TagAliases } from "./TagAliases";
import { Modal } from "./components";
import { ExportTagDialog } from "./ExportTagDialog";
import { FloatingResizeHandles } from "./FloatingResizeHandles";
import { useFloatingHistory } from "./useFloatingHistory";
import "./floating.css";
import "./tag-organize.css";
import {
  tagTone,
  tagStyle,
  configureTagColors,
  tagCustomColor,
} from "./tagColors";
import { useTagArrival, reducedMotion } from "./microMotion";
import { FloatingHistory } from "./FloatingHistory";
import { TaskGlyph } from "./TaskStatus";

type Presets = { ids: string[]; tags: Tag[] };
type WindowOptions = { collapsed: boolean; alwaysOnTop: boolean };
type Annotation = {
  tagName: string;
  applied: number;
  existing: number;
  imported: number;
  names: string[];
  undoId?: string;
};

export default function Floating() {
  useLanguage();
  const [presets, setPresets] = useState<Presets>({ ids: [], tags: [] });
  const ref = useRef(presets);
  const paletteScroll = useRef<HTMLDivElement>(null);
  ref.current = presets;
  const [boot, setBoot] = useState<Bootstrap>();
  useEffect(() => {
    if (boot) applyAppearance(boot.settings);
  }, [
    boot?.settings.language,
    boot?.settings.accentColor,
    boot?.settings.uiFontSize,
  ]);
  const {
    operations,
    record,
    markUndone,
    ready: historyReady,
  } = useFloatingHistory(boot?.dataPath);
  useTagArrival(
    paletteScroll,
    boot?.dataPath,
    presets.tags.filter((tag) => presets.ids.includes(tag.id)),
  );
  const [menuClosing, setMenuClosing] = useState(false);
  const menuTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const menuTrigger = useRef<HTMLButtonElement>(null);
  useEffect(() => () => clearTimeout(menuTimer.current), []);
  const [tagQuery, setTagQuery] = useState("");
  const [selected, setSelected] = useState("");
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<{
    tag: Tag;
    mode: "rename" | "delete";
  }>();
  const [groupTag, setGroupTag] = useState<Tag>();
  const [managingGroups, setManagingGroups] = useState(false);
  const [closedGroups, setClosedGroups] = useState<string[]>([]);
  const [groupMenu, setGroupMenu] = useState<{
    id: string;
    name: string;
    version: number;
    x: number;
    y: number;
  }>();
  const [groupRename, setGroupRename] = useState<typeof groupMenu>();
  const [groupName, setGroupName] = useState("");
  const [groupError, setGroupError] = useState("");
  const groupTrigger = useRef<HTMLButtonElement | null>(null);
  const groupMenuAction = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (groupMenu) groupMenuAction.current?.focus();
  }, [groupMenu]);
  const [colorTag, setColorTag] = useState<Tag>();
  const [color, setColor] = useState("#71a58b");
  const [menu, setMenu] = useState<{ tag: Tag; x: number; y: number }>();
  const menuSurface = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (menu && !menuClosing)
      menuSurface.current
        ?.querySelector<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')
        ?.focus();
  }, [menu, menuClosing]);
  const [exportTag, setExportTag] = useState<Tag>();
  function highlightMenu(event: React.SyntheticEvent<HTMLDivElement>) {
    const item = (event.target as HTMLElement).closest<HTMLElement>(
      '[role="menuitem"]',
    );
    if (!item || (item as HTMLButtonElement).disabled) return;
    const highlight =
      event.currentTarget.querySelector<HTMLElement>(".menu-glide");
    if (highlight)
      Object.assign(highlight.style, {
        transform: `translateY(${item.offsetTop}px)`,
        height: `${item.offsetHeight}px`,
        opacity: "1",
      });
  }
  function dismissMenu() {
    if (menuTimer.current) return;
    const finish = () => {
      setMenu(undefined);
      setMenuClosing(false);
      menuTrigger.current?.focus();
      menuTimer.current = undefined;
    };
    if (reducedMotion()) {
      finish();
      return;
    }
    setMenuClosing(true);
    menuTimer.current = setTimeout(finish, 140);
  }
  const actionLock = useRef(false);
  const [collapsed, setCollapsed] = useState(false);
  const [alwaysOnTop, setAlwaysOnTop] = useState(true);
  const [busy, setBusy] = useState(false);
  const [dragTag, setDragTag] = useState("");
  const [dropTag, setDropTag] = useState("");
  const [incoming, setIncoming] = useState(false);
  const [name, setName] = useState("");
  const [status, setStatus] = useState<{ text: UiCopy; error: boolean }>({
    text: msg("就绪"),
    error: false,
  });
  const gesture = useRef<
    { id: string; x: number; y: number; started: boolean } | undefined
  >(undefined);
  const notify = useCallback(
    (text: UiCopy, error = false, log = false, undoId?: string) => {
      setStatus({ text, error });
      if (log) record(ui(text), undoId);
    },
    [record],
  );
  const refresh = useCallback(async () => {
    const [p, b, options] = await Promise.all([
      api<Presets>("floating.presets"),
      api<Bootstrap>("bootstrap"),
      api<WindowOptions>("floating.state"),
    ]);
    configureTagColors(b.settings.tagColors);
    setPresets(p);
    setBoot(b);
    setCollapsed(options.collapsed);
    setAlwaysOnTop(options.alwaysOnTop);
    setSelected((old) => (p.ids.includes(old) ? old : p.ids[0] || ""));
  }, []);
  const run = useCallback(
    async (task: () => Promise<unknown>) => {
      if (actionLock.current) return;
      actionLock.current = true;
      setBusy(true);
      try {
        await task();
      } catch (e) {
        notify(message(e), true);
      } finally {
        actionLock.current = false;
        setBusy(false);
      }
    },
    [notify],
  );
  const apply = useCallback(
    (tagId: string, paths: string[]) =>
      run(() => api("annotation.apply", { tagId, paths })),
    [run],
  );
  useEffect(() => {
    void refresh().catch((e) => notify(message(e), true));
    const win = getCurrentWebviewWindow();
    const subscriptions = [
      listen<{ visible: boolean }>("floating-visibility", ({ payload }) => {
        if (payload.visible) setClosedGroups([]);
      }),
      listen(
        "library-changed",
        () => void refresh().catch((e) => notify(message(e), true)),
      ),
      listen("tag-drag-end", () => {
        setDragTag("");
        gesture.current = undefined;
      }),
      listen<{ ok: boolean; result?: Annotation; error?: string }>(
        "annotation-result",
        ({ payload: p }) => {
          setDragTag("");
          if (!p.ok) {
            notify(message(p.error), true);
            return;
          }
          const r = p.result!;
          const target =
            r.names.length === 1
              ? r.names[0]
              : tf("{0} 个文件", r.names.length);
          notify(
            r.applied
              ? msg(
                  "已为 {0} 添加「{1}」{2}",
                  target,
                  r.tagName,
                  r.imported ? msg("，并加入文件库") : "",
                )
              : msg("{0} 已有「{1}」", target, r.tagName),
            false,
            true,
            r.undoId,
          );
        },
      ),
      win.onCloseRequested((e) => {
        e.preventDefault();
        void api("floating.close");
      }),
      win.onResized(() => setMenu(undefined)),
      win.onDragDropEvent(async ({ payload: p }) => {
        if (p.type === "leave") {
          setIncoming(false);
          setDropTag("");
          return;
        }
        setIncoming(p.type !== "drop");
        const scale = await win.scaleFactor();
        const el = document
          .elementFromPoint(p.position.x / scale, p.position.y / scale)
          ?.closest<HTMLElement>("[data-preset-id]");
        const id = el?.dataset.presetId || "";
        setDropTag(p.type === "drop" ? "" : id);
        if (p.type === "drop") {
          if (id) void apply(id, p.paths);
          else notify(msg("请将文件放到标签上"), true);
        }
      }),
    ];
    return () => {
      subscriptions.forEach((p) => p.then((fn) => fn()));
    };
  }, [refresh, notify, apply]);
  useEffect(() => {
    document.documentElement.classList.add("floating-window");
    const media = matchMedia("(prefers-color-scheme: dark)");
    const update = () => {
      const effective =
        !boot?.settings.theme || boot.settings.theme === "system"
          ? media.matches
            ? "dark"
            : "light"
          : boot.settings.theme;
      document.documentElement.dataset.theme = effective;
      void api("theme.sync", { theme: effective }).catch((error) =>
        notify(message(error), true),
      );
    };
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, [boot?.settings.theme]);
  const save = async (ids: string[]) => {
    await api("floating.save", { ids });
    await refresh();
  };
  const pin = (tag: Tag) =>
    void run(async () => {
      await save([...new Set([...ref.current.ids, tag.id])]);
      setAdding(false);
      setName("");
      notify(msg("已添加「{0}」", tag.name), false, true);
    });
  const create = () =>
    void run(async () => {
      if (!name.trim()) return;
      const tag = await api<Tag>("tag.create", { name: name.trim() });
      await save([...new Set([...ref.current.ids, tag.id])]);
      setName("");
      setAdding(false);
      notify(msg("已添加「{0}」", tag.name), false, true);
    });
  const changeTag = () =>
    void run(async () => {
      if (!editing || (editing.mode === "rename" && !name.trim())) return;
      const result = await api<{ undoId?: string }>(
        editing.mode === "rename" ? "tags.rename" : "tags.delete",
        {
          id: editing.tag.id,
          version: editing.tag.version,
          name: name.trim(),
        },
      );
      setEditing(undefined);
      await refresh();
      notify(
        editing.mode === "rename"
          ? msg("已将「{0}」改为「{1}」", editing.tag.name, name.trim())
          : msg("已删除「{0}」", editing.tag.name),
        false,
        true,
        result.undoId,
      );
    });
  const choose = () =>
    void run(async () => {
      const paths = await open({
        multiple: true,
        directory: false,
        title: ui("选择需要标注的文件"),
      });
      if (paths)
        await api("annotation.apply", {
          tagId: selected,
          paths: Array.isArray(paths) ? paths : [paths],
        });
    });
  const resize = () =>
    void run(async () => {
      setGroupMenu(undefined);
      const next = await api<WindowOptions>("floating.resize", {
        collapsed: !collapsed,
      });
      setCollapsed(next.collapsed);
    });
  const groupIds = (id: string) => {
    const members = boot?.settings.tagGroups?.members ?? {};
    const group = members[id] ?? "";
    return ref.current.ids.filter((x) => (members[x] ?? "") === group);
  };
  const moveTag = (id: string, delta: number) =>
    void run(async () => {
      const ids = [...ref.current.ids];
      const grouped = groupIds(id);
      const index = grouped.indexOf(id),
        next = index + delta;
      if (index < 0 || next < 0 || next >= grouped.length) return;
      const from = ids.indexOf(id),
        to = ids.indexOf(grouped[next]);
      [ids[from], ids[to]] = [ids[to], ids[from]];
      await save(ids);
      notify(msg("标签顺序已保存"));
    });
  const sorting = useFloatingSort(
    paletteScroll,
    presets.ids,
    !busy && !tagQuery && !collapsed,
    async (ids) => {
      await run(async () => {
        await save(ids);
        notify(msg("标签顺序已保存"));
      });
    },
    async (tagId, groupId) => {
      await run(async () => {
        const state = boot?.settings.tagGroups ?? emptyGroups;
        await api("tag-groups.update", {
          op: "assign",
          tagId,
          groupId,
          version: state.version,
        });
        await refresh();
        setClosedGroups((old) => old.filter((id) => id !== groupId));
        const tag = presets.tags.find((t) => t.id === tagId);
        const category = state.groups.find((g) => g.id === groupId);
        notify(
          msg(
            "已将「{0}」移到分类「{1}」",
            tag?.name ?? "",
            category?.name ?? ui("未分类"),
          ),
        );
      });
    },
  );
  const pinned = sorting.ids
    .map((id) => presets.tags.find((t) => t.id === id))
    .filter((t): t is Tag => !!t);
  const visiblePinned = pinned.filter((t) => tagSearch(t, tagQuery));
  return (
    <div
      className={`floating-shell ${collapsed ? "is-collapsed" : ""} ${incoming ? "receiving-files" : ""}`}
      onKeyDown={(e) => {
        if (
          e.key === "Escape" &&
          !adding &&
          !editing &&
          !exportTag &&
          !groupRename
        ) {
          if (groupMenu) {
            setGroupMenu(undefined);
            groupTrigger.current?.focus();
            return;
          }
          void api("floating.cancel");
          gesture.current = undefined;
          setDragTag("");
          if (menu) dismissMenu();
        }
      }}
    >
      {!collapsed && (
        <FloatingResizeHandles
          onError={(error) => notify(message(error), true)}
        />
      )}
      <header className="floating-titlebar">
        <div className="floating-handle" data-tauri-drag-region>
          <img src="/icon.png" alt="" draggable={false} />
          <div data-tauri-drag-region>
            <strong data-tauri-drag-region>{ui("拾签")}</strong>
          </div>
        </div>
        <button
          className={`float-icon float-pin ${alwaysOnTop ? "is-pinned" : ""}`}
          data-tooltip={alwaysOnTop ? ui("取消置顶") : ui("置顶浮窗")}
          aria-label={alwaysOnTop ? ui("取消置顶") : ui("置顶浮窗")}
          aria-pressed={alwaysOnTop}
          disabled={busy}
          onClick={() =>
            void run(async () => {
              await api("floating.topmost", { value: !alwaysOnTop });
              setAlwaysOnTop(!alwaysOnTop);
            })
          }
        >
          {alwaysOnTop ? <Pin size={15} /> : <PinOff size={15} />}
        </button>
        <button
          className="float-icon"
          data-tooltip={ui("打开工作台")}
          aria-label={ui("打开工作台")}
          onClick={() => void run(() => api("main.show"))}
        >
          <ArrowUpRight size={17} />
        </button>
        <button
          className="float-icon"
          data-tooltip={collapsed ? ui("展开标签浮窗") : ui("收起标签浮窗")}
          aria-label={collapsed ? ui("展开标签浮窗") : ui("收起标签浮窗")}
          disabled={busy}
          onClick={resize}
        >
          {collapsed ? <ChevronDown size={17} /> : <ChevronUp size={17} />}
        </button>
        <button
          className="float-icon"
          data-tooltip={ui("关闭浮窗")}
          aria-label={ui("关闭浮窗")}
          disabled={busy}
          onClick={() => void run(() => api("floating.close"))}
        >
          <X size={16} />
        </button>
      </header>
      {!collapsed && (
        <>
          <div className="floating-intro">
            <h1>{ui("全部标签")}</h1>
            <button
              className="float-icon"
              aria-label={ui("管理标签分类")}
              data-tooltip={ui("管理分类")}
              onClick={() => setManagingGroups(true)}
            >
              <FolderOpen size={15} />
            </button>
          </div>
          <div className="floating-search">
            <input
              aria-label={ui("查找标签")}
              placeholder={ui("查找标签")}
              value={tagQuery}
              onChange={(e) => setTagQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") {
                  e.preventDefault();
                  paletteScroll.current
                    ?.querySelector<HTMLButtonElement>(
                      '.tag-branch:not([data-closed="true"]) .palette-tag',
                    )
                    ?.focus();
                }
              }}
            />
            {tagQuery && (
              <button
                className="float-icon"
                aria-label={ui("清除标签搜索")}
                onClick={() => setTagQuery("")}
              >
                <X size={14} />
              </button>
            )}
          </div>
          <div className="palette-stage">
            <div
              className="palette-chips has-groups"
              aria-label={ui("全部标签")}
              ref={paletteScroll}
            >
              {groupedTags(
                visiblePinned,
                boot?.settings.tagGroups ?? emptyGroups,
              )
                .filter((g) => !tagQuery || g.tags.length > 0)
                .map((category) => (
                  <TagBranch
                    key={category.id}
                    id={category.id}
                    name={category.id ? category.name : ui(category.name)}
                    count={category.tags.length}
                    floating
                    dropTarget={
                      sorting.targetGroup === (category.id || "unclassified")
                    }
                    forcedOpen={!!tagQuery}
                    closed={closedGroups.includes(category.id)}
                    onContextMenu={
                      category.id
                        ? (event) => {
                            event.preventDefault();
                            groupTrigger.current = event.currentTarget;
                            setMenu(undefined);
                            setGroupMenu({
                              id: category.id,
                              name: category.name,
                              version: boot?.settings.tagGroups?.version ?? 0,
                              x: Math.max(
                                8,
                                Math.min(event.clientX, innerWidth - 192),
                              ),
                              y: Math.max(
                                66,
                                Math.min(event.clientY, innerHeight - 64),
                              ),
                            });
                          }
                        : undefined
                    }
                    onToggle={() => {
                      setClosedGroups((old) =>
                        old.includes(category.id)
                          ? old.filter((id) => id !== category.id)
                          : [...old, category.id],
                      );
                    }}
                  >
                    {category.tags.map((tag) => (
                      <div
                        key={tag.id}
                        data-preset-id={tag.id}
                        data-motion-tag={tag.id}
                        style={tagStyle(tag.id)}
                        className={`palette-chip tag-color tone-${tagTone(tag.id)} ${selected === tag.id ? "chosen" : ""} ${dropTag === tag.id ? "drop-active" : ""} ${dragTag === tag.id ? "drag-active" : ""} ${sorting.ghost?.id === tag.id ? "sort-placeholder" : ""}`}
                      >
                        <button
                          className="palette-tag"
                          aria-label={tf("标签：{0}", tag.name)}
                          aria-pressed={selected === tag.id}
                          data-tooltip={tf("拖动「{0}」到文件", tag.name)}
                          disabled={busy}
                          onClick={() => setSelected(tag.id)}
                          onKeyDown={(e) => {
                            if (
                              e.altKey &&
                              ["ArrowUp", "ArrowDown"].includes(e.key)
                            ) {
                              e.preventDefault();
                              e.stopPropagation();
                              moveTag(tag.id, e.key === "ArrowUp" ? -1 : 1);
                            }
                          }}
                          onPointerDown={(e) => {
                            if (e.button !== 0) return;
                            e.currentTarget.setPointerCapture(e.pointerId);
                            gesture.current = {
                              id: tag.id,
                              x: e.clientX,
                              y: e.clientY,
                              started: false,
                            };
                          }}
                          onPointerMove={(e) => {
                            const g = gesture.current;
                            if (
                              !g ||
                              g.started ||
                              Math.hypot(e.clientX - g.x, e.clientY - g.y) < 5
                            )
                              return;
                            g.started = true;
                            setDragTag(tag.id);
                            setSelected(tag.id);
                            void api("floating.drag", {
                              tagId: tag.id,
                              accent: getComputedStyle(document.documentElement)
                                .getPropertyValue("--accent")
                                .trim(),
                            }).catch((error) => {
                              setDragTag("");
                              notify(message(error), true);
                            });
                          }}
                          onPointerUp={() => {
                            if (!gesture.current?.started)
                              gesture.current = undefined;
                          }}
                          onLostPointerCapture={() => {
                            if (!gesture.current?.started)
                              gesture.current = undefined;
                          }}
                        >
                          <TagIcon size={14} />
                          <span>{tag.name}</span>
                        </button>
                        <button
                          className="tag-more"
                          aria-label={tf("管理标签{0}", tag.name)}
                          data-tooltip={
                            tagQuery
                              ? ui("点击管理标签；清空搜索后可拖动排序")
                              : ui("点击管理 · 拖动排序或移到分类")
                          }
                          aria-haspopup="menu"
                          aria-expanded={menu?.tag.id === tag.id}
                          disabled={busy}
                          onPointerDown={(e) => sorting.down(e, tag.id)}
                          onClick={(e) => {
                            if (sorting.consumeClick() && e.detail > 0) {
                              e.preventDefault();
                              return;
                            }
                            e.stopPropagation();
                            clearTimeout(menuTimer.current);
                            menuTimer.current = undefined;
                            setMenuClosing(false);
                            menuTrigger.current = e.currentTarget;
                            const r = e.currentTarget.getBoundingClientRect();
                            setMenu({
                              tag,
                              x: Math.min(r.right - 184, innerWidth - 196),
                              y: Math.min(r.bottom + 5, innerHeight - 350),
                            });
                          }}
                        >
                          <svg
                            width="15"
                            height="15"
                            viewBox="0 0 15 15"
                            aria-hidden="true"
                          >
                            {Array.from({ length: 9 }, (_, i) => (
                              <circle
                                key={i}
                                cx={3 + (i % 3) * 4.5}
                                cy={3 + Math.floor(i / 3) * 4.5}
                                r="1"
                                fill="currentColor"
                              />
                            ))}
                          </svg>
                        </button>
                      </div>
                    ))}
                  </TagBranch>
                ))}
              {!pinned.length && (
                <p className="floating-empty">{ui("添加常用标签，开始标注")}</p>
              )}
            </div>
            <button
              className="glass-add"
              aria-label={ui("添加标签")}
              disabled={busy}
              onWheel={(e) => {
                const list = paletteScroll.current;
                if (list)
                  list.scrollBy(
                    0,
                    e.deltaY *
                      (e.deltaMode === 1
                        ? 16
                        : e.deltaMode === 2
                          ? list.clientHeight
                          : 1),
                  );
              }}
              onClick={() => {
                setName("");
                setAdding(true);
                notify(msg("就绪"));
              }}
            >
              <Plus size={15} />
              {ui("添加标签")}
            </button>
          </div>
          <div className="floating-assist">
            <button
              disabled={!selected || busy}
              onClick={choose}
              data-tooltip={presets.tags.find((t) => t.id === selected)?.name}
            >
              <ImagePlus size={15} />
              {ui("选择文件标注")}
            </button>
          </div>
          {(busy || dragTag || incoming || status.error) && (
            <div
              className={`floating-status ${status.error && !dragTag ? "has-error" : ""}`}
              role="status"
            >
              {busy ? (
                <TaskGlyph status="working" />
              ) : dragTag ? (
                <TagIcon size={15} />
              ) : (
                <span className={`status-dot ${status.error ? "error" : ""}`} />
              )}
              <span>
                {busy && !dragTag
                  ? ui("正在处理…")
                  : dragTag
                    ? ui("移到文件上松手 · Esc 取消")
                    : incoming
                      ? ui("放到标签上完成标注")
                      : ui(status.text)}
              </span>
            </div>
          )}
          <FloatingHistory
            operations={operations}
            ready={historyReady}
            undoId={boot?.undoId}
            busy={busy}
            onUndo={(operation) =>
              void run(async () => {
                await api("undo", { expectedId: operation.undoId });
                markUndone(operation.id);
                await refresh();
                notify(msg("已撤销此操作"));
              })
            }
          />
        </>
      )}
      {menu && (
        <>
          <button
            className="float-menu-dismiss"
            aria-label={ui("关闭标签菜单")}
            onClick={dismissMenu}
          />
          <div
            ref={menuSurface}
            className={`float-tag-menu ${menuClosing ? "is-closing" : ""}`}
            inert={menuClosing}
            aria-hidden={menuClosing || undefined}
            role="menu"
            aria-label={tf("标签{0}的操作", menu.tag.name)}
            style={{ left: Math.max(12, menu.x), top: Math.max(66, menu.y) }}
            onPointerOver={highlightMenu}
            onFocusCapture={highlightMenu}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.stopPropagation();
                dismissMenu();
              }
              if (["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) {
                e.preventDefault();
                const items = [
                  ...e.currentTarget.querySelectorAll<HTMLButtonElement>(
                    '[role="menuitem"]:not(:disabled)',
                  ),
                ];
                const index = items.indexOf(
                  document.activeElement as HTMLButtonElement,
                );
                const next =
                  e.key === "Home"
                    ? 0
                    : e.key === "End"
                      ? items.length - 1
                      : (index +
                          (e.key === "ArrowDown" ? 1 : -1) +
                          items.length) %
                        items.length;
                items[next]?.focus();
              }
            }}
          >
            <span className="menu-glide" aria-hidden="true" />
            <strong>{menu.tag.name}</strong>
            <button
              role="menuitem"
              disabled={
                busy || groupIds(menu.tag.id).indexOf(menu.tag.id) === 0
              }
              onClick={() => {
                moveTag(menu.tag.id, -1);
                dismissMenu();
              }}
            >
              <ChevronUp size={14} />
              {ui("前移")}
            </button>
            <button
              role="menuitem"
              disabled={
                busy ||
                groupIds(menu.tag.id).indexOf(menu.tag.id) ===
                  groupIds(menu.tag.id).length - 1
              }
              onClick={() => {
                moveTag(menu.tag.id, 1);
                dismissMenu();
              }}
            >
              <ChevronDown size={14} />
              {ui("后移")}
            </button>
            <button
              role="menuitem"
              autoFocus
              onClick={() => {
                menuTrigger.current?.focus();
                setEditing({ tag: menu.tag, mode: "rename" });
                setName(menu.tag.name);
                setMenu(undefined);
                notify(msg("就绪"));
              }}
            >
              <Edit3 size={14} />
              {ui("修改名称")}
            </button>
            <button
              role="menuitem"
              onClick={() => {
                setColorTag(menu.tag);
                setColor(tagCustomColor(menu.tag.id) || "#71a58b");
                setMenu(undefined);
              }}
            >
              <Palette size={14} />
              {ui("设置颜色")}
            </button>
            <button
              role="menuitem"
              onClick={() => {
                setGroupTag(menu.tag);
                setMenu(undefined);
              }}
            >
              <FolderOpen size={14} />
              {ui("移动到分类")}
            </button>
            <button
              role="menuitem"
              disabled={menu.tag.count === 0}
              onClick={() => {
                menuTrigger.current?.focus();
                setExportTag(menu.tag);
                setMenu(undefined);
              }}
            >
              <Download size={14} />
              {ui("导出标签文件")}
            </button>
            <button
              role="menuitem"
              className="danger"
              onClick={() => {
                menuTrigger.current?.focus();
                setEditing({ tag: menu.tag, mode: "delete" });
                setMenu(undefined);
                notify(msg("就绪"));
              }}
            >
              <Trash2 size={14} />
              {ui("删除标签")}
            </button>
          </div>
        </>
      )}
      {adding && (
        <Modal
          title={ui("添加标签")}
          dismissDisabled={busy}
          onClose={() => {
            if (!busy) setAdding(false);
          }}
        >
          <div className="modal-content float-editor">
            <form
              onSubmit={(e) => {
                e.preventDefault();
                create();
              }}
            >
              <input
                autoFocus
                aria-label={ui("标签名称")}
                placeholder={ui("搜索或新建标签")}
                value={name}
                disabled={busy}
                onChange={(e) => setName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && e.nativeEvent.isComposing)
                    e.preventDefault();
                }}
              />
              <button
                className="button primary"
                disabled={busy || !name.trim()}
              >
                <Plus size={15} />
                {ui("添加")}
              </button>
            </form>
            <div className="float-existing-tags">
              {presets.tags
                .filter(
                  (t) => !presets.ids.includes(t.id) && tagSearch(t, name),
                )
                .map((t) => (
                  <button
                    style={tagStyle(t.id)}
                    className={`tag-color tone-${tagTone(t.id)}`}
                    key={t.id}
                    disabled={busy}
                    onClick={() => pin(t)}
                  >
                    <TagIcon size={14} />
                    {t.name}
                    <Plus size={13} />
                  </button>
                ))}
            </div>
            {status.error && (
              <p role="alert" className="tag-form-error">
                {ui(status.text)}
              </p>
            )}
          </div>
        </Modal>
      )}
      {groupMenu && (
        <>
          <button
            className="float-menu-dismiss"
            aria-label={ui("关闭分类菜单")}
            onClick={() => {
              setGroupMenu(undefined);
              groupTrigger.current?.focus();
            }}
          />
          <div
            className="float-tag-menu"
            role="menu"
            aria-label={ui("分类操作")}
            style={{ left: groupMenu.x, top: groupMenu.y }}
          >
            <button
              ref={groupMenuAction}
              role="menuitem"
              disabled={busy}
              onClick={() => {
                setGroupRename(groupMenu);
                setGroupName(groupMenu.name);
                setGroupError("");
                setGroupMenu(undefined);
              }}
            >
              <Edit3 size={14} />
              {ui("重命名分类")}
            </button>
          </div>
        </>
      )}
      {groupRename && (
        <Modal
          title={ui("重命名分类")}
          onClose={() => {
            if (!busy) setGroupRename(undefined);
          }}
        >
          <form
            className="modal-content floating-group-rename"
            onSubmit={(event) => {
              event.preventDefault();
              void run(async () => {
                setGroupError("");
                try {
                  await api("tag-groups.update", {
                    op: "rename",
                    id: groupRename.id,
                    version: groupRename.version,
                    name: groupName.trim(),
                  });
                  setGroupRename(undefined);
                  await refresh();
                  notify(msg("分类名称已更新"), false, true);
                  groupTrigger.current?.focus();
                } catch (error) {
                  setGroupError(message(error));
                }
              });
            }}
          >
            <label>
              {ui("分类名称")}
              <input
                autoFocus
                aria-label={ui("分类名称")}
                value={groupName}
                maxLength={40}
                disabled={busy}
                onChange={(event) => setGroupName(event.target.value)}
              />
            </label>
            {groupError && (
              <p role="alert" className="tag-form-error">
                {groupError}
              </p>
            )}
            <div className="dialog-actions">
              <button
                type="button"
                className="button quiet"
                disabled={busy}
                onClick={() => setGroupRename(undefined)}
              >
                {ui("取消")}
              </button>
              <button
                className="button primary"
                disabled={busy || !groupName.trim()}
              >
                {ui("保存")}
              </button>
            </div>
          </form>
        </Modal>
      )}
      {editing?.mode === "rename" && (
        <TagAliases
          tag={editing.tag}
          onClose={() => setEditing(undefined)}
          onSaved={async (undoId, name) => {
            await refresh();
            notify(
              msg("已将「{0}」更新为「{1}」，别名同步", editing.tag.name, name),
              false,
              true,
              undoId,
            );
          }}
        />
      )}
      {editing?.mode === "delete" && (
        <Modal
          title={ui("删除标签")}
          dismissDisabled={busy}
          onClose={() => {
            if (!busy) setEditing(undefined);
          }}
        >
          <div className="modal-content">
            <p>
              {ui("删除「")}
              {editing.tag.name}
              {ui("」及其")}
              {editing.tag.count} {ui("个文件关联、")}
              {editing.tag.aliases?.length ?? 0} {ui("个别名？原文件保留。")}
            </p>
            <button
              className="button primary"
              disabled={busy}
              onClick={changeTag}
            >
              {ui("删除标签")}
            </button>
            {status.error && (
              <p role="alert" className="tag-form-error">
                {ui(status.text)}
              </p>
            )}
          </div>
        </Modal>
      )}
      {sorting.ghost && (
        <div
          className={`float-sort-ghost tag-color tone-${tagTone(sorting.ghost.id)}`}
          aria-hidden="true"
          style={{
            ...tagStyle(sorting.ghost.id),
            left: sorting.ghost.x,
            top: sorting.ghost.y,
            width: sorting.ghost.width,
            height: sorting.ghost.height,
          }}
        >
          <TagIcon size={14} />
          <span>
            {presets.tags.find((t) => t.id === sorting.ghost?.id)?.name}
          </span>
        </div>
      )}
      <span className="sr-only" role="status">
        {sorting.ghost ? ui("正在调整标签顺序，松开保存，按 Esc 取消") : ""}
      </span>
      {groupTag && (
        <GroupAssignDialog
          tag={groupTag}
          onClose={() => setGroupTag(undefined)}
          onUpdated={refresh}
        />
      )}
      {managingGroups && (
        <Modal title={ui("管理分类")} onClose={() => setManagingGroups(false)}>
          <div className="modal-content">
            <GroupManager tags={boot?.tags ?? []} onUpdated={refresh} />
          </div>
        </Modal>
      )}
      {colorTag && (
        <Modal
          title={ui("设置标签颜色")}
          onClose={() => setColorTag(undefined)}
          dismissDisabled={busy}
        >
          <div className="modal-content float-color-editor">
            <span
              className={`tag-color tone-${tagTone(colorTag.id)} color-preview`}
              style={{ "--custom-tag-color": color } as React.CSSProperties}
            >
              <TagIcon size={15} />
              {colorTag.name}
            </span>
            <div
              className="color-swatches"
              role="group"
              aria-label={ui("常用标签颜色")}
            >
              {[
                { name: ui("绿色"), color: "#71a58b" },
                { name: ui("蓝色"), color: "#638cc1" },
                { name: ui("粉色"), color: "#c57494" },
                { name: ui("琥珀色"), color: "#bb943b" },
                { name: ui("紫色"), color: "#9572b8" },
                { name: ui("青色"), color: "#4a9c91" },
              ].map((x) => (
                <button
                  key={x.color}
                  aria-label={x.name}
                  aria-pressed={color === x.color}
                  style={{ background: x.color }}
                  onClick={() => setColor(x.color)}
                />
              ))}
            </div>
            <label className="custom-color-label">
              {ui("自定义颜色")}
              <input
                type="color"
                aria-label={ui("自定义标签颜色")}
                value={color}
                onChange={(e) => setColor(e.target.value)}
              />
            </label>
            <div className="semantic-actions">
              <button
                className="button primary"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await api("tag.color", { id: colorTag.id, color });
                    await refresh();
                    setColorTag(undefined);
                    notify(msg("已更新「{0}」颜色", colorTag.name));
                  })
                }
              >
                {ui("保存颜色")}
              </button>
              <button
                className="button"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await api("tag.color", { id: colorTag.id, color: "" });
                    await refresh();
                    setColorTag(undefined);
                    notify(msg("已恢复默认颜色"));
                  })
                }
              >
                {ui("恢复默认")}
              </button>
            </div>
            {status.error && (
              <p role="alert" className="tag-form-error">
                {ui(status.text)}
              </p>
            )}
          </div>
        </Modal>
      )}
      {exportTag && (
        <ExportTagDialog
          tag={exportTag}
          onClose={() => setExportTag(undefined)}
          onComplete={(result) =>
            notify(
              msg("已导出「{0}」的 {1} 个文件", exportTag.name, result.copied),
              false,
              true,
            )
          }
        />
      )}
    </div>
  );
}
