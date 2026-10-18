(() => {
  const token = document.currentScript.dataset.token;
  const appearance = document.createElement("style");
  appearance.textContent = "::selection{background:color-mix(in srgb,var(--reader-accent,#2c6654) 28%,white)!important;color:#2c3037!important}";
  document.head.append(appearance);
  let scale = 1, total = 0, last = 0, anchor = null;
  addEventListener("message", (event) => {
    if (event.source !== parent || event.data?.token !== token) return;
    if (event.data.type === "shiqian-position") {
      const { top, left } = event.data;
      if (Number.isFinite(top) && Number.isFinite(left) && top >= 0 && left >= 0 && top <= 1e8 && left <= 1e8)
        scrollTo(left * scale, top * scale);
      return;
    }
    if (event.data.type === "shiqian-appearance") {
      if (/^#[0-9a-f]{6}$/i.test(event.data.accent))
        document.documentElement.style.setProperty("--reader-accent", event.data.accent);
      return;
    }
    if (event.data.type !== "shiqian-zoom") return;
    const next = event.data.scale;
    if (!Number.isFinite(next) || next < 0.25 || next > 4) return;
    const point = anchor || {x:0,y:0};
    const x = (scrollX + point.x) / scale, y = (scrollY + point.y) / scale;
    document.body.style.zoom = String(next);
    scale = next;
    scrollTo(x * scale - point.x, y * scale - point.y);
    anchor = null;
  });
  addEventListener("wheel", (event) => {
    if (!event.ctrlKey && !event.metaKey) return;
    event.preventDefault();
    const now = performance.now();
    if (now - last > 180) total = 0;
    last = now;
    total += event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? innerHeight : 1);
    if (Math.abs(total) >= 12) {
      anchor = {x:event.clientX,y:event.clientY};
      parent.postMessage({ type: "shiqian-zoom-wheel", token, direction: total < 0 ? 1 : -1 }, "*");
      total = 0;
    }
  }, { passive: false });
  let positionFrame = 0;
  addEventListener("scroll", () => {
    cancelAnimationFrame(positionFrame);
    positionFrame = requestAnimationFrame(() => parent.postMessage({ type: "shiqian-position", token, top: scrollY / scale, left: scrollX / scale }, "*"));
  }, { passive: true });
  parent.postMessage({ type: "shiqian-zoom-ready", token }, "*");
})();
