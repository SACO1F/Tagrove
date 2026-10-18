type Bounds = { left: number; top: number; width: number; height: number };

/** Only the active card is promoted. Its layout wrapper and hit rectangle stay flat. */
export function createTagDropTilt(enabled: boolean) {
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");
  const entries = new Map<
    HTMLElement,
    { original: string; timer?: ReturnType<typeof setTimeout> }
  >();
  let active: HTMLElement | undefined;
  const release = (card: HTMLElement, immediate = false) => {
    const entry = entries.get(card);
    if (!entry) return;
    clearTimeout(entry.timer);
    card.style.transform = entry.original;
    const finish = () => {
      card.classList.remove("tag-drop-tilt");
      entries.delete(card);
    };
    if (immediate || reduced.matches) finish();
    else entry.timer = setTimeout(finish, 190);
  };
  const reset = () => {
    if (active) release(active);
    active = undefined;
  };
  const onReduced = () => {
    if (reduced.matches) {
      for (const card of entries.keys()) release(card, true);
      active = undefined;
    }
  };
  reduced.addEventListener("change", onReduced);
  return {
    update(card: HTMLElement | null, x: number, y: number, rect?: Bounds) {
      if (!enabled || reduced.matches || !card || !rect) {
        reset();
        return;
      }
      if (active !== card) {
        reset();
        active = card;
      }
      let entry = entries.get(card);
      if (!entry) {
        entry = { original: card.style.transform };
        entries.set(card, entry);
      }
      clearTimeout(entry.timer);
      const nx = Math.max(
        -1,
        Math.min(1, ((x - rect.left) / Math.max(rect.width, 1)) * 2 - 1),
      );
      const ny = Math.max(
        -1,
        Math.min(1, ((y - rect.top) / Math.max(rect.height, 1)) * 2 - 1),
      );
      const amplitude = card.classList.contains("file-list") ? 3 : 7;
      const transform = `perspective(950px) rotateX(${(-ny * amplitude).toFixed(2)}deg) rotateY(${(nx * amplitude).toFixed(2)}deg) scale(1.018)`;
      card.classList.add("tag-drop-tilt");
      card.style.transform = transform;
      return transform;
    },
    reset,
    dispose() {
      reduced.removeEventListener("change", onReduced);
      for (const card of entries.keys()) release(card, true);
      active = undefined;
    },
  };
}
