#!/usr/bin/env node
// i18n-audit — CLI entry point.
//
// Walks web/src, extracts every translation call, compares against
// messages/{en,ko}.json, and exits non-zero if any key is missing
// in either bundle or any dynamic prefix resolves to a leaf.
//
// Usage:
//   node web/scripts/i18n-audit.mjs           # default: audit web/src
//   node web/scripts/i18n-audit.mjs --json    # emit machine-readable output
//   node web/scripts/i18n-audit.mjs --src=X   # override the scan root
//
// Exit codes:
//   0 — clean
//   1 — at least one finding
//   2 — usage error or bundle load failure

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  auditCalls,
  findTranslationCalls,
  loadBundle,
  reasonLabel,
  walkSource,
} from "./lib/i18n-auditor.mjs";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const WEB_ROOT = path.resolve(__dirname, "..");

function parseArgs(argv) {
  const args = { src: path.join(WEB_ROOT, "src"), json: false };
  for (const a of argv) {
    if (a === "--json") args.json = true;
    else if (a.startsWith("--src=")) args.src = path.resolve(a.slice(6));
    else if (a === "-h" || a === "--help") {
      args.help = true;
    } else {
      process.stderr.write(`unknown argument: ${a}\n`);
      process.exit(2);
    }
  }
  return args;
}

function printHelp() {
  process.stdout.write(
    `i18n-audit — scan the frontend tree for untranslated keys.\n\n` +
      `Usage: node web/scripts/i18n-audit.mjs [options]\n\n` +
      `Options:\n` +
      `  --src=<dir>   Scan root (default: web/src)\n` +
      `  --json        Emit findings as JSON instead of a pretty table\n` +
      `  --help        Print this message\n`,
  );
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
    return 0;
  }

  if (!fs.existsSync(args.src)) {
    process.stderr.write(`i18n-audit: src directory not found: ${args.src}\n`);
    return 2;
  }

  let bundles;
  try {
    bundles = {
      en: loadBundle(path.join(WEB_ROOT, "messages/en.json")),
      ko: loadBundle(path.join(WEB_ROOT, "messages/ko.json")),
    };
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    process.stderr.write(`i18n-audit: ${msg}\n`);
    return 2;
  }

  const files = walkSource(args.src);
  const allCalls = [];
  for (const f of files) {
    const src = fs.readFileSync(f, "utf8");
    // Cheap fast-path: skip files that never reference `useTranslations`.
    // A full AST parse per file isn't free; the vast majority of files
    // have no i18n calls at all.
    if (!src.includes("useTranslations")) continue;
    allCalls.push(...findTranslationCalls(f, src));
  }

  const findings = auditCalls(allCalls, bundles);

  if (args.json) {
    process.stdout.write(JSON.stringify({ findings }, null, 2) + "\n");
  } else if (findings.length === 0) {
    process.stdout.write(
      `i18n-audit: scanned ${allCalls.length} call(s) across ${files.length} file(s) — no missing keys.\n`,
    );
  } else {
    process.stdout.write(
      `i18n-audit: ${findings.length} finding(s) across ${
        new Set(findings.map((f) => f.file)).size
      } file(s):\n\n`,
    );
    const byFile = new Map();
    for (const f of findings) {
      const rel = path.relative(WEB_ROOT, f.file);
      if (!byFile.has(rel)) byFile.set(rel, []);
      byFile.get(rel).push(f);
    }
    for (const [file, rows] of byFile) {
      process.stdout.write(`  ${file}\n`);
      for (const r of rows) {
        process.stdout.write(
          `    ${r.path}  (${reasonLabel(r.reason)}, line ${r.line})\n`,
        );
      }
    }
    process.stdout.write("\n");
  }

  return findings.length === 0 ? 0 : 1;
}

main().then(
  (code) => process.exit(code),
  (err) => {
    process.stderr.write(`i18n-audit: unexpected error: ${err?.stack ?? err}\n`);
    process.exit(2);
  },
);
