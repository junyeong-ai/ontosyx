// api-error-envelope-audit — reject legacy local HTTP error envelopes.
//
// Backend HTTP errors and Next local route errors share one wire shape:
//
//   { "error": { "code": "...", "class": "client_error|server_error", "params": {} } }
//
// This audit guards Next route handlers / server helpers from reintroducing
// the older `{ error: { type, message } }` shape. SSE event payloads and tests
// are intentionally out of scope; those protocols are validated separately.

import fs from "node:fs";
import path from "node:path";
import ts from "typescript";

const ROOT = path.resolve(new URL("..", import.meta.url).pathname);
const SCAN_DIRS = ["src/app", "src/lib/server"];

function walk(dir) {
  if (!fs.existsSync(dir)) return [];
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    if (entry.name.startsWith(".")) continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...walk(full));
      continue;
    }
    if (/\.(ts|tsx)$/.test(entry.name)) files.push(full);
  }
  return files;
}

function propName(node) {
  if (ts.isIdentifier(node.name) || ts.isStringLiteral(node.name)) {
    return node.name.text;
  }
  return null;
}

function objectHasProperty(node, name) {
  return node.properties.some(
    (prop) => ts.isPropertyAssignment(prop) && propName(prop) === name,
  );
}

function findLegacyErrorObjects(sourceFile) {
  const findings = [];

  function visit(node) {
    if (ts.isObjectLiteralExpression(node) && objectHasProperty(node, "error")) {
      const errorProp = node.properties.find(
        (prop) => ts.isPropertyAssignment(prop) && propName(prop) === "error",
      );
      if (
        errorProp &&
        ts.isPropertyAssignment(errorProp) &&
        ts.isObjectLiteralExpression(errorProp.initializer)
      ) {
        const errorObject = errorProp.initializer;
        const hasLegacyType = objectHasProperty(errorObject, "type");
        const hasLegacyMessage = objectHasProperty(errorObject, "message");
        const hasCanonicalCode = objectHasProperty(errorObject, "code");
        const hasCanonicalClass = objectHasProperty(errorObject, "class");
        const hasCanonicalParams = objectHasProperty(errorObject, "params");

        if (
          (hasLegacyType || hasLegacyMessage) &&
          !(hasCanonicalCode && hasCanonicalClass && hasCanonicalParams)
        ) {
          const pos = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile));
          findings.push({
            line: pos.line + 1,
            column: pos.character + 1,
            keys: errorObject.properties
              .map((prop) => propName(prop))
              .filter(Boolean)
              .join(", "),
          });
        }
      }
    }
    ts.forEachChild(node, visit);
  }

  visit(sourceFile);
  return findings;
}

const findings = [];
for (const relDir of SCAN_DIRS) {
  for (const file of walk(path.join(ROOT, relDir))) {
    if (file.includes(`${path.sep}__tests__${path.sep}`)) continue;
    if (/\.test\.(ts|tsx)$/.test(file)) continue;
    const text = fs.readFileSync(file, "utf8");
    if (!text.includes("error")) continue;
    const sourceFile = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
    for (const finding of findLegacyErrorObjects(sourceFile)) {
      findings.push({ file: path.relative(ROOT, file), ...finding });
    }
  }
}

if (findings.length > 0) {
  console.error(
    `api-error-envelope-audit: ${findings.length} legacy local HTTP error envelope(s):\n`,
  );
  for (const finding of findings) {
    console.error(
      `  ${finding.file}:${finding.line}:${finding.column} — error keys: ${finding.keys}`,
    );
  }
  console.error(
    "\nUse { error: { code, class, params } } or the shared server apiErrorResponse() helper.",
  );
  process.exit(1);
}

console.log("api-error-envelope-audit: local HTTP error envelopes are canonical.");
