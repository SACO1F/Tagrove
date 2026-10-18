export function localDateValue(date: Date): string {
  return `${String(date.getFullYear()).padStart(4, "0")}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}
export function parseDateValue(value: string): Date | undefined {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || value.startsWith("0000"))
    return undefined;
  const date = new Date(`${value}T12:00:00`);
  return Number.isFinite(date.getTime()) && localDateValue(date) === value
    ? date
    : undefined;
}
export function dateBoundary(value: string, end = false): number | undefined {
  const date = parseDateValue(value);
  if (!date) return undefined;
  date.setHours(0, 0, 0, 0);
  if (end) date.setDate(date.getDate() + 1);
  return date.getTime();
}
