// Capture the real English desktop UI in a fresh, isolated synthetic library.
// Run from site/: node scripts/capture-english.cjs
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
  "landing-en-" + new Date().toISOString().replace(/[:.]/g, "-"),
);
const material = path.join(run, "material", "Riverside tower");
const shots = path.join(run, "shots");
const frames = path.join(run, "frames");
const port = "9238";
for (const folder of [material, shots, frames])
  fs.mkdirSync(folder, { recursive: true });
for (const [source, name] of [
  ["tower-dusk.jpg", "Riverside tower.jpg"],
  ["facade-detail.jpg", "Concrete facade.jpg"],
  ["material-board.jpg", "Material palette.jpg"],
  ["node-drawing.jpg", "Wall detail.jpg"],
]) {
  fs.copyFileSync(path.join(site, "assets", source), path.join(material, name));
}
fs.copyFileSync(
  path.join(root, "qa/v0-landing/material/江畔塔楼/参考/方案文本-初稿.pdf"),
  path.join(material, "Concept draft.pdf"),
);
fs.writeFileSync(
  path.join(material, "Facade joint study.md"),
  "# Facade joint study\n\nCoordinate concrete joints and window reveals with the structural team. Use a 1200 mm module, 3 mm anodized aluminum lining and 12 mm sealant joints.\n\n## To discuss\n\n- Align facade joints with the interior ceiling.\n- Confirm parapet heights.\n- Reserve channels for night lighting.\n\nSynthetic material for the Tagrove demo.\n",
);
fs.writeFileSync(
  path.join(material, "Design review.md"),
  "# Riverside tower design review\n\n1. Urban context and tower massing\n2. Concrete facade and vertical shading\n3. Material palette and cost range\n4. Next steps: detail design and tender\n",
);
fs.writeFileSync(
  path.join(material, "Material notes.md"),
  "# Material notes\n\nConcrete panels and aluminum shading.\n\nReview the facade palette with the team before the next design meeting.\n",
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
  if (lines.length)
    throw Error(name + " contains Chinese text: " + lines.join(" | "));
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
    await api(p, "settings.save", { key: "language", value: "en" });
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
      已完成: "Completed",
      待处理: "To do",
      参考素材: "Reference material",
      灵感: "Inspiration",
      参考: "Reference",
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
      "Facade",
      "Project reference",
      "To discuss",
      "Materials",
      "Details",
      "Web clips",
    ])
      made[name] = await api(p, "tag.create", { name });
    for (const [name, frags] of [
      ["Facade", ["Concrete", "Concept", "Wall"]],
      ["Project reference", ["Riverside", "Facade joint", "Design review"]],
      ["To discuss", ["Facade joint", "Concept"]],
      ["Materials", ["Material palette", "Concrete", "Facade estimate"]],
      ["Details", ["Wall"]],
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
    await shot(p, "workspace-en");
    await record(p, "workspace-en", async () => {
      await openFile(p, "Facade joint study.md");
      await pause(700);
      await glide(p, 1120, 330, 600);
    });
    await p.reload();
    await ready(p);
    await record(p, "search-en", async () => {
      const toggle = p
        .getByRole("button", { name: "Expand search", exact: true })
        .first();
      await pointAt(p, toggle);
      await toggle.click();
      const box = p.getByRole("textbox", { name: "Search files", exact: true });
      await pointAt(p, box);
      await box.click();
      await box.pressSequentially("Facade", { delay: 160 });
      await pause(900);
      await glide(p, 760, 470, 700);
    });
    await shot(p, "search-en");
    await p.reload();
    await ready(p);
    await openFile(p, "Concept draft.pdf");
    await pause(2200);
    await shot(p, "reader-pdf-en");
    await record(p, "reader-pdf-en", async () => {
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
    await openFile(p, "Facade joint study.md");
    await pause(800);
    await shot(p, "annotation-en");
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
    await shot(f, "palette-en");
    await record(f, "palette-en", async () => {
      const search = f.locator("input").first();
      await pointAt(f, search);
      await search.click();
      await search.pressSequentially("Facade", { delay: 170 });
      await pause(500);
      for (let i = 0; i < 6; i++) {
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
    await shot(p, "workspace-dark-en");
    fs.writeFileSync(
      path.join(run, "capture-result.json"),
      JSON.stringify(
        { run, language: "en", files: files.map((f) => f.name), errors },
        null,
        2,
      ),
    );
    fs.writeFileSync(
      path.join(site, "scripts", "latest-capture.txt"),
      run + "\n",
    );
    console.log("English captures ready: " + run);
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
