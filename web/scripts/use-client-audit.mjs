#!/usr/bin/env node
// `"use client"` placement gate.
//
// Turbopack only recognises the directive when it sits on line 1 of
// the module. A leading block comment — even a documentation header —
// silently demotes the file to a server component, which then crashes
// at hydration the moment a hook runs. The bug is invisible at
// build time and at typecheck time; only a runtime smoke test catches
// it. We catch it statically here instead.
//
// Walks every `.ts` / `.tsx` under `src/` that contains a `"use client"`
// directive and asserts the directive lives on line 1, before any
// comment, blank line, or other token.

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..");
const SRC = path.join(ROOT, "src");

const DIRECTIVE = /^["']use client["'];?\s*$/;

async function* walk(dir) {
  for (const entry of await fs.readdir(dir, { withFileTypes: true })) {
    if (entry.name.startsWith(".") || entry.name === "node_modules") continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      yield* walk(full);
    } else if (/\.tsx?$/.test(entry.name)) {
      yield full;
    }
  }
}

const violations = [];

for await (const file of walk(SRC)) {
  const text = await fs.readFile(file, "utf8");
  // Cheap guard: skip files that don't even mention the directive.
  if (!text.includes("use client")) continue;
  const lines = text.split("\n");
  // Find the directive's actual line number.
  const directiveLine = lines.findIndex((line) => DIRECTIVE.test(line.trim()));
  if (directiveLine < 0) continue; // string literal mention, not a directive
  if (directiveLine !== 0) {
    violations.push({
      file: path.relative(ROOT, file),
      line: directiveLine + 1,
      precedingFirstLine: lines[0],
    });
  }
}

if (violations.length === 0) {
  console.log(`use-client-audit: ✓ all directives on line 1`);
  process.exit(0);
}

console.error(`use-client-audit: ✗ ${violations.length} violation(s)\n`);
console.error(
  `Turbopack treats \`"use client"\` as effective only when it is the very first`,
);
console.error(
  `line of the module — any preceding comment or blank line silently demotes`,
);
console.error(
  `the file to a server component. Move the directive to line 1; the doc`,
);
console.error(`comment goes after.\n`);
for (const v of violations) {
  console.error(`  ${v.file}:${v.line}`);
  console.error(`    line 1 was: ${v.precedingFirstLine}`);
}
process.exit(1);
