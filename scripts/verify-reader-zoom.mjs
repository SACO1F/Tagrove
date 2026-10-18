import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
const bridge = readFileSync(
  new URL("../src/reader-zoom-bridge.js", import.meta.url),
  "utf8",
);
const config = JSON.parse(
  readFileSync(
    new URL("../src-tauri/tauri.conf.json", import.meta.url),
    "utf8",
  ),
);
if (bridge.includes("\r"))
  throw new Error(
    "Reader bridge must keep LF line endings for browser CSP hash normalization.",
  );
const hash = createHash("sha256").update(bridge).digest("base64");
if (!config.app.security.csp.includes(`'sha256-${hash}'`)) {
  throw new Error(
    `Reader zoom bridge changed. Review its source and update the exact CSP hash to sha256-${hash} before building.`,
  );
}
console.log("Reader zoom bridge CSP hash verified.");
