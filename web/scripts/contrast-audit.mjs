#!/usr/bin/env node
// Contrast gate.
//
// Parses `globals.css` for the documented colour pairs and asserts
// each one clears the WCAG AA contrast threshold for normal text
// (4.5:1) — the same threshold axe-core enforces, but applied at the
// token level so a regression in `globals.css` is caught before any
// page renders.
//
// Pairs are declared inline below. Each pair is checked in light and
// (when defined) dark mode; the worse of the two has to pass.

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..");
const CSS_PATH = path.join(ROOT, "src", "app", "globals.css");

// `[fg, bg, threshold]`. Threshold defaults to 4.5 (WCAG AA normal).
// Ratios are computed in sRGB linear-luminance space (no alpha — alpha
// blends are too parent-dependent to assert here; rely on axe e2e for
// those).
const PAIRS = [
  { fg: "foreground", bg: "surface-base", threshold: 4.5 },
  { fg: "foreground-strong", bg: "surface-base", threshold: 4.5 },
  { fg: "foreground-muted", bg: "surface-base", threshold: 4.5 },
  { fg: "foreground-muted", bg: "surface-raised", threshold: 4.5 },
  { fg: "foreground-muted", bg: "surface-inset", threshold: 4.5 },
  { fg: "foreground-subtle", bg: "surface-base", threshold: 4.5 },
  { fg: "foreground-subtle", bg: "surface-raised", threshold: 4.5 },
  { fg: "foreground-onbrand", bg: "brand-solid", threshold: 4.5 },
  { fg: "foreground-onbrand", bg: "warning-foreground", threshold: 4.5 },
  { fg: "foreground-on-accent", bg: "danger-solid", threshold: 4.5 },
  { fg: "brand-foreground", bg: "surface-base", threshold: 4.5 },
  { fg: "brand-foreground", bg: "brand-surface", threshold: 4.5 },
  { fg: "danger-foreground", bg: "surface-base", threshold: 4.5 },
  { fg: "warning-foreground", bg: "surface-base", threshold: 4.5 },
  { fg: "info-foreground", bg: "surface-base", threshold: 4.5 },
  { fg: "concept-foreground", bg: "surface-base", threshold: 4.5 },
];

function parseHex(hex) {
  const v = hex.replace("#", "");
  if (v.length !== 6) return null;
  const r = parseInt(v.slice(0, 2), 16);
  const g = parseInt(v.slice(2, 4), 16);
  const b = parseInt(v.slice(4, 6), 16);
  if ([r, g, b].some(Number.isNaN)) return null;
  return [r, g, b];
}

function relativeLuminance([r, g, b]) {
  const lin = (c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

function contrastRatio(fg, bg) {
  const lf = relativeLuminance(fg);
  const lb = relativeLuminance(bg);
  const [light, dark] = lf > lb ? [lf, lb] : [lb, lf];
  return (light + 0.05) / (dark + 0.05);
}

function extractDeclarations(css) {
  // Scope tokens by mode. Light = the first :root block; dark = the
  // :root inside `@media (prefers-color-scheme: dark)`.
  const light = {};
  const dark = {};

  const lightMatch = css.match(/:root\s*{([^}]+)}/);
  if (lightMatch) collectVars(lightMatch[1], light);

  const darkMatch = css.match(
    /@media\s*\(prefers-color-scheme:\s*dark\)\s*{\s*:root\s*{([^}]+)}\s*}/,
  );
  if (darkMatch) collectVars(darkMatch[1], dark);

  return { light, dark };
}

function collectVars(block, target) {
  const re = /--([\w-]+):\s*([^;]+);/g;
  let m;
  while ((m = re.exec(block))) {
    target[m[1]] = m[2].trim();
  }
}

function resolve(token, table) {
  let value = table[token];
  let depth = 0;
  while (value && /^var\(/.test(value) && depth < 5) {
    const inner = value.match(/var\(--([\w-]+)\)/);
    if (!inner) break;
    value = table[inner[1]];
    depth += 1;
  }
  return value;
}

async function main() {
  const css = await fs.readFile(CSS_PATH, "utf8");
  const { light, dark } = extractDeclarations(css);

  const failures = [];
  const skipped = [];
  let checked = 0;

  for (const pair of PAIRS) {
    for (const [mode, table] of [
      ["light", light],
      ["dark", dark],
    ]) {
      const fgVal = resolve(pair.fg, table);
      const bgVal = resolve(pair.bg, table);
      if (!fgVal || !bgVal) continue; // token not defined in this mode

      const fgHex = parseHex(fgVal);
      const bgHex = parseHex(bgVal);
      if (!fgHex || !bgHex) {
        // Alpha-blended (rgba / color-mix) — pure-CSS contrast depends
        // on the parent surface so axe e2e is the source of truth here.
        skipped.push({ fg: pair.fg, bg: pair.bg, mode, fgVal, bgVal });
        continue;
      }
      checked += 1;
      const ratio = contrastRatio(fgHex, bgHex);
      if (ratio < pair.threshold) {
        failures.push({
          fg: pair.fg,
          bg: pair.bg,
          mode,
          fgVal,
          bgVal,
          ratio: ratio.toFixed(2),
          threshold: pair.threshold,
        });
      }
    }
  }

  if (failures.length === 0) {
    const skipNote = skipped.length
      ? ` ${skipped.length} alpha-blended pair(s) deferred to axe e2e.`
      : "";
    console.log(
      `contrast-audit: ${checked} pair(s) checked across light + dark — all clear AA.${skipNote}`,
    );
    process.exit(0);
  }

  console.error(`contrast-audit: ${failures.length} failure(s):\n`);
  for (const f of failures) {
    console.error(
      `  ${f.fg} (${f.fgVal}) on ${f.bg} (${f.bgVal})  [${f.mode}]  ${f.ratio}:1 < ${f.threshold}:1`,
    );
  }
  process.exit(1);
}

main().catch((err) => {
  console.error(`contrast-audit: ${err.message}`);
  process.exit(2);
});
