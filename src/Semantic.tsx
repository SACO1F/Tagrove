import { SettingsSection } from "./SettingsSection";
import { currentLanguage } from "./i18n";
import { t as ui, tf } from "./i18n";
import { useEffect, useState, useRef } from "react";
import { Search, Sparkles, SlidersHorizontal } from "lucide-react";
import { api, message, size, type Query } from "./api";
import { Modal } from "./components";
import { RubberSegment } from "./ReaderControls";
import "./semantic.css";
import { GlideSelect } from "./GlideSelect";
import SquishSwitch from "./SquishSwitch";
type Config = {
  enabled: boolean;
  endpoint: string;
  model: string;
  maxRequests: number;
  minSimilarity: number;
};
type Status = {
  config: Config;
  hasKey: boolean;
  files: number;
  ready: number;
  bytes: number;
  diskBytes: number;
  eligible: number;
  task: {
    state: string;
    requested: number;
    completed: number;
    failed: number;
    error: string;
  };
};
type Plan = {
  fingerprint: string;
  endpoint: string;
  model: string;
  requests: number;
  reuse: number;
  total: number;
  pending: number;
  cached: number;
  files: { id: string; name: string; characters: number; samples: number }[];
};
const initial: Config = {
  enabled: false,
  endpoint: "",
  model: "",
  maxRequests: 100,
  minSimilarity: 0.35,
};
export function SemanticSettings() {
  const testToken = useRef(0);
  const [connection, setConnection] = useState<{
    endpoint: string;
    model: string;
    dimension: number;
    durationMs: number;
  }>();

  const [status, setStatus] = useState<Status>();
  const [config, setConfig] = useState(initial);
  const [key, setKey] = useState("");
  const [clearKey, setClearKey] = useState(false);
  const [consent, setConsent] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [plan, setPlan] = useState<Plan>();
  const [confirmClear, setConfirmClear] = useState(false);
  const [loaded, setLoaded] = useState(false);
  useEffect(() => {
    let active = true,
      pending = false;
    const poll = async () => {
      if (pending) return;
      pending = true;
      try {
        const s = await api<Status>("semantic.status");
        if (active) {
          setStatus(s);
          if (!loaded) {
            setConfig(s.config);
            setLoaded(true);
          }
        }
      } catch (e) {
        if (active) setError(message(e));
      } finally {
        pending = false;
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 2500);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [loaded]);
  async function act(action: string, payload: unknown = {}) {
    setBusy(true);
    setError("");
    try {
      await api(action, payload);
      setStatus(await api<Status>("semantic.status"));
      setPlan(undefined);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => {
    testToken.current++;
    setConnection(undefined);
  }, [config, key, clearKey]);
  const dirty =
    !!status && JSON.stringify(config) !== JSON.stringify(status.config);
  const running = status?.task.state === "running";
  const endpointHint = (() => {
    try {
      const url = new URL(config.endpoint.trim());
      if (url.hostname === "api.deepseek.com")
        return ui(
          "DeepSeek 官方当前未提供 Embeddings 接口。deepseek-flash 可用于 AI 标注；语义检索请改用向量服务和向量模型。",
        );
      const base = config.endpoint.trim().replace(/\/+$/, "");
      const target = base.endsWith("/embeddings")
        ? base
        : `${base}${url.pathname === "/" ? "/v1" : ""}/embeddings`;
      return tf(
        "实际请求地址：{0}。请使用向量模型，对话模型不能用于此处。",
        target,
      );
    } catch {
      return ui(
        "支持 OpenAI 兼容的 Embeddings 接口；可填写服务根地址、/v1 地址或完整 /embeddings 地址。",
      );
    }
  })();
  return (
    <SettingsSection
      className="semantic-settings"
      title={
        <>
          <Sparkles size={16} />
          {ui("语义检索 · 可选")}
        </>
      }
    >
      <div className="setting-row">
        <div>
          <p>{ui("用含义寻找相关内容；默认关闭，模型由你自行配置。")}</p>
        </div>
        <SquishSwitch
          label={ui("启用")}
          checked={config.enabled}
          disabled={busy || !loaded}
          onChange={(enabled) => {
            setConfig({ ...config, enabled });
            setPlan(undefined);
          }}
        />
      </div>
      <p className="semantic-endpoint-hint">{endpointHint}</p>
      <div className="semantic-fields">
        <label>
          {ui("Embeddings 服务地址")}
          <input
            value={config.endpoint}
            placeholder="http://localhost:11434/v1"
            onChange={(e) => {
              setConfig({ ...config, endpoint: e.target.value });
              setPlan(undefined);
            }}
          />
        </label>
        <label>
          {ui("向量模型名称")}
          <input
            value={config.model}
            placeholder={ui("填写服务中已安装的模型名称")}
            onChange={(e) => {
              setConfig({ ...config, model: e.target.value });
              setPlan(undefined);
            }}
          />
        </label>
        <label>
          {ui("API 密钥")}
          <input
            type="password"
            autoComplete="off"
            value={key}
            placeholder={
              status?.hasKey ? ui("已保存；留空保留") : ui("本地服务可留空")
            }
            onChange={(e) => setKey(e.target.value)}
          />
        </label>
        <label>
          {ui("单次建索引请求上限")}
          <input
            type="number"
            min={1}
            max={200}
            value={config.maxRequests}
            onChange={(e) => {
              setConfig({ ...config, maxRequests: Number(e.target.value) });
              setPlan(undefined);
            }}
          />
        </label>
        <label>
          {ui("相关度阈值（0～0.95）")}
          <input
            type="number"
            min={0}
            max={0.95}
            step={0.05}
            value={Number(config.minSimilarity.toFixed(2))}
            onChange={(e) =>
              setConfig({ ...config, minSimilarity: Number(e.target.value) })
            }
          />
        </label>
      </div>
      <label className="semantic-check">
        <input
          type="checkbox"
          checked={clearKey}
          onChange={(e) => setClearKey(e.target.checked)}
        />
        {ui("清除已保存的语义密钥")}
      </label>
      {config.enabled && (
        <label className="semantic-check semantic-consent">
          <input
            type="checkbox"
            checked={consent}
            onChange={(e) => setConsent(e.target.checked)}
          />
          {ui(
            "我确认向此服务发送正文样本和搜索文字。每文件最多 3 段，每段 1000 字符；不自动发送文件名、路径或标签。服务可能收费。",
          )}
        </label>
      )}
      <div className="semantic-actions">
        <button
          className="button primary"
          disabled={busy || !status || (config.enabled && !consent)}
          onClick={() =>
            void act("semantic.settings.save", {
              config,
              apiKey: key,
              clearKey,
              consent,
            }).then(() => {
              setKey("");
              setClearKey(false);
            })
          }
        >
          {ui("保存语义设置")}
        </button>
        <button
          className="button"
          disabled={
            busy ||
            dirty ||
            !!key ||
            clearKey ||
            running ||
            !status?.config.enabled
          }
          onClick={async () => {
            setBusy(true);
            setError("");
            try {
              setPlan(await api<Plan>("semantic.plan"));
            } catch (e) {
              setError(message(e));
            } finally {
              setBusy(false);
            }
          }}
        >
          {ui("预览发送范围")}
        </button>
        <button
          className="button"
          disabled={
            busy ||
            dirty ||
            !!key ||
            clearKey ||
            running ||
            !status?.config.enabled
          }
          onClick={async () => {
            const token = ++testToken.current;
            setBusy(true);
            setError("");
            setConnection(undefined);
            try {
              const result = await api<{
                endpoint: string;
                model: string;
                dimension: number;
                durationMs: number;
              }>("semantic.test");
              if (token === testToken.current) setConnection(result);
            } catch (e) {
              if (token === testToken.current) setError(message(e));
            } finally {
              setBusy(false);
            }
          }}
        >
          {ui("测试连接")}
        </button>
        <button
          className="button"
          disabled={busy || !running}
          onClick={() => void act("semantic.cancel")}
        >
          {ui("取消建立")}
        </button>
        <button
          className="button"
          disabled={busy || !status?.files}
          onClick={() => setConfirmClear(true)}
        >
          {ui("清理语义缓存")}
        </button>
      </div>
      {status && (
        <p className="semantic-summary" role="status">
          {ui("正文可用")}
          {status.eligible} {ui("个 · 语义可用")}
          {status.ready} {ui("个 / 缓存")} {status.files} {ui("个 · 内容")}
          {size(status.bytes)} {ui("/ 32 MiB · 磁盘")} {size(status.diskBytes)}
          <br />
          {ui("任务：")}
          {(
            {
              idle: ui("未开始"),
              running: ui("建立中"),
              completed: ui("本轮完成"),
              failed: ui("失败"),
              interrupted: ui("已中断"),
              cancelled: ui("已取消"),
            } as Record<string, string>
          )[status.task.state] ?? status.task.state}{" "}
          {ui("· 请求")}
          {status.task.requested} {ui("· 完成")}
          {status.task.completed} {ui("· 失败")} {status.task.failed}
          {status.task.error && (
            <>
              <br />
              {message(status.task.error)}
            </>
          )}
        </p>
      )}
      <p className="semantic-hint">
        {ui(
          "仅使用最新 500 个已完成正文索引的文件，向量最多 2048 维。手动建立；取消后丢弃迟到结果，已发送请求仍可能计费。关闭、重启与精确搜索不会调用服务。语义缓存不随资料包或备份迁移。",
        )}
      </p>
      <p className="semantic-hint">
        {ui(
          "测试连接只在点击后发送一段合成文字，1 次请求，不读取文件或建立缓存；远程服务可能计费。",
        )}
      </p>
      {connection && (
        <p className="semantic-hint" role="status">
          {ui("连接成功 ·")}
          {connection.dimension} {ui("维 ·")}
          {connection.durationMs} ms
          <br />
          {connection.endpoint} · {connection.model}
        </p>
      )}
      {plan && (
        <div
          className="semantic-plan"
          role="region"
          aria-label={ui("发送范围预览")}
        >
          <strong>
            {ui("将发送")}
            {plan.requests} {ui("次请求 · 尚待处理")}
            {plan.pending} {ui("个 · 已缓存")} {plan.cached}{" "}
            {ui("个 · 本机复用")}
            {plan.reuse} {ui("个")}
          </strong>
          <p>
            {plan.endpoint} · {plan.model}
          </p>
          <ul>
            {plan.files.map((f) => (
              <li key={f.id}>
                {f.name}
                <small>
                  {f.samples} {ui("段 /")}
                  {f.characters} {ui("字符")}
                </small>
              </li>
            ))}
          </ul>
          <button
            className="button primary"
            disabled={busy || !plan.total || dirty}
            onClick={() =>
              void act("semantic.build", { fingerprint: plan.fingerprint })
            }
          >
            {ui("确认发送并建立")}
          </button>
        </div>
      )}
      {confirmClear && (
        <div className="semantic-warning">
          {ui("清理派生向量和样本；原文件、标签和正文索引保留。")}
          <div className="semantic-actions">
            <button
              className="button danger"
              disabled={busy}
              onClick={() => {
                setConfirmClear(false);
                void act("semantic.clear");
              }}
            >
              {ui("确认清理")}
            </button>
            <button className="button" onClick={() => setConfirmClear(false)}>
              {ui("保留缓存")}
            </button>
          </div>
        </div>
      )}
      {error && (
        <p className="semantic-error" role="alert">
          {error}
        </p>
      )}
    </SettingsSection>
  );
}
type FilterPreview = {
  query: Query;
  rules: string[];
  warnings: string[];
  anchor: number;
};
export function SemanticSearch({
  query,
  onApply,
  onClose,
  onConfigure,
}: {
  query: Query;
  onApply: (q: Query) => void;
  onClose: () => void;
  onConfigure: () => void;
}) {
  const [tab, setTab] = useState(0);
  const [text, setText] = useState(query.text);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [preview, setPreview] = useState<FilterPreview>();
  const [status, setStatus] = useState<Status>();
  useEffect(() => {
    void api<Status>("semantic.status")
      .then(setStatus)
      .catch((e) => setError(message(e)));
  }, []);
  async function search() {
    setBusy(true);
    setError("");
    try {
      if (tab === 0) {
        const r = await api<{ session: string }>("semantic.search", { text });
        onApply({
          ...query,
          text: text.trim(),
          searchMode: "unified",
          semanticSession: r.session,
          offset: 0,
          sort: "relevance",
        });
        onClose();
      } else setPreview(await api<FilterPreview>("filters.preview", { text }));
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function apply() {
    if (!preview) return;
    setBusy(true);
    try {
      const fresh = await api<FilterPreview>("filters.preview", {
        text,
        anchor: preview.anchor,
      });
      if (JSON.stringify(fresh.query) !== JSON.stringify(preview.query)) {
        setPreview(fresh);
        setError(ui("标签或条件已变化，请重新核对后应用"));
        return;
      }
      onApply({ ...fresh.query, searchMode: "unified", semanticSession: "" });
      onClose();
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={ui("智能补充")}
      onClose={onClose}
      dismissDisabled={busy}
      className="semantic-dialog"
    >
      <div className="modal-content semantic-dialog-body">
        <RubberSegment index={tab} role="tablist" aria-label={ui("查找方式")}>
          {[ui("智能补充"), ui("自然语言筛选")].map((label, i) => (
            <button
              key={ui(label)}
              role="tab"
              id={`semantic-tab-${i}`}
              aria-controls="semantic-search-panel"
              tabIndex={tab === i ? 0 : -1}
              onKeyDown={(e) => {
                if (
                  ["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)
                ) {
                  e.preventDefault();
                  const next =
                    e.key === "Home" ? 0 : e.key === "End" ? 1 : 1 - tab;
                  setTab(next);
                  setPreview(undefined);
                  setError("");
                  document.getElementById(`semantic-tab-${next}`)?.focus();
                }
              }}
              aria-selected={tab === i}
              disabled={busy}
              onClick={() => {
                setTab(i);
                setPreview(undefined);
                setError("");
              }}
            >
              {i === 0 ? (
                <Sparkles size={15} />
              ) : (
                <SlidersHorizontal size={15} />
              )}{" "}
              {ui(label)}
            </button>
          ))}
        </RubberSegment>
        <div
          id="semantic-search-panel"
          role="tabpanel"
          aria-labelledby={`semantic-tab-${tab}`}
          className="semantic-panel"
        >
          <p>
            {tab === 0
              ? ui("保留本地搜索结果，补充含义相关的文件；相似度仅供参考。")
              : ui(
                  "在本机解析筛选条件，核对后再应用。不会发送请求或修改文件。",
                )}
          </p>
          <label className="semantic-query-label">
            {tab === 0 ? ui("想寻找什么内容？") : ui("描述筛选条件")}
            <textarea
              aria-label={ui("智能查找描述")}
              maxLength={256}
              value={text}
              placeholder={
                tab === 0
                  ? ui("例如：怎样准备 Agent 岗位面试")
                  : ui("例如：最近7天的 PDF，包含「参考素材」，排除「已完成」")
              }
              onChange={(e) => {
                setText(e.target.value);
                setPreview(undefined);
                setError("");
              }}
            />
          </label>
          {tab === 0 && (
            <p className="semantic-hint">
              {status?.config.enabled
                ? tf(
                    "发送一次查询至 {0}，模型 {1}；后续翻页和筛选复用结果，10 分钟有效。",
                    status.config.endpoint,
                    status.config.model,
                  )
                : ui(
                    "语义检索尚未启用；可先使用自然语言筛选，或在偏好设置配置模型并手动建立索引。",
                  )}
            </p>
          )}
          <div className="semantic-actions">
            <button
              className="button primary"
              disabled={
                busy || !text.trim() || (tab === 0 && !status?.config.enabled)
              }
              onClick={() => void search()}
            >
              <Search size={15} />
              {busy
                ? ui("处理中…")
                : tab === 0
                  ? ui("发送查询并补充")
                  : ui("预览筛选条件")}
            </button>
            {tab === 0 && (
              <button className="button" disabled={busy} onClick={onConfigure}>
                {ui("语义设置")}
              </button>
            )}
          </div>
          {preview && (
            <div className="semantic-plan">
              <strong>{ui("将替换当前筛选")}</strong>
              <ul>
                {preview.rules.map((r) => (
                  <li key={r}>{r}</li>
                ))}
              </ul>
              {preview.warnings.map((w) => (
                <p className="semantic-hint" key={w}>
                  {w}
                </p>
              ))}
              <button
                className="button primary"
                disabled={busy}
                onClick={() => void apply()}
              >
                {ui("确认应用这些条件")}
              </button>
            </div>
          )}
          {error && (
            <p className="semantic-error" role="alert">
              {error}
            </p>
          )}
        </div>
      </div>
    </Modal>
  );
}

export function ImportSemanticPrompt({
  ids,
  onClose,
  onConfigure,
  onStarted,
}: {
  ids: string[];
  onClose: () => void;
  onConfigure: () => void;
  onStarted: () => void;
}) {
  const [status, setStatus] = useState<Status>();
  const [mode, setMode] = useState("update");
  const [indexed, setIndexed] = useState(false);
  const [plan, setPlan] = useState<Plan>();
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let active = true,
      pending = false;
    const refresh = async () => {
      if (pending) return;
      pending = true;
      try {
        const s = await api<Status>("semantic.status");
        const index = await api<{ enabled: boolean }>("index.status");
        if (!active) return;
        setStatus(s);
        setIndexed(index.enabled);
        if (s.config.enabled && index.enabled) {
          const p = await api<Plan>("semantic.plan", { ids, mode });
          if (active) {
            setPlan(p);
            setError("");
          }
        }
      } catch (e) {
        if (active) setError(message(e));
      } finally {
        pending = false;
      }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 1500);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, [ids, mode]);
  return (
    <Modal
      title={ui("生成语义索引")}
      onClose={onClose}
      dismissDisabled={busy}
      className="semantic-dialog"
    >
      <div className="modal-content semantic-dialog-body">
        <p>
          {ui("只处理已选的")}
          {ids.length}{" "}
          {ui(
            "个文件（最多 500 个）。图片等没有可用正文的文件会跳过；未完成正文索引的文件需等待处理。",
          )}
        </p>
        <GlideSelect
          value={mode}
          ariaLabel={ui("语义处理方式")}
          onChange={(value) => {
            setPlan(undefined);
            setMode(value);
          }}
          disabled={busy || status?.task.state === "running"}
          options={[
            { value: "update", label: ui("补齐缺失并更新变化") },
            { value: "missing", label: ui("仅补齐缺失") },
            { value: "rebuild", label: ui("重新生成所选文件") },
          ]}
        />
        {!indexed && (
          <div>
            <p>{ui("需要先在本机建立正文索引，不发送模型请求。")}</p>
            <button
              className="button"
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                try {
                  await api("index.control", { action: "enable" });
                  setIndexed(true);
                } catch (e) {
                  setError(message(e));
                } finally {
                  setBusy(false);
                }
              }}
            >
              {ui("启用正文索引")}
            </button>
          </div>
        )}
        {status && !status.config.enabled && (
          <div>
            <p>{ui("语义检索未启用，请先配置可用的向量服务与模型。")}</p>
            <button className="button" onClick={onConfigure}>
              {ui("配置向量服务")}
            </button>
          </div>
        )}
        {status?.task.state === "running" && (
          <p>{ui("另一个语义任务正在处理，请等待完成后再确认。")}</p>
        )}
        {plan && (
          <div
            className="semantic-plan"
            role="region"
            aria-label={ui("新文件发送范围")}
          >
            <strong>
              {ui("将发送")}
              {plan.requests} {ui("次请求 · 已缓存")}
              {plan.cached} {ui("个 · 待处理")} {plan.pending}{" "}
              {ui("个 · 本机复用")}
              {plan.reuse} {ui("个")}
            </strong>
            <p>
              {plan.endpoint} · {plan.model}
            </p>
            <ul>
              {plan.files.map((f) => (
                <li key={f.id}>
                  {f.name}
                  <small>
                    {f.samples} {ui("段 /")}
                    {f.characters} {ui("字符")}
                  </small>
                </li>
              ))}
            </ul>
            {!plan.total && (
              <p>
                {ui(
                  "没有可发送的文件：可能已有缓存、尚未完成正文索引，或格式暂不支持。",
                )}
              </p>
            )}
          </div>
        )}
        <p className="semantic-hint">
          {ui(
            "确认后向上述服务发送所列正文样本，远程服务可能计费。请求上限与语义缓存预算沿用偏好设置；此次不会处理资料库内其他文件。内容样本未变时在本机复用，不发送请求；重新生成会再次发送所选正文。",
          )}
        </p>
        <div className="semantic-actions">
          <button
            className="button primary"
            disabled={busy || !plan?.total || status?.task.state === "running"}
            onClick={async () => {
              if (!plan) return;
              setBusy(true);
              try {
                await api("semantic.build", {
                  ids,
                  mode,
                  fingerprint: plan.fingerprint,
                });
                onStarted();
              } catch (e) {
                setError(message(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            {ui("确认范围并生成")}
          </button>
          <button className="button" disabled={busy} onClick={onConfigure}>
            {ui("语义设置")}
          </button>
          <button className="button" disabled={busy} onClick={onClose}>
            {ui("暂不生成")}
          </button>
        </div>
        {error && (
          <p className="semantic-error" role="alert">
            {error}
          </p>
        )}
      </div>
    </Modal>
  );
}

type QueueItem = {
  id: string;
  name: string;
  batch: string;
  created: number;
  state: string;
  error: string;
};
const queueStates: Record<string, string> = {
  pending: "待生成",
  cached: "已缓存",
  indexing: "等待正文索引",
  unsupported: "不支持正文",
  index_failed: "正文索引失败",
  empty: "无可用正文",
  model_changed: "模型已变化",
  changed: "正文已变化",
  running: "生成中",
  waiting: "等待生成",
  failed: "生成失败",
  interrupted: "已中断",
  cancelled: "已取消",
  unavailable: "原文件不可用",
};
export function SemanticTaskSection({
  onSelect,
}: {
  onSelect: (ids: string[]) => void;
}) {
  const [items, setItems] = useState<QueueItem[]>([]),
    [status, setStatus] = useState<Status>();
  const [selected, setSelected] = useState<string[]>([]),
    [batch, setBatch] = useState("all"),
    [page, setPage] = useState(0),
    [error, setError] = useState("");
  useEffect(() => {
    let active = true,
      pending = false;
    const poll = async () => {
      if (pending) return;
      pending = true;
      try {
        const [q, s] = await Promise.all([
          api<{ items: QueueItem[] }>("semantic.queue"),
          api<Status>("semantic.status"),
        ]);
        if (active) {
          setItems(q.items);
          setStatus(s);
        }
      } catch (e) {
        if (active) setError(message(e));
      } finally {
        pending = false;
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 2000);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, []);
  const batches = [...new Set(items.map((x) => x.batch))];
  const visible = items.filter((x) => batch === "all" || x.batch === batch),
    rows = visible.slice(page * 40, (page + 1) * 40);
  const retry = items
    .filter((x) => ["failed", "interrupted", "cancelled"].includes(x.state))
    .map((x) => x.id)
    .slice(0, 500);
  const running = status?.task.state === "running";
  return (
    <section className="semantic-queue">
      <h3>{ui("语义索引与待处理文件")}</h3>
      <p className="semantic-summary">
        {ui("已缓存")}
        {status?.ready || 0} {ui("个 ·")}{" "}
        {running
          ? ui("正在生成")
          : queueStates[status?.task.state || ""] ||
            {
              completed: ui("任务完成"),
              idle: ui("未运行"),
              failed: ui("任务失败"),
            }[status?.task.state || ""] ||
            ui("未运行")}{" "}
        {ui("· 本次完成")}
        {status?.task.completed || 0} {ui("个 / 已发送")}{" "}
        {status?.task.requested || 0} {ui("次请求")}
      </p>
      {status?.task.error && (
        <p className="semantic-error" role="alert">
          {message(status.task.error)}
        </p>
      )}
      <div className="semantic-actions">
        <button
          className="button primary"
          disabled={!selected.length || running}
          onClick={() => onSelect(selected)}
        >
          {ui("预览所选范围（")}
          {selected.length}）
        </button>
        <button
          className="button"
          disabled={!retry.length || running}
          onClick={() => onSelect(retry)}
        >
          {ui("预览失败与中断项（")}
          {retry.length}）
        </button>
        {running && (
          <button
            className="button"
            onClick={() =>
              void api("semantic.cancel").catch((e) => setError(message(e)))
            }
          >
            {ui("取消生成")}
          </button>
        )}
      </div>
      <p className="semantic-hint">
        {ui(
          "导入提示消失后仍可在这里处理；重启保留记录，不自动发送请求。最多保留最近 2,000 个文件，每次选择不超过 500 个。",
        )}
      </p>
      {!!batches.length && (
        <GlideSelect
          value={batch}
          ariaLabel={ui("待处理批次")}
          onChange={(v) => {
            setBatch(v);
            setPage(0);
          }}
          options={[
            { value: "all", label: ui("全部批次") },
            ...batches.map((value, i) => ({
              value,
              label: tf(
                "{0} · 批次 {1}",
                new Date(
                  (items.find((x) => x.batch === value)?.created || 0) * 1000,
                ).toLocaleString(currentLanguage()),
                batches.length - i,
              ),
            })),
          ]}
        />
      )}
      {!rows.length && (
        <p className="muted">
          {ui("暂无待处理记录。新加入的文件会在这里保留。")}
        </p>
      )}
      <div className="semantic-queue-list">
        {rows.map((x) => (
          <label className="semantic-queue-row" key={x.id}>
            <input
              type="checkbox"
              checked={selected.includes(x.id)}
              disabled={
                [
                  "unsupported",
                  "indexing",
                  "index_failed",
                  "empty",
                  "unavailable",
                  "waiting",
                  "running",
                ].includes(x.state) ||
                (!selected.includes(x.id) && selected.length >= 500)
              }
              onChange={(e) =>
                setSelected((old) =>
                  e.target.checked
                    ? [...old, x.id]
                    : old.filter((id) => id !== x.id),
                )
              }
            />
            <span className="semantic-queue-name" data-tooltip={x.name}>
              {x.name}
              {x.error && <small>{message(x.error)}</small>}
            </span>
            <small>{ui(queueStates[x.state] || x.state)}</small>
          </label>
        ))}
      </div>
      {visible.length > 40 && (
        <div className="semantic-actions">
          <button
            className="button"
            disabled={!page}
            onClick={() => setPage(page - 1)}
          >
            {ui("上一页")}
          </button>
          <span>
            {page + 1} / {Math.ceil(visible.length / 40)}
          </span>
          <button
            className="button"
            disabled={(page + 1) * 40 >= visible.length}
            onClick={() => setPage(page + 1)}
          >
            {ui("下一页")}
          </button>
        </div>
      )}
      {!!selected.length && (
        <button className="button quiet" onClick={() => setSelected([])}>
          {ui("清除选择")}
        </button>
      )}
      {error && (
        <p className="semantic-error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
