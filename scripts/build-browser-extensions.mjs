import { readFileSync, writeFileSync, mkdirSync, readdirSync, copyFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = path.join(root, "browser-extension");
const version = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8")).version;
const manifest = JSON.parse(readFileSync(path.join(source, "manifest.json"), "utf8"));
if (manifest.version !== version) throw Error("Desktop and extension versions differ");
const digest = createHash("sha256").update(Buffer.from(manifest.key, "base64")).digest().subarray(0, 16);
const extensionId = [...digest].map((n) => String.fromCharCode(97 + (n >> 4), 97 + (n & 15))).join("");
if (extensionId !== readFileSync(path.join(source, "extension-id.txt"), "utf8").trim()) throw Error("Extension identity changed");

function files(directory, prefix = "") {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    if (entry.isSymbolicLink()) throw Error("Unexpected extension symlink");
    const relative = path.join(prefix, entry.name);
    if (entry.isDirectory()) return files(path.join(directory, entry.name), relative);
    if (!/\.(js|json|html|css|md|txt|png)$/.test(entry.name)) throw Error(`Unexpected extension asset: ${relative}`);
    return [relative];
  });
}
const output = path.join(root, "releases", `v${version}`, "extensions");
for (const browser of ["chrome", "edge"]) {
  const destination = path.join(output, browser);
  mkdirSync(destination, { recursive: true });
  for (const relative of files(source)) {
    const target = path.join(destination, relative);
    mkdirSync(path.dirname(target), { recursive: true });
    copyFileSync(path.join(source, relative), target);
  }
  writeFileSync(path.join(destination, "manifest.json"), JSON.stringify({
    ...manifest,
    name: `拾签 Tagrove · 网页采集 (${browser === "edge" ? "Edge" : "Chrome"})`,
    version_name: `${version} · ${browser === "edge" ? "Microsoft Edge" : "Google Chrome"}`,
  }, null, 2) + "\n");
  console.log(`${browser}: ${destination}; version ${version}; extension ID ${extensionId}`);
}
