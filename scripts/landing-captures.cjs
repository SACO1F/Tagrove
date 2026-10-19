// Capture real Tagrove UI for the introduction page.
// Launches the built desktop app against an isolated QA library, seeds a
// synthetic reference set through the app's own api actions, then photographs
// the states the landing page shows as evidence. No UI is mocked here.
const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
const { chromium } = require(process.env.PLAYWRIGHT_MODULE);

const root = path.resolve(__dirname, "..");
const qa = path.join(root, "qa", "v0-landing");
const out = path.join(qa, "shots");
const port = Number(process.env.SHIQIAN_CDP_PORT || "9231");
const exe = process.env.LANDING_EXE || path.join(root, "src-tauri", "target", "release", "shiqian.exe");
const material = path.join(qa, "material");
fs.mkdirSync(out, { recursive: true });

const api = (page, action, payload = {}) =>
  page.evaluate(({ action, payload }) => window.__TAURI_INTERNALS__.invoke("api", { action, payload }), {
    action,
    payload,
  });
const win = (page, label, width, height) =>
  page
    .evaluate(
      ({ label, width, height }) =>
        window.__TAURI_INTERNALS__.invoke("plugin:window|set_size", { label, value: { Logical: { width, height } } }),
      { label, width, height }
    )
    .catch(() => {});

const log = [];
const failures = [];
const step = async (name, fn) => {
  try {
    await fn();
  } catch (e) {
    failures.push(name + ": " + (e && e.message ? e.message.split("\n")[0] : String(e)));
    console.log("STEP FAILED", name, e && e.message ? e.message.split("\n").slice(0, 2).join(" / ") : e);
  }
};
const shot = async (page, name, opts = {}) => {
  await page.screenshot({ path: path.join(out, name), animations: "disabled", ...opts });
  log.push(name);
};
const settle = async (page, ms = 700) => page.waitForTimeout(ms);
const imagesReady = async (page) => {
  await page
    .waitForFunction(
      () => {
        const imgs = [...document.querySelectorAll("main img")].filter((i) => i.closest("[class*=masonry], [class*=gallery], [class*=card]"));
        if (!imgs.length) return true;
        return imgs.every((i) => i.complete && i.naturalWidth > 0);
      },
      null,
      { timeout: 20000 }
    )
    .catch(() => {});
};

