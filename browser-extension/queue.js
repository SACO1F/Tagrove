import "./diagnostics.js";
const list = document.getElementById("queue"),
  notice = document.getElementById("notice");
async function call(message) {
  const reply = await chrome.runtime.sendMessage(message);
  if (!reply?.ok)
    throw Object.assign(Error(reply?.error || "操作失败"), {
      diagnostic: reply?.diagnostic,
    });
  return reply;
}
const when = (time) =>
  time ? new Date(time).toLocaleString("zh-CN") : "尚未尝试";
function button(text, label, run) {
  const el = document.createElement("button");
  el.textContent = text;
  el.className = "secondary";
  el.setAttribute("aria-label", label);
  el.addEventListener("click", async () => {
    el.disabled = true;
    try {
      await run();
    } catch (error) {
      notice.textContent = error.message;
    } finally {
      el.disabled = false;
    }
  });
  return el;
}
let rendering = false,
  again = false;
async function render() {
  if (rendering) {
    again = true;
    return;
  }
  rendering = true;
  try {
    const { queue } = await call({ action: "list" });
    const active = document.activeElement,
      focusId = active?.closest("li")?.dataset.id,
      focusAction = active?.dataset.action;
    const expanded = new Set(
      [...list.querySelectorAll("li:has(details[open])")].map(
        (row) => row.dataset.id,
      ),
    );
    list.replaceChildren();
    notice.textContent = queue.length
      ? `${queue.filter((r) => r.status === "done").length} 已保存 · ${queue.filter((r) => r.status !== "done").length} 待处理`
      : "暂无采集记录";
    document.getElementById("retry").disabled = !queue.some(
      (r) => r.status !== "done" && r.retryable !== false,
    );
    for (const record of [...queue].reverse()) {
      const row = document.createElement("li"),
        body = document.createElement("div"),
        title = document.createElement("strong"),
        url = document.createElement("p"),
        state = document.createElement("p"),
        meta = document.createElement("p"),
        actions = document.createElement("div");
      row.dataset.id = record.id;
      row.dataset.status = record.status;
      actions.className = "queue-actions";
      title.textContent =
        (record.capture.kind === "web_image" ? "图片 · " : "") +
        record.capture.title;
      url.textContent =
        record.capture.kind === "web_image"
          ? record.capture.imageUrl
          : record.capture.url;
      url.className = "hint";
      state.textContent =
        (record.status === "done"
          ? record.result?.duplicate
            ? "已保存 · 复用已有资料"
            : "已保存"
          : record.status === "sending"
            ? record.localPending
              ? "请在拾签窗口选择已下载的原图；取消后保留任务"
              : "正在发送"
            : record.retryable === false
              ? "需要处理 · 请检查内容或版本后重新采集"
              : "等待重试") + (record.error ? " · " + record.error : "");
      state.className = "queue-state";
      meta.textContent =
        `创建 ${when(record.createdAt)} · 已尝试 ${record.attempts || 0} 次` +
        (record.lastAttemptAt ? ` · 最近 ${when(record.lastAttemptAt)}` : "") +
        (record.errorCode ? ` · ${record.errorCode}` : "");
      meta.className = "hint";
      if (record.status !== "done" && record.capture.kind === "web_image") {
        const source = button(
          "打开来源网页",
          "打开来源网页：" + record.capture.title,
          () => chrome.tabs.create({ url: record.capture.url }),
        );
        const local = button(
          "选择已下载原图",
          "选择已下载原图：" + record.capture.title,
          async () => {
            await call({ action: "image.local.start", id: record.id });
            await render();
            scheduleLocalPoll();
          },
        );
        local.disabled = record.status === "sending";
        local.dataset.action = "local";
        actions.append(source, local);
      }
      if (record.status !== "done") {
        actions.append(
          button("重新编辑", "重新编辑：" + record.capture.title, () =>
            chrome.tabs.create({
              url: chrome.runtime.getURL("popup.html?edit=" + record.id),
            }),
          ),
        );
        const edit = actions.querySelector('button[aria-label^="重新编辑："]');
        if (edit) edit.disabled = record.status === "sending";
        const retry = button(
          "重试",
          "重试：" + record.capture.title,
          async () => {
            await call({ action: "retry", id: record.id });
            await render();
          },
        );
        retry.disabled =
          record.status === "sending" || record.retryable === false;
        retry.dataset.action = "retry";
        actions.append(retry);
      }
      const remove = button(
        "删除",
        "删除队列项：" + record.capture.title,
        async () => {
          if (
            record.status !== "done" &&
            !confirm("这条资料尚未确认保存，仍要删除本地队列项吗？")
          )
            return;
          await call({ action: "remove", id: record.id });
          await render();
        },
      );
      remove.dataset.action = "remove";
      remove.disabled = record.status === "sending";
      actions.append(remove);
      body.append(title, url, state, meta);
      if (record.status === "failed") {
        const panel = ShiqianDiagnostics.panel(
          record.diagnostic ||
            ShiqianDiagnostics.diagnose(
              Object.assign(Error(record.error), { code: record.errorCode }),
              { ...record, legacy: true },
            ),
        );
        panel.querySelector("details").open = expanded.has(record.id);
        body.append(panel);
      }
      if (record.status !== "done" && record.capture.kind === "web_image") {
        const hint = document.createElement("p");
        hint.className = "hint";
        hint.textContent =
          "来源站点兼容重试仍失败时：在浏览器下载原图，再点击选择已下载原图。保留此任务的来源、标题和标签；请确认选择的是对应图片。";
        body.append(hint);
      }
      row.append(body, actions);
      list.append(row);
    }
    if (queue.some((r) => r.localPending)) scheduleLocalPoll();
    if (focusId && focusAction)
      [...list.children]
        .find((row) => row.dataset.id === focusId)
        ?.querySelector(`[data-action="${focusAction}"]`)
        ?.focus();
  } catch (error) {
    notice.textContent = error.message;
  } finally {
    rendering = false;
    if (again) {
      again = false;
      void render();
    }
  }
}
document.getElementById("retry").addEventListener("click", async (event) => {
  event.target.disabled = true;
  notice.textContent = "正在重试…";
  try {
    await call({ action: "retry" });
  } catch (error) {
    notice.textContent = error.message;
  } finally {
    await render();
  }
});
chrome.storage.onChanged.addListener((changes, area) => {
  if (area === "local" && changes.queue) void render();
});
let localPoll;
function scheduleLocalPoll() {
  if (localPoll) return;
  localPoll = setTimeout(async () => {
    localPoll = null;
    try {
      const { queue } = await call({ action: "list" });
      for (const r of queue.filter((r) => r.localPending))
        await call({ action: "image.local.status", id: r.id });
    } catch (error) {
      notice.textContent = error.message;
    }
    await render();
  }, 1500);
}
void render();
