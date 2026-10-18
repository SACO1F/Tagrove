import { api, LocalFile, message } from "./api";
export interface Draft {
  text: string;
  saved: string;
  version: number;
  error: string;
  saving?: Promise<void>;
}
export class NoteDrafts {
  drafts = new Map<string, Draft>();
  onChange = () => {};
  constructor(private library: string) {
    try {
      const stored = JSON.parse(
        localStorage.getItem(`shiqian-note-drafts:${library}`) || "{}",
      ) as Record<string, Draft>;
      for (const [id, d] of Object.entries(stored)) {
        this.drafts.set(id, {
          ...d,
          error: "发现尚未保存的备注草稿，请检查后保存",
        });
      }
    } catch {
      /* First run or malformed UI draft storage: never alter the database. */
    }
  }
  get(f: LocalFile) {
    let d = this.drafts.get(f.id);
    if (!d) {
      d = { text: f.note, saved: f.note, version: f.noteVersion, error: "" };
      this.drafts.set(f.id, d);
    } else if (!d.saving && d.text === d.saved && f.noteVersion >= d.version) {
      Object.assign(d, {
        text: f.note,
        saved: f.note,
        version: f.noteVersion,
        error: "",
      });
    }
    return d;
  }
  edit(f: LocalFile, text: string) {
    const d = this.get(f);
    d.text = text;
    d.error = "";
    this.persist();
    this.onChange();
  }
  persist() {
    const data: Record<string, unknown> = {};
    for (const [id, d] of this.drafts) {
      if (d.text !== d.saved)
        data[id] = {
          text: d.text,
          saved: d.saved,
          version: d.version,
          error: d.error,
        };
    }
    try {
      localStorage.setItem(
        `shiqian-note-drafts:${this.library}`,
        JSON.stringify(data),
      );
    } catch {
      for (const d of this.drafts.values())
        if (d.text !== d.saved)
          d.error = "本机草稿存储已满，请保持窗口打开直到备注保存成功";
    }
  }
  async flush(id: string) {
    const d = this.drafts.get(id);
    if (!d) return;
    if (d.saving) return d.saving;
    const promise = (async () => {
      while (d.text !== d.saved) {
        const text = d.text;
        try {
          const r = await api<{ version: number }>("note.save", {
            id,
            text,
            version: d.version,
          });
          d.saved = text;
          d.version = r.version;
          d.error = "";
          this.persist();
          this.onChange();
        } catch (e) {
          const current = await api<LocalFile>("file", { id }).catch(
            () => undefined,
          );
          if (current?.note === text) {
            d.saved = text;
            d.version = current.noteVersion;
            d.error = "";
            this.persist();
            continue;
          }
          d.error = String(e);
          this.persist();
          throw e;
        }
      }
    })();
    d.saving = promise;
    this.onChange();
    try {
      await promise;
    } finally {
      d.saving = undefined;
      this.onChange();
    }
  }
  async flushAll() {
    for (const id of this.drafts.keys()) await this.flush(id);
  }
  discard(f: LocalFile) {
    this.drafts.set(f.id, {
      text: f.note,
      saved: f.note,
      version: f.noteVersion,
      error: "",
    });
    this.persist();
    this.onChange();
  }
  clear() {
    this.drafts.clear();
    this.persist();
    this.onChange();
  }
}
