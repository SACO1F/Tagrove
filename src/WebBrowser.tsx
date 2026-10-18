import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  ArrowLeft,
  ArrowRight,
  RotateCw,
  ExternalLink,
  Globe,
  BookOpen,
} from "lucide-react";
import { api, message, type LocalFile } from "./api";
import { t as ui } from "./i18n";
import "./web-browser.css";
import "./reader-controls.css";
export function WebBrowser({
  file,
  children,
}: {
  file: LocalFile;
  children: ReactNode;
}) {
  const [online, setOnline] = useState(true),
    [error, setError] = useState("");
  const host = useRef<HTMLDivElement>(null);
  const activeSession = useRef("");
  const [attempt, setAttempt] = useState(0);
  const [url, setUrl] = useState(file.webSource?.url || "");
  useEffect(() => {
    if (!online) return;
    const session = crypto.randomUUID();
    activeSession.current = session;
    setError("");
    let alive = true,
      ready = false,
      opening = false,
      busy = false,
      last = "",
      lastStatus = 0,
      failed = false;
    const payload = { session, id: file.id };
    function visibility() {
      const el = host.current;
      if (!el) return { active: false, hidden: true, r: null };
      const r = el.getBoundingClientRect();
      const dialogs = [
        ...document.querySelectorAll<HTMLElement>(
          '[role="dialog"][aria-modal="true"]',
        ),
      ].filter(
        (d) =>
          d.getClientRects().length && d.getAttribute("aria-hidden") !== "true",
      );
      const top = dialogs.at(-1);
      const active =
        !document.hidden &&
        !el.closest('[inert],[aria-hidden="true"]') &&
        (!top || top.contains(el)) &&
        r.width >= 20 &&
        r.height >= 20;
      // A file list is also a listbox: only actual overlapping popup surfaces obscure the view.
      const hidden =
        !active ||
        [
          ...document.querySelectorAll<HTMLElement>(
            '[role="listbox"],[role="menu"]',
          ),
        ].some((menu) => {
          if (
            menu.classList.contains("file-area") ||
            menu.contains(el) ||
            !menu.getClientRects().length
          )
            return false;
          const b = menu.getBoundingClientRect();
          return (
            b.left < r.right &&
            b.right > r.left &&
            b.top < r.bottom &&
            b.bottom > r.top
          );
        });
      return { active, hidden, r };
    }
    async function sync() {
      if (!alive || busy || failed) return;
      const { active, hidden, r } = visibility();
      if (!active) {
        if (ready) {
          ready = false;
          last = "";
          void api("browser.close", payload).catch(() => {});
        }
        return;
      }
      if (!ready) {
        if (opening) return;
        opening = true;
        try {
          await api("browser.open", payload);
          if (alive) {
            ready = true;
            last = "";
          } else await api("browser.close", payload);
        } catch (e) {
          failed = true;
          if (alive) setError(message(e));
        } finally {
          opening = false;
        }
        return;
      }
      const bounds = {
        ...payload,
        x: r!.x,
        y: r!.y,
        width: r!.width,
        height: r!.height,
        hidden,
      };
      const key = JSON.stringify(bounds);
      busy = true;
      try {
        if (key !== last) {
          const result = await api<{ inactive?: boolean }>(
            "browser.bounds",
            bounds,
          );
          if (result.inactive) {
            ready = false;
            last = "";
            return;
          }
          last = key;
        }
        if (Date.now() - lastStatus > 1000) {
          lastStatus = Date.now();
          const status = await api<{ url?: string; inactive?: boolean }>(
            "browser.status",
            payload,
          );
          if (status.inactive) {
            ready = false;
            last = "";
          } else if (alive && status.url) setUrl(status.url);
        }
      } catch (e) {
        if (alive) setError(message(e));
      } finally {
        busy = false;
      }
    }
    const timer = setInterval(() => void sync(), 50);
    void sync();
    return () => {
      alive = false;
      clearInterval(timer);
      void api("browser.close", payload).catch(() => {});
    };
  }, [online, file.id, attempt]);
  return (
    <section className="web-browser-reader">
      <div className="web-browser-toolbar">
        <div
          className="reader-control-group reader-mode-control"
          role="group"
          aria-label={ui("网页预览模式")}
        >
          <button
            className="button quiet"
            aria-pressed={online}
            onClick={() => {
              setError("");
              setOnline(true);
            }}
          >
            <Globe size={14} />
            {ui("在线网页")}
          </button>
          <button
            className="button quiet"
            aria-pressed={!online}
            onClick={() => setOnline(false)}
          >
            <BookOpen size={14} />
            {ui("阅读副本")}
          </button>
        </div>
        {online && (
          <>
            {[
              ["browser.back", ArrowLeft, "后退"],
              ["browser.forward", ArrowRight, "前进"],
              ["browser.reload", RotateCw, "刷新网页"],
            ].map(([action, Icon, label]) => {
              const Glyph = Icon as typeof ArrowLeft;
              return (
                <button
                  key={String(action)}
                  className="icon-button"
                  aria-label={ui(String(label))}
                  onClick={() =>
                    void api(String(action), {
                      session: activeSession.current,
                    }).catch((e) => setError(message(e)))
                  }
                >
                  <Glyph size={16} />
                </button>
              );
            })}
            <button
              className="icon-button"
              aria-label={ui("在浏览器中打开")}
              onClick={() => void api("capture.source.open", { id: file.id })}
            >
              <ExternalLink size={16} />
            </button>
          </>
        )}
      </div>
      {online && (
        <div className="web-browser-address" data-tooltip={url}>
          {url}
        </div>
      )}
      {error && (
        <div role="alert">
          <p>{error}</p>
          <button className="button" onClick={() => setAttempt((n) => n + 1)}>
            {ui("重试网页")}
          </button>
        </div>
      )}
      {online ? <div className="web-browser-host" ref={host} /> : children}
    </section>
  );
}
