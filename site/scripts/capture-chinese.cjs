// Capture the real Chinese desktop UI in a fresh, isolated synthetic library.
// Run from site/: node scripts/capture-chinese.cjs
const fs = require("node:fs");
const path = require("node:path");
const assert = require("node:assert/strict");
const { execFileSync } = require("node:child_process");
const { chromium } = require(
  process.env.PLAYWRIGHT_MODULE ||
    "C:/Users/10052/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright",
);
const root = path.resolve(__dirname, "../..");
const site = path.join(root, "site");
const run = path.join(
  root,
  "qa",
  "landing-zh-" + new Date().toISOString().replace(/[:.]/g, "-"),
);
const material = path.join(run, "material", "江畔塔楼");
const shots = path.join(run, "shots");
const frames = path.join(run, "frames");
const port = "9238";
for (const folder of [material, shots, frames])
  fs.mkdirSync(folder, { recursive: true });
for (const [source, name] of [
  ["tower-dusk.jpg", "江畔塔楼-黄昏.jpg"],
  ["facade-detail.jpg", "混凝土立面.jpg"],
  ["material-board.jpg", "外墙材料样板.jpg"],
  ["node-drawing.jpg", "墙身节点.jpg"],
]) {
  fs.copyFileSync(path.join(site, "assets", source), path.join(material, name));
}
fs.copyFileSync(
  path.join(__dirname, "fixtures/sample-zh.pdf"),
  path.join(material, "方案文本-初稿.pdf"),
);
fs.writeFileSync(
  path.join(material, "幕墙分缝讨论.md"),
  "# 幕墙分缝讨论\n\n立面分缝与窗套节点需要与结构专业复核。当前方案采用 1200 mm 模数，窗套内衬 3 mm 阳极氧化铝板，收口打胶宽度控制为 12 mm。\n\n## 待讨论\n\n- 分缝是否与室内吊顶对齐\n- 阳台栏板高度与分缝关系\n- 夜景照明预留槽位置\n\n以上为拾签演示用的虚构资料。\n",
);
fs.writeFileSync(
  path.join(material, "方案汇报大纲.md"),
  "# 江畔塔楼方案汇报\n\n1. 城市界面与塔楼体量\n2. 清水混凝土与竖向遮阳\n3. 材料样板与造价区间\n4. 下一步：节点深化与幕墙招标\n",
);
fs.writeFileSync(
  path.join(material, "材料说明.md"),
  "# 材料说明\n\n清水混凝土挂板与竖向铝遮阳。\n\n请在下一次设计会议前，与同事一起核对外墙材料样板。\n",
);
const env = {
  ...process.env,
  SHIQIAN_QA_DATA_DIR: path.join(run, "library"),
  SHIQIAN_CDP_PORT: port,
  PLAYWRIGHT_MODULE:
    process.env.PLAYWRIGHT_MODULE ||
    "C:/Users/10052/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright",
};
const api = (p, action, payload = {}) =>
  p.evaluate(
    ({ action, payload }) =>
      window.__TAURI_INTERNALS__.invoke("api", { action, payload }),
    { action, payload },
  );
const pause = (ms) => new Promise((r) => setTimeout(r, ms));
let browser,
  launched = false;
