import { useEffect, useState, type ReactNode } from "react";
import { PanelsTopLeft, FileText, ScanSearch } from "lucide-react";
import "./reader-controls.css";
import { api, type LocalFile, message } from "./api";
import { t as ui } from "./i18n";
import { GlideSelect } from "./GlideSelect";
import { PdfReader } from "./PdfReader";
import "./office-layout.css";
type Engine = { engine: string; kind: string };
const names: Record<string, string> = {
  microsoft: "Microsoft Office",
  wps: "WPS Office",
  libreoffice: "LibreOffice",
};
export function OfficeLayout({
  file,
  children,
  legacyContent,
}: {
  file: LocalFile;
  children?: ReactNode;
  legacyContent?: (engine: string) => ReactNode;
}) {
  const [engines, setEngines] = useState<Engine[]>([]),
    [engine, setEngine] = useState(""),
    [mode, setMode] = useState("layout"),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(true),
    [attempt, setAttempt] = useState(0);
  const kind = /^(doc|docx|docm|rtf)$/i.test(file.extension)
    ? "word"
    : /^(xls|xlsx|xlsm)$/i.test(file.extension)
      ? "excel"
      : "powerpoint";
  function refresh() {
    setLoading(true);
    setError("");
    void api<{ engines: Engine[] }>("office.engines")
      .then((result) => {
        setAttempt((value) => value + 1);
        const available = result.engines.filter((e) => e.kind === kind);
        setEngines(available);
        setEngine((current) =>
          available.some(
            (e) =>
              e.engine ===
              (current ||
                localStorage.getItem("shiqian:office-engine:" + kind)),
          )
            ? current ||
              localStorage.getItem("shiqian:office-engine:" + kind) ||
              ""
            : available[0]?.engine || "",
        );
      })
      .catch((e) => setError(message(e)))
      .finally(() => setLoading(false));
  }
  useEffect(() => {
    if (mode === "layout" || legacyContent) refresh();
  }, [kind, mode]);
  return (
    <section className="office-layout">
      <div className="office-layout-toolbar">
        <div
          className="reader-control-group reader-mode-control"
          role="group"
          aria-label={ui("Office 预览模式")}
        >
          <button
            className="button quiet"
            aria-pressed={mode === "layout"}
            onClick={() => setMode("layout")}
          >
            <PanelsTopLeft size={14} />
            {ui("原排版")}
          </button>
          {(children || legacyContent) && (
            <button
              className="button"
              aria-pressed={mode === "content"}
              onClick={() => setMode("content")}
            >
              <FileText size={14} />
              {ui("内容预览")}
            </button>
          )}
        </div>
        {mode === "layout" && engines.length > 0 && (
          <GlideSelect
            value={engine}
            options={[
              ...engines.map((e) => ({
                value: e.engine,
                label: names[e.engine],
              })),
              { value: "install", label: ui("安装增强组件") },
            ]}
            onChange={(value) => {
              if (value === "install") {
                void api("office.component").catch((e) => setError(message(e)));
                return;
              }
              setEngine(value);
              localStorage.setItem("shiqian:office-engine:" + kind, value);
            }}
            ariaLabel={ui("预览引擎")}
          />
        )}
        {mode === "layout" && (
          <div className="reader-mode-control">
            <button
              className="button quiet"
              onClick={refresh}
              disabled={loading}
            >
              <ScanSearch size={14} />
              {ui("检测引擎")}
            </button>
          </div>
        )}
      </div>
      {error && engine && <p role="alert" className="office-engine-error">{error}</p>}
      {mode === "content" ? (
        legacyContent && loading && !engine ? (
          <div className="office-engine-status" role="status">
            <span className="sr-only">{ui("正在检测本机预览引擎…")}</span>
          </div>
        ) : (
          children ||
          (engine && legacyContent?.(engine)) || (
            <p>{ui("请先选择可用预览引擎")}</p>
          )
        )
      ) : loading && !engine ? (
        <div className="office-engine-status" role="status">
          <span className="sr-only">{ui("正在检测本机预览引擎…")}</span>
        </div>
      ) : engine ? (
        <PdfReader key={`${file.id}:${engine}:${attempt}`} file={file} engine={engine} />
      ) : (
        <div className="reader-message">
          <p>
            {error ||
              ui(
                "未检测到可用预览引擎。安装 Microsoft Office、WPS Office 或 LibreOffice 后重新检测。",
              )}
          </p>
          <button
            className="button"
            onClick={() =>
              void api("office.component").catch((e) => setError(message(e)))
            }
          >
            {ui("按需安装增强预览组件")}
          </button>
        </div>
      )}
    </section>
  );
}
