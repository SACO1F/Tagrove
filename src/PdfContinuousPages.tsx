import { pdfEngine } from "./pdf";
import { t as ui, tf } from "./i18n";
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useImperativeHandle,
  type RefObject,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { AlertCircle, LoaderCircle, RotateCcw } from "lucide-react";
import type { PDFDocumentProxy, PDFPageProxy, RenderTask } from "pdfjs-dist";
import { message } from "./api";

type PageSize = { width: number; height: number };
export type PdfScrollHandle = {
  go: (page: number) => void;
  fittedScale: () => number;
  captureZoom: (point: { x: number; y: number }) => void;
};
export type PdfZoom = number | "fit" | "auto" | "page";
const initialSize = { width: 595, height: 842 };
function scaledSize(
  size: PageSize,
  width: number,
  zoom: PdfZoom,
  height: number,
) {
  const fitWidth = Math.max(0.1, (width - 40) / size.width);
  const fitPage = Math.max(
    0.1,
    Math.min(fitWidth, (height - 40) / size.height),
  );
  const scale =
    typeof zoom === "number"
      ? zoom
      : zoom === "fit"
        ? fitWidth
        : zoom === "page"
          ? fitPage
          : Math.min(1, fitWidth);
  return { width: size.width * scale, height: size.height * scale, scale };
}

