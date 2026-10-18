import "./diagnostics.js";
const HOST = "local.shiqian.capture";
let serial = Promise.resolve();
const locked = (fn) => {
  const result = serial.then(fn);
  serial = result.catch(() => {});
  return result;
};
const load = async () => {
  const { queue = [] } = await chrome.storage.local.get("queue");
  return queue;
};
const save = async (queue) => {
  try {
    await chrome.storage.local.set({ queue });
  } catch (error) {
    throw Object.assign(Error(String(error?.message || error)), {
      code: "BROWSER_STORAGE",
    });
  }
};
let desktopVersion = "";
const problem = (error, context = {}) =>
  ShiqianDiagnostics.diagnose(error, { desktopVersion, ...context });
const native = async (action, capture) => {
  let reply;
  try {
    reply = await chrome.runtime.sendNativeMessage(HOST, {
      protocol: 1,
      action,
      capture,
    });
  } catch (error) {
    const raw = String(error?.message || error);
    const code = /not found|not registered/i.test(raw)
      ? "BRIDGE_NOT_REGISTERED"
      : /forbidden|policy|permission/i.test(raw)
        ? "BRIDGE_FORBIDDEN"
        : /exited|disconnect|connect|closed/i.test(raw)
          ? "BRIDGE_OFFLINE"
          : "BRIDGE_TRANSPORT";
    throw Object.assign(Error(raw), { code });
  }
  if (!reply?.ok)
    throw Error(reply?.error || "BRIDGE_PROTOCOL: 桌面没有返回有效回执");
  if (
    ((action === "capture" || action === "capture.image") &&
      (typeof reply.result?.id !== "string" ||
        !reply.result.id ||
        typeof reply.result.duplicate !== "boolean")) ||
    (action === "ping" && reply.result?.protocol !== 1)
  )
    throw Error("BRIDGE_PROTOCOL: 桌面没有返回有效回执");
  if (action === "ping") desktopVersion = reply.result.version || "";
  return reply.result;
};
function validate(capture) {
  let url;
  try {
    url = new URL(capture.url);
  } catch {
    throw Error("请输入完整的 HTTP 或 HTTPS 链接。");
  }
  if (
    !["http:", "https:"].includes(url.protocol) ||
    url.username ||
    url.password ||
    capture.url.length > 8192 ||
    /[\x00-\x1f\x7f]/.test(capture.url)
  )
    throw Error("仅支持不含登录凭据的 HTTP 或 HTTPS 页面。");
  if (
    !capture.title ||
    [...capture.title].length > 300 ||
    /[\x00-\x1f\x7f]/.test(capture.title)
  )
    throw Error("标题应为 1～300 个字符，不能包含控制字符。");
  if (!["web_page", "web_excerpt", "web_image"].includes(capture.kind))
    throw Error("不支持此采集类型。");
  if ([...capture.excerpt].length > 20000 || capture.excerpt.includes("\0"))
    throw Error("摘录最多 20,000 个字符，不能包含空字符。");
  if (capture.kind === "web_image") {
    let image;
    try {
      image = new URL(capture.imageUrl);
    } catch {
      throw Error("请输入完整的图片 HTTP 或 HTTPS 地址。");
    }
    if (
      !["http:", "https:"].includes(image.protocol) ||
      image.username ||
      image.password ||
      capture.imageUrl.length > 8192 ||
      /[\x00-\x1f\x7f]/.test(capture.imageUrl)
    )
      throw Error("仅支持不含登录凭据的 HTTP 或 HTTPS 图片地址。");
  }
  if (capture.kind === "web_excerpt" && !capture.excerpt)
    throw Error("请先选择或填写需要保存的文字。");
  if (
    capture.tagIds &&
    (!Array.isArray(capture.tagIds) ||
      capture.tagIds.length > 50 ||
      capture.tagIds.some(
        (id) =>
          typeof id !== "string" ||
          !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
            id,
          ),
      ))
  )
    throw Error("标签选择无效，最多选择50个标签。");
  if (
    capture.pageText &&
    (typeof capture.pageText !== "string" ||
      [...capture.pageText].length > 12000 ||
      capture.pageText.includes("\0"))
  )
    throw Error("网页正文过长或包含无效字符。");
  return capture;
}
async function send(record, queue) {
  record.status = "sending";
  record.lastAttemptAt = Date.now();
  record.attempts = (record.attempts || 0) + 1;
  record.error = "";
  record.errorCode = "";
  delete record.diagnostic;
  await save(queue);
  try {
    if (record.capture.tagIds?.length || record.capture.pageText) {
      const status = await native("ping");
      if (!status.capabilities?.includes("captureTags"))
        throw Error("BRIDGE_PROTOCOL: 请先升级桌面端至 v0.6.5，再重试此项。");
    }
    record.result = await native(
      record.capture.kind === "web_image" ? "capture.image" : "capture",
      record.capture,
    );
    record.status = "done";
    record.completedAt = Date.now();
    record.retryable = true;
  } catch (error) {
    const p = problem(error, {
      ...record,
      elapsedMs: Date.now() - record.lastAttemptAt,
    });
    record.diagnostic = p;
    record.status = "failed";
    record.error = p.message;
    record.errorCode = p.code;
    record.retryable = p.retryable;
  }
  await save(queue);
}
function applyLocalState(record, state) {
  if (state.status === "done") {
    if (!state.result?.id || typeof state.result.duplicate !== "boolean")
      throw Error("BRIDGE_PROTOCOL: 本地原图没有有效保存回执");
    record.status = "done";
    record.result = state.result;
    record.completedAt = Date.now();
    record.retryable = true;
    record.error = "";
    record.errorCode = "";
    delete record.diagnostic;
    delete record.localPending;
  } else if (state.status === "pending") {
    record.status = "sending";
    record.localPending = true;
    record.retryable = false;
    record.error = "";
    record.errorCode = "";
    delete record.diagnostic;
  } else if (state.status === "failed" || state.status === "cancelled") {
    const diagnostic = problem(
      Error(
        state.error ||
          "IMAGE_LOCAL_CANCELLED: 已取消原图选择，原任务仍在队列中",
      ),
      { ...record, elapsedMs: Date.now() - record.lastAttemptAt },
    );
    diagnostic.retryable = false;
    record.status = "failed";
    record.error = diagnostic.message;
    record.errorCode = diagnostic.code;
    record.diagnostic = diagnostic;
    record.retryable = false;
    delete record.localPending;
  } else throw Error("BRIDGE_PROTOCOL: 原图选择状态无效");
}
const retryCandidates = (queue, id) =>
  queue
    .filter(
      (r) =>
        r.status !== "done" && r.retryable !== false && (!id || r.id === id),
    )
    .sort((a, b) => (a.lastAttemptAt || 0) - (b.lastAttemptAt || 0))
    .slice(
      0,
      queue.some(
        (r) =>
          r.status !== "done" &&
          r.capture.kind === "web_image" &&
          r.retryable !== false,
      )
        ? 2
        : 10,
    );
