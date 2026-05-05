#!/usr/bin/env node
// Source-size budget gate.
//
// Measures the total source byte footprint per top-level surface in
// `src/` and asserts it stays at or below the recorded baseline. The
// gate is a *source-level* proxy for bundle size — it doesn't require
// `next build` so it runs in seconds in CI and catches the kind of
// regressions that bloat the bundle long before the build artifact
// would: a copy-paste of an entire icon set into one component, an
// unintended `import` of every locale's bundle, a vendored library
// landing in `lib/` instead of `package.json`.
//
// What we measure:
//   * Total source bytes per top-level surface (`src/app`,
//     `src/components/...`, `src/lib`, `src/hooks`, `src/i18n`,
//     `src/types`, …) — excludes tests, node_modules, generated
//     files, and `.next`.
//   * The whole-tree byte total as a single budget line.
//
// Ratchet semantics:
//   * Recorded budgets live in `source-size-budget.json`.
//   * A measured value over budget × (1 + slack) fails — the slack is
//     5% per surface, 3% on the whole-tree total. Below budget passes
//     silently; the budget file isn't auto-rewritten on drop so the
//     ratchet stays one-way (run `--update` to lock in a smaller
//     budget after a sustained reduction).
//
// Slack matters because the surface metric jitters with normal
// formatting / comment tweaks; without it, every typo fix would
// re-trigger the gate. The whole-tree number gets a tighter slack
// because it averages out per-surface noise.

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..");
const SRC = path.join(ROOT, "src");
const BUDGET_PATH = path.join(HERE, "source-size-budget.json");

const args = new Set(process.argv.slice(2));
const updateBudget = args.has("--update");

const PER_SURFACE_SLACK = 0.05;
const TOTAL_SLACK = 0.03;

// Map a file path to a stable surface bucket. Keep the partition
// shallow — too granular and the buckets jitter with file moves;
// too coarse and the gate misses regressions per area.
function surfaceFor(relPath) {
  const segments = relPath.split(path.sep);
  // src/app/.../page.tsx → app/<top-route>
  if (segments[0] === "app") {
    if (segments.length >= 2 && !segments[1].endsWith(".tsx")) {
      return `app/${segments[1].replace(/^\(([^)]+)\)$/, "$1")}`;
    }
    return "app/_root";
  }
  if (segments[0] === "components") {
    return segments.length >= 2 ? `components/${segments[1]}` : "components/_root";
  }
  if (segments[0] === "lib") return "lib";
  if (segments[0] === "hooks") return "hooks";
  if (segments[0] === "i18n") return "i18n";
  if (segments[0] === "types") return "types";
  if (segments[0] === "test-utils") return "test-utils";
  return segments[0] ?? "_root";
}

function shouldCount(name) {
  if (name.startsWith(".")) return false;
  if (name === "node_modules") return false;
  if (name === "__tests__") return false;
  if (name.endsWith(".test.ts") || name.endsWith(".test.tsx")) return false;
  if (name.endsWith(".d.ts")) return false;
  // Generated wire shapes — counting them as "source" misleads the
  // ratchet because the file size tracks the OpenAPI schema, not the
  // engineering choices that affect the bundle.
  if (name === "api.generated.ts") return false;
  return true;
}

async function* walk(dir) {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    if (!shouldCount(entry.name)) continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      yield* walk(full);
    } else if (/\.(tsx?|mjs|cjs|css)$/.test(entry.name)) {
      yield full;
    }
  }
}

async function measure() {
  const surfaceBytes = new Map();
  let total = 0;
  let fileCount = 0;

  for await (const file of walk(SRC)) {
    const stat = await fs.stat(file);
    const rel = path.relative(SRC, file);
    const surface = surfaceFor(rel);
    surfaceBytes.set(surface, (surfaceBytes.get(surface) ?? 0) + stat.size);
    total += stat.size;
    fileCount += 1;
  }

  const surfaces = Object.fromEntries(
    [...surfaceBytes.entries()].sort(([a], [b]) => a.localeCompare(b)),
  );
  return { surfaces, total, fileCount };
}

async function loadBudget() {
  try {
    const text = await fs.readFile(BUDGET_PATH, "utf8");
    return JSON.parse(text);
  } catch (err) {
    if (err.code === "ENOENT") return null;
    throw err;
  }
}

function formatKb(bytes) {
  return `${(bytes / 1024).toFixed(1)}kb`;
}

async function main() {
  const measured = await measure();

  if (updateBudget) {
    await fs.writeFile(
      BUDGET_PATH,
      `${JSON.stringify(measured, null, 2)}\n`,
      "utf8",
    );
    console.log(
      `source-size-audit: budget updated — ${formatKb(measured.total)} across ${measured.fileCount} files, ${Object.keys(measured.surfaces).length} surfaces.`,
    );
    process.exit(0);
  }

  const budget = await loadBudget();
  if (!budget) {
    console.error(
      `source-size-audit: no budget file at ${path.relative(ROOT, BUDGET_PATH)} — run with \`--update\` to record one.`,
    );
    process.exit(2);
  }

  const regressions = [];

  // Per-surface check.
  for (const [surface, bytes] of Object.entries(measured.surfaces)) {
    const budgeted = budget.surfaces?.[surface];
    if (budgeted === undefined) {
      // New surface — strict pass. The next `--update` rolls it in.
      continue;
    }
    const cap = Math.ceil(budgeted * (1 + PER_SURFACE_SLACK));
    if (bytes > cap) {
      regressions.push({
        kind: "surface",
        surface,
        budget: budgeted,
        cap,
        actual: bytes,
        deltaPct: ((bytes - budgeted) / budgeted) * 100,
      });
    }
  }

  // Whole-tree check.
  if (typeof budget.total === "number") {
    const totalCap = Math.ceil(budget.total * (1 + TOTAL_SLACK));
    if (measured.total > totalCap) {
      regressions.push({
        kind: "total",
        budget: budget.total,
        cap: totalCap,
        actual: measured.total,
        deltaPct: ((measured.total - budget.total) / budget.total) * 100,
      });
    }
  }

  if (regressions.length === 0) {
    console.log(
      `source-size-audit: ${formatKb(measured.total)} across ${measured.fileCount} files — within budget.`,
    );
    process.exit(0);
  }

  console.error(
    `source-size-audit: ${regressions.length} budget regression(s):\n`,
  );
  for (const r of regressions) {
    if (r.kind === "surface") {
      console.error(
        `  surface ${r.surface}: ${formatKb(r.actual)} (+${r.deltaPct.toFixed(1)}%) over budget ${formatKb(r.budget)} cap ${formatKb(r.cap)}`,
      );
    } else {
      console.error(
        `  total: ${formatKb(r.actual)} (+${r.deltaPct.toFixed(1)}%) over budget ${formatKb(r.budget)} cap ${formatKb(r.cap)}`,
      );
    }
  }
  console.error(
    `\nIf the regression is intentional (a feature shipped, not bloat), run \`pnpm source-size-audit -- --update\` to lock in the new budget.`,
  );
  process.exit(1);
}

main().catch((err) => {
  console.error(`source-size-audit: ${err.message}`);
  process.exit(2);
});
