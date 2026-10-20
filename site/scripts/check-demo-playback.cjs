// Regression checks for hover playback; no browser or user library is accessed.
const fs = require("node:fs"),
  vm = require("node:vm"),
  assert = require("node:assert/strict"),
  path = require("node:path");
class Element {
  constructor() {
    this.attrs = {};
    this.handlers = {};
    this.dataset = {};
    this.textContent = "";
    this.currentTime = 0;
    this.paused = true;
    this.loadCount = 0;
    this.playCount = 0;
    this.pending = [];
  }
  setAttribute(k, v) {
    this.attrs[k] = v;
  }
  getAttribute(k) {
    return this.attrs[k] ?? null;
  }
  hasAttribute(k) {
    return k in this.attrs;
  }
  removeAttribute(k) {
    delete this.attrs[k];
  }
  set src(v) {
    this.attrs.src = v;
  }
  get src() {
    return this.attrs.src;
  }
  addEventListener(k, fn) {
    (this.handlers[k] ||= []).push(fn);
  }
  fire(k, e = {}) {
    for (const f of this.handlers[k] || []) f(e);
  }
  querySelector(k) {
    return this.parts[k];
  }
  contains(el) {
    return Object.values(this.parts || {}).includes(el);
  }
  pause() {
    this.paused = true;
  }
  load() {
    this.loadCount++;
  }
  play() {
    this.playCount++;
    this.paused = false;
    return new Promise((resolve, reject) =>
      this.pending.push({ resolve, reject }),
    );
  }
}
const demos = Array.from({ length: 4 }, (_, i) => {
  const d = new Element(),
    v = new Element(),
    b = new Element(),
    label = new Element();
  v.dataset.src = "clip" + i + ".mp4";
  b.parts = { span: label };
  d.parts = { video: v, ".motion-toggle": b };
  return d;
});
const doc = new Element(),
  win = new Element(),
  reduced = new Element();
doc.hidden = false;
reduced.matches = false;
doc.querySelectorAll = () => demos;
win.matchMedia = () => reduced;
let intersection;
win.IntersectionObserver = true;
const context = {
  Array,
  Map,
  document: doc,
  window: win,
  IntersectionObserver: class {
    constructor(cb) {
      intersection = cb;
    }
    observe() {}
  },
};
const page = fs.readFileSync(
  path.join(__dirname, process.argv[2] || "../index.html"),
  "utf8",
);
const scripts = [...page.matchAll(/<script>([\s\S]*?)<\/script>/g)];
vm.runInNewContext(scripts.at(-1)[1], context);
const video = (i) => demos[i].parts.video;
const button = (i) => demos[i].parts[".motion-toggle"];
const flush = () => new Promise((r) => setImmediate(r));
async function resolvePlay(i) {
  video(i)
    .pending.splice(0)
    .forEach((p) => p.resolve());
  await flush();
}
(async () => {
  for (let i = 0; i < 4; i++) {
    assert(video(i).paused);
    assert.equal(video(i).currentTime, 0);
    assert.equal(video(i).playCount, 0);
  }
  demos[0].fire("pointerenter", { pointerType: "mouse" });
  assert.equal(
    video(0).playCount,
    0,
    "refresh must not start while pointer stays still",
  );
  intersection([{ isIntersecting: true, target: demos[0] }]);
  assert.equal(video(0).loadCount, 1);
  assert.equal(video(0).playCount, 0, "preloading does not autoplay");
  demos[0].fire("pointermove", { pointerType: "mouse" });
  assert.equal(video(0).playCount, 1);
  assert.equal(button(0).getAttribute("aria-busy"), "true");
  await resolvePlay(0);
  assert.equal(demos[0].dataset.playing, "true");
  video(0).currentTime = 3;
  demos[0].fire("pointerleave");
  assert(video(0).paused);
  assert.equal(video(0).currentTime, 0);
  assert.equal(demos[0].dataset.playing, "false");
  demos[0].fire("pointerenter", { pointerType: "mouse" });
  assert.equal(video(0).currentTime, 0);
  await resolvePlay(0);
  demos[1].fire("pointerenter", { pointerType: "mouse" });
  assert(video(0).paused);
  assert.equal(video(0).currentTime, 0);
  await resolvePlay(1);
  demos[2].fire("pointerenter", { pointerType: "mouse" });
  demos[2].fire("pointerleave");
  await resolvePlay(2);
  assert.equal(
    demos[2].dataset.playing,
    "false",
    "late play completion must not reveal a stopped clip",
  );
  assert(video(2).paused);
  demos[2].fire("pointerenter", { pointerType: "mouse" });
  demos[2].fire("pointerleave");
  demos[2].fire("pointerenter", { pointerType: "mouse" });
  await resolvePlay(2);
  assert.equal(
    demos[2].dataset.playing,
    "true",
    "rapid reentry preserves newest request",
  );
  win.fire("pageshow");
  for (let i = 0; i < 4; i++) {
    assert(video(i).paused);
    assert.equal(video(i).currentTime, 0);
    assert.equal(demos[i].dataset.playing, "false");
  }
  demos[3].fire("pointermove", { pointerType: "touch" });
  assert.equal(video(3).playCount, 0, "touch scrolling does not play");
  button(3).fire("click");
  await resolvePlay(3);
  assert.equal(demos[3].dataset.playing, "true");
  button(3).fire("click");
  assert(video(3).paused);
  reduced.matches = true;
  demos[1].fire("pointerenter", { pointerType: "mouse" });
  assert(video(1).paused);
  button(1).fire("click");
  await resolvePlay(1);
  assert.equal(
    demos[1].dataset.playing,
    "true",
    "explicit play remains available",
  );
  reduced.fire("change", { matches: true });
  assert(video(1).paused);
  reduced.matches = false;
  demos[0].fire("pointermove", { pointerType: "mouse" });
  await resolvePlay(0);
  doc.hidden = true;
  doc.fire("visibilitychange");
  assert(video(0).paused);
  assert.equal(video(0).currentTime, 0);
  doc.hidden = false;
  demos[1].fire("pointermove", { pointerType: "mouse" });
  await resolvePlay(1);
  intersection([{ isIntersecting: false, target: demos[1] }]);
  assert(video(1).paused);
  demos[0].fire("pointerenter", { pointerType: "mouse" });
  video(0)
    .pending.splice(0)
    .forEach((p) => p.reject(new Error("media failed")));
  await flush();
  assert.equal(demos[0].dataset.playing, "false");
  console.log(
    "PASS: static refresh, hover autoplay, leave and reenter reset, preload without play, one active clip, async leave and rapid reentry, touch/keyboard button, reduced motion, hidden/offscreen reset, failed play fallback.",
  );
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
