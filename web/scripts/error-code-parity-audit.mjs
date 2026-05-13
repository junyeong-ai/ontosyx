#!/usr/bin/env node
// error-code-parity-audit — locks the FE i18n catalogue against
// every backend wire-code source. Two parity contracts ride in one
// gate:
//
// 1. `errors.<llm code>` ↔ `ApiErrorCode::as_str` arms
//    (`crates/ox-api/src/error.rs`). HTTP error envelope + agent
//    `Failed` SSE event share the namespace.
//
// 2. `errors.tool_<wire>` ↔ entelix `Error::wire_code()` arms
//    (`../entelix/crates/entelix-core/src/error.rs`). Agent
//    `ToolError` SSE event keys i18n off the entelix bucket
//    directly — the FE catalogue must stay in lockstep so a new
//    entelix bucket doesn't silently fall back to `tool_unknown`.
//
// The audit parses each match body (no Cargo / Rust toolchain
// dependency) and asserts every extracted literal has a matching
// template in both `messages/{ko,en}.json`.

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..");
const REPO_ROOT = path.resolve(ROOT, "..");
const MESSAGES = path.join(ROOT, "messages");
const ERROR_RS = path.join(REPO_ROOT, "crates", "ox-api", "src", "error.rs");
const ENTELIX_ERROR_RS = path.resolve(
  REPO_ROOT,
  "..",
  "entelix",
  "crates",
  "entelix-core",
  "src",
  "error.rs",
);

async function extractCodes() {
  const source = await fs.readFile(ERROR_RS, "utf8");
  // Anchor on `impl ApiErrorCode {` so a future sibling enum's
  // `as_str` (e.g. `ApiErrorClass::as_str -> "client_error"`) does
  // not accidentally feed its literals into the wire-code set. The
  // body is the first `pub fn as_str` inside that impl block.
  const implMatch = source.match(/impl ApiErrorCode \{[\s\S]*?\n\}/);
  if (!implMatch) {
    throw new Error(
      `Could not locate \`impl ApiErrorCode\` block in ${ERROR_RS}.`,
    );
  }
  const asStrMatch = implMatch[0].match(
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

/// Parse `Error::wire_signal` in `entelix-core/src/error.rs` and
/// return the sorted set of buckets it can emit. `wire_signal` is
/// the private `(wire_code, wire_class)` matcher behind the public
/// `Error::envelope`; its arms are the single source of truth for
/// the entelix wire taxonomy. The body mixes top-level arms with a
/// nested `match kind { ... }` for `Provider` errors
/// (network / tls / dns / HTTP); both layers feed the same set of
/// `&'static str` literals through one regex over the function.
async function extractEntelixWireBuckets() {
  const source = await fs.readFile(ENTELIX_ERROR_RS, "utf8");
  const implMatch = source.match(/impl Error \{[\s\S]*?\n\}\n/);
  if (!implMatch) {
    throw new Error(
      `Could not locate \`impl Error\` block in ${ENTELIX_ERROR_RS}.`,
    );
  }
  const wireMatch = implMatch[0].match(
    /fn wire_signal\(&self\) -> \(&'static str, ErrorClass\) \{[\s\S]*?\n    \}/,
  );
  if (!wireMatch) {
    throw new Error(
      `Could not locate Error::wire_signal body in ${ENTELIX_ERROR_RS}.`,
    );
  }
  const literalRe = /\(\s*"([a-z_][a-z0-9_]*)"\s*,/g;
  const buckets = new Set();
  let m;
  while ((m = literalRe.exec(wireMatch[0])) !== null) {
    buckets.add(m[1]);
  }
  if (buckets.size === 0) {
    throw new Error(
      `Found Error::wire_signal but extracted zero buckets — parser drift?`,
    );
  }
  return [...buckets].sort();
}

async function loadBundle(name) {
  const file = path.join(MESSAGES, name);
  const raw = await fs.readFile(file, "utf8");
  const bundle = JSON.parse(raw);
  return bundle.errors ?? {};
}

/// Report a parity break against a single namespace (`llm` or
/// `tool`) and accumulate the failure flag for the final exit
/// code. `isOwnKey` decides which catalogue entries belong to this
/// namespace — explicit ownership keeps stale-key detection from
/// being confused by underscores inside ApiErrorCode literals
/// (`llm_rate_limited`, `validation_error`, …).
/// Returns `true` when the namespace is in lockstep.
function reportParity({ namespace, expected, ko, en, keyOf, isOwnKey }) {
  const missingKo = expected.filter((c) => !(keyOf(c) in ko));
  const missingEn = expected.filter((c) => !(keyOf(c) in en));
  const expectedKeys = new Set(expected.map(keyOf));
  const extraKo = Object.keys(ko).filter(
    (k) => isOwnKey(k) && !expectedKeys.has(k),
  );
  const extraEn = Object.keys(en).filter(
    (k) => isOwnKey(k) && !expectedKeys.has(k),
  );
  if (
    missingKo.length === 0 &&
    missingEn.length === 0 &&
    extraKo.length === 0 &&
    extraEn.length === 0
  ) {
    return true;
  }
  if (missingKo.length > 0) {
    console.error(
      `\n[${namespace}] messages/ko.json missing templates for:`,
    );
    for (const c of missingKo) console.error(`  ${keyOf(c)}`);
  }
  if (missingEn.length > 0) {
    console.error(
      `\n[${namespace}] messages/en.json missing templates for:`,
    );
    for (const c of missingEn) console.error(`  ${keyOf(c)}`);
  }
  if (extraKo.length > 0) {
    console.error(
      `\n[${namespace}] messages/ko.json has stale templates with no BE source:`,
    );
    for (const c of extraKo) console.error(`  ${c}`);
  }
  if (extraEn.length > 0) {
    console.error(
      `\n[${namespace}] messages/en.json has stale templates with no BE source:`,
    );
    for (const c of extraEn) console.error(`  ${c}`);
  }
  return false;
}

async function main() {
  const apiCodes = await extractCodes();
  const entelixBuckets = await extractEntelixWireBuckets();
  const ko = await loadBundle("ko.json");
  const en = await loadBundle("en.json");

  // Namespace ownership is explicit prefix-based: every key is
  // either tool-side (`tool_*`), the shared `unknown` singleton, or
  // llm-side (everything else, the full ApiErrorCode set). The
  // singleton fallbacks (`unknown` / `tool_unknown`) are operator-
  // facing fallbacks rather than ApiErrorCode / entelix bucket
  // sources, so each namespace's owner predicate excludes them
  // from the stale-key sweep.
  const llmOk = reportParity({
    namespace: "llm",
    expected: apiCodes,
    ko,
    en,
    keyOf: (c) => c,
    isOwnKey: (k) => !k.startsWith("tool_") && k !== "unknown",
  });
  const toolOk = reportParity({
    namespace: "tool",
    expected: entelixBuckets,
    ko,
    en,
    keyOf: (c) => `tool_${c}`,
    isOwnKey: (k) => k.startsWith("tool_") && k !== "tool_unknown",
  });

  if (llmOk && toolOk) {
    console.log(
      `error-code-parity-audit: ${apiCodes.length} llm code(s) + ${entelixBuckets.length} tool bucket(s) covered by both bundles.`,
    );
    return;
  }
  console.error(
    `\nFix: keep the bundles in lockstep with ApiErrorCode::as_str ` +
      `(crates/ox-api/src/error.rs) and entelix Error::wire_code ` +
      `(../entelix/crates/entelix-core/src/error.rs).`,
  );
  process.exit(1);
}

main().catch((err) => {
  console.error(err);
  process.exit(2);
});
