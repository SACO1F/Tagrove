import { t as ui, tf } from "./i18n";
import { useEffect, useState } from "react";
import {
  Globe,
  FileText,
  File,
  ImageIcon,
  LoaderCircle,
  AlertCircle,
} from "lucide-react";
import { pdfEngine, pdfOptions } from "./pdf";
import { api, LocalFile, message } from "./api";
import { PreviewCache } from "./preview-cache";
import { FileGlyph } from "./FileGlyph";

interface PreviewData {
  kind: string;
  data?: string;
  text?: string;
  truncated?: boolean;
  revision: string;
  error?: string;
}
let active = 0;
const waiting: Array<() => void> = [];
const previews = new Map<string, Promise<PreviewData>>();
const readyPreviews = new PreviewCache<PreviewData>(24 * 1024 * 1024);
const ratios = new Map<string, number>();
export function previewKey(file: LocalFile) {
  return `${file.id}:${file.revision}:${file.identity}:${file.status}`;
}
export function previewRatio(file: LocalFile) {
  return ratios.get(previewKey(file)) ?? (file.kind === "pdf" ? 0.707 : 4 / 3);
}
async function limited<T>(fn: () => Promise<T>): Promise<T> {
  if (active >= 3) await new Promise<void>((r) => waiting.push(r));
  active++;
  try {
    return await fn();
  } finally {
    active--;
    waiting.shift()?.();
  }
}
export function clearPreviews() {
  previews.clear();
  readyPreviews.clear();
  ratios.clear();
}
async function load(file: LocalFile, large: boolean): Promise<PreviewData> {
  const key = `${previewKey(file)}:${large}`;
  const ready = readyPreviews.get(key);
  if (ready) return ready;
  if (!previews.has(key)) {
    const promise = limited(async () => {
      const value = await api<PreviewData>(
        /^(docx|xlsx|csv|tsv)$/i.test(file.extension)
          ? "office.summary"
          : "preview",
        { id: file.id, large },
      );
      if (value.kind !== "pdf") return value;
      const pdf = await pdfEngine();
      const data = Uint8Array.from(atob(value.data!), (c) => c.charCodeAt(0));
      const task = pdf.getDocument(pdfOptions(data));
      try {
        const doc = await task.promise;
        const page = await doc.getPage(1);
        const base = page.getViewport({ scale: 1 });
        const scale = (large ? 1800 : 420) / Math.max(base.width, base.height);
        const viewport = page.getViewport({ scale });
        const canvas = document.createElement("canvas");
        canvas.width = Math.ceil(viewport.width);
        canvas.height = Math.ceil(viewport.height);
        await page.render({ canvas, viewport }).promise;
        const encoded = canvas.toDataURL("image/png").split(",")[1];
        await api("preview.cache", {
          id: file.id,
          large,
          revision: value.revision,
          data: encoded,
        }).catch(() => {});
        return { ...value, kind: "image", data: encoded };
      } finally {
        await task.destroy();
      }
    })
      .then((value) => {
        if (previews.get(key) === promise) {
          readyPreviews.set(key, value);
          // Keep only pending promises; resolved promises retain the large payload.
          previews.delete(key);
        }
        return value;
      })
      .catch((error) => {
        if (previews.get(key) === promise) previews.delete(key);
        return {
          kind: "error",
          revision: file.revision,
          error: message(error),
        };
      });
    previews.set(key, promise);
  }
  return previews.get(key)!;
}
export function Preview({
  file,
  large = false,
  compact = false,
  onDimensions,
}: {
  file: LocalFile;
  large?: boolean;
  compact?: boolean;
  onDimensions?: (width: number, height: number) => void;
}) {
  const key = `${previewKey(file)}:${large}`;
  const [result, setResult] = useState<PreviewData | undefined>(() =>
    readyPreviews.get(key),
  );
  useEffect(() => {
    let alive = true;
    setResult(readyPreviews.get(key));
    if (
      file.kind === "image" ||
      file.kind === "pdf" ||
      (large && file.kind === "text") ||
      (!compact && /^(docx|xlsx|csv|tsv)$/i.test(file.extension))
    )
      load(file, large).then((r) => {
        if (alive) setResult(r);
      });
    else setResult({ kind: "unsupported", revision: file.revision });
    return () => {
      alive = false;
    };
  }, [file.id, file.revision, file.identity, file.status, large, compact]);
  if (file.webSource && file.kind === "web")
    return (
      <div className={`web-card-preview ${compact ? "is-compact" : ""}`}>
        <Globe size={compact ? 26 : 38} strokeWidth={1.5} />
        {!compact && (
          <>
            <small>
              {file.webSource.kind === "web_excerpt"
                ? ui("文字摘录")
                : ui("网页链接")}
            </small>
            {(file.webSource.reading?.text ?? file.webSource.excerpt) && (
              <p>{file.webSource.reading?.text ?? file.webSource.excerpt}</p>
            )}
            <small>{new URL(file.webSource.url).hostname}</small>
          </>
        )}
      </div>
    );
  if (compact && file.kind !== "image" && file.kind !== "pdf")
    return (
      <div className="preview-placeholder file-glyph-placeholder">
        <FileGlyph extension={file.extension} compact />
      </div>
    );
  if (!result)
    return (
      <div className="preview-placeholder">
        <LoaderCircle className="spin" size={20} />
      </div>
    );
  if (result.kind === "image")
    return (
      <img
        className="preview-image"
        src={`data:image/png;base64,${result.data}`}
        alt={file.name}
        draggable={false}
        onLoad={(event) => {
          const { naturalWidth: width, naturalHeight: height } =
            event.currentTarget;
          if (width > 0 && height > 0) {
            ratios.set(previewKey(file), width / height);
            if (ratios.size > 300) ratios.delete(ratios.keys().next().value!);
            onDimensions?.(width, height);
          }
        }}
      />
    );
  if (result.kind === "office-summary" && !compact)
    return (
      <div className="office-card-summary">
        <FileGlyph extension={file.extension} compact />
        <span className="extension-stamp">{file.extension.toUpperCase()}</span>
        <pre>{result.text || ui("暂无文字摘要")}</pre>
      </div>
    );
  if (result.kind === "text")
    return (
      <div className="text-preview">
        <pre>{result.text || ui("（空文件）")}</pre>
        {result.truncated && <small>{ui("仅预览前 200 KiB")}</small>}
      </div>
    );
  if (result.kind === "error" && !/^(docx|xlsx|csv|tsv)$/i.test(file.extension))
    return (
      <div
        className="preview-placeholder preview-error"
        data-tooltip={result.error}
      >
        <AlertCircle size={compact ? 20 : 28} />
        {!compact && <span>{large ? result.error : ui("无法预览")}</span>}
      </div>
    );
  const Icon =
    file.kind === "image" ? ImageIcon : file.kind === "other" ? File : FileText;
  return (
    <div className={`preview-placeholder file-type-${file.kind}`}>
      {file.kind === "image" ||
      (file.kind === "other" && !/^(html?|xml)$/i.test(file.extension)) ? (
        <Icon size={compact ? 23 : 40} strokeWidth={1.25} />
      ) : (
        <FileGlyph extension={file.extension} compact={compact} />
      )}
      {!compact && (
        <span className="extension-stamp">
          {file.extension.toUpperCase() || "FILE"}
        </span>
      )}
      {large && <small>{ui("使用默认程序打开以查看完整内容")}</small>}
    </div>
  );
}
