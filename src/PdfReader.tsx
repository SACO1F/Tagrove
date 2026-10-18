import { t as ui, tf } from "./i18n";
import { useEffect, useRef, useState } from "react";
import {
  ChevronLeft,
  ChevronRight,
  LoaderCircle,
  AlertCircle,
  RotateCcw,
  X,
} from "lucide-react";
import type { PDFDocumentLoadingTask, PDFDocumentProxy } from "pdfjs-dist";
import { api, LocalFile, message } from "./api";
import { pdfEngine, pdfOptions } from "./pdf";
import {
  PdfContinuousPages,
  type PdfScrollHandle,
  type PdfZoom,
} from "./PdfContinuousPages";
import "./pdf-reader.css";
import { PreviewLoading } from "./PreviewLoading";
import { ReaderZoomControl, useZoomWheel } from "./ReaderZoom";
import { useReadingValue, useReadingPosition } from "./ReadingState";

export function PdfReader({
  file,
  engine,
}: {
  file: LocalFile;
  engine?: string;
}) {
  const target = file.contentMatch?.location;
  const targetPage = target?.kind === "pdf" ? target.page : undefined;
  const [doc, setDoc] = useState<PDFDocumentProxy>();
  const [page, setPage] = useState(1);
  const [draft, setDraft] = useState("1");
  const [zoom, setZoom] = useReadingValue<PdfZoom>("pdfZoom", "page");
  const position = useReadingPosition();
  const initial = useRef({
    index: targetPage ? targetPage - 1 : position.get("pdfIndex"),
    fraction: targetPage ? 0 : position.get("pdfFraction"),
  });
  const [width, setWidth] = useState(600);
  const [height, setHeight] = useState(600);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [stopped, setStopped] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [passwordPrompt, setPasswordPrompt] = useState("");
  const [password, setPassword] = useState("");
  const passwordReply = useRef<((value: string) => void) | undefined>(
    undefined,
  );
  const viewport = useRef<HTMLDivElement>(null);
  const surface = useRef<PdfScrollHandle | null>(null);
  const pageInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!doc || !targetPage || stopped) return;
    const next = Math.min(doc.numPages, Math.max(1, targetPage));
    surface.current?.go(next);
    setPage(next);
    setDraft(String(next));
  }, [doc, targetPage, stopped]);

  useEffect(() => {
    if (stopped) {
      setDoc(undefined);
      setLoading(false);
      return;
    }
    let alive = true;
    let task: PDFDocumentLoadingTask | undefined;
    setDoc(undefined);
    setLoading(true);
    setError("");
    setPage(1);
    setDraft("1");
    setPasswordPrompt("");
    setPassword("");
    passwordReply.current = undefined;
    void (async () => {
      const pdf = await pdfEngine();
      if (!alive) return;
      const result = await api<{ data: string }>(
        engine ? "office.layout" : "pdf.document",
        {
          id: file.id,
          engine,
        },
      );
      if (!alive) return;
      const bytes = Uint8Array.from(atob(result.data), (c) => c.charCodeAt(0));
      task = pdf.getDocument(pdfOptions(bytes));
      task.onPassword = (reply: (value: string) => void, reason: number) => {
        if (!alive) return;
        passwordReply.current = reply;
        setPasswordPrompt(
          reason === 2 ? ui("密码不正确，请重试") : ui("此 PDF 需要密码"),
        );
        setPassword("");
        setLoading(false);
      };
      const document = await task.promise;
      if (!alive) return;
      passwordReply.current = undefined;
      setPasswordPrompt("");
      setPassword("");
      setDoc(document);
      setLoading(false);
    })().catch((e) => {
      if (alive) {
        setError(message(e));
        setLoading(false);
      }
    });
    return () => {
      alive = false;
      passwordReply.current = undefined;
      void task?.destroy().catch(() => {});
    };
  }, [
    file.id,
    file.revision,
    file.identity,
    file.status,
    attempt,
    stopped,
    engine,
  ]);

  useEffect(() => {
    const el = viewport.current;
    if (!el) return;
    let timer: ReturnType<typeof setTimeout>;
    const observer = new ResizeObserver(() => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        if (el.clientWidth >= 100) {
          setWidth(el.clientWidth);
          setHeight(el.clientHeight);
        }
      }, 100);
    });
    setWidth(el.clientWidth);
    setHeight(el.clientHeight);
    observer.observe(el);
    return () => {
      clearTimeout(timer);
      observer.disconnect();
    };
  }, []);

  function go(value: number) {
    if (!doc || !Number.isFinite(value)) {
      setDraft(String(page));
      return;
    }
    const next = Math.min(doc.numPages, Math.max(1, Math.round(value)));
    surface.current?.go(next);
    setPage(next);
    setDraft(String(next));
  }
  function scaleBy(direction: number, point?: { x: number; y: number }) {
    if (point) surface.current?.captureZoom(point);
    const current =
      typeof zoom === "number" ? zoom : surface.current?.fittedScale() || 1;
    setZoom(
      Math.min(
        4,
        Math.max(0.25, Math.round((current + direction * 0.25) * 100) / 100),
      ),
    );
  }
  useZoomWheel(viewport, scaleBy, !!doc && !stopped);
  function retry() {
    setStopped(false);
    setAttempt((a) => a + 1);
  }

  return (
    <section
      className="pdf-reader"
      aria-label={ui("PDF 连续阅读器")}
      onKeyDown={(e) => {
        if ((e.target as HTMLElement).closest("input,textarea")) return;
        if (e.key === "ArrowRight" || e.key === "PageDown") {
          e.preventDefault();
          e.stopPropagation();
          go(page + 1);
        }
        if (e.key === "ArrowLeft" || e.key === "PageUp") {
          e.preventDefault();
          e.stopPropagation();
          go(page - 1);
        }
      }}
    >
      <div
        className="pdf-toolbar"
        role="toolbar"
        aria-label={ui("PDF 阅读工具")}
      >
        {targetPage && (
          <button
            className="button quiet"
            disabled={!doc || stopped}
            data-tooltip={tf("搜索命中第 {0} 页", targetPage)}
            onClick={() => go(targetPage)}
          >
            {ui("返回命中页")}
          </button>
        )}
        <div className="pdf-control-group">
          <button
            className="icon-button"
            aria-label={ui("上一页")}
            data-tooltip={ui("上一页")}
            disabled={!doc || stopped || page <= 1}
            onClick={() => go(page - 1)}
          >
            <ChevronLeft size={17} />
          </button>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              go(Number(draft));
            }}
          >
            <input
              ref={pageInput}
              aria-label={ui("PDF 页码")}
              inputMode="numeric"
              value={draft}
              disabled={!doc || stopped}
              onChange={(e) => setDraft(e.target.value)}
              onBlur={() => {
                if (draft !== String(page)) go(Number(draft));
              }}
            />
            <span aria-label={ui("PDF 总页数")}>/ {doc?.numPages ?? "—"}</span>
          </form>
          <button
            className="icon-button"
            aria-label={ui("下一页")}
            data-tooltip={ui("下一页")}
            disabled={!doc || stopped || page >= doc.numPages}
            onClick={() => go(page + 1)}
          >
            <ChevronRight size={17} />
          </button>
        </div>
        <ReaderZoomControl
          onStep={scaleBy}
          label=" PDF"
          value={
            typeof zoom === "number"
              ? Math.round(zoom * 100)
              : zoom === "fit"
                ? "width"
                : zoom
          }
          scale={
            typeof zoom === "number"
              ? zoom
              : surface.current?.fittedScale() || 1
          }
          disabled={!doc || stopped}
          onChange={(next) =>
            setZoom(
              typeof next === "number"
                ? next / 100
                : next === "width"
                  ? "fit"
                  : next,
            )
          }
        />
        {(loading || passwordPrompt) && !stopped && (
          <button
            className="button pdf-cancel"
            onClick={() => setStopped(true)}
          >
            <X size={14} />
            {ui("取消加载")}
          </button>
        )}
      </div>
      <div
        ref={viewport}
        className="pdf-viewport is-continuous"
        tabIndex={0}
        aria-label={ui("PDF 页面")}
      >
        {doc && !error && !stopped && !passwordPrompt && (
          <PdfContinuousPages
            doc={doc}
            viewport={viewport}
            navigation={surface}
            width={width}
            height={height}
            zoom={zoom}
            initial={initial.current}
            onPosition={(index, fraction) => {
              position.set("pdfIndex", index);
              position.set("pdfFraction", fraction);
            }}
            onPage={(next) => {
              setPage(next);
              if (document.activeElement !== pageInput.current)
                setDraft(String(next));
            }}
          />
        )}
        {stopped ? (
          <div className="pdf-message" role="status">
            <span>{ui("预览已取消")}</span>
            <button className="button" onClick={retry}>
              <RotateCcw size={15} />
              {ui("重新加载")}
            </button>
          </div>
        ) : error ? (
          <div className="pdf-message" role="alert">
            <AlertCircle size={24} />
            <span>{error}</span>
            <small>{ui("可以使用下方按钮打开原文件")}</small>
            <button className="button" onClick={retry}>
              <RotateCcw size={15} />
              {ui("重试")}
            </button>
          </div>
        ) : passwordPrompt ? (
          <form
            className="pdf-message"
            onSubmit={(e) => {
              e.preventDefault();
              if (passwordReply.current) {
                setLoading(true);
                setPasswordPrompt("");
                passwordReply.current(password);
                setPassword("");
              }
            }}
          >
            <label htmlFor="pdf-password">{passwordPrompt}</label>
            <input
              id="pdf-password"
              type="password"
              autoComplete="off"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoFocus
            />
            <button className="button primary" type="submit">
              {ui("解锁阅读")}
            </button>
            <small>{ui("密码仅用于本次本地阅读")}</small>
          </form>
        ) : (
          loading && (
            <PreviewLoading
              label={engine ? "正在生成原排版预览…" : "正在打开 PDF…"}
            />
          )
        )}
      </div>
      <div className="pdf-reader-status" role="status">
        {doc && !stopped && !error
          ? tf("连续阅读 · 第 {0} 页 / 共 {1} 页", page, doc.numPages)
          : ui("本地只读 · 最大 64 MiB")}
      </div>
    </section>
  );
}