const errors = [];
async function ready(p) {
  await p.locator("#workspace-sidebar").waitFor({ timeout: 30000 });
  await pause(700);
}
async function shot(p, name) {
  await pause(500);
  const cjk = await p.locator("body").innerText();
  const lines = cjk.split("\n").filter((x) => /[\u3400-\u9fff]/.test(x));
  if (!lines.length)
    throw Error(
      name + " must contain Chinese interface text: " + lines.join(" | "),
    );
  await p.screenshot({
    path: path.join(shots, name + ".png"),
    animations: "disabled",
  });
}
const pointerPosition = new WeakMap();
async function glide(p, x, y, duration = 650) {
  const from = pointerPosition.get(p) || { x: 40, y: 80 };
  const steps = Math.max(1, Math.round(duration / 33));
  for (let i = 1; i <= steps; i++) {
    const t = i / steps,
      ease = t * t * (3 - 2 * t);
    await p.mouse.move(
      from.x + (x - from.x) * ease,
      from.y + (y - from.y) * ease,
    );
    await pause(duration / steps);
  }
  pointerPosition.set(p, { x, y });
}
async function pointAt(p, locator) {
  await locator.scrollIntoViewIfNeeded();
  const b = await locator.boundingBox();
  assert(b, "Target is not visible");
  await glide(p, b.x + b.width * 0.5, b.y + Math.min(b.height * 0.5, 90));
}
async function record(p, name, action) {
  const dir = path.join(frames, name);
  fs.mkdirSync(dir, { recursive: true });
  // Presentation-only pointer overlay: actual recorded mouse events, no UI text changes.
  await p.evaluate(() => {
    // Flatten the transparent floating window onto the page's paper backdrop.
    // This changes only the recording matte, not the component or its labels.
    if (document.documentElement.classList.contains("floating-window")) {
      window.__demoRootStyle = document.documentElement.getAttribute("style");
      document.documentElement.style.setProperty(
        "background",
        "#fffdf8",
        "important",
      );
    }
    const pointer = document.createElement("div");
    pointer.id = "qa-demo-pointer";
    pointer.setAttribute("aria-hidden", "true");
    pointer.style.cssText =
      "position:fixed;left:40px;top:80px;width:20px;height:26px;z-index:2147483647;pointer-events:none;filter:drop-shadow(0 1px 2px #fff)";
    pointer.innerHTML =
      '<svg viewBox="0 0 20 26" xmlns="http://www.w3.org/2000/svg"><path d="M2 1v21l5-5 4 8 4-2-4-8h7z" fill="#244d85" stroke="white" stroke-width="1.4"/></svg>';
    document.body.appendChild(pointer);
    window.__demoPointerMove = (e) => {
      pointer.style.left = e.clientX + "px";
      pointer.style.top = e.clientY + "px";
    };
    document.addEventListener("pointermove", window.__demoPointerMove);
  });
  pointerPosition.set(p, { x: 40, y: 80 });
  const session = await p.context().newCDPSession(p);
  const timeline = [];
  const pending = [];
  session.on("Page.screencastFrame", (event) => {
    const file = String(timeline.length).padStart(5, "0") + ".jpg";
    timeline.push({ file, time: event.metadata.timestamp });
    pending.push(
      fs.promises.writeFile(
        path.join(dir, file),
        Buffer.from(event.data, "base64"),
      ),
    );
    session
      .send("Page.screencastFrameAck", { sessionId: event.sessionId })
      .catch(() => {});
  });
  await session.send("Emulation.setDefaultBackgroundColorOverride", {
    color: { r: 255, g: 253, b: 248, a: 1 },
  });
  await session.send("Page.startScreencast", {
    format: "jpeg",
    quality: 90,
    everyNthFrame: 1,
  });
  try {
    await pause(250);
    await action();
    await pause(650);
  } finally {
    await session.send("Page.stopScreencast");
    await Promise.all(pending);
    await session.detach();
    await p.evaluate(() => {
      document.removeEventListener("pointermove", window.__demoPointerMove);
      delete window.__demoPointerMove;
      document.getElementById("qa-demo-pointer")?.remove();
      if ("__demoRootStyle" in window) {
        if (window.__demoRootStyle === null)
          document.documentElement.removeAttribute("style");
        else
          document.documentElement.setAttribute(
            "style",
            window.__demoRootStyle,
          );
        delete window.__demoRootStyle;
      }
    });
  }
  assert(timeline.length > 20, name + " needs continuous motion");
  fs.writeFileSync(path.join(dir, "timeline.json"), JSON.stringify(timeline));
  console.log(name + ": " + timeline.length + " compositor frames captured");
}
async function openFile(p, name) {
  const files = (
    await api(p, "query", { scope: "all", text: "", offset: 0, limit: 100 })
  ).files;
  const file = files.find((f) => f.name === name);
  assert(file, "Missing reference " + name);
  const el = p.locator('[data-file-id="' + file.id + '"]').first();
  await el.scrollIntoViewIfNeeded();
  await pointAt(p, el);
  await el.click();
  await pause(900);
}
(async () => {
  try {
    execFileSync(
      process.execPath,
      [
        "scripts/launch-qa.cjs",
        path.join(root, "src-tauri/target/release/shiqian.exe"),
      ],
      { cwd: root, env, windowsHide: true, stdio: "inherit" },
    );
    launched = true;
    browser = await chromium.connectOverCDP("http://127.0.0.1:" + port);
    const context = browser.contexts()[0];
    const p = context.pages().find((x) => !x.url().includes("floating"));
    p.on("pageerror", (e) => errors.push(String(e)));
    await ready(p);
    let boot = await api(p, "bootstrap");
    assert(path.resolve(boot.dataPath).startsWith(run + path.sep));
    await p.evaluate(
      (d) => localStorage.setItem("shiqian-onboarding:v1:" + d, "done"),
      boot.dataPath,
    );
    await api(p, "settings.save", { key: "language", value: "zh-CN" });
    await api(p, "settings.save", { key: "theme", value: "light" });
    await p
      .evaluate(() =>
        window.__TAURI_INTERNALS__.invoke("plugin:window|set_size", {
          label: "main",
          value: { Logical: { width: 1280, height: 820 } },
        }),
      )
      .catch(() => {});
    await p.reload();
    await ready(p);
    console.log("Fresh English library: " + boot.dataPath);
    console.log("Default tags: " + boot.tags.map((t) => t.name).join(", "));
    await api(p, "floating.presets");
    const seedNames = {
      已完成: "已完成",
      待处理: "待处理",
      参考素材: "参考素材",
      灵感: "灵感",
      参考: "参考",
    };
    for (const tag of (await api(p, "bootstrap")).tags) {
      if (seedNames[tag.name])
        await api(p, "tags.rename", {
          id: tag.id,
          name: seedNames[tag.name],
          version: tag.version,
        });
    }
    await api(p, "import", { paths: [material], recursive: true });
    let files = [];
    for (let i = 0; i < 60; i++) {
      files =
        (
          await api(p, "query", {
            scope: "all",
            text: "",
            offset: 0,
            limit: 100,
          })
        ).files || [];
      if (files.length >= 8) break;
      await pause(300);
    }
    assert.equal(files.length, 8);
    const made = {};
    for (const name of [
      "建筑外墙",
      "项目参考",
      "待讨论",
      "材料做法",
      "节点图纸",
      "网页收藏",
    ])
      made[name] = await api(p, "tag.create", { name });
    for (const [name, frags] of [
      ["建筑外墙", ["混凝土", "方案文本", "墙身"]],
      ["项目参考", ["江畔塔楼", "幕墙分缝", "方案汇报"]],
      ["待讨论", ["幕墙分缝", "方案文本"]],
      ["材料做法", ["外墙材料样板", "混凝土", "材料说明"]],
      ["节点图纸", ["墙身"]],
    ]) {
      await api(p, "annotation.apply", {
        tagId: made[name].id,
        ids: files
          .filter((f) => frags.some((s) => f.name.includes(s)))
          .map((f) => f.id),
      });
    }
    await p.reload();
    await ready(p);
    await pause(1600);
    await shot(p, "workspace-zh");
    await record(p, "workspace-zh", async () => {
      await openFile(p, "幕墙分缝讨论.md");
      await pause(700);
      await glide(p, 1120, 330, 600);
    });
    await p.reload();
    await ready(p);
    await record(p, "search-zh", async () => {
      const toggle = p
        .getByRole("button", { name: "展开搜索栏", exact: true })
        .first();
      await pointAt(p, toggle);
      await toggle.click();
      const box = p.getByRole("textbox", { name: "搜索文件", exact: true });
      await pointAt(p, box);
      await box.click();
      await box.pressSequentially("建筑外墙", { delay: 160 });
      await pause(900);
      await glide(p, 760, 470, 700);
    });
    await shot(p, "search-zh");
    await p.reload();
    await ready(p);
    await openFile(p, "方案文本-初稿.pdf");
    await pause(2200);
    await shot(p, "reader-pdf-zh");
    await record(p, "reader-pdf-zh", async () => {
      await glide(p, 1120, 520, 750);
      for (let i = 0; i < 48; i++) {
        await p.mouse.wheel(0, 10);
        await pause(33);
      }
      await pause(450);
      for (let i = 0; i < 24; i++) {
        await p.mouse.wheel(0, -8);
        await pause(33);
      }
    });
    await p.reload();
    await ready(p);
    await openFile(p, "幕墙分缝讨论.md");
    await pause(800);
    await shot(p, "annotation-zh");
    await api(p, "floating.open");
    let f;
    for (let i = 0; i < 60; i++) {
      f = context.pages().find((x) => x.url().includes("floating"));
      if (f) break;
      await pause(200);
    }
    assert(f, "Floating palette must open");
    await f.locator(".palette-chips").waitFor();
    await f
      .evaluate(() =>
        window.__TAURI_INTERNALS__.invoke("plugin:window|set_size", {
          label: "floating",
          value: { Logical: { width: 360, height: 520 } },
        }),
      )
      .catch(() => {});
    await pause(1200);
    await shot(f, "palette-zh");
    await record(f, "palette-zh", async () => {
      const search = f.locator("input").first();
      await pointAt(f, search);
      await search.click();
      await search.pressSequentially("建筑外墙", { delay: 170 });
      await pause(500);
      for (let i = 0; i < 4; i++) {
        await search.press("Backspace");
        await pause(100);
      }
      await glide(f, 86, 330, 700);
      await pause(300);
    });
    await api(p, "floating.close");
    await api(p, "settings.save", { key: "theme", value: "dark" });
    await p.reload();
    await ready(p);
    await pause(1200);
    await shot(p, "workspace-dark-zh");
    fs.writeFileSync(
      path.join(run, "capture-result.json"),
      JSON.stringify(
        { run, language: "zh-CN", files: files.map((f) => f.name), errors },
        null,
        2,
      ),
    );
    fs.writeFileSync(
      path.join(site, "scripts", "latest-capture-zh.txt"),
      run + "\n",
    );
    console.log("Chinese captures ready: " + run);
    if (errors.length) throw Error(errors.join("\n"));
  } finally {
    if (browser) await browser.close().catch(() => {});
    if (launched)
      execFileSync(process.execPath, ["scripts/close-qa.cjs"], {
        cwd: root,
        env,
        windowsHide: true,
        stdio: "inherit",
      });
  }
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
