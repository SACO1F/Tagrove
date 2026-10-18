import { t as ui } from "./i18n";
import { useEffect, useMemo, useState } from "react";
import { AlertCircle, Code, Eye, LoaderCircle } from "lucide-react";
import { api, LocalFile, message } from "./api";
import "./reader-controls.css";
import DOMPurify from "dompurify";
import { useReadingValue, useReadingScroll } from "./ReadingState";
import { useRef } from "react";
import { ZoomFrame } from "./ZoomFrame";
export function isHtml(file: LocalFile) {
  return /^(html|htm)$/i.test(file.extension);
}
export function HtmlReader({ file }: { file: LocalFile }) {
  const [text, setText] = useState<string>();
  const [error, setError] = useState("");
  const [source, setSource] = useReadingValue<boolean>("htmlSource", false);
  const scroll = useRef<HTMLPreElement>(null);
  useReadingScroll(scroll, "htmlSource", source && text !== undefined);
  useEffect(() => {
    let alive = true;
    setText(undefined);
    setError("");
    void api<{ text: string }>("html.document", { id: file.id })
      .then((r) => {
        if (alive) setText(r.text);
      })
      .catch((e) => {
        if (alive) setError(message(e));
      });
    return () => {
      alive = false;
    };
  }, [file.id, file.revision, file.identity, file.status]);
  const content = useMemo(() => {
    if (text === undefined) return "";
    // Sanitize file markup; the opaque frame only permits its trusted zoom bridge.
    const policy =
      "default-src 'none'; script-src 'none'; connect-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; object-src 'none'; frame-src 'none'; base-uri 'none'; form-action 'none'";
    const staticText = DOMPurify.sanitize(text, {
      USE_PROFILES: { html: true },
      WHOLE_DOCUMENT: true,
      FORBID_TAGS: [
        "script",
        "iframe",
        "form",
        "input",
        "meta",
        "base",
        "object",
        "embed",
      ],
      FORBID_ATTR: ["srcset"],
      ALLOWED_URI_REGEXP:
        /^(?:data:image\/(?:png|jpe?g|gif|webp|svg\+xml);|#)/i,
    });
    return `<!doctype html><html><head><meta http-equiv="Content-Security-Policy" content="${policy}"><meta charset="utf-8"><style>body{margin:20px;overflow-wrap:anywhere;font:15px/1.6 system-ui,sans-serif;color:#2c3037;background:white}img{max-width:100%}a{pointer-events:none!important}</style></head><body>${staticText}</body></html>`;
  }, [text]);
  return (
    <section className="html-reader" aria-label={ui("HTML 本地预览")}>
      <div
        className="pdf-toolbar"
        role="toolbar"
        aria-label={ui("HTML 预览工具")}
      >
        <div
          className="pdf-control-group reader-mode-control"
          role="group"
          aria-label={ui("HTML 阅读模式")}
        >
          <button
            className="button"
            aria-pressed={!source}
            onClick={() => setSource(false)}
          >
            <Eye size={15} />
            {ui("静态预览")}
          </button>
          <button
            className="button"
            aria-pressed={source}
            onClick={() => setSource(true)}
          >
            <Code size={15} />
            {ui("源码")}
          </button>
        </div>
        <span className="subtle">{ui("脚本和外部资源已禁用")}</span>
      </div>
      {error ? (
        <div className="pdf-message" role="alert">
          <AlertCircle size={24} />
          <span>{error}</span>
          <small>{ui("可以使用下方按钮打开原文件")}</small>
        </div>
      ) : text === undefined ? (
        <div className="pdf-message" role="status">
          <LoaderCircle className="spin" size={24} />
          <span>{ui("正在读取 HTML…")}</span>
        </div>
      ) : source ? (
        <pre ref={scroll} className="html-source">
          {text}
        </pre>
      ) : (
        <ZoomFrame
          title={ui("HTML 静态页面")}
          document={content}
          stateKey="html"
        />
      )}
    </section>
  );
}
