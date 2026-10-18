import { t as ui, tf } from "./i18n";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { AlertCircle, LoaderCircle } from "lucide-react";
import { api, LocalFile, message, ContentLocation } from "./api";
import { GlideSelect } from "./GlideSelect";
import "./office-reader.css";
import {
  useReadingValue,
  useReadingScroll,
  useReadingPosition,
} from "./ReadingState";
import { displayCell, OfficeCell } from "./office-format";

type Paragraph = {
  type: "paragraph";
  heading: number;
  label?: string;
  level?: number;
  runs: {
    text?: string;
    bold?: boolean;
    italic?: boolean;
    image?: string;
    imageError?: string;
  }[];
};
type Block = Paragraph | { type: "table"; rows: Paragraph[][][] };
type Cell = OfficeCell | "";
type Images = Record<string, { data: string; width: number; height: number }>;
type Document = {
  kind: "docx";
  blocks: Block[];
  images: Images;
  warnings: string[];
};
type Sheet = {
  kind: "sheet";
  sheets: string[];
  selected: number;
  columns: number;
  rows: { number: number; cells: Cell[] }[];
  truncated: boolean;
  date1904?: boolean;
  mergedRanges?: string[];
};
export function isOfficePreview(file: LocalFile) {
  return /^(docx|xlsx|csv|tsv)$/i.test(file.extension);
}
function ParagraphView({ p, images }: { p: Paragraph; images: Images }) {
  return (
    <p
      className={p.heading ? `office-heading office-heading-${p.heading}` : ""}
      role={p.heading ? "heading" : undefined}
      aria-level={p.heading || undefined}
      style={{ paddingLeft: p.label ? `${(p.level || 0) * 20}px` : undefined }}
    >
      {p.label && <span className="office-list-marker">{p.label}</span>}
      {p.runs.map((r, i) =>
        r.image && images[r.image] ? (
          <img
            key={i}
            className="office-embedded-image"
            src={`data:image/png;base64,${images[r.image].data}`}
            width={images[r.image].width}
            height={images[r.image].height}
            alt={ui("文档内嵌图片")}
          />
        ) : r.imageError ? (
          <span key={i} className="office-image-error">
            {r.imageError}
          </span>
        ) : (
          <span
            key={i}
            style={{
              fontWeight: r.bold ? 650 : undefined,
              fontStyle: r.italic ? "italic" : undefined,
            }}
          >
            {r.text}
          </span>
        ),
      )}
    </p>
  );
}
function WordView({
  data,
  target,
  jump,
}: {
  data: Document;
  target?: ContentLocation | null;
  jump: number;
}) {
  const scroll = useRef<HTMLDivElement>(null);
  const position = useReadingPosition();
  const saved = useRef({
    index: position.get("wordIndex"),
    fraction: position.get("wordFraction"),
  });
  const restoring = useRef(!target && position.get("wordSeen") === 1);
  const virtual = useVirtualizer({
    count: data.blocks.length,
    getScrollElement: () => scroll.current,
    estimateSize: () => 64,
    initialOffset: () => (target ? 0 : saved.current.index * 64),
    overscan: 8,
    onChange: (instance) => {
      if (restoring.current) return;
      const top = instance.scrollOffset || 0;
      const item = instance.getVirtualItems().find((row) => row.end > top);
      if (!item) return;
      position.set("wordSeen", 1);
      position.set("wordIndex", item.index);
      position.set(
        "wordFraction",
        Math.max(0, Math.min(1, (top - item.start) / item.size)),
      );
    },
  });
  useLayoutEffect(() => {
    if (!restoring.current) return;
    const index = Math.max(
      0,
      Math.min(data.blocks.length - 1, Math.round(saved.current.index)),
    );
    virtual.scrollToIndex(index, { align: "start" });
    const timer = setTimeout(() => {
      const start = virtual.getOffsetForIndex(index, "start")?.[0] || 0;
      const item = virtual.getVirtualItems().find((row) => row.index === index);
      virtual.scrollToOffset(
        start + saved.current.fraction * (item?.size || 64),
      );
      restoring.current = false;
    }, 250);
    return () => clearTimeout(timer);
  }, [virtual, data]);
  useLayoutEffect(() => {
    if (scroll.current) scroll.current.scrollLeft = position.get("wordLeft");
  }, []);
  useEffect(() => {
    if (target?.kind !== "word" || target.block === undefined) return;
    const frame = requestAnimationFrame(() =>
      virtual.scrollToIndex(target.block!, { align: "center" }),
    );
    return () => cancelAnimationFrame(frame);
  }, [data, target, jump, virtual]);
  return (
    <div
      className="office-word-scroll"
      ref={scroll}
      onScroll={(e) => position.set("wordLeft", e.currentTarget.scrollLeft)}
      tabIndex={0}
      aria-label={ui("Word 文档内容")}
    >
      {!data.blocks.length && (
        <p className="office-empty">{ui("未找到可显示的段落")}</p>
      )}
      <article
        className="office-paper"
        style={{ height: virtual.getTotalSize() }}
      >
        {virtual.getVirtualItems().map((v) => {
          const b = data.blocks[v.index];
          return (
            <div
              key={v.key}
              data-index={v.index}
              ref={virtual.measureElement}
              className={`office-block${target?.block === v.index ? " is-search-hit" : ""}`}
              style={{ transform: `translateY(${v.start}px)` }}
            >
              {b.type === "paragraph" ? (
                <ParagraphView p={b} images={data.images} />
              ) : (
                <table className="office-word-table">
                  <tbody>
                    {b.rows.map((row, i) => (
                      <tr key={i}>
                        {row.map((cell, j) => (
                          <td
                            key={j}
                            className={
                              target?.block === v.index &&
                              target.tableRow === i &&
                              target.tableColumn === j
                                ? "is-search-hit-cell"
                                : undefined
                            }
                          >
                            {cell.map((p, k) => (
                              <ParagraphView
                                key={k}
                                p={p}
                                images={data.images}
                              />
                            ))}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          );
        })}
      </article>
    </div>
  );
}
function columnName(index: number) {
  let s = "";
  for (let n = index + 1; n; n = Math.floor((n - 1) / 26))
    s = String.fromCharCode(65 + ((n - 1) % 26)) + s;
  return s;
}
function SheetView({
  data,
  target,
  jump,
}: {
  data: Sheet;
  target?: ContentLocation | null;
  jump: number;
}) {
  const scroll = useRef<HTMLDivElement>(null);
  const position = useReadingPosition();
  useReadingScroll(scroll, "sheet" + data.selected, !target);
  const virtual = useVirtualizer({
    count: data.rows.length,
    getScrollElement: () => scroll.current,
    estimateSize: () => 36,
    initialOffset: () =>
      target ? 0 : position.get("sheet" + data.selected + "Top"),
    overscan: 8,
    scrollMargin: 36,
  });
  useEffect(() => {
    if (target?.kind !== "sheet" || target.sheet !== data.selected) return;
    const row = data.rows.findIndex((r) => r.number === target.row);
    if (row < 0) return;
    const frame = requestAnimationFrame(() => {
      virtual.scrollToIndex(row, { align: "center" });
      if (scroll.current)
        scroll.current.scrollLeft = Math.max(
          0,
          52 + (target.column || 0) * 156 - scroll.current.clientWidth / 2,
        );
    });
    return () => cancelAnimationFrame(frame);
  }, [data, target, jump, virtual]);
  const width = 52 + data.columns * 156;
  return (
    <div
      ref={scroll}
      className="office-sheet-scroll"
      tabIndex={0}
      aria-label={ui("工作表内容")}
    >
      <div
        role="table"
        aria-label={data.sheets[data.selected]}
        aria-rowcount={data.rows.length + 1}
        aria-colcount={data.columns + 1}
        style={{ width, minWidth: "100%" }}
      >
        <div
          className="office-sheet-header"
          role="row"
          style={{ gridTemplateColumns: `52px repeat(${data.columns},156px)` }}
        >
          <span role="columnheader">{ui("行")}</span>
          {Array.from({ length: data.columns }, (_, i) => (
            <span key={i} role="columnheader">
              {columnName(i)}
            </span>
          ))}
        </div>
        <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
          {virtual.getVirtualItems().map((v) => (
            <div
              className="office-sheet-row"
              key={v.key}
              role="row"
              aria-rowindex={v.index + 2}
              style={{
                transform: `translateY(${v.start - 36}px)`,
                gridTemplateColumns: `52px repeat(${data.columns},156px)`,
              }}
            >
              <span role="rowheader">{data.rows[v.index].number}</span>
              {Array.from({ length: data.columns }, (_, i) => {
                const cell = data.rows[v.index].cells[i];
                const shown = cell
                  ? displayCell(cell, data.date1904)
                  : undefined;
                return (
                  <span
                    key={i}
                    role="cell"
                    tabIndex={0}
                    data-tooltip={
                      cell
                        ? tf(
                            "{0}\n原始值：{1}{2}{3}",
                            shown?.text || cell.value,
                            cell.value,
                            shown?.unsupported
                              ? ui("\n此显示格式暂不支持，保留原始值")
                              : "",
                            cell.formula
                              ? tf(
                                  "\n公式：={0}{1}",
                                  cell.formula,
                                  !cell.value ? ui("（无保存结果）") : "",
                                )
                              : "",
                          )
                        : undefined
                    }
                    className={`${cell && cell.formula ? "has-formula" : ""}${target?.sheet === data.selected && target.row === data.rows[v.index].number && target.column === i ? " is-search-hit-cell" : ""}`}
                    data-format-fallback={shown?.unsupported || undefined}
                  >
                    {cell
                      ? shown?.text ||
                        (cell.formula ? ui("未保存计算结果") : "")
                      : ""}
                  </span>
                );
              })}
            </div>
          ))}
        </div>
      </div>
      {!data.rows.length && <p className="office-empty">{ui("空工作表")}</p>}
    </div>
  );
}
export function OfficeReader({
  file,
  engine,
}: {
  file: LocalFile;
  engine?: string;
}) {
  const target = file.contentMatch?.location;
  const [sheet, setSheet] = useReadingValue("sheet", target?.sheet || 0);
  const [jump, setJump] = useState(0);
  const [data, setData] = useState<Document | Sheet>();
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    if (target?.kind === "sheet") setSheet(target.sheet || 0);
  }, [target?.sheet, target?.kind]);
  useEffect(() => {
    let alive = true;
    setData(undefined);
    setLoading(true);
    setError("");
    void api<Document | Sheet>(
      engine ? "office.normalized" : "office.document",
      { id: file.id, sheet, engine },
    )
      .then((d) => {
        if (alive) {
          setData(d);
          setLoading(false);
        }
      })
      .catch((e) => {
        if (alive) {
          setError(message(e));
          setLoading(false);
        }
      });
    return () => {
      alive = false;
    };
  }, [file.id, file.revision, file.identity, file.status, sheet, engine]);
  return (
    <section className="office-reader" aria-label={ui("Office 本地预览")}>
      <div
        className="pdf-toolbar"
        role="toolbar"
        aria-label={ui("Office 预览工具")}
      >
        {data?.kind === "sheet" ? (
          <GlideSelect
            value={String(sheet)}
            options={data.sheets.map((name, i) => ({
              value: String(i),
              label: name,
            }))}
            onChange={(v) => setSheet(Number(v))}
            ariaLabel={ui("选择工作表")}
            disabled={loading}
          />
        ) : (
          <span>
            {file.extension.toUpperCase()} {ui("内容预览")}
          </span>
        )}
        <span
          className="subtle"
          data-tooltip={
            data?.kind === "sheet" && data.mergedRanges?.length
              ? tf(
                  "合并范围（按原始单元格阅读）：{0}",
                  data.mergedRanges.slice(0, 20).join("、"),
                )
              : undefined
          }
        >
          {data?.kind === "sheet"
            ? tf(
                "{0} 行 · {1} 列{2}{3}",
                data.rows.length.toLocaleString(),
                data.columns,
                data.truncated ? ui(" · 内容已截取") : "",
                data.mergedRanges?.length
                  ? tf(" · {0} 处合并", data.mergedRanges.length)
                  : "",
              )
            : ui("段落、图片与列表 · 只读")}
        </span>
      </div>
      {target && (
        <div className="office-search-source">
          <span data-tooltip={file.contentMatch?.coverage}>
            {ui("命中位置：")}
            {target.label}
          </span>
          <button
            className="button quiet"
            disabled={loading || !!error}
            onClick={() => {
              setSheet(target.sheet || 0);
              setJump((n) => n + 1);
            }}
          >
            {ui("返回命中位置")}
          </button>
        </div>
      )}
      {error ? (
        <div className="pdf-message" role="alert">
          <AlertCircle size={24} />
          <span>{error}</span>
          <small>{ui("可以使用下方按钮打开原文件")}</small>
        </div>
      ) : loading ? (
        <div className="pdf-message" role="status">
          <LoaderCircle className="spin" size={24} />
          <span>{ui("正在读取文档…")}</span>
        </div>
      ) : data?.kind === "docx" ? (
        <WordView data={data} target={target} jump={jump} />
      ) : data ? (
        <SheetView key={sheet} data={data} target={target} jump={jump} />
      ) : null}
      <div
        className="office-reader-note"
        data-warning={
          data?.kind === "docx"
            ? data.warnings.length > 0
            : data?.truncated || undefined
        }
      >
        {data?.kind === "sheet"
          ? ui(
              "支持常见日期、数字、百分比和货币格式；不支持的格式保留原始值。公式使用保存结果；图表、合并布局尚未还原。最多 10,000 行、100 列、100,000 单元格。",
            )
          : tf(
              "{0}支持内嵌 PNG／JPEG／WebP 图片和常见列表；不还原分页、文本框与复杂排版。完整效果请打开原文件。",
              data?.kind === "docx" && data.warnings.length
                ? data.warnings.join("；") + "。"
                : "",
            )}
      </div>
    </section>
  );
}
