#!/usr/bin/env node
// i18n-dotted-key-audit — reject keys containing `.`.
//
// next-intl uses `.` as the path separator for nested namespaces.
// A key whose own name contains `.` (e.g. `codes.add` rather than
// nested `codes: { add }`) trips a runtime `INVALID_KEY` error AND
// breaks lookups under the parent path. This static gate fails CI
// the moment such a key lands in either bundle, instead of
// surfacing as a hydration-time crash in the user's browser.
//
// What we check:
//   * For every leaf path in `messages/{ko,en}.json`, the *final*
//     segment of the key (the actual property name in the parent
//     object) must NOT contain `.`.
//
// What we DO NOT check:
//   * Whether the locale bundles are in parity — the existing
//     `i18n-parity-audit` covers that.
//   * Whether the keys are referenced by any code — `i18n-audit`
//     covers reachability from `useTranslations` call sites.

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..");
const MESSAGES = path.join(ROOT, "messages");

/**
 * Recursive walk that yields every (path, owningPropertyName) pair.
 * The path is purely diagnostic — the property name is the bit we
 * actually validate against the dotted-key rule.
 */
function* walk(obj, prefix = []) {
  for (const [key, value] of Object.entries(obj)) {
    const next = [...prefix, key];
    yield { path: next.join("."), key };
    if (value !== null && typeof value === "object" && !Array.isArray(value)) {
      yield* walk(value, next);
    }
  }
}

async function audit(file) {
  const raw = await fs.readFile(file, "utf8");
  const bundle = JSON.parse(raw);
  const offenders = [];
  for (const { path: keyPath, key } of walk(bundle)) {
    if (key.includes(".")) {
      offenders.push({ path: keyPath, key });
    }
  }
  return offenders;
}

async function main() {
  const files = ["ko.json", "en.json"].map((n) => path.join(MESSAGES, n));
  let bad = 0;
  for (const file of files) {
    const offenders = await audit(file);
    if (offenders.length > 0) {
      bad += offenders.length;
      console.error(
        `\n${path.relative(ROOT, file)}: ${offenders.length} dotted key(s)`,
      );
      for (const o of offenders) {
        console.error(`  ${o.path}  (offending segment: "${o.key}")`);
      }
    }
  }
  if (bad > 0) {
    console.error(
      `\n${bad} dotted key(s) detected. next-intl interprets "." as the namespace ` +
        `separator — flatten or nest the offending entries (e.g. ` +
        `"codes.add" → "codesAdd" or { codes: { add } }).`,
    );
    process.exit(1);
  }
  console.log("✓ no dotted keys detected in i18n bundles");
}

main().catch((err) => {
  console.error(err);
  process.exit(2);
});
