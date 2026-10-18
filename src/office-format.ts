// Deliberately bounded Excel display subset. Unsupported codes retain raw values.
export type OfficeCell = {
  value: string;
  formula?: string;
  numeric?: boolean;
  formatId?: number;
  formatCode?: string;
};
const builtins: Record<number, string> = {
  0: "General",
  1: "0",
  2: "0.00",
  3: "#,##0",
  4: "#,##0.00",
  9: "0%",
  10: "0.00%",
  11: "0.00E+00",
  14: "mm-dd-yy",
  15: "d-mmm-yy",
  16: "d-mmm",
  17: "mmm-yy",
  18: "h:mm AM/PM",
  19: "h:mm:ss AM/PM",
  20: "h:mm",
  21: "h:mm:ss",
  22: "m/d/yy h:mm",
  49: "@",
};
function dateValue(
  value: number,
  code: string,
  date1904: boolean,
): string | undefined {
  if (value < 0 || value > 2958465 || /\[|\]|;|\\|"/.test(code)) return;
  // Distinguish minute tokens from month tokens; preserve Excel's fictitious leap day.
  const seconds = Math.round((value - Math.floor(value)) * 86400);
  const day = Math.floor(value) + Math.floor(seconds / 86400);
  const date = new Date(
    Date.UTC(date1904 ? 1904 : 1899, date1904 ? 0 : 11, date1904 ? 1 : 31) +
      (day - (!date1904 && day >= 60 ? 1 : 0)) * 86400000,
  );
  if (!Number.isFinite(date.getTime())) return;
  const fake = !date1904 && day === 60;
  const year = fake ? 1900 : date.getUTCFullYear(),
    month = fake ? 2 : date.getUTCMonth() + 1,
    dayOfMonth = fake ? 29 : date.getUTCDate(),
    hour = Math.floor(seconds / 3600) % 24,
    minute = Math.floor(seconds / 60) % 60,
    second = seconds % 60;
  const twelve = /AM\/PM/i.test(code);
  const tokens = code.match(
    /AM\/PM|y{2,4}|m{1,4}|d{1,4}|h{1,2}|s{1,2}|[^ymdhs]+/gi,
  );
  if (!tokens || tokens.join("") !== code) return;
  const pad = (n: number) => String(n).padStart(2, "0");
  let time = false;
  const out: string[] = [];
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i],
      low = t.toLowerCase();
    if (/^h+$/.test(low)) {
      time = true;
      const h = twelve ? hour % 12 || 12 : hour;
      out.push(t.length === 2 ? pad(h) : String(h));
    } else if (/^m+$/.test(low)) {
      const minutes = time || /^s/i.test(tokens[i + 2] || "");
      if (minutes) out.push(t.length === 2 ? pad(minute) : String(minute));
      else if (t.length >= 3)
        out.push(
          new Intl.DateTimeFormat("en-US", {
            month: t.length === 3 ? "short" : "long",
            timeZone: "UTC",
          }).format(new Date(Date.UTC(2000, month - 1, 1))),
        );
      else out.push(t.length === 2 ? pad(month) : String(month));
    } else if (/^d+$/.test(low)) {
      if (t.length >= 3)
        out.push(
          new Intl.DateTimeFormat("en-US", {
            weekday: t.length === 3 ? "short" : "long",
            timeZone: "UTC",
          }).format(date),
        );
      else out.push(t.length === 2 ? pad(dayOfMonth) : String(dayOfMonth));
    } else if (/^y+$/.test(low))
      out.push(t.length === 2 ? pad(year % 100) : String(year));
    else if (/^s+$/.test(low)) {
      time = true;
      out.push(t.length === 2 ? pad(second) : String(second));
    } else if (low === "am/pm") out.push(hour < 12 ? "AM" : "PM");
    else if (/^[\s/:.,\-]+$/.test(t)) out.push(t);
    else return;
  }
  return out.join("");
}
export function displayCell(
  cell: OfficeCell,
  date1904 = false,
): { text: string; unsupported: boolean } {
  const raw = cell.value;
  if (!cell.numeric || !raw) return { text: raw, unsupported: false };
  const code = cell.formatCode ?? builtins[cell.formatId ?? 0];
  if (code === undefined) return { text: raw, unsupported: true };
  if (code === "General" || code === "@")
    return { text: raw, unsupported: false };
  const value = Number(raw);
  if (!Number.isFinite(value) || raw.trim() === "")
    return { text: raw, unsupported: true };
  if (/[ymdhs]/i.test(code) && !/[Ee][+-]0/.test(code)) {
    const text = dateValue(value, code, date1904);
    return { text: text ?? raw, unsupported: text === undefined };
  }
  if (code === "0.00E+00")
    return {
      text: value
        .toExponential(2)
        .replace(
          /e([+-])(\d+)$/,
          (_, sign, digits) => `E${sign}${digits.padStart(2, "0")}`,
        ),
      unsupported: false,
    };
  // Supports fixed decimal / grouping / percent / literal currency. Conditions,
  // accounting, scaling, fractions and multi-section codes intentionally fall back.
  const match = code.match(
    /^("[^"\n]{0,8}"|[$¥￥€£])?(#,##0|0)(\.0{1,8})?(%)?("[^"\n]{0,8}")?$/,
  );
  if (!match) return { text: raw, unsupported: true };
  const decimals = match[3]?.length ? match[3].length - 1 : 0;
  const amount = value * (match[4] ? 100 : 1);
  if (!Number.isFinite(amount)) return { text: raw, unsupported: true };
  const text = new Intl.NumberFormat("en-US", {
    useGrouping: match[2].startsWith("#"),
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  }).format(amount);
  return {
    text: `${(match[1] || "").replace(/"/g, "")}${text}${match[4] || ""}${(match[5] || "").replace(/"/g, "")}`,
    unsupported: false,
  };
}