/** Only visible pages and their immediate neighbours allocate canvases. */
export function PdfContinuousPages({
  doc,
  viewport,
  navigation,
  width,
  height,
  zoom,
  onPage,
  initial = { index: 0, fraction: 0 },
  onPosition,
}: {
  doc: PDFDocumentProxy;
  viewport: RefObject<HTMLDivElement | null>;
  navigation: RefObject<PdfScrollHandle | null>;
  width: number;
  height: number;
  zoom: PdfZoom;
  onPage: (page: number) => void;
  initial?: { index: number; fraction: number };
  onPosition?: (index: number, fraction: number) => void;
}) {
  const sizes = useRef(new Map<number, PageSize>());
  const fallback = useRef(initialSize);
  const pointerAnchor = useRef<
    | {
        index: number;
        rx: number;
        ry: number;
        x: number;
        y: number;
      }
    | undefined
  >(undefined);
  const anchor = useRef({
    index: Math.max(0, Math.min(doc.numPages - 1, Math.round(initial.index))),
    fraction: Math.max(0, Math.min(1, initial.fraction)),
  });
  const anchorLock = useRef<{ index: number; fraction: number } | null>(null);
  const settleTimer = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  const geometry = useRef({ width, height, zoom });
  const surface = useRef<HTMLDivElement>(null);
  const pageChanged = useRef(onPage);
  const positionChanged = useRef(onPosition);
  positionChanged.current = onPosition;
  pageChanged.current = onPage;
  const virtual = useVirtualizer({
    count: doc.numPages,
    getScrollElement: () => viewport.current,
    estimateSize: (index) =>
      scaledSize(
        sizes.current.get(index) || fallback.current,
        width,
        zoom,
        height,
      ).height,
    overscan: 1,
    gap: 18,
    paddingStart: 20,
    paddingEnd: 20,
    initialOffset: () =>
      anchor.current.index *
        (scaledSize(initialSize, width, zoom, height).height + 18) +
      20,
    onChange: (instance) => {
      if (
        anchorLock.current ||
        geometry.current.width !== width ||
        geometry.current.height !== height ||
        geometry.current.zoom !== zoom
      )
        return;
      const offset = instance.scrollOffset || 0;
      const item = instance
        .getVirtualItems()
        .find((item) => item.end > offset + 21);
      if (!item) return;
      anchor.current = {
        index: item.index,
        fraction: Math.max(0, (offset - item.start) / item.size),
      };
      positionChanged.current?.(anchor.current.index, anchor.current.fraction);
      const readingHeight = viewport.current?.clientHeight || 0;
      // Follow the upper reading area rather than the centre: a landscape
      // page must not immediately report the next portrait page after a jump.
      const readingPoint = offset + Math.min(160, readingHeight / 4);
      const current = instance
        .getVirtualItems()
        .find((row) => row.end > readingPoint);
      const atEnd =
        offset > 20 && offset + readingHeight >= instance.getTotalSize() - 21;
      pageChanged.current(atEnd ? doc.numPages : (current || item).index + 1);
    },
  });
  function restoreAnchor(saved: { index: number; fraction: number }) {
    const total = virtual.getTotalSize();
    if (surface.current) surface.current.style.height = `${total}px`;
    const offset = virtual.getOffsetForIndex(saved.index, "start")?.[0] || 0;
    const size = scaledSize(
      sizes.current.get(saved.index) || fallback.current,
      width,
      zoom,
      height,
    );
    const p = pointerAnchor.current,
      el = viewport.current;
    if (p && el) {
      const pageSize = scaledSize(
        sizes.current.get(p.index) || fallback.current,
        width,
        zoom,
        height,
      );
      const pageOffset = virtual.getOffsetForIndex(p.index, "start")?.[0] || 0;
      virtual.scrollToOffset(pageOffset + p.ry * pageSize.height - p.y);
      el.scrollLeft =
        Math.max(
          0,
          ((surface.current?.clientWidth || 0) - pageSize.width) / 2,
        ) +
        p.rx * pageSize.width -
        p.x;
    } else virtual.scrollToOffset(offset + saved.fraction * size.height);
  }
  function settleAnchor(saved: { index: number; fraction: number }) {
    anchorLock.current = saved;
    anchor.current = saved;
    clearTimeout(settleTimer.current);
    settleTimer.current = setTimeout(() => {
      restoreAnchor(saved);
      anchorLock.current = null;
      pageChanged.current((pointerAnchor.current?.index ?? saved.index) + 1);
      positionChanged.current?.(anchor.current.index, anchor.current.fraction);
      pointerAnchor.current = undefined;
    }, 250);
  }
  useEffect(() => {
    const el = viewport.current;
    if (!el) return;
    const release = () => {
      clearTimeout(settleTimer.current);
      anchorLock.current = null;
      pointerAnchor.current = undefined;
    };
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) release();
    };
    el.addEventListener("wheel", wheel, { passive: true });
    el.addEventListener("pointerdown", release);
    el.addEventListener("touchstart", release, { passive: true });
    return () => {
      release();
      el.removeEventListener("wheel", wheel);
      el.removeEventListener("pointerdown", release);
      el.removeEventListener("touchstart", release);
    };
  }, [viewport]);
  useLayoutEffect(() => {
    const saved = { ...anchor.current };
    // New page dimensions arrive asynchronously after a scale change. Retain
    // the logical reading anchor until those measurements have settled.
    anchorLock.current = saved;
    virtual.measure();
    // Materialize new positions and commit the scroll extent before the
    // browser clamps the restored reading anchor to the previous size.
    restoreAnchor(saved);
    geometry.current = { width, height, zoom };
    settleAnchor(saved);
  }, [width, height, zoom, virtual]);
  useImperativeHandle(
    navigation,
    () => ({
      captureZoom: (point) => {
        const el = viewport.current;
        if (!el) return;
        const bounds = el.getBoundingClientRect();
        const pages = [...el.querySelectorAll<HTMLElement>(".pdf-page-slot")];
        const slot = pages.find((el) => {
          const r = el.getBoundingClientRect();
          return point.y >= r.top && point.y <= r.bottom;
        });
        const page = slot?.querySelector<HTMLElement>(".pdf-page");
        if (!page || !slot) return;
        const r = page.getBoundingClientRect();
        pointerAnchor.current = {
          index: Number(slot.dataset.page) - 1,
          rx: (point.x - r.left) / r.width,
          ry: (point.y - r.top) / r.height,
          x: point.x - bounds.left,
          y: point.y - bounds.top,
        };
      },
      go: (page) => {
        pointerAnchor.current = undefined;
        const index = Math.min(doc.numPages - 1, Math.max(0, page - 1));
        anchor.current = { index, fraction: 0 };
        settleAnchor(anchor.current);
        virtual.scrollToIndex(index, { align: "start" });
        pageChanged.current(index + 1);
      },
      fittedScale: () =>
        scaledSize(
          sizes.current.get(anchor.current.index) || fallback.current,
          width,
          zoom,
          height,
        ).scale,
    }),
    [doc, virtual, width, height, zoom],
  );
  const items = virtual.getVirtualItems();
  const widest = Math.max(
    width - 40,
    ...items.map(
      (item) =>
        scaledSize(
          sizes.current.get(item.index) || fallback.current,
          width,
          zoom,
          height,
        ).width,
    ),
  );
  return (
    <div
      ref={surface}
      className="pdf-surface pdf-continuous-surface"
      style={{ height: virtual.getTotalSize(), width: widest + 40 }}
    >
      {items.map((item) => (
        <div
          key={item.key}
          className="pdf-page-slot"
          data-page={item.index + 1}
          style={{
            transform: `translateY(${item.start}px)`,
            height: item.size,
          }}
        >
          <PdfPage
            doc={doc}
            page={item.index + 1}
            width={width}
            height={height}
            zoom={zoom}
            estimate={sizes.current.get(item.index) || fallback.current}
            onSize={(size) => {
              sizes.current.set(item.index, size);
              if (item.index === 0) fallback.current = size;
              const saved = anchorLock.current;
              if (saved)
                virtual.shouldAdjustScrollPositionOnItemSizeChange = () =>
                  false;
              virtual.resizeItem(
                item.index,
                scaledSize(size, width, zoom, height).height,
              );
              virtual.shouldAdjustScrollPositionOnItemSizeChange = undefined;
              if (saved) {
                restoreAnchor(saved);
                settleAnchor(saved);
              }
            }}
          />
        </div>
      ))}
    </div>
  );
}