async function drafts() {
  const { drafts = {} } = await chrome.storage.local.get("drafts");
  return Object.fromEntries(
    Object.entries(drafts).filter(
      ([, d]) => d.createdAt > Date.now() - 86400000,
    ),
  );
}
chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (
    sender.id !== chrome.runtime.id ||
    !sender.url?.startsWith(chrome.runtime.getURL(""))
  )
    return false;
  locked(async () => {
    if (message.action === "status") {
      try {
        return { connected: true, ...(await native("ping")) };
      } catch (error) {
        const p = problem(error);
        return { connected: false, error: p.message, errorCode: p.code };
      }
    }
    if (message.action === "tags") {
      const status = await native("ping");
      if (!status.capabilities?.includes("captureTags"))
        throw Error("BRIDGE_PROTOCOL: 标签选择需要桌面端 v0.6.5 或更新版本。");
      return await native("tags", {
        query: String(message.query || "").slice(0, 80),
      });
    }
    if (message.action === "list") return { queue: await load() };
    if (message.action === "draft") {
      const entries = await drafts();
      await chrome.storage.local.set({ drafts: entries });
      return { draft: entries[message.id]?.capture };
    }
    if (message.action === "draft.remove") {
      const entries = await drafts();
      delete entries[message.id];
      await chrome.storage.local.set({ drafts: entries });
      return { ok: true };
    }
    if (message.action === "submit") {
      const input = message.capture || {};
      let capture;
      try {
        capture = validate({
          requestId: crypto.randomUUID(),
          kind: input.kind,
          title: String(input.title || "").trim(),
          url: String(input.url || "").trim(),
          excerpt:
            input.kind === "web_excerpt"
              ? String(input.excerpt || "").trim()
              : "",
          ...(input.kind === "web_image"
            ? { imageUrl: String(input.imageUrl || "").trim() }
            : {}),
          ...(input.tagIds?.length
            ? { tagIds: [...new Set(input.tagIds)] }
            : {}),
          ...(input.kind === "web_page" && input.pageText
            ? { pageText: String(input.pageText).trim() }
            : {}),
        });
      } catch (error) {
        throw Object.assign(error, { code: "CAPTURE_INVALID" });
      }
      const queue = await load();
      if (message.replaceId) {
        const i = queue.findIndex(
          (r) => r.id === message.replaceId && r.status !== "done",
        );
        if (i >= 0) queue.splice(i, 1);
      }
      while (queue.length >= 200 && queue.some((r) => r.status === "done"))
        queue.splice(
          queue.findIndex((r) => r.status === "done"),
          1,
        );
      if (queue.length >= 200)
        throw Error("CAPTURE_QUEUE_FULL: 队列已满，请先处理或删除旧任务。");
      const record = {
        id: capture.requestId,
        capture,
        status: "pending",
        createdAt: Date.now(),
        attempts: 0,
        error: "",
      };
      queue.push(record);
      await save(queue);
      await send(record, queue);
      return { record };
    }
    if (
      message.action === "image.local.start" ||
      message.action === "image.local.status"
    ) {
      const queue = await load(),
        record = queue.find((r) => r.id === message.id);
      if (
        !record ||
        record.capture.kind !== "web_image" ||
        record.status === "done"
      )
        throw Error("IMAGE_INVALID: 没有可恢复的图片任务");
      try {
        if (message.action === "image.local.start") {
          const status = await native("ping");
          if (!status.capabilities?.includes("localImageRecovery"))
            throw Error(
              "BRIDGE_PROTOCOL: 请选择 v0.6.10 或更新桌面端，并修复浏览器连接",
            );
          record.lastAttemptAt = Date.now();
          record.attempts = (record.attempts || 0) + 1;
          const state = await native(
            "capture.image.local.start",
            record.capture,
          );
          applyLocalState(record, state);
        } else if (record.localPending) {
          const state = await native(
            "capture.image.local.status",
            record.capture,
          );
          applyLocalState(record, state);
        }
      } catch (error) {
        const diagnostic = problem(error, {
          ...record,
          elapsedMs: Date.now() - record.lastAttemptAt,
        });
        diagnostic.retryable = false;
        record.status = "failed";
        record.error = diagnostic.message;
        record.errorCode = diagnostic.code;
        record.diagnostic = diagnostic;
        record.retryable = false;
        delete record.localPending;
      }
      await save(queue);
      return { record };
    }
    if (message.action === "retry") {
      const queue = await load();
      for (const record of retryCandidates(queue, message.id))
        await send(record, queue);
      return { queue };
    }
    if (message.action === "remove") {
      const queue = await load();
      await save(queue.filter((r) => r.id !== message.id));
      return { ok: true };
    }
    throw Error("不支持的操作。");
  }).then(
    (result) => respond({ ok: true, ...result }),
    (error) => {
      const diagnostic = problem(error, { capture: message.capture });
      respond({
        ok: false,
        error: diagnostic.message,
        errorCode: diagnostic.code,
        diagnostic,
      });
    },
  );
  return true;
});
async function ensureAlarm() {
  if (!(await chrome.alarms.get("retry")))
    await chrome.alarms.create("retry", { periodInMinutes: 1 });
}
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({
      id: "page",
      title: "保存网页链接到拾签",
      contexts: ["page"],
    });
    chrome.contextMenus.create({
      id: "image",
      title: "保存图片到拾签",
      contexts: ["image"],
      documentUrlPatterns: ["http://*/*", "https://*/*"],
    });
    chrome.contextMenus.create({
      id: "selection",
      title: "保存选中文字到拾签",
      contexts: ["selection"],
    });
  });
  void ensureAlarm().catch(console.error);
});
chrome.runtime.onStartup.addListener(() => {
  void ensureAlarm().catch(console.error);
});
async function showCapturePopup(tab, popup) {
  await chrome.action.setPopup({ tabId: tab.id, popup });
  try {
    await chrome.action.openPopup({ windowId: tab.windowId });
  } catch {
    await chrome.action.setBadgeText({ tabId: tab.id, text: "•" });
    await chrome.action.setTitle({
      tabId: tab.id,
      title: "采集草稿已准备好，点击拾签继续保存",
    });
  }
}
chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (!tab?.url) return;
  locked(async () => {
    const entries = await drafts();
    if (Object.keys(entries).length >= 20) {
      await showCapturePopup(tab, "popup.html?draftLimit=1");
      return;
    }
    const id = crypto.randomUUID();
    entries[id] = {
      createdAt: Date.now(),
      capture: {
        title: tab.title || tab.url,
        url: tab.url,
        kind:
          info.menuItemId === "image"
            ? "web_image"
            : info.menuItemId === "selection"
              ? "web_excerpt"
              : "web_page",
        excerpt:
          info.menuItemId === "selection" ? info.selectionText || "" : "",
        ...(info.menuItemId === "image" ? { imageUrl: info.srcUrl || "" } : {}),
      },
    };
    if (entries[id].capture.kind === "web_page") {
      try {
        const data = await chrome.scripting.executeScript({
          target: { tabId: tab.id },
          func: () =>
            [
              ...(
                document.querySelector("article, main, [role=main]") ||
                document.body
              ).innerText.slice(0, 24000),
            ]
              .slice(0, 12000)
              .join(""),
        });
        entries[id].capture.pageText = data[0]?.result || "";
      } catch {}
    }
    await chrome.storage.local.set({ drafts: entries });
    await showCapturePopup(tab, "popup.html?draft=" + id);
  }).catch(console.error);
});
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "retry")
    locked(async () => {
      const queue = await load();
      for (const record of retryCandidates(queue)) await send(record, queue);
    }).catch(console.error);
});
// Recovery is serialized before any submissions, preserving v0.6.0 queue IDs.
locked(async () => {
  const queue = await load();
  let changed = false;
  for (const record of queue) {
    if (record.status === "sending") {
      record.status = record.localPending ? "sending" : "pending";
      changed = true;
    }
    if (record.status === "failed" && record.retryable === undefined) {
      const p = problem(record.error);
      record.errorCode = p.code;
      record.error = p.message;
      record.retryable = p.retryable;
      changed = true;
    }
  }
  if (changed) await save(queue);
  await ensureAlarm();
}).catch(console.error);
