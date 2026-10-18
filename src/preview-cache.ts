/** Byte-budgeted LRU of encoded previews. Budget excludes decoded WebView image textures. */
export class PreviewCache<T extends { data?: string; text?: string }> {
  private values = new Map<string, { value: T; bytes: number }>();
  bytes = 0;
  constructor(
    readonly budget: number,
    readonly maxEntries = 300,
  ) {}
  get(key: string) {
    const entry = this.values.get(key);
    if (!entry) return undefined;
    this.values.delete(key);
    this.values.set(key, entry);
    return entry.value;
  }
  set(key: string, value: T) {
    this.delete(key);
    // Conservative UTF-16 estimate, plus object/key overhead.
    const bytes =
      2 * ((value.data?.length ?? 0) + (value.text?.length ?? 0) + key.length) +
      512;
    if (bytes > this.budget) return;
    this.values.set(key, { value, bytes });
    this.bytes += bytes;
    while (this.bytes > this.budget || this.values.size > this.maxEntries)
      this.delete(this.values.keys().next().value!);
  }
  delete(key: string) {
    const entry = this.values.get(key);
    if (entry) this.bytes -= entry.bytes;
    this.values.delete(key);
  }
  clear() {
    this.values.clear();
    this.bytes = 0;
  }
}
