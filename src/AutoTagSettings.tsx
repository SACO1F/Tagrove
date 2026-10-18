import { SettingsSection } from "./SettingsSection";
import { t as ui, tf } from "./i18n";
import { useEffect, useState } from "react";
import SquishSwitch from "./SquishSwitch";
import { Folder, Sparkles, LoaderCircle } from "lucide-react";
import { api, message, type AiConfig } from "./api";

interface Settings {
  config: AiConfig;
  hasKey: boolean;
  summary: Record<string, number>;
}
export function AutoTagSettings({
  folderEnabled,
  onboarding = false,
  onSaved,
  onBusyChange,
}: {
  folderEnabled: boolean;
  onboarding?: boolean;
  onSaved?: () => void;
  onBusyChange?: (busy: boolean) => void;
}) {
  const [folder, setFolder] = useState(folderEnabled);
  const [settings, setSettings] = useState<Settings>();
  const [config, setConfig] = useState<AiConfig>();
  const [key, setKey] = useState("");
  const [clearKey, setClearKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState(false);
  useEffect(() => {
    let alive = true;
    api<Settings>("ai.settings")
      .then((s) => {
        if (alive) {
          setSettings(s);
          setConfig(s.config);
        }
      })
      .catch((e) => {
        if (alive) {
          setError(true);
          setNotice(message(e));
        }
      });
    const timer = setInterval(
      () =>
        void api<Settings>("ai.settings")
          .then((s) => {
            if (alive) setSettings(s);
          })
          .catch(() => {}),
      1500,
    );
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);
  const run = async (fn: () => Promise<string>) => {
    setBusy(true);
    onBusyChange?.(true);
    setNotice("");
    setError(false);
    try {
      setNotice(await fn());
    } catch (e) {
      setError(true);
      setNotice(message(e));
    } finally {
      setBusy(false);
      onBusyChange?.(false);
    }
  };
  const dirty =
    !!config &&
    (JSON.stringify(config) !== JSON.stringify(settings?.config) ||
      !!key ||
      clearKey);
  return (
    <>
      {!onboarding && (
        <SettingsSection
          className="auto-tag-settings"
          title={
            <>
              <Folder size={16} />
              {ui("文件夹自动标签")}
            </>
          }
        >
          <label className="auto-tag-toggle">
            <input
              type="checkbox"
              checked={folder}
              disabled={busy}
              onChange={(e) => {
                const value = e.target.checked;
                void run(async () => {
                  await api("settings.save", {
                    key: "folderAutoTagging",
                    value,
                  });
                  setFolder(value);
                  return value
                    ? ui("新导入文件会获得所在文件夹的标签")
                    : ui("已关闭；已有标签会保留");
                });
              }}
            />
            {ui("导入时使用所在文件夹名称作为标签")}
          </label>
          <p className="subtle">
            {ui(
              "例如「素材 / 海报 / 封面.png」添加「海报」。同名标签会复用；递归导入时采用每个文件的直接父文件夹。",
            )}
          </p>
          <button
            className="button quiet folder-backfill-action"
            disabled={busy || !folder}
            onClick={() =>
              void run(async () => {
                const result = await api<{ changed: number }>("folders.apply");
                return tf("已为 {0} 个文件补齐文件夹标签", result.changed);
              })
            }
          >
            {ui("为已有文件补齐标签")}
          </button>
        </SettingsSection>
      )}
      <SettingsSection
        className="auto-tag-settings"
        collapsible={!onboarding}
        title={
          <>
            <Sparkles size={16} />
            {ui("AI 自动标注")}
          </>
        }
      >
        <p className="muted">
          {ui(
            "优先匹配已有标签；没有合适标签时可以新建。所有 AI 标注带独立记号，确认后会保留。",
          )}
        </p>
        {!config ? (
          <p className="subtle">{ui("正在读取设置…")}</p>
        ) : (
          <>
            <label className="auto-tag-field">
              {ui("服务地址")}
              <input
                aria-label={ui("AI 服务地址")}
                value={config.endpoint}
                placeholder={ui("http://localhost:11434/v1 或 HTTPS 服务地址")}
                onChange={(e) =>
                  setConfig({ ...config, endpoint: e.target.value })
                }
              />
            </label>
            <label className="auto-tag-field">
              {ui("模型名称")}
              <input
                aria-label={ui("AI 模型名称")}
                value={config.model}
                placeholder={ui("填写服务提供的模型名，图片识别需要视觉模型")}
                onChange={(e) =>
                  setConfig({ ...config, model: e.target.value })
                }
              />
            </label>
            <label className="auto-tag-field">
              API Key
              <input
                aria-label="AI API Key"
                type="password"
                autoComplete="off"
                value={key}
                placeholder={
                  settings?.hasKey
                    ? ui("已加密保存，留空保持不变")
                    : ui("本地服务无需密钥时可留空")
                }
                onChange={(e) => {
                  setKey(e.target.value);
                  setClearKey(false);
                }}
              />
            </label>
            {settings?.hasKey && (
              <label className="auto-tag-toggle">
                <input
                  type="checkbox"
                  checked={clearKey}
                  onChange={(e) => setClearKey(e.target.checked)}
                />
                {ui("清除已保存的密钥")}
              </label>
            )}
            <p className="subtle">
              {ui(
                "兼容 Chat Completions 接口。密钥按当前 Windows 用户加密，不进入标注备份；更换服务地址需要重新填写密钥。",
              )}
            </p>
            <SquishSwitch
              className="ai-new-tags-switch"
              checked={config.allowNewTags}
              disabled={busy}
              onChange={(allowNewTags) =>
                setConfig({ ...config, allowNewTags })
              }
              label={ui("允许 AI 在标签池没有合适标签时创建新标签")}
            />
            <div className="ai-sharing-note">
              {ui(
                "启用后，新导入图片的预览，或文档最多 12,000 字符的摘录，会连同文件名和已有标签池发送到上方服务。远程服务可能计费；使用本机模型时，请填写本机服务地址。",
              )}
            </div>
            <SquishSwitch
              className="ai-consent-switch"
              checked={config.enabled}
              disabled={busy}
              onChange={(enabled) => setConfig({ ...config, enabled })}
              label={ui("启用 AI 自动标注，允许向上述服务发送分析内容")}
            />
            <div className="settings-buttons">
              <button
                className="button primary"
                disabled={busy || !dirty}
                onClick={() =>
                  void run(async () => {
                    const result = await api<Settings>("ai.settings.save", {
                      config,
                      apiKey: key,
                      clearKey,
                    });
                    setSettings(result);
                    setConfig(result.config);
                    setKey("");
                    setClearKey(false);
                    onSaved?.();
                    return ui(
                      "AI 设置已保存，新导入文件按此设置处理；已有文件可在详情中重新识别",
                    );
                  })
                }
              >
                {busy && <LoaderCircle size={14} className="spin" />}
                {onboarding ? ui("保存并继续") : ui("保存 AI 设置")}
              </button>
              <button
                className="button"
                disabled={busy || dirty || !config.endpoint || !config.model}
                onClick={() =>
                  void run(
                    async () =>
                      (await api<{ message: string }>("ai.test")).message,
                  )
                }
              >
                {ui("测试连接")}
              </button>
            </div>
            <p className="subtle">
              {ui(
                "支持 JPEG、PNG、WebP、纯文本、可提取文字的 PDF、DOCX、XLSX 和 PPTX。扫描 PDF、旧版 Office 和其他格式会显示未支持，不会凭文件名猜标签。",
              )}
            </p>
            {settings && (
              <div className="ai-queue-summary">
                <span>
                  {ui("等待")}
                  {settings.summary.queued || 0} {ui("· 处理中")}{" "}
                  {settings.summary.running || 0} {ui("· 已完成")}{" "}
                  {settings.summary.done || 0} {ui("· 失败")}{" "}
                  {settings.summary.failed || 0} {ui("· 未支持")}{" "}
                  {settings.summary.unsupported || 0}
                </span>
                {!!(settings.summary.queued || settings.summary.running) && (
                  <button
                    disabled={busy}
                    onClick={() =>
                      void run(async () => {
                        await api("ai.cancel");
                        return ui("已取消队列；已发送的请求结束后会丢弃结果");
                      })
                    }
                  >
                    {ui("取消队列")}
                  </button>
                )}
              </div>
            )}
          </>
        )}
        {notice && (
          <p
            role="status"
            className={error ? "ai-feedback error" : "ai-feedback"}
          >
            {notice}
          </p>
        )}
      </SettingsSection>
    </>
  );
}
