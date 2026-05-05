#!/usr/bin/env node
// error-code-parity-audit — every backend `ApiErrorCode` variant has
// a matching `errors.<code>` template in BOTH locale bundles.
//
// The FE renders error prose by looking up `errors.<code>` with the
// `params` map interpolated; a missing template falls back to
// `errors.unknown` and the user sees a generic "unknown error
// occurred". This audit catches that drift at PR time instead of
// surfacing as a localisation hole in production.
//
// Source of truth: the `as_str()` match in
// `crates/ox-api/src/error.rs::ApiErrorCode::as_str`. We parse the
// match arms (no Cargo / Rust toolchain dependency) and check every
// extracted snake_case literal against `messages/{ko,en}.json`.

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..");
const REPO_ROOT = path.resolve(ROOT, "..");
const MESSAGES = path.join(ROOT, "messages");
const ERROR_RS = path.join(REPO_ROOT, "crates", "ox-api", "src", "error.rs");

async function extractCodes() {
  const source = await fs.readFile(ERROR_RS, "utf8");
  // Locate the `as_str` body and pull every `Variant => "literal"` arm.
  // The match is the single source of truth for the wire string —
  // every variant lands in exactly one arm.
  const asStrMatch = source.match(
    /pub fn as_str\(self\) -> &'static str \{[\s\S]*?\}\s*\}/,
  );
  if (!asStrMatch) {
    throw new Error(
      `Could not locate ApiErrorCode::as_str body in ${ERROR_RS}.`,
    );
  }
  const armRe = /=>\s*"([a-z_][a-z0-9_]*)"\s*,/g;
  const codes = new Set();
  let m;
  while ((m = armRe.exec(asStrMatch[0])) !== null) {
    codes.add(m[1]);
  }
  if (codes.size === 0) {
    throw new Error(
      `Found ApiErrorCode::as_str but extracted zero codes — parser drift?`,
    );
  }
  return [...codes].sort();
}

async function loadBundle(name) {
  const file = path.join(MESSAGES, name);
  const raw = await fs.readFile(file, "utf8");
  const bundle = JSON.parse(raw);
  return bundle.errors ?? {};
}

async function main() {
  const codes = await extractCodes();
  const ko = await loadBundle("ko.json");
  const en = await loadBundle("en.json");

  const missingKo = codes.filter((c) => !(c in ko));
  const missingEn = codes.filter((c) => !(c in en));
  // The reverse direction: any `errors.<code>` in the bundle that
  // doesn't exist on the BE? Catches stale templates left behind
  // when a code is renamed / removed.
  const extraKo = Object.keys(ko).filter(
    (k) => !codes.includes(k) && k !== "unknown",
  );
  const extraEn = Object.keys(en).filter(
    (k) => !codes.includes(k) && k !== "unknown",
  );

  if (
    missingKo.length === 0 &&
    missingEn.length === 0 &&
    extraKo.length === 0 &&
    extraEn.length === 0
  ) {
    console.log(
      `error-code-parity-audit: ${codes.length} code(s) covered by both bundles.`,
    );
    return;
  }

  if (missingKo.length > 0) {
    console.error(
      `\nmessages/ko.json is missing errors.<code> templates for:`,
    );
    for (const c of missingKo) console.error(`  ${c}`);
  }
  if (missingEn.length > 0) {
    console.error(
      `\nmessages/en.json is missing errors.<code> templates for:`,
    );
    for (const c of missingEn) console.error(`  ${c}`);
  }
  if (extraKo.length > 0) {
    console.error(
      `\nmessages/ko.json has stale errors.<code> templates not on the BE:`,
    );
    for (const c of extraKo) console.error(`  ${c}`);
  }
  if (extraEn.length > 0) {
    console.error(
      `\nmessages/en.json has stale errors.<code> templates not on the BE:`,
    );
    for (const c of extraEn) console.error(`  ${c}`);
  }
  console.error(
    `\nFix: add or remove keys to keep the bundles in lockstep with ` +
      `crates/ox-api/src/error.rs::ApiErrorCode::as_str.`,
  );
  process.exit(1);
}

main().catch((err) => {
  console.error(err);
  process.exit(2);
});