(async () => {
  const errors = [];
  let browser;
  try {
    // Every run starts from an empty library so tags and counts are deterministic.
    fs.rmSync(path.join(qa, "library"), { recursive: true, force: true });
    execFileSync(process.execPath, ["scripts/launch-qa.cjs", exe], {
      cwd: root,
      env: { ...process.env, SHIQIAN_QA_DATA_DIR: path.join(qa, "library"), SHIQIAN_CDP_PORT: String(port) },
      windowsHide: true,
      stdio: "inherit",
    });
    browser = await chromium.connectOverCDP("http://127.0.0.1:" + port);
    const ctx = browser.contexts()[0];
    let p = ctx.pages().find((x) => !x.url().includes("floating"));
    p.on("pageerror", (e) => errors.push(String(e)));
    await p.locator("#workspace-sidebar").waitFor({ timeout: 60000 });

    const boot = await api(p, "bootstrap");
    await p.evaluate((d) => localStorage.setItem(`shiqian-onboarding:v1:${d}`, "done"), boot.dataPath);
    await api(p, "settings.save", { key: "language", value: "zh-CN" });
    await api(p, "settings.save", { key: "theme", value: "light" });
    await p.reload();
    await p.locator("#workspace-sidebar").waitFor();
    await win(p, "main", 1360, 870);
    await settle(p, 500);

    // Seed: import the synthetic folder, then tag it the way a studio would.
    await api(p, "import", { paths: [material], recursive: true });
    let files = [];
    for (let i = 0; i < 60; i++) {
      const r = await api(p, "query", { scope: "all", text: "", offset: 0, limit: 100 });
      files = r.files || [];
      if (files.length >= 9) break;
      await settle(p, 500);
    }
    console.log("imported files:", files.length);
    const byName = (frag) => files.find((f) => f.name.includes(frag));
    const TAGS = ["建筑外墙", "项目参考", "待讨论", "材料做法", "室内灵感", "造价参考", "节点图纸"];
    const made = {};
    for (const name of TAGS) made[name] = await api(p, "tag.create", { name });
    const assign = async (tag, frags) => {
      const ids = frags.map(byName).filter(Boolean).map((f) => f.id);
      if (ids.length) await api(p, "annotation.apply", { tagId: made[tag].id, ids });
    };
    await assign("建筑外墙", ["立面细部", "方案文本", "墙身节点"]);
    await assign("项目参考", ["黄昏", "幕墙分缝", "汇报大纲"]);
    await assign("待讨论", ["幕墙分缝", "方案文本", "黄昏"]);
    await assign("材料做法", ["材料样板", "立面细部", "成本估算"]);
    await assign("室内灵感", ["大堂采光"]);
    await assign("造价参考", ["成本估算"]);
    await assign("节点图纸", ["墙身节点"]);
    const pdf = byName("方案文本");
    if (pdf) await api(p, "note.save", { id: pdf.id, text: "立面分缝与窗套做法，汇报第 14 页引用过。", version: pdf.noteVersion ?? 0 });

    files = (await api(p, "query", { scope: "all", text: "", offset: 0, limit: 100 })).files || [];
    await p.reload();
    await p.locator("#workspace-sidebar").waitFor();
    await settle(p, 1200);
    await imagesReady(p);
    await shot(p, "01-gallery-light.png");

    // Search with visible match evidence.
    await step("search", async () => {
      const names = await p
        .locator("button")
        .evaluateAll((els) =>
          els
            .filter((e) => e.getAttribute("aria-label"))
            .slice(0, 60)
            .map((e) => e.getAttribute("aria-label") + (e.offsetParent ? "" : " [hidden]"))
        );
      fs.writeFileSync(path.join(qa, "buttons.json"), JSON.stringify(names, null, 1));
      await p.getByRole("button", { name: "展开搜索栏", exact: true }).first().click({ timeout: 8000 });
      await settle(p, 700);
      const box = p.locator('input[aria-label="搜索文件"]').first();
      await box.fill("外墙");
      await settle(p, 1800);
      await shot(p, "02-search-evidence.png");
      await box.fill("");
      await settle(p, 500);
      await p.getByRole("button", { name: "收起搜索栏", exact: true }).first().click({ timeout: 4000 }).catch(() => {});
    });
    await settle(p, 700);

    // Inspector steps need the unfiltered gallery again.
    await p.reload();
    await p.locator("#workspace-sidebar").waitFor();
    await settle(p, 1800);
    await imagesReady(p);
    const leafText = await p
      .locator("main")
      .evaluate((el) =>
        [...el.querySelectorAll("*")]
          .filter((n) => !n.children.length && n.textContent.trim())
          .map((n) => n.textContent.trim())
          .slice(0, 120)
      )
      .catch(() => []);
    fs.writeFileSync(path.join(qa, "cards.json"), JSON.stringify(leafText, null, 1));
    const openFile = async (frag) => {
      const pt = await p.evaluate((frag) => {
        const norm = (s) => s.replace(/\s+/g, "").normalize("NFC");
        const els = [...document.querySelectorAll("main *")].filter(
          (n) => !n.children.length && norm(n.textContent) === norm(frag)
        );
        const el = els[0] || [...document.querySelectorAll("main *")].find((n) => !n.children.length && norm(n.textContent).includes(norm(frag)));
        if (!el) return null;
        el.scrollIntoView({ block: "center" });
        const r = el.getBoundingClientRect();
        return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
      }, frag);
      if (!pt) {
        // Fall back to the first card in the gallery, which is always selectable.
        await p.mouse.click(260, 260);
        return "fallback";
      }
      await settle(p, 350);
      await p.mouse.click(pt.x, pt.y);
      return true;
    };

    // Inspector reading a PDF.
    if (pdf) {
      await step("inspector-pdf", async () => {
        if (!(await openFile("方案文本"))) throw Error("no clickable card for 方案文本; see cards.json");
        await settle(p, 2800);
        await shot(p, "03-inspector-pdf.png");
      });
    }

    // Inspector on a document with the annotation tab open.
    await step("inspector-annotation", async () => {
      if (!(await openFile("幕墙分缝"))) throw Error("no clickable card for 幕墙分缝; see cards.json");
      await settle(p, 1600);
      const tab = p.getByRole("button", { name: "标注", exact: true }).first();
      if (await tab.count()) {
        await tab.click({ timeout: 4000 }).catch(() => {});
        await settle(p, 900);
      }
      await shot(p, "04-inspector-annotation.png");
    });

    // The floating tag palette, as its own always-on-top window.
    await api(p, "floating.open");
    let f;
    for (let i = 0; i < 60; i++) {
      f = ctx.pages().find((x) => x.url().includes("floating"));
      if (f) break;
      await settle(p, 200);
    }
    if (f) {
      f.on("pageerror", (e) => errors.push("floating:" + e));
      await f.locator(".palette-chips").waitFor({ timeout: 20000 });
      await win(f, "floating", 360, 520);
      await settle(f, 800);
      await f.screenshot({ path: path.join(out, "05-palette-light.png"), animations: "disabled" });
      log.push("05-palette-light.png");
      await api(p, "settings.save", { key: "theme", value: "dark" });
      await settle(f, 1200);
      await f.screenshot({ path: path.join(out, "06-palette-dark.png"), animations: "disabled" });
      log.push("06-palette-dark.png");
      await api(p, "settings.save", { key: "theme", value: "light" });
    } else {
      errors.push("floating window never appeared");
    }

    // Dark workspace: the theme is read at boot, so reload before shooting.
    await api(p, "settings.save", { key: "theme", value: "dark" });
    await p.reload();
    await p.locator("#workspace-sidebar").waitFor();
    await settle(p, 1600);
    await imagesReady(p);
    const dark = await p.evaluate(() => document.documentElement.dataset.theme || "");
    if (dark !== "dark") throw Error("theme did not apply, got: " + dark);
    await shot(p, "07-gallery-dark.png");
    await api(p, "settings.save", { key: "theme", value: "light" });

    fs.writeFileSync(
      path.join(qa, "capture-result.json"),
      JSON.stringify({ exe, dataPath: boot.dataPath, files: files.length, shots: log, errors }, null, 2)
    );
    console.log("captured:", log.join(" "));
    if (errors.length) console.log("page errors:", errors.join(" | "));
  } catch (e) {
    console.error("FAILED:", e && e.message ? e.message : e);
    process.exitCode = 1;
  } finally {
    if (browser) await browser.close().catch(() => {});
    execFileSync(process.execPath, ["scripts/close-qa.cjs"], {
      cwd: root,
      env: { ...process.env, SHIQIAN_CDP_PORT: String(port) },
      windowsHide: true,
    });
  }
})();
