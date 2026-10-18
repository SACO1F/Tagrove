import { t as ui, tf } from "./i18n";
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, LoaderCircle, ScanSearch, Link2 } from "lucide-react";
import { api, message } from "./api";
import { Modal } from "./components";
import { GlideSelect } from "./GlideSelect";
import "./path-repair.css";

type Candidate = {
  id: string;
  path: string;
  verified: boolean;
  reason: string;
};
type Row = {
  id: string;
  name: string;
  oldPath: string;
  status: string;
  candidates: Candidate[];
};
type Plan = {
  token: string;
  oldRoot: string;
  newRoot: string;
  rows: Row[];
  collectionCount: number;
  warnings: string[];
};
export type RepairResult = {
  repaired: number;
  collections: number;
  oldRoot: string;
  newRoot: string;
  mapCollections: boolean;
};
const labels: Record<string, string> = {
  matched: "已验证",
  confirm: "待确认",
  ambiguous: "多个候选",
  missing: "未找到",
  conflict: "路径或内容冲突",
};
export function PathRepair({
  onClose,
  onRepaired,
}: {
  onClose: () => void;
  onRepaired: (r: RepairResult) => Promise<void>;
}) {
  const [oldRoot, setOldRoot] = useState("");
  const [newRoot, setNewRoot] = useState("");
  const [plan, setPlan] = useState<Plan>();
  const [choices, setChoices] = useState<Record<string, string>>({});
  const [confirmed, setConfirmed] = useState<Record<string, boolean>>({});
  const [mapCollections, setMapCollections] = useState(true);
  const [phase, setPhase] = useState<"" | "scan" | "apply">("");
  const [error, setError] = useState("");
  const [result, setResult] = useState<RepairResult>();
  const [filter, setFilter] = useState("all");
  const [page, setPage] = useState(0);
  const busy = !!phase;
  async function browse(target: "old" | "new") {
    try {
      const p = await open({
        directory: true,
        multiple: false,
        title:
          target === "old"
            ? ui("选择原目录（不存在时请手动填写）")
            : ui("选择新的文件目录"),
      });
      if (typeof p === "string") {
        (target === "old" ? setOldRoot : setNewRoot)(p);
        setPlan(undefined);
        setError("");
      }
    } catch (e) {
      setError(message(e));
    }
  }
  async function scan() {
    setPhase("scan");
    setError("");
    setPlan(undefined);
    setResult(undefined);
    try {
      const p = await api<Plan>("repair.preview", { oldRoot, newRoot });
      setPlan(p);
      setChoices(
        Object.fromEntries(
          p.rows
            .filter((r) => r.status === "matched")
            .map((r) => [r.id, r.candidates[0].id]),
        ),
      );
      setConfirmed({});
      setPage(0);
      setFilter("all");
    } catch (e) {
      setError(message(e));
    } finally {
      setPhase("");
    }
  }
  const selected =
    plan?.rows.filter((r) => {
      const c = r.candidates.find((c) => c.id === choices[r.id]);
      return c && (c.verified || confirmed[r.id]);
    }) ?? [];
  async function apply() {
    if (!plan || !selected.length || busy) return;
    setPhase("apply");
    setError("");
    try {
      const r = await api<RepairResult>("repair.apply", {
        token: plan.token,
        mapCollections,
        choices: selected.map((row) => ({
          id: row.id,
          candidate: choices[row.id],
          confirmed: !!confirmed[row.id],
        })),
      });
      setResult(r);
      setPlan(undefined);
      await onRepaired(r);
    } catch (e) {
      setError(message(e));
      setPlan(undefined);
    } finally {
      setPhase("");
    }
  }
  const rows =
    plan?.rows.filter((r) => filter === "all" || r.status === filter) ?? [];
  return (
    <Modal
      title={ui("修复文件路径")}
      onClose={onClose}
      dismissDisabled={busy}
      wide
    >
      <div className="modal-content path-repair">
        {!result && (
          <>
            <p className="muted">
              {ui(
                "文件夹改名、移动或换盘后，重新定位文件并保留标注。只更新拾签记录。",
              )}
            </p>
            {(["old", "new"] as const).map((target) => (
              <label className="repair-root" key={target}>
                <span>{target === "old" ? ui("旧目录") : ui("新目录")}</span>
                <div>
                  <input
                    aria-label={target === "old" ? ui("旧目录") : ui("新目录")}
                    value={target === "old" ? oldRoot : newRoot}
                    disabled={busy}
                    placeholder={
                      target === "old"
                        ? ui("记录中的原目录；不存在时也可以填写")
                        : ui("现在存放这些文件的目录")
                    }
                    onChange={(e) => {
                      (target === "old" ? setOldRoot : setNewRoot)(
                        e.target.value,
                      );
                      setPlan(undefined);
                      setError("");
                    }}
                  />
                  <button
                    className="icon-button"
                    aria-label={
                      target === "old" ? ui("选择旧目录") : ui("选择新目录")
                    }
                    disabled={busy}
                    onClick={() => void browse(target)}
                  >
                    <FolderOpen size={17} />
                  </button>
                </div>
              </label>
            ))}
            <div className="repair-scan-actions">
              <button
                className="button"
                disabled={busy || !oldRoot.trim() || !newRoot.trim()}
                onClick={() => void scan()}
              >
                <ScanSearch size={16} />
                {plan ? ui("重新扫描") : ui("扫描匹配")}
              </button>
              {phase === "scan" && (
                <button
                  className="button quiet"
                  onClick={() =>
                    void api("repair.cancel").catch((e) => setError(message(e)))
                  }
                >
                  {ui("取消扫描")}
                </button>
              )}
              {busy && (
                <span role="status">
                  <LoaderCircle size={15} className="spin" />
                  {phase === "scan"
                    ? ui("正在扫描与核对…")
                    : ui("正在重新核对并修复…")}
                </span>
              )}
            </div>
          </>
        )}
        {plan && (
          <>
            <div className="repair-summary" role="status">
              <strong>
                {plan.rows.length} {ui("条记录 · 已选")}
                {selected.length} {ui("项")}
              </strong>
              <span>
                {Object.entries(labels)
                  .map(
                    ([status, label]) =>
                      `${label} ${plan.rows.filter((r) => r.status === status).length}`,
                  )
                  .join(" · ")}
              </span>
            </div>
            <div className="repair-filter">
              <GlideSelect
                ariaLabel={ui("路径匹配结果")}
                value={filter}
                onChange={(v) => {
                  setFilter(v);
                  setPage(0);
                }}
                disabled={busy}
                options={[
                  { value: "all", label: ui("全部结果") },
                  ...Object.entries(labels).map(([value, label]) => ({
                    value,
                    label: ui(label),
                  })),
                ]}
              />
              <button
                className="button quiet"
                disabled={busy}
                onClick={() => {
                  setChoices(
                    Object.fromEntries(
                      plan.rows
                        .filter((r) => r.status === "matched")
                        .map((r) => [r.id, r.candidates[0].id]),
                    ),
                  );
                  setConfirmed({});
                }}
              >
                {ui("仅选择已验证")}
              </button>
              <button
                className="button quiet"
                disabled={busy}
                onClick={() => {
                  setChoices({});
                  setConfirmed({});
                }}
              >
                {ui("取消选择")}
              </button>
            </div>
            <div className="repair-results" aria-label={ui("路径匹配列表")}>
              {rows.slice(page * 25, (page + 1) * 25).map((row) => {
                const candidate = row.candidates.find(
                  (c) => c.id === choices[row.id],
                );
                return (
                  <div
                    className="repair-row"
                    key={row.id}
                    data-repair-id={row.id}
                  >
                    <div className="repair-row-heading">
                      <strong>{row.name}</strong>
                      <span className={`repair-state ${row.status}`}>
                        {ui(labels[row.status])}
                      </span>
                    </div>
                    <div className="repair-path" data-tooltip={row.oldPath}>
                      {row.oldPath}
                    </div>
                    {!!row.candidates.length && (
                      <GlideSelect
                        ariaLabel={tf("新位置：{0}", row.name)}
                        value={choices[row.id] ?? ""}
                        disabled={busy}
                        options={[
                          { value: "", label: ui("暂不修复") },
                          ...row.candidates.map((c) => ({
                            value: c.id,
                            label: c.path,
                          })),
                        ]}
                        onChange={(id) => {
                          setChoices((v) => ({ ...v, [row.id]: id }));
                          setConfirmed((v) => ({ ...v, [row.id]: false }));
                        }}
                      />
                    )}
                    {candidate && (
                      <>
                        <p className="repair-reason">{candidate.reason}</p>
                        {!candidate.verified && (
                          <label className="repair-confirm">
                            <input
                              type="checkbox"
                              aria-label={tf("确认候选：{0}", row.name)}
                              checked={!!confirmed[row.id]}
                              disabled={busy}
                              onChange={(e) =>
                                setConfirmed((v) => ({
                                  ...v,
                                  [row.id]: e.target.checked,
                                }))
                              }
                            />
                            {ui("我已核对此文件，确认关联")}
                          </label>
                        )}
                      </>
                    )}
                    {!row.candidates.length && (
                      <p className="repair-reason">
                        {row.status === "conflict"
                          ? ui("候选位置已关联其他记录，或内容／类型不符。")
                          : ui(
                              "未找到可关联文件，保留原记录；也可从文件详情单独重新关联。",
                            )}
                      </p>
                    )}
                  </div>
                );
              })}
              {!rows.length && <p className="muted">{ui("此分类没有结果")}</p>}
            </div>
            {rows.length > 25 && (
              <div className="repair-pages">
                <button
                  className="button quiet"
                  disabled={busy || page === 0}
                  onClick={() => setPage((v) => v - 1)}
                >
                  {ui("上一页")}
                </button>
                <span>
                  {page + 1} / {Math.ceil(rows.length / 25)}
                </span>
                <button
                  className="button quiet"
                  disabled={busy || (page + 1) * 25 >= rows.length}
                  onClick={() => setPage((v) => v + 1)}
                >
                  {ui("下一页")}
                </button>
              </div>
            )}
            {
              <label className="repair-confirm">
                <input
                  type="checkbox"
                  checked={mapCollections}
                  disabled={busy}
                  onChange={(e) => setMapCollections(e.target.checked)}
                />
                {ui("同步目录筛选条件")}
                {plan.collectionCount
                  ? tf("（含 {0} 个保存筛选）", plan.collectionCount)
                  : ""}
              </label>
            }
            {
              <p className="subtle">
                {ui("目录条件会整体映射；未修复记录仍保留旧路径。")}
              </p>
            }
            {plan.warnings.map((w) => (
              <p className="subtle" key={w}>
                {w}
              </p>
            ))}
            <p className="subtle">
              {ui(
                "普通标签、备注与收藏保留；文件夹标签按现有设置更新。可在工作台撤销本轮修复。",
              )}
            </p>
          </>
        )}
        {result && (
          <div className="repair-success" role="status">
            <Link2 size={24} />
            <strong>
              {ui("已修复")}
              {result.repaired} {ui("个文件路径")}
            </strong>
            <span>
              {result.collections
                ? tf("已同步 {0} 个保存筛选", result.collections)
                : ui("标注、备注与收藏保留")}
            </span>
            <p className="subtle">
              {ui("原文件没有搬动。可在当前会话中撤销。")}
            </p>
          </div>
        )}
        {error && (
          <p className="tag-form-error" role="alert">
            {error}
          </p>
        )}
        <div className="repair-actions">
          <button className="button" disabled={busy} onClick={onClose}>
            {result ? ui("完成") : ui("关闭")}
          </button>
          {plan && (
            <button
              className="button primary"
              disabled={busy || !selected.length}
              onClick={() => void apply()}
            >
              <Link2 size={15} />
              {phase === "apply"
                ? ui("正在修复…")
                : tf("修复 {0} 项", selected.length)}
            </button>
          )}
        </div>
      </div>
    </Modal>
  );
}
