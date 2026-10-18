export function FileGlyph({
  extension,
  compact = false,
}: {
  extension: string;
  compact?: boolean;
}) {
  const ext = extension.toLowerCase();
  const kind = /^(xlsx?|csv|tsv)$/.test(ext)
    ? "sheet"
    : /^(docx?|rtf)$/.test(ext)
      ? "word"
      : /^(html?|xml)$/.test(ext)
        ? "html"
        : /^(md|markdown)$/.test(ext)
          ? "markdown"
          : "text";
  return (
    <svg
      className={`file-glyph glyph-${kind}`}
      data-format={kind}
      width={compact ? 27 : 44}
      height={compact ? 27 : 44}
      viewBox="0 0 32 36"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path
        className="glyph-outline"
        d="M6 2h14l7 7v23a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2Z"
      />
      <path className="glyph-outline" d="M20 2v7h7" />
      {kind === "sheet" ? (
        <>
          <rect x="9" y="15" width="13" height="13" rx="1" />
          <path d="M9 19h13M9 23.5h13M14 15v13" />
        </>
      ) : kind === "html" ? (
        <>
          <path d="m12 17-4 4 4 4m8-8 4 4-4 4m-3-9-2 10" />
        </>
      ) : kind === "markdown" ? (
        <>
          <path d="M10 26V16l6 7 6-7v10" />
        </>
      ) : kind === "word" ? (
        <>
          <path strokeWidth="2.1" d="M9 16h13" />
          <path d="M9 21h13M9 26h9" />
        </>
      ) : (
        <path d="M9 16h13M9 21h13M9 26h8" />
      )}
    </svg>
  );
}