function PdfPage({
  doc,
  page,
  width,
  height,
  zoom,
  estimate,
  onSize,
}: {
  doc: PDFDocumentProxy;
  page: number;
  width: number;
  height: number;
  zoom: PdfZoom;
  estimate: PageSize;
  onSize: (size: PageSize) => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const sizeCallback = useRef(onSize);
  sizeCallback.current = onSize;
  const [size, setSize] = useState(estimate);
  const [ready, setReady] = useState(false),
    [error, setError] = useState(""),
    [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const el = host.current;
    return () => {
      el?.querySelectorAll("canvas").forEach((canvas) => {
        canvas.width = 1;
        canvas.height = 1;
      });
      el?.replaceChildren();
    };
  }, []);
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    let alive = true,
      render: RenderTask | undefined,
      sheet: PDFPageProxy | undefined;
    let textLayer: { cancel(): void; render(): Promise<void> } | undefined;
    const canvas = document.createElement("canvas");
    if (!el.querySelector("canvas")) setReady(false);
    setError("");
    // Keep the previous raster visible until its replacement is ready on resize.
    void (async () => {
      sheet = await doc.getPage(page);
      if (!alive) return;
      const base = sheet.getViewport({ scale: 1 });
      const dimensions = { width: base.width, height: base.height };
      setSize(dimensions);
      sizeCallback.current(dimensions);
      const css = scaledSize(dimensions, width, zoom, height);
      const output = Math.min(
        window.devicePixelRatio || 1,
        2,
        Math.sqrt(8_000_000 / (css.width * css.height)),
        8192 / Math.max(css.width, css.height),
      );
      const pixels = sheet.getViewport({ scale: css.scale * output });
      canvas.width = Math.ceil(pixels.width);
      canvas.height = Math.ceil(pixels.height);
      canvas.style.width = "100%";
      canvas.style.height = "100%";
      canvas.setAttribute("role", "img");
      canvas.setAttribute(
        "aria-label",
        tf("第 {0} 页，共 {1} 页", page, doc.numPages),
      );
      render = sheet.render({
        canvas,
        viewport: pixels,
        background: "#ffffff",
      });
      await render.promise;
      const text = document.createElement("div");
      text.className = "pdf-text-layer";
      text.style.setProperty(
        "--total-scale-factor",
        String(css.scale * sheet.userUnit),
      );
      const { TextLayer } = await pdfEngine();
      textLayer = new TextLayer({
        textContentSource: await sheet.getTextContent(),
        container: text,
        viewport: sheet.getViewport({ scale: css.scale }),
      });
      await textLayer.render();
      if (alive) {
        el.querySelectorAll("canvas").forEach((old) => {
          old.width = 1;
          old.height = 1;
        });
        el.replaceChildren(canvas, text);
        setReady(true);
      }
    })()
      .catch((e) => {
        if (alive) setError(message(e));
      })
      .finally(() => {
        sheet?.cleanup();
        if (!alive && !el.contains(canvas)) {
          canvas.width = 1;
          canvas.height = 1;
        }
      });
    return () => {
      alive = false;
      render?.cancel();
      textLayer?.cancel();
    };
  }, [doc, page, width, height, zoom, attempt]);
  const css = scaledSize(size, width, zoom, height);
  return (
    <div
      className="pdf-page"
      style={{ width: css.width, height: css.height }}
      aria-label={tf("PDF 第 {0} 页", page)}
    >
      <div ref={host} className="pdf-page-raster" />
      {error ? (
        <div className="pdf-page-message" role="alert">
          <AlertCircle size={20} />
          <span>
            {ui("第")}
            {page} {ui("页：")}
            {error}
          </span>
          <button className="button" onClick={() => setAttempt((n) => n + 1)}>
            <RotateCcw size={14} />
            {ui("重试此页")}
          </button>
        </div>
      ) : (
        !ready && (
          <div className="pdf-page-message" role="status">
            <LoaderCircle className="spin" size={20} />
            <span>
              {ui("正在渲染第")}
              {page} {ui("页…")}
            </span>
          </div>
        )
      )}
    </div>
  );
}
