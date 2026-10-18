const fs = require("fs"),
  ts = require("typescript"),
  vm = require("vm"),
  assert = require("node:assert/strict");
const e = {};
vm.runInNewContext(
  ts.transpileModule(fs.readFileSync("src/locale-en.ts", "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS },
  }).outputText,
  { exports: e },
);
const used = new Set();
let calls = 0,
  structured = 0;
for (const [key, value] of Object.entries(e.english)) {
  assert.equal(typeof value, "string", key);
  assert.deepEqual(
    (key.match(/\{\d+\}/g) || []).sort(),
    (value.match(/\{\d+\}/g) || []).sort(),
    key + ": placeholder mismatch",
  );
}
for (const name of fs
  .readdirSync("src")
  .filter((x) => /\.tsx?$/.test(x) && !x.startsWith("locale-"))) {
  const f = ts.createSourceFile(
    name,
    fs.readFileSync("src/" + name, "utf8"),
    ts.ScriptTarget.Latest,
    true,
    name.endsWith("tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );
  function visit(n) {
    if (
      ts.isCallExpression(n) &&
      ["ui", "tf", "msg"].includes(n.expression.getText(f)) &&
      ts.isStringLiteral(n.arguments[0])
    ) {
      const key = n.arguments[0].text;
      assert.ok(e.english[key], name + ": Missing translation: " + key);
      used.add(key);
      calls++;
      if (n.expression.getText(f) === "msg") structured++;
    }
    if (
      ts.isCallExpression(n) &&
      ["tell", "notify", "ask", "setNotice"].includes(n.expression.getText(f))
    ) {
      for (const a of [...n.arguments].slice(
        0,
        n.expression.getText(f) === "ask" ? 3 : 1,
      ))
        assert.ok(
          !(
            (ts.isStringLiteral(a) ||
              ts.isTemplateExpression(a) ||
              ts.isNoSubstitutionTemplateLiteral(a)) &&
            /[\u3400-\u9fff]/.test(a.getText(f))
          ),
          name + ": Unlocalized dynamic message: " + a.getText(f),
        );
    }
    ts.forEachChild(n, visit);
  }
  visit(f);
}
console.log(
  `PASS ${calls} UI calls (${structured} structured messages), ${used.size} distinct keys; all ${Object.keys(e.english).length} catalog entries and placeholders checked; dynamic message sinks checked. User data and raw diagnostics are excluded.`,
);
