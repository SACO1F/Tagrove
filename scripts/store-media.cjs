// Deterministic store artwork from the existing logo; no application UI is mocked.
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'C:/Users/10052/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const root = path.resolve(__dirname, '..');
const version = JSON.parse(fs.readFileSync(path.join(root, 'package.json'))).version;
const out = path.join(root, 'releases', `v${version}`, 'store-submission', 'assets');
const logo = 'data:image/png;base64,' + fs.readFileSync(path.join(root, 'public/icon.png')).toString('base64');
(async () => {
  for (const folder of ['promotional', 'screenshots']) fs.mkdirSync(path.join(out, folder), { recursive: true });
  const browser = await chromium.launch({ executablePath: 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', headless: true });
  try {
    for (const [width, height, name] of [[440, 280, 'small-440x280.png'], [1400, 560, 'large-1400x560.png']]) {
      const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 1 });
      const large = width > 500;
      await page.setContent(`<!doctype html><meta charset="utf-8"><style>*{box-sizing:border-box}body{margin:0;background:#234575;color:white;font-family:'Segoe UI',sans-serif}.tile{width:100vw;height:100vh;display:flex;align-items:center;justify-content:center;gap:${large ? 72 : 24}px;background:radial-gradient(ellipse at 20% 8%,#38709a,transparent 70%)}img{width:${large ? 310 : 145}px;height:auto}.brand{font-size:${large ? 78 : 32}px;letter-spacing:-.04em;font-weight:600}.name{font:400 ${large ? 28 : 17}px/1.8 'Microsoft YaHei',sans-serif;letter-spacing:.22em;color:#dce7fa}</style><div class="tile"><img src="${logo}" alt=""><div><div class="brand">Tagrove</div><div class="name">拾签</div></div></div>`);
      await page.locator('img').evaluate(el => el.decode());
      await page.screenshot({ path: path.join(out, 'promotional', name), animations: 'disabled' });
      await page.close();
    }
    for (const [source, name] of [['edge-capture-tags.png', '01-capture-and-tags-1280x800.png'], ['edge-queue.png', '02-capture-queue-1280x800.png']]) {
      const bytes = fs.readFileSync(path.join(root, 'qa/v0613-edge', source));
      if (bytes.readUInt32BE(16) !== 1280 || bytes.readUInt32BE(20) !== 800) throw Error('Recapture real UI at 1280x800 first: ' + source);
      fs.writeFileSync(path.join(out, 'screenshots', name), bytes);
    }
    console.log('Created promotional artwork and copied two unmodified real extension screenshots.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
