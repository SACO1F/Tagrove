import "./diagnostics.js";
const $ = (id) => document.getElementById(id);
const call = async (message) => {
  const reply = await chrome.runtime.sendMessage(message);
  if (!reply?.ok)
    throw Object.assign(Error(reply?.error || "操作未完成，请重试"), {
      diagnostic: reply?.diagnostic,
    });
  return reply;
};
const params = new URLSearchParams(location.search),
  draftId = params.get("draft"),
  editId = params.get("edit");
let pageText = "";
const selectedTags = new Map();
let tagRequest = 0;
const sync = () => {
  const kind = document.querySelector('input[name="kind"]:checked').value;
  $("excerpt-field").hidden = kind !== "web_excerpt";
  $("image-field").hidden = kind !== "web_image";
  if (kind === "web_image")
    $("capture-hint").textContent =
      "核对来源网页和图片地址后保存；仅采集这张图片。";
  $("submit").textContent =
    kind === "web_image" ? "保存图片到拾签" : "保存到拾签";
};
document
  .querySelectorAll('input[name="kind"]')
  .forEach((el) => el.addEventListener("change", sync));
async function connection() {
  $("check-connection").disabled = true;
  try {
    const status = await call({ action: "status" });
    $("connection").textContent = status.connected
      ? "已连接本机拾签 · v" + status.version
      : status.error;
  } catch (error) {
    $("connection").textContent = error.message;
  } finally {
    $("check-connection").disabled = false;
  }
}
(async () => {
  try {
    let draft;
    if (editId) {
      const queue = (await call({ action: "list" })).queue;
      draft = queue.find(
        (r) => r.id === editId && r.status !== "done",
      )?.capture;
    } else if (draftId) {
      draft = (await call({ action: "draft", id: draftId })).draft;
      if (!draft)
        $("capture-hint").textContent =
          "草稿不存在或已超过 24 小时，请返回原网页重新采集。";
    } else if (params.has("draftLimit")) {
      $("capture-hint").textContent =
        "未提交草稿已达 20 条。请先保存草稿，或等待旧草稿过期后重新采集。";
    } else {
      const [tab] = await chrome.tabs.query({
        active: true,
        currentWindow: true,
      });
      draft = {
        title: tab?.title || "",
        url: /^https?:/.test(tab?.url || "") ? tab.url : "",
        kind: "web_page",
        excerpt: "",
      };
      if (tab?.id && draft.url) {
        try {
          const results = await chrome.scripting.executeScript({
            target: { tabId: tab.id },
            func: () => {
              // Read text fields explicitly; password and other sensitive control types are excluded.
              const active = document.activeElement;
              if (
                active instanceof HTMLInputElement &&
                active.type === "password"
              )
                return "";
              if (
                active instanceof HTMLTextAreaElement ||
                (active instanceof HTMLInputElement &&
                  ["text", "search", "url", "email", "tel"].includes(
                    active.type,
                  ))
              ) {
                const start = active.selectionStart,
                  end = active.selectionEnd;
                if (start !== null && end !== null && end > start)
                  return active.value.slice(start, end);
              }
              return window.getSelection()?.toString() || "";
            },
          });
          draft.excerpt = results[0]?.result || "";
          try {
            const bodies = await chrome.scripting.executeScript({
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
            draft.pageText = bodies[0]?.result || "";
          } catch {}
          if (draft.excerpt.trim()) draft.kind = "web_excerpt";
          $("capture-hint").textContent = draft.excerpt
            ? "已读取当前选中文字，请核对摘录后保存。"
            : "未检测到选中文字。可保存链接，或切换“文字摘录”手动填写。";
        } catch {
          $("capture-hint").textContent =
            "此页面限制读取选区。可保存链接，或手动粘贴文字摘录。";
        }
      } else
        $("capture-hint").textContent =
          "仅支持 HTTP／HTTPS 网页。请返回普通网页采集，或手动填写来源。";
    }
    if (draft) {
      pageText = draft.pageText || "";
      const cache =
        (await chrome.storage.local.get("captureTagCatalog"))
          .captureTagCatalog || [];
      for (const id of draft.tagIds || [])
        selectedTags.set(
          id,
          cache.find((t) => t.id === id)?.name || "不可用标签",
        );
      $("title").value = draft.title;
      $("url").value = draft.url;
      $("excerpt").value = draft.excerpt || "";
      $("image-url").value = draft.imageUrl || "";
      document.querySelector(
        `input[value="${["web_excerpt", "web_image"].includes(draft.kind) ? draft.kind : "web_page"}"]`,
      ).checked = true;
      sync();
    }
  } catch (error) {
    $("capture-hint").textContent = error.message;
  }
  await connection();
  await loadTags();
})();
$("check-connection").addEventListener("click", connection);
$("capture-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  $("submit").disabled = true;
  $("result").textContent = "正在暂存并发送…";
  $("failure-details").replaceChildren();
  try {
    const capture = {
      title: $("title").value,
      url: $("url").value,
      kind: document.querySelector('input[name="kind"]:checked').value,
      excerpt: $("excerpt").value,
      tagIds: [...selectedTags.keys()],
      pageText:
        document.querySelector('input[name="kind"]:checked').value ===
        "web_page"
          ? pageText
          : "",
      ...(document.querySelector('input[name="kind"]:checked').value ===
      "web_image"
        ? { imageUrl: $("image-url").value }
        : {}),
    };
    const { record } = await call({
      action: "submit",
      capture,
      replaceId: editId,
    });
    $("result").textContent =
      record.status === "done"
        ? record.result.duplicate
          ? "资料已存在，未重复保存。"
          : "已保存到拾签。" +
            (record.result.aiQueued ? "AI 建议将在桌面待确认。" : "")
        : "已暂存到浏览器队列。" + (record.error || "等待本机连接。");
    if (record.status === "failed")
      $("failure-details").append(
        ShiqianDiagnostics.panel(
          record.diagnostic ||
            ShiqianDiagnostics.diagnose(record.error, {
              ...record,
              legacy: true,
            }),
        ),
      );
    if (record.status === "failed" && capture.kind === "web_image") {
      const recover = document.createElement("button");
      recover.type = "button";
      recover.className = "secondary";
      recover.textContent = "到采集队列选择已下载原图";
      recover.onclick = () =>
        chrome.tabs.create({ url: chrome.runtime.getURL("queue.html") });
      $("failure-details").append(recover);
    }
    if (draftId) {
      await call({ action: "draft.remove", id: draftId });
      const [tab] = await chrome.tabs.query({
        active: true,
        currentWindow: true,
      });
      if (tab?.id !== undefined) {
        await chrome.action.setPopup({ tabId: tab.id, popup: "popup.html" });
        await chrome.action.setBadgeText({ tabId: tab.id, text: "" });
        await chrome.action.setTitle({ tabId: tab.id, title: "保存到拾签" });
      }
    }
  } catch (error) {
    $("result").textContent = "尚未确认保存，请保留当前内容。";
    $("failure-details").replaceChildren(
      ShiqianDiagnostics.panel(
        error.diagnostic || ShiqianDiagnostics.diagnose(error),
      ),
    );
  } finally {
    $("submit").disabled = false;
  }
});

$("preview-image").addEventListener("click", () => {
  const img = $("image-preview"),
    status = $("image-preview-status");
  try {
    const url = new URL($("image-url").value);
    if (
      !["http:", "https:"].includes(url.protocol) ||
      url.username ||
      url.password
    )
      throw Error();
    status.textContent = "正在加载图片预览…";
    img.hidden = true;
    img.onload = () => {
      img.hidden = false;
      status.textContent = "预览仅供核对；保存时由桌面校验原图。";
    };
    img.onerror = () => {
      img.hidden = true;
      status.textContent =
        "预览不可用，可核对地址后尝试保存；部分网站限制跨域预览。";
    };
    img.src = url.href;
  } catch {
    status.textContent = "请输入不含登录凭据的 HTTP 或 HTTPS 图片地址。";
    img.removeAttribute("src");
    img.hidden = true;
  }
});
$("image-url").addEventListener("input", () => {
  $("image-preview").hidden = true;
  $("image-preview").removeAttribute("src");
  $("image-preview-status").textContent = "";
});

$("url").addEventListener("input", () => {
  pageText = "";
  try {
    const url = new URL($("url").value);
    if (
      ["http:", "https:"].includes(url.protocol) &&
      !url.username &&
      !url.password
    )
      $("capture-hint").textContent =
        document.querySelector('input[name="kind"]:checked').value ===
        "web_image"
          ? "核对来源网页和图片地址后保存；仅采集这张图片。"
          : "核对标题、来源和内容后保存。";
  } catch {}
});

function renderTags(tags) {
  for (const tag of tags) {
    if (selectedTags.has(tag.id)) selectedTags.set(tag.id, tag.name);
  }
  $("tag-count").textContent = String(selectedTags.size);
  $("selected-tags").replaceChildren();
  for (const [id, name] of selectedTags) {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "tag-chip";
    chip.textContent = name + " ×";
    chip.setAttribute("aria-label", "取消标签：" + name);
    chip.onclick = () => {
      selectedTags.delete(id);
      renderTags(tags);
    };
    $("selected-tags").append(chip);
  }
  $("tag-options").replaceChildren();
  for (const tag of tags) {
    if (selectedTags.has(tag.id)) selectedTags.set(tag.id, tag.name);
    const label = document.createElement("label"),
      box = document.createElement("input"),
      name = document.createElement("span");
    label.className = "tag-choice";
    box.type = "checkbox";
    box.checked = selectedTags.has(tag.id);
    name.textContent = tag.name;
    box.onchange = () => {
      if (box.checked && selectedTags.size >= 50) {
        box.checked = false;
        $("tag-status").textContent = "最多选择50个标签。";
        return;
      }
      if (box.checked) selectedTags.set(tag.id, tag.name);
      else selectedTags.delete(tag.id);
      renderTags(tags);
    };
    label.append(box, name);
    $("tag-options").append(label);
  }
}
async function loadTags() {
  const request = ++tagRequest,
    query = $("tag-search").value;
  $("refresh-tags").disabled = true;
  try {
    const result = await call({ action: "tags", query });
    if (request !== tagRequest) return;
    const old =
      (await chrome.storage.local.get("captureTagCatalog")).captureTagCatalog ||
      [];
    const cache = new Map(old.map((t) => [t.id, t]));
    for (const t of result.tags) cache.set(t.id, t);
    await chrome.storage.local.set({
      captureTagCatalog: [...cache.values()].slice(-1500),
    });
    renderTags(result.tags);
    $("tag-status").textContent = result.truncated
      ? "显示前500个匹配标签，请搜索缩小范围。"
      : result.tags.length
        ? "可多选，文件夹标签和待确认 AI 标签不在此处显示。"
        : "没有匹配标签，可先到桌面端创建。";
  } catch (e) {
    if (request !== tagRequest) return;
    const cache =
      (await chrome.storage.local.get("captureTagCatalog")).captureTagCatalog ||
      [];
    renderTags(
      cache.filter((t) =>
        t.name.toLocaleLowerCase().includes(query.toLocaleLowerCase()),
      ),
    );
    $("tag-status").textContent = cache.length
      ? "暂未连接，使用缓存标签；采集仍可暂存。"
      : e.message;
  } finally {
    if (request === tagRequest) $("refresh-tags").disabled = false;
  }
}
let tagSearchTimer;
$("tag-search").addEventListener("input", () => {
  clearTimeout(tagSearchTimer);
  tagSearchTimer = setTimeout(loadTags, 250);
});
$("refresh-tags").addEventListener("click", loadTags);
