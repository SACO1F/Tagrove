import { useEffect, useRef, useState } from "react";
import { ExternalLink, Globe, RefreshCw, Check } from "lucide-react";
import { api, message } from "./api";
import { t as ui } from "./i18n";
import "./web-capture.css";

export type BridgeStatus = {
  enabled: boolean;
  registrationError?: string;
  extensionId: string;
  registered: boolean;
  removable: boolean;
  registrationState: string;
  storeLastSeen?: number;
  developmentLastSeen?: number;
};
const preferenceKey = (library: string) =>
  `tagrove-extension-guide:v1:${library}:edge`;
function preference(library: string) {
  try {
    return localStorage.getItem(preferenceKey(library));
  } catch {
    return null;
  }
}

/** Installation is always a user action. A native registration alone is not proof of installation. */
export function BrowserExtensionGuide({
  library,
  status,
  compact = false,
}: {
  library: string;
  status?: BridgeStatus;
  compact?: boolean;
}) {
  const [observed, setObserved] = useState<BridgeStatus>();
  const [expanded, setExpanded] = useState(
    () => !["never", "connected"].includes(preference(library) || ""),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const panel = useRef<HTMLDivElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const active = status ?? observed;
  const verified = !!(active?.storeLastSeen || active?.developmentLastSeen);
  function remember(value: string) {
    try {
      localStorage.setItem(preferenceKey(library), value);
      return true;
    } catch {
      setError(ui("引导状态未能保存，下次可能再次显示；资料不受影响。"));
      return false;
    }
  }
  useEffect(() => {
    if (verified) {
      try {
        localStorage.setItem(preferenceKey(library), "connected");
      } catch {
        /* Retain explicit status even when UI storage is unavailable. */
      }
      if (panel.current?.contains(document.activeElement))
        toggle.current?.focus();
      setExpanded(false);
    }
  }, [verified, library]);
  useEffect(() => {
    let live = true;
    async function inspect() {
      if (document.hidden || panel.current?.closest("[inert]")) return;
      try {
        const value = await api<BridgeStatus>("capture.bridge.status", {
          browser: "edge",
        });
        if (live) setObserved(value);
      } catch {
        /* Explicit Check connection displays failures; background checks stay quiet. */
      }
    }
    if (!status) void inspect();
    const timer =
      expanded && !status
        ? window.setInterval(() => void inspect(), 3000)
        : undefined;
    return () => {
      live = false;
      if (timer) clearInterval(timer);
    };
  }, [expanded, status]);
  async function check() {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const value = await api<BridgeStatus>("capture.bridge.status", {
        browser: "edge",
      });
      setObserved(value);
      setNotice(
        value.storeLastSeen || value.developmentLastSeen
          ? ui("已验证扩展通信。暂时不可用时，可检查或修复本机连接。")
          : value.registered && value.enabled
            ? ui(
                "本机连接已就绪。请在 Edge 扩展中点击“检查连接”，完成通信验证。",
              )
            : ui("请先在下方连接或修复 Edge，再打开扩展检查连接。"),
      );
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function install() {
    setBusy(true);
    setError("");
    try {
      await api("capture.extension.open", { browser: "edge" });
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  if (compact && (!expanded || verified)) return null;
  return (
    <div
      className={`browser-install-guide ${compact ? "is-compact" : ""}`}
      ref={panel}
    >
      {!compact && (
        <button
          ref={toggle}
          type="button"
          className="button quiet browser-guide-toggle"
          aria-expanded={expanded}
          onClick={() => setExpanded((v) => !v)}
        >
          {verified ? <Check size={16} /> : <Globe size={16} />}
          {verified ? ui("已验证扩展通信") : ui("安装与连接指南")}
        </button>
      )}
      {expanded && (
        <div className="browser-guide-content">
          <strong>{ui("将网页保存到拾签")}</strong>
          <p>
            {ui(
              "安装 Edge 扩展，即可保存网页、文字和图片，并在保存前选择标签。",
            )}
          </p>
          {!compact && (
            <ol>
              <li>{ui("前往 Edge 商店，由你确认安装扩展。")}</li>
              <li>{ui("在下方启用或修复 Edge 本机连接。")}</li>
              <li>{ui("点击浏览器工具栏中的拾签扩展，再点击“检查连接”。")}</li>
            </ol>
          )}
          <div className="capture-actions">
            <button
              type="button"
              className="button primary"
              disabled={busy}
              onClick={() => void install()}
            >
              <ExternalLink size={15} />
              {ui("安装 Edge 扩展")}
            </button>
            {!compact && (
              <button
                type="button"
                className="button"
                disabled={busy}
                onClick={() => void check()}
              >
                <RefreshCw size={15} />
                {ui("检查连接")}
              </button>
            )}
            <button
              type="button"
              className="button quiet"
              disabled={busy}
              onClick={() => setExpanded(false)}
            >
              {ui("稍后")}
            </button>
            <button
              type="button"
              className="button quiet"
              disabled={busy}
              onClick={() => {
              if (remember("never")) setExpanded(false);
              }}
            >
              {ui("不再提醒")}
            </button>
          </div>
          <small>
            {compact
              ? ui(
                  "安装后，可在偏好设置的“浏览器采集”中完成连接；稍后设置也不影响本地整理。",
                )
            : verified
              ? ui("已验证扩展通信。暂时不可用时，可检查或修复本机连接。")
              : ui(
                  "尚未验证通信，不代表未安装。商店页面将在审核通过并上架后开放。",
                )}
          </small>
        </div>
      )}
      {notice && (
        <p role="status" className="subtle">
          {notice}
        </p>
      )}
      {error && (
        <p role="alert" className="tag-form-error">
          {error}
        </p>
      )}
    </div>
  );
}
