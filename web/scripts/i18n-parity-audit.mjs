#!/usr/bin/env node
// i18n-parity-audit — locale-bundle key-set divergence gate.
//
// `i18n-audit` already catches "the code calls `t("foo")` but no
// bundle has the key". This audit catches the inverse: one locale
// has a key the other doesn't. Without this gate, an admin can
// silently land an English-only key — `next-intl` falls back to the
// raw key string at runtime and the Korean user sees `settings.foo`
// in their UI.
//
// What we check:
//   * Every leaf path in `messages/en.json` exists in `messages/ko.json`.
//   * Every leaf path in `messages/ko.json` exists in `messages/en.json`.
//   * Both bundles are valid JSON.
//
// What we DO NOT check:
//   * Whether the translation is correct or human-vs-machine —
//     that's a translation-management concern, not a static gate.
//   * Whether ICU plural / select forms are equivalent — a future
//     pass could parse the ICU AST and assert plural categories
//     match per locale, but that's a larger ADR.

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..");
const MESSAGES = path.join(ROOT, "messages");

function flatten(obj, prefix = []) {
  /** @type {string[]} */
  const out = [];
  for (const [key, value] of Object.entries(obj)) {
    const next = [...prefix, key];
    if (value !== null && typeof value === "object" && !Array.isArray(value)) {
      out.push(...flatten(value, next));
    } else {
      out.push(next.join("."));
    }
  }
  return out;
}

async function loadBundle(file) {
  const full = path.join(MESSAGES, file);
  const text = await fs.readFile(full, "utf8").catch((err) => {
    if (err.code === "ENOENT") {
      console.error(`i18n-parity-audit: missing bundle ${path.relative(ROOT, full)}`);
      process.exit(2);
    }
    throw err;
  });
  try {
    return JSON.parse(text);
  } catch (err) {
    console.error(
      `i18n-parity-audit: ${file} is not valid JSON — ${err.message}`,
    );
    process.exit(2);
  }
}

async function main() {
  const en = await loadBundle("en.json");
  const ko = await loadBundle("ko.json");

  const enKeys = new Set(flatten(en));
  const koKeys = new Set(flatten(ko));

  const missingInKo = [...enKeys].filter((k) => !koKeys.has(k)).sort();
  const missingInEn = [...koKeys].filter((k) => !enKeys.has(k)).sort();

  if (missingInKo.length === 0 && missingInEn.length === 0) {
    console.log(
      `i18n-parity-audit: ${enKeys.size} keys in lockstep across en + ko.`,
    );
    process.exit(0);
  }

  console.error(`i18n-parity-audit: ${missingInKo.length + missingInEn.length} divergence(s):\n`);
  if (missingInKo.length > 0) {
    console.error(`  Missing in ko.json (${missingInKo.length}):`);
    for (const k of missingInKo) console.error(`    ${k}`);
  }
  if (missingInEn.length > 0) {
    console.error(`  Missing in en.json (${missingInEn.length}):`);
    for (const k of missingInEn) console.error(`    ${k}`);
  }
  console.error(
    `\nAdd the missing key(s) to bring both bundles into lockstep, or remove from the source bundle if the key is no longer used.`,
  );
  process.exit(1);
}

main().catch((err) => {
  console.error(`i18n-parity-audit: ${err.message}`);
  process.exit(2);
});
