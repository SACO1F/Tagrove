const vm = require("node:vm"),
  fs = require("node:fs"),
  assert = require("node:assert/strict"),
  crypto = require("node:crypto");
const id = fs.readFileSync("browser-extension/extension-id.txt", "utf8").trim(),
  origin = `chrome-extension://${id}/`;
let state = { queue: [] },
  listener,
  clicked,
  alarm,
  startup,
  installed,
  alarmValue,
  alarmCreates = 0,
  mode = "offline",
  opened = [],
  actions = [],
  menus = [];
const event = (fn) => ({ addListener: fn || (() => {}) });
const clone = (x) => structuredClone(x);
const chrome = {
  runtime: {
    id,
    getManifest: () => JSON.parse(fs.readFileSync("browser-extension/manifest.json", "utf8")),
    getURL: (s) => origin + s,
    onMessage: event((fn) => (listener = fn)),
    onInstalled: event((fn) => (installed = fn)),
    onStartup: event((fn) => (startup = fn)),
    sendNativeMessage: async (_host, m) => {
      actions.push(m.action);
      if (mode.startsWith("local")) {
        if (m.action === "ping")
          return {
            ok: true,
            result: {
              protocol: 1,
              version: "0.6.10",
              capabilities: ["localImageRecovery"],
            },
          };
        if (m.action === "capture.image.local.start")
          return { ok: true, result: { status: "pending" } };
        if (m.action === "capture.image.local.status")
          return {
            ok: true,
            result:
              mode === "localCancel"
                ? { status: "cancelled" }
                : mode === "localFail"
                  ? { status: "failed", error: "IMAGE_TYPE: not image" }
                  : {
                      status: "done",
                      result: { id: "restored-image", duplicate: false },
                    },
          };
      }

      if (mode === "http403")
        return {
          ok: false,
          error: "IMAGE_ACCESS: 图片服务返回 HTTP 403，拒绝访问",
        };
      if (mode === "mystery")
        throw Error("Unexpected native transport failure");
      if (mode === "imageType")
        return { ok: false, error: "IMAGE_TYPE: invalid bytes" };
      if (mode === "imageDownload")
        return { ok: false, error: "IMAGE_DOWNLOAD: timeout" };
      if (mode === "offline") throw Error("Native host has exited.");
      if (mode === "missing")
        throw Error("Specified native messaging host not found.");
      if (mode === "forbidden")
        throw Error(
          "Access to the specified native messaging host is forbidden.",
        );
      if (mode === "invalid")
        return { ok: false, error: "CAPTURE_CONFLICT: conflicting payload" };
      if (mode === "storage")
        return { ok: false, error: "CAPTURE_STORAGE: no space" };
      if (mode === "badAck") return { ok: true };
      return {
        ok: true,
        result:
          m.action === "ping"
            ? { protocol: 1, version: "0.6.1" }
            : { id: "saved", duplicate: false },
      };
    },
  },
  storage: {
    local: {
      get: async (key) => ({ [key]: clone(state[key]) }),
      set: async (data) => {
        if (mode === "quota") throw Error("QUOTA_BYTES quota exceeded");
        state = clone({ ...state, ...data });
      },
    },
  },
  contextMenus: {
    removeAll: (fn) => fn(),
    create(info) {
      menus.push(info);
    },
    onClicked: event((fn) => (clicked = fn)),
  },
  alarms: {
    get: async () => alarmValue,
    create: async () => {
      alarmCreates++;
      alarmValue = { name: "retry" };
    },
    onAlarm: event((fn) => (alarm = fn)),
  },
  action: {
    setPopup: async (v) => opened.push(origin + v.popup),
    openPopup: async () => {},
    setBadgeText: async () => {},
    setTitle: async () => {},
  },
  tabs: {
    create: async () => {
      throw Error("Right-click capture must not create a tab");
    },
  },
};
const source =
  fs.readFileSync("browser-extension/diagnostics.js", "utf8") +
  "\n" +
  fs
    .readFileSync("browser-extension/background.js", "utf8")
    .replace('import "./diagnostics.js";', "");
function worker() {
  vm.runInNewContext(source, {
    chrome,
    crypto: { randomUUID: () => crypto.randomUUID() },
    URL,
    console,
  });
}
const call = (m) =>
  new Promise((resolve) =>
    listener(m, { id, url: origin + "popup.html" }, resolve),
  );
