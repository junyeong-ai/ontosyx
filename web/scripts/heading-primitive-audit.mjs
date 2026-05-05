#!/usr/bin/env node
// heading-primitive-audit — reject new raw `<h1>`–`<h6 className=…>`
// JSX elements; require `<Heading level={N} size={M}>` so the
// document outline (level) and visual tier (size) stay decoupled.
//
// Rationale: the `<Heading>` primitive lives in
// `components/ui/heading.tsx` and pairs an explicit `level` (a11y
// outline) with an explicit `size` (visual tier from
// `--heading-{1..6}-size` tokens). A raw `<h2 className="text-sm
// font-semibold text-foreground-strong">` couples the two, makes
// re-tiering a multi-file find-and-replace, and silently drifts
// from the design system whenever the inline class changes.
//
// One-way ratchet: the existing baseline of raw headings is grand-
// fathered (87 sites at the time of writing) but new violations
// fail CI. The baseline file is git-tracked; running with
// `--update` regenerates it after a sanctioned cleanup pass.

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..");
const SRC = path.join(ROOT, "src");
const BASELINE = path.join(ROOT, "scripts", "heading-primitive.baseline.json");

const HEADING_RE = /<h([1-6])\s+className=/g;

// Skip generated / test files. Test files exercise primitives
// directly and would be over-restricted by this rule; the
// `<Heading>` primitive itself can render any tag, so its own
// implementation is permitted to use the raw element via the
// switch below.
const SKIP_DIR_RE = /(?:^|\/)(node_modules|\.next|__tests__)\//;
const SKIP_FILE_RE = /\.(?:test|spec|generated)\.(?:tsx?|js)$|heading\.tsx$/;

const ALLOWED_CLASSES = new Set([
  // sr-only headings serve a11y-only and don't render visual chrome
  "sr-only",
]);

async function* walk(dir) {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    const rel = path.relative(ROOT, full);
    if (SKIP_DIR_RE.test(`/${rel}/`)) continue;
    if (entry.isDirectory()) {
      yield* walk(full);
    } else if (/\.tsx?$/.test(entry.name) && !SKIP_FILE_RE.test(entry.name)) {
      yield full;
    }
  }
}

async function findViolations() {
  const violations = [];
  for await (const file of walk(SRC)) {
    const source = await fs.readFile(file, "utf8");
    HEADING_RE.lastIndex = 0;
    let match;
    while ((match = HEADING_RE.exec(source)) !== null) {
      // Pull the className value via a narrow follow-up scan so we
      // can whitelist `sr-only` headings without false-positives.
      const tail = source.slice(match.index, match.index + 200);
      const classMatch = /className="([^"]*)"/.exec(tail);
      const className = classMatch?.[1] ?? "";
      if (ALLOWED_CLASSES.has(className.trim())) continue;
      const lineNumber =
        source.slice(0, match.index).split("\n").length;
      violations.push({
        file: path.relative(ROOT, file),
        line: lineNumber,
        level: match[1],
        className,
      });
    }
  }
  return violations;
}

function violationKey(v) {
  return `${v.file}:${v.line}`;
}

async function loadBaseline() {
  try {
    const raw = await fs.readFile(BASELINE, "utf8");
    return JSON.parse(raw);
  } catch (err) {
    if (err.code === "ENOENT") return [];
    throw err;
  }
}

async function main() {
  const updateMode = process.argv.includes("--update");
  const violations = await findViolations();
  const baseline = await loadBaseline();
  const baselineKeys = new Set(baseline.map(violationKey));

  if (updateMode) {
    const ordered = violations.sort((a, b) => {
      if (a.file !== b.file) return a.file.localeCompare(b.file);
      return a.line - b.line;
    });
    await fs.writeFile(BASELINE, `${JSON.stringify(ordered, null, 2)}\n`);
    console.log(
      `heading-primitive-audit: baseline rewritten — ${ordered.length} site(s).`,
    );
    return;
  }

  const newViolations = violations.filter(
    (v) => !baselineKeys.has(violationKey(v)),
  );

  if (newViolations.length === 0) {
    const stale = baseline.filter(
      (b) => !violations.some((v) => violationKey(v) === violationKey(b)),
    );
    if (stale.length > 0) {
      console.log(
        `heading-primitive-audit: ${violations.length} site(s) tracked, ${stale.length} baseline entry(ies) cleaned up since last update.`,
      );
      console.log(
        "Run `pnpm heading-primitive-audit -- --update` to refresh the baseline ratchet.",
      );
    } else {
      console.log(
        `heading-primitive-audit: ${violations.length} site(s) tracked, no new violations.`,
      );
    }
    return;
  }

  console.error(
    `\n${newViolations.length} new raw <hN className=…> violation(s) — replace with <Heading level={N} size={M}>:`,
  );
  for (const v of newViolations) {
    console.error(`  ${v.file}:${v.line}  <h${v.level} className="${v.className}">`);
  }
  console.error(
    `\nThe primitive lives in src/components/ui/heading.tsx. Pass an explicit
\`level\` (document outline / a11y) and \`size\` (visual tier) — they
are intentionally decoupled. After the migration, re-run with
\`--update\` to ratchet the baseline.`,
  );
  process.exit(1);
}

main().catch((err) => {
  console.error(err);
  process.exit(2);
});
