#!/usr/bin/env node
// Design-rule audit — project-specific Tailwind / token / naming
// conventions that the standard linter can't express. Each rule is
// a regex over file contents; failures print with file:line and a
// remediation hint.
//
// Why a script and not a Biome rule: these conventions depend on
// AST-aware regex selectors (e.g. "transition utility WITHOUT a
// duration token") and project-specific design-system tokens that
// no general-purpose linter ships. The audit pattern is already
// established for ui-drift / contrast / i18n; this fits cleanly
// alongside.

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const SRC = path.join(ROOT, "src");

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

const RAW_PALETTE_RE = /(?:^|\s)(?:text|bg|border|ring|fill|stroke|from|to|via|divide|outline|placeholder|caret|accent|decoration|shadow)-(?:slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)-[0-9]/;

const TEXT_TINY_RE = /text-\[(?:[0-9]|10)px\]/;
const MAGIC_W_RE = /(?:^|\s)w-\[(?:2[5-9]|[3-9][0-9]|[1-9][0-9]{2,})px\]/;
const IS_OPEN_PROP_RE = /\bis(Open|Visible)=/;
const DOUBLE_OPACITY_RE =
  /(?:text|bg|border|ring|fill|stroke|divide|outline)-[a-z][\w-]*\/[0-9]+\/[0-9]+/;
const RAW_TRANSITION_RE =
  /(?:^|\s)transition-(?:colors|all|opacity|transform|shadow|\[)(?![^"`\n]*\bduration-(?:\[var\(--duration|0\b))/;
const SONNER_IMPORT_RE = /from\s+["']sonner["']/;
const ACTION_SELECTOR_RE =
  /export\s+(?:const|function|\{[^}]*)\s*selectAction[A-Z]/;

const RULES = [
  {
    id: "raw-palette",
    re: RAW_PALETTE_RE,
    hint: "Use a semantic token (text-foreground / bg-surface-base / border-divider / etc.).",
  },
  {
    id: "text-tiny",
    re: TEXT_TINY_RE,
    hint: "Sub-11px text is below readability + WCAG limits. Use text-2xs (11px) or larger.",
  },
  {
    id: "magic-width",
    re: MAGIC_W_RE,
    hint: "Magic pixel widths are forbidden. Use a width token (w-rail / w-sidebar-narrow / w-sidebar / w-inspector / w-panel / w-panel-wide).",
  },
  {
    id: "is-prefix-prop",
    re: IS_OPEN_PROP_RE,
    hint: "Use bare adjective prop names: `open`, `visible` — never `isOpen` / `isVisible`.",
  },
  {
    id: "double-opacity",
    re: DOUBLE_OPACITY_RE,
    hint: "Double opacity / line-height modifier is invalid Tailwind syntax (e.g. `bg-X/50/20`). Collapse to a single modifier.",
  },
  {
    id: "raw-transition",
    re: RAW_TRANSITION_RE,
    hint: "Raw transition utility without a motion token. Pair with a duration token (quick / base / slow / slower) or opt out with duration-0.",
  },
];

// Per-file path rules.
const PATH_RULES = [
  {
    id: "sonner-import",
    appliesTo: (rel) =>
      rel.startsWith("src/") &&
      rel !== "src/components/ui/toast.tsx" &&
      !rel.includes("/__tests__/") &&
      !rel.endsWith(".test.ts") &&
      !rel.endsWith(".test.tsx"),
    re: SONNER_IMPORT_RE,
    hint: "Import `toast` / `Toaster` from `@/components/ui/toast` instead. The wrapper owns variant styling, icons, and queue config.",
  },
  {
    id: "action-selector-wrapper",
    appliesTo: (rel) => rel === "src/lib/store/selectors.ts",
    re: ACTION_SELECTOR_RE,
    hint: "Action selector wrappers are forbidden. Read actions inline at the call site: useAppStore((s) => s.fooBar).",
  },
];

// ---------------------------------------------------------------------------
// Walk
// ---------------------------------------------------------------------------

async function* walk(dir) {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  for (const e of entries) {
    if (e.name.startsWith(".") || e.name === "node_modules") continue;
    const full = path.join(dir, e.name);
    if (e.isDirectory()) {
      yield* walk(full);
    } else if (/\.(ts|tsx|js|jsx|mjs|cjs)$/.test(e.name)) {
      // Skip generated + tests for the design rules.
      if (e.name === "api.generated.ts") continue;
      yield full;
    }
  }
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

const findings = [];
for await (const file of walk(SRC)) {
  const rel = path.relative(ROOT, file);
  const isTest =
    rel.includes("/__tests__/") ||
    rel.endsWith(".test.ts") ||
    rel.endsWith(".test.tsx");
  const content = await fs.readFile(file, "utf8");
  const lines = content.split("\n");

  // Path-scoped rules first (apply across whole file).
  for (const rule of PATH_RULES) {
    if (!rule.appliesTo(rel)) continue;
    for (let i = 0; i < lines.length; i++) {
      if (rule.re.test(lines[i])) {
        findings.push({
          file: rel,
          line: i + 1,
          rule: rule.id,
          hint: rule.hint,
          snippet: lines[i].trim(),
        });
      }
    }
  }

  // Token / utility regex rules — skip test files (they often
  // reference structural class names in selectors / assertions).
  if (isTest) continue;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    for (const rule of RULES) {
      if (rule.re.test(line)) {
        findings.push({
          file: rel,
          line: i + 1,
          rule: rule.id,
          hint: rule.hint,
          snippet: line.trim().slice(0, 200),
        });
      }
    }
  }
}

if (findings.length === 0) {
  console.log("design-rules-audit: ✓ no violations");
  process.exit(0);
}

const grouped = new Map();
for (const f of findings) {
  if (!grouped.has(f.rule)) grouped.set(f.rule, []);
  grouped.get(f.rule).push(f);
}

for (const [rule, list] of grouped) {
  console.log(`\n[${rule}] ${list.length} violation(s) — ${list[0].hint}`);
  for (const f of list.slice(0, 10)) {
    console.log(`  ${f.file}:${f.line}  ${f.snippet}`);
  }
  if (list.length > 10) {
    console.log(`  … and ${list.length - 10} more`);
  }
}

console.error(`\ndesign-rules-audit: ${findings.length} violation(s)`);
process.exit(1);