const capture = (n) => ({
  title: "资料" + n,
  url: "https://example.test/" + n,
  kind: "web_excerpt",
  excerpt: "学习资料",
});
(async () => {
  worker();
  await call({ action: "list" });
  assert.equal(alarmCreates, 1);
  alarmValue = undefined;
  await startup();
  await new Promise(setImmediate);
  assert.equal(alarmCreates, 2);
  assert.equal(
    listener({ action: "list" }, { id, url: "https://evil.test/" }, () => {
      throw Error("untrusted response");
    }),
    false,
  );
  await Promise.all([
    clicked(
      { menuItemId: "selection", selectionText: "第一份" },
      { title: "一", url: "https://example.test/1" },
    ),
    clicked(
      { menuItemId: "selection", selectionText: "第二份" },
      { title: "二", url: "https://example.test/2" },
    ),
  ]);
  await call({ action: "list" });
  const ids = opened.map((url) => new URL(url).searchParams.get("draft"));
  assert.notEqual(ids[0], ids[1]);
  assert.equal(
    (await call({ action: "draft", id: ids[0] })).draft.excerpt,
    "第一份",
  );
  assert.equal(
    (await call({ action: "draft", id: ids[1] })).draft.excerpt,
    "第二份",
  );
  await call({ action: "draft.remove", id: ids[0] });
  assert.equal((await call({ action: "draft", id: ids[0] })).draft, undefined);
  state.drafts.expired = {
    createdAt: Date.now() - 86400001,
    capture: capture("old"),
  };
  await call({ action: "draft", id: "expired" });
  assert.equal(state.drafts.expired, undefined);
  for (let n = 0; n < 20; n++)
    clicked(
      { menuItemId: "page" },
      { title: "草稿" + n, url: "https://example.test/" + n },
    );
  await call({ action: "list" });
  assert.equal(Object.keys(state.drafts).length, 20);
  assert.match(opened.at(-1), /draftLimit/);
  mode = "missing";
  let status = await call({ action: "status" });
  assert.equal(status.errorCode, "BRIDGE_NOT_REGISTERED");
  mode = "forbidden";
  assert.equal(
    (await call({ action: "status" })).errorCode,
    "BRIDGE_FORBIDDEN",
  );
  mode = "invalid";
  const permanent = (await call({ action: "submit", capture: capture("bad") }))
    .record;
  assert.equal(permanent.retryable, false);
  const attempts = permanent.attempts;
  await alarm({ name: "retry" });
  await call({ action: "list" });
  assert.equal(state.queue[0].attempts, attempts);
  mode = "storage";
  const transient = (await call({ action: "submit", capture: capture("disk") }))
    .record;
  assert.equal(transient.retryable, true);
  mode = "good";
  await call({ action: "retry", id: transient.id });
  assert.equal(state.queue[1].status, "done");
  assert.equal(state.queue[0].status, "failed");
  mode = "badAck";
  assert.equal(
    (await call({ action: "submit", capture: capture("ack") })).record.status,
    "failed",
  );
  mode = "offline";
  const legacy = {
    id: "legacy-id",
    capture: { ...capture("legacy"), requestId: "legacy-id" },
    status: "sending",
    createdAt: 1,
    error: "",
  };
  state.queue = [legacy];
  worker();
  await call({ action: "list" });
  assert.equal(state.queue[0].id, "legacy-id");
  assert.equal(state.queue[0].status, "pending");
  for (let n = 0; n < 199; n++)
    assert.equal(
      (await call({ action: "submit", capture: capture(n) })).ok,
      true,
    );
  assert.equal(state.queue.length, 200);
  assert.equal(
    (await call({ action: "submit", capture: capture("full") })).ok,
    false,
  );
  const retained = state.queue.map((r) => r.id);
  mode = "good";
  await call({ action: "retry", id: retained[100] });
  assert.equal(state.queue.filter((r) => r.status === "done").length, 1);
  assert.equal(state.queue[100].id, retained[100]);
  await call({ action: "submit", capture: capture("new") });
  assert.equal(state.queue.length, 200);
  assert(
    retained
      .filter((_, i) => i !== 100)
      .every((id) => state.queue.some((r) => r.id === id)),
  );
  state.queue = [];
  state.drafts = {};
  mode = "good";
  await installed();
  assert(menus.some((m) => m.id === "image" && m.contexts.includes("image")));
  clicked(
    { menuItemId: "image", srcUrl: "https://images.example.test/photo.png" },
    { title: "图片页面", url: "https://example.test/page" },
  );
  await call({ action: "list" });
  const latest = new URL(opened.at(-1)).searchParams.get("draft");
  const draft = (await call({ action: "draft", id: latest })).draft;
  assert.equal(draft.kind, "web_image");
  assert.equal(draft.imageUrl, "https://images.example.test/photo.png");
  const image = {
    title: "采集图片",
    url: "https://example.test/page",
    kind: "web_image",
    imageUrl: "https://images.example.test/photo.png",
    excerpt: "ignored",
  };
  const saved = (await call({ action: "submit", capture: image })).record;
  assert.equal(saved.status, "done");
  assert.equal(saved.capture.excerpt, "");
  assert(actions.includes("capture.image"));
  mode = "imageType";
  const invalid = (await call({ action: "submit", capture: image })).record;
  assert.equal(invalid.retryable, false);
  mode = "imageDownload";
  const retry = (await call({ action: "submit", capture: image })).record;
  assert.equal(retry.retryable, true);
  const before = retry.id;
  mode = "good";
  await call({ action: "retry", id: before });
  assert.equal(state.queue.find((r) => r.id === before).status, "done");
  assert.equal(
    (
      await call({
        action: "submit",
        capture: { ...image, imageUrl: "data:image/png;base64,AAAA" },
      })
    ).ok,
    false,
  );
  let fallbackBadge = "";
  chrome.action.openPopup = async () => {
    throw Error("Popup unavailable");
  };
  chrome.action.setBadgeText = async (v) => {
    fallbackBadge = v.text;
  };
  state.drafts = {};
  await clicked(
    { menuItemId: "selection", selectionText: "fallback" },
    {
      id: 1,
      windowId: 2,
      title: "Fallback",
      url: "https://example.test/fallback",
    },
  );
  await call({ action: "list" });
  assert.equal(fallbackBadge, "•");
  const fallbackId = new URL(opened.at(-1)).searchParams.get("draft");
  assert.equal(
    (await call({ action: "draft", id: fallbackId })).draft.excerpt,
    "fallback",
  );
  state.queue = [];
  mode = "imageDownload";
  const diagnosticTransient = (await call({ action: "submit", capture: image }))
    .record;
  assert.ok(diagnosticTransient.diagnostic);
  mode = "online";
  await call({ action: "retry", id: diagnosticTransient.id });
  const diagnosticRecovered = (await call({ action: "list" })).queue.find(
    (r) => r.id === diagnosticTransient.id,
  );
  assert.equal(diagnosticRecovered.status, "done");
  assert.equal(diagnosticRecovered.diagnostic, undefined);
  assert.equal(diagnosticRecovered.error, "");
  mode = "http403";
  const failed = (await call({ action: "submit", capture: image })).record;
  assert.equal(failed.diagnostic.httpStatus, "403");
  assert.match(failed.diagnostic.detail, /HTTP 403/);
  assert.equal(failed.diagnostic.stage, "访问图片服务器");
  assert.equal(failed.retryable, false);
  mode = "online";
  const done = (await call({ action: "submit", capture: capture("new") }))
    .record;
  assert.equal(done.status, "done");
  assert.equal(done.diagnostic, undefined);
  mode = "mystery";
  const unknown = (
    await call({ action: "submit", capture: capture("unknown") })
  ).record;
  assert.equal(unknown.errorCode, "BRIDGE_TRANSPORT");
  assert.match(unknown.diagnostic.detail, /Unexpected native/);
  mode = "quota";
  const quota = await call({ action: "submit", capture: capture("quota") });
  assert.equal(quota.ok, false);
  assert.equal(quota.diagnostic.code, "BROWSER_STORAGE");
  mode = "online";
  const invalidInput = await call({
    action: "submit",
    capture: { ...capture("invalid"), url: "bad" },
  });
  assert.equal(invalidInput.diagnostic.code, "CAPTURE_INVALID");
  assert.match(invalidInput.diagnostic.detail, /HTTP/);
  const ctx = { URL, console };
  vm.runInNewContext(
    fs.readFileSync("browser-extension/diagnostics.js", "utf8"),
    ctx,
  );
  const D = ctx.ShiqianDiagnostics;
  const secret = D.diagnose(
    Error(
      "IMAGE_DOWNLOAD: HTTP 503 https://user:pass@example.test/private?token=secret#secret Bearer abc123 token=hidden; C:\\Users\\private\\file.txt",
    ),
    {
      capture: {
        url: "https://example.test/private?token=secret",
        title: "private title",
        excerpt: "private text",
      },
    },
  );
  const report = D.report(secret);
  for (const value of [
    "user:pass",
    "token=secret",
    "abc123",
    "hidden;",
    "private title",
    "private text",
    "file.txt",
  ])
    assert(!report.includes(value), value);
  assert.equal(secret.httpStatus, "503");
  assert.equal(secret.retryable, true);
  assert.equal(D.diagnose(Error("mystery")).code, "UNKNOWN_ERROR");
  assert.match(D.report(D.diagnose(Error("old"), { legacy: true })), /旧记录/);
  state.queue = [];
  mode = "http403";
  const localFailed = (await call({ action: "submit", capture: image })).record;
  mode = "localCancel";
  const choosing = (
    await call({ action: "image.local.start", id: localFailed.id })
  ).record;
  assert.equal(choosing.localPending, true);
  assert.equal(choosing.status, "sending");
  assert.equal(choosing.retryable, false);
  worker();
  await call({ action: "list" });
  assert.equal(state.queue[0].localPending, true);
  const cancelled = (
    await call({ action: "image.local.status", id: localFailed.id })
  ).record;
  assert.equal(cancelled.status, "failed");
  assert.equal(cancelled.errorCode, "IMAGE_LOCAL_CANCELLED");
  assert.equal(cancelled.diagnostic.retryable, false);
  mode = "localFail";
  await call({ action: "image.local.start", id: localFailed.id });
  const badLocal = (
    await call({ action: "image.local.status", id: localFailed.id })
  ).record;
  assert.equal(badLocal.errorCode, "IMAGE_TYPE");
  assert.equal(badLocal.status, "failed");
  mode = "localSuccess";
  await call({ action: "image.local.start", id: localFailed.id });
  const localDone = (
    await call({ action: "image.local.status", id: localFailed.id })
  ).record;
  assert.equal(localDone.status, "done");
  assert.equal(localDone.result.id, "restored-image");
  assert.equal(localDone.diagnostic, undefined);
  assert.equal(localDone.localPending, undefined);
  assert.equal(localDone.capture.imageUrl, image.imageUrl);
  assert.equal(
    (await call({ action: "image.local.start", id: localFailed.id })).ok,
    false,
  );
  mode = "http403";
  const oldCompatibility = (await call({ action: "submit", capture: image }))
    .record;
  mode = "online";
  const oldDesktop = (
    await call({ action: "image.local.start", id: oldCompatibility.id })
  ).record;
  assert.equal(oldDesktop.errorCode, "BRIDGE_PROTOCOL");
  console.log(
    "PASS v0.6.10 local recovery: pending state, worker restart, cancel retained, bad image retained, reselect success with same capture and cleared error, completed task blocked, old desktop capability guard.",
  );
  console.log(
    "PASS v0.6.9: HTTP 403 detail, stage and permanent retry; unknown transport preserved; quota + validation diagnostics; success has no stale diagnostic; HTTP 503 retry; report redaction and legacy fallback.",
  );
  console.log(
    "PASS v0.6.6 action popup routing without new tabs; blocked-popup fallback preserves draft and badge",
  );
  console.log(
    "PASS v0.6.2 image draft/menu, capture.image routing, image payload preservation, permanent type vs transient download errors, same-ID retry, unsupported URL rejection; all inherited v0.6.1 queue assertions also passed.",
  );
  console.log(
    "PASS inherited worker: isolated concurrent drafts / TTL / cap; startup alarm recreation; specific connection failures; permanent vs disk retry; malformed ACK rejected; old sending ID recovery; per-item retry; 200-task no-loss capacity and untrusted-page denial.",
  );
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
