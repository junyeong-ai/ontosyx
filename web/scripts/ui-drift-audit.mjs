#!/usr/bin/env node
// UI drift gate.
//
// Asserts that every consumer of the design system follows the
// primitive-first / token-driven contract:
//
//   1. No raw palette colours (`text-emerald-700`, `bg-red-500`, …)
//      — only semantic tokens (`text-brand-foreground`, `bg-danger-surface`).
//   2. No `text-white` / `bg-white` / `text-black` / `bg-black` in feature
//      code — only the collab presence palette (allow-listed) may use them.
//   3. No raw `<input>` / `<select>` / `<textarea>` outside the
//      form-input primitives — every form control flows through
//      `FormInput`, `Settings*`, or `FormField`.
//   4. No direct `@base-ui/react` import outside `components/ui/` —
//      modal / dialog / popover behaviour is owned by the primitive layer.
//   5. No `outline-none` / `focus:outline-none` without a paired
//      `focus-visible:ring*` (or `focus:ring*`) on the same element —
//      keyboard focus must always be visible.
//   6. No raw numeric z-index (`z-50`, `z-10`, …) — every layer picks
//      its semantic role from the z-index hierarchy in `globals.css`
//      (`z-canvas`, `z-chrome`, `z-presence`, `z-banner`, `z-overlay`,
//      `z-modal`, `z-popover`, `z-toast`, `z-tooltip`, `z-skip-link`).
//      Hardcoding a number short-circuits the layer order; a new
//      surface that lands at `z-50` collides silently with every
//      modal in the app.
//   7. No raw `<kbd>` — every key chord renders through the
//      `<KeyboardShortcut>` primitive so the chip styling, the
//      per-platform glyph rendering (`mod+k` → ⌘K on macOS,
//      Ctrl+K elsewhere), and the `<kbd>` semantics live in one
//      file. Without this, every consumer hand-rolls
//      `<kbd className="rounded font-mono ...">` and the visual
//      register drifts pane to pane.
//   8. No physical directional Tailwind utilities (`ml-`, `mr-`,
//      `pl-`, `pr-`, `text-left`, `text-right`, `border-l-`,
//      `border-r-`, `rounded-l-`, `rounded-r-`) — every consumer
//      uses the logical equivalents (`ms-`, `me-`, `ps-`, `pe-`,
//      `text-start`, `text-end`, `border-s-`, `border-e-`,
//      `rounded-s-`, `rounded-e-`). The `<html dir>` toggle in
//      `RootLayout` flips the active locale's direction; logical
//      utilities follow it automatically, physical ones don't.
//      Absolute positioning (`left-0`, `right-1/2`, `-translate-x-…`,
//      `inset-x-…`) and negative-margin physical forms are out of
//      scope — those are layout coordinates, not directional
//      spacing, and flipping them silently mirrors panes.
//
// Ratchet semantics: existing drift is captured in `ui-drift-baseline.json`
// (counts per file per rule). The gate fails only if any cell in the
// matrix grows — every migration that lowers the count is a one-way
// ratchet. Regenerate the baseline with `--update` once a migration
// lands.
//
// **The baseline file is git-tracked** — branches that diverge on drift
// counts converge through normal merge-conflict resolution. An
// untracked baseline would let two branches independently lower their
// counts and silently regress on merge.
//
// Findings print as `path:line  rule  snippet` so CI fails fast and
// the offending site is one click away in the editor.

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "..");
const SRC = path.join(ROOT, "src");
const BASELINE_PATH = path.join(HERE, "ui-drift-baseline.json");

const args = new Set(process.argv.slice(2));
const updateBaseline = args.has("--update");

// Files that legitimately use the disallowed primitives. Every entry
// here is a deliberate exemption — the file's own comments explain why.
const ALLOW_LIST = {
  rawWhiteBlack: new Set([
    "src/components/ui/avatar.tsx",
    "src/components/collab/lock-indicator.tsx",
    "src/components/collab/presence-avatars.tsx",
    "src/components/collab/remote-cursor-layer.tsx",
  ]),
  rawFormControl: new Set([
    "src/components/ui/form-input.tsx",
    "src/components/ui/select.tsx",
    "src/components/ui/form-field.tsx",
    "src/components/ui/checkbox.tsx",
    "src/components/ui/radio.tsx",
    "src/components/providers/prompt-provider.tsx",
    "src/components/providers/confirm-provider.tsx",
    // `useImeAwareInput` is the IME-aware base hook; raw `<input>`
    // ref typing is part of its surface.
    "src/hooks/use-ime-aware-input.ts",
    // `type="file"` trigger — invisible (`className="hidden"`),
    // wired to a Button that delegates the click. Not a visual
    // primitive concern.
    "src/components/layout/mode-actions.tsx",
    // Canvas command-bar / command-palette / command-preview each
    // own a custom prompt surface — terminal-style `>` prefix or AI
    // wand glyph, multi-state phase chrome (preview vs commit),
    // inline diff overlay. Wrapping these in `FormInput` would
    // either lose the prompt-glyph affordance or require threading
    // every state through a primitive that shouldn't carry that
    // surface area. The bare `<input>` is the right call here.
    "src/components/workbench/canvas/command-bar.tsx",
    "src/components/workbench/canvas/command-palette.tsx",
    "src/components/workbench/canvas/command-preview.tsx",
    // Test fixture — exercises the workbench page shell with a
    // synthetic raw input to verify aria-live region wiring.
    "src/components/workbench/__tests__/workbench-page-shell.test.tsx",
    // ChipInput primitive — the bare `<input>` is the cursor row
    // inside the chip-rendering wrapper. The wrapper carries the
    // visible focus-within ring; routing through `FormInput` would
    // either collapse the chip+input composition or duplicate the
    // chip-tag rendering inside FormInput's surface area.
    "src/components/ui/chip-input.tsx",
    // LocalizedTextInput primitive — the multiline branch ships a
    // bare `<textarea>` because FormInput is single-line only.
    // Promoting multiline into FormInput would either fork the
    // primitive or carry textarea-only props on the input shape.
    "src/components/forms/primitives/localized-text-input.tsx",
  ]),
  baseUi: new Set([
    "src/components/providers/confirm-provider.tsx",
    "src/components/providers/prompt-provider.tsx",
    // WelcomeModal — the multi-step onboarding wizard with
    // `AnimatePresence` between slides. The Modal primitive
    // is the standard single-pane shape; promoting wizard
    // pagination + per-slide motion choreography into Modal
    // would either bloat the primitive's API surface or fork
    // it. The bare `<Dialog>` here owns the wizard-specific
    // shape that doesn't recur elsewhere.
    "src/components/onboarding/welcome-modal.tsx",
  ]),
  rawKbd: new Set([
    // The KeyboardShortcut primitive is the one and only place where
    // `<kbd>` lives. Every other surface routes through it.
    "src/components/ui/keyboard-shortcut.tsx",
  ]),
};

const RAW_PALETTE = /\b(text|bg|border|fill|stroke|ring|placeholder|outline|divide|from|via|to|shadow)-(?:slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)-\d+\b/g;
const RAW_WHITE_BLACK = /\b(text|bg|border|fill|stroke|ring|placeholder|outline)-(?:white|black)(?:\/\d+)?\b/g;
const BASE_UI_IMPORT = /from\s+["']@base-ui\/react/;
const RAW_FORM_CONTROL = /<\s*(input|select|textarea)\b/g;
const NAKED_OUTLINE_NONE = /\boutline-none\b(?![^"`\n]*\bfocus(?:-visible)?:ring)/;
const RAW_Z_INDEX = /\bz-(0|10|20|30|40|50|60|70|80|90)\b/g;
const RAW_KBD = /<\s*kbd\b/g;
// Directional spacing / typography / radii / borders / inset
// anchors. Use logical equivalents so RTL locales flip naturally:
//   ml/mr → ms/me            text-left → text-start
//   pl/pr → ps/pe            text-right → text-end
//   border-l/r → border-s/e  rounded-l/r → rounded-s/e
//
// Absolute / fixed inset anchors (`left-2 top-2`, `-right-1.5`)
// also belong here — they ARE direction-sensitive when they pin a
// surface to the inline-start or inline-end side. Centering with
// `left-1/2 -translate-x-1/2` is mathematically symmetric so the
// `1/2` form is exempt.
const PHYSICAL_DIRECTIONAL =
  /\b(ml|mr|pl|pr)-(?:\d|auto|px|\[)|text-(?:left|right)\b|\b(?:border|rounded)-(?:l|r)(?:-|\b)|(?<![/\w-])-?(?:right|left)-(?:\d+(?:\.\d+)?|\[[^\]]+\])\b(?!\/2)/g;

async function* walk(dir) {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    if (entry.name.startsWith(".") || entry.name === "node_modules") continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      yield* walk(full);
    } else if (/\.(tsx?|mjs|cjs)$/.test(entry.name)) {
      yield full;
    }
  }
}

function rel(p) {
  return path.relative(ROOT, p);
}

async function collectFindings() {
  /** @type {Array<{file: string; line: number; rule: string; snippet: string}>} */
  const findings = [];
  let scanned = 0;

  for await (const file of walk(SRC)) {
    scanned += 1;
    const relPath = rel(file);
    const content = await fs.readFile(file, "utf8");
    const lines = content.split("\n");

    const allowWhiteBlack = ALLOW_LIST.rawWhiteBlack.has(relPath);
    const allowForm = ALLOW_LIST.rawFormControl.has(relPath);
    const allowBaseUi =
      ALLOW_LIST.baseUi.has(relPath) || relPath.startsWith("src/components/ui/");

    for (let i = 0; i < lines.length; i += 1) {
      const lineText = lines[i];
      const lineNo = i + 1;
      const stripped = lineText
        .replace(/\/\/[^\n]*$/g, "")
        .replace(/\/\*[\s\S]*?\*\//g, "");

      const palMatch = stripped.match(RAW_PALETTE);
      if (palMatch) {
        for (const m of palMatch) {
          findings.push({ file: relPath, line: lineNo, rule: "raw-palette", snippet: m });
        }
      }

      if (!allowWhiteBlack) {
        const wbMatch = stripped.match(RAW_WHITE_BLACK);
        if (wbMatch) {
          for (const m of wbMatch) {
            findings.push({ file: relPath, line: lineNo, rule: "raw-white-black", snippet: m });
          }
        }
      }

      if (!allowBaseUi && BASE_UI_IMPORT.test(stripped)) {
        findings.push({ file: relPath, line: lineNo, rule: "base-ui-direct-import", snippet: stripped.trim() });
      }

      if (!allowForm) {
        const fcMatch = stripped.match(RAW_FORM_CONTROL);
        if (fcMatch) {
          for (const m of fcMatch) {
            const idx = stripped.indexOf(m);
            const prefix = stripped.slice(Math.max(0, idx - 1), idx);
            if (prefix === '"' || prefix === "'" || prefix === "`") continue;
            findings.push({ file: relPath, line: lineNo, rule: "raw-form-control", snippet: m });
          }
        }
      }

      const zMatch = stripped.match(RAW_Z_INDEX);
      if (zMatch) {
        for (const m of zMatch) {
          findings.push({ file: relPath, line: lineNo, rule: "raw-z-index", snippet: m });
        }
      }

      if (!ALLOW_LIST.rawKbd.has(relPath)) {
        const kbdMatch = stripped.match(RAW_KBD);
        if (kbdMatch) {
          for (const m of kbdMatch) {
            findings.push({ file: relPath, line: lineNo, rule: "raw-kbd", snippet: m });
          }
        }
      }

      const physMatch = stripped.match(PHYSICAL_DIRECTIONAL);
      if (physMatch) {
        for (const m of physMatch) {
          findings.push({
            file: relPath,
            line: lineNo,
            rule: "physical-directional",
            snippet: m,
          });
        }
      }

      if (NAKED_OUTLINE_NONE.test(stripped)) {
        // Walk forward until the className= attribute closes — `focus:ring`
        // could land 10+ lines below `outline-none` in a multi-line
        // tailwind blob. Stop at the next `>`/`}` so we don't bleed
        // into the next element. 12-line cap as a safety bound.
        const windowEnd = Math.min(lines.length, i + 12);
        const window = lines.slice(i, windowEnd).join(" ");
        if (!/focus(?:-visible)?:ring/.test(window)) {
          findings.push({ file: relPath, line: lineNo, rule: "outline-none-without-ring", snippet: lineText.trim() });
        }
      }
    }
  }

  return { findings, scanned };
}

function summarise(findings) {
  /** @type {Record<string, Record<string, number>>} */
  const matrix = {};
  for (const f of findings) {
    matrix[f.file] = matrix[f.file] ?? {};
    matrix[f.file][f.rule] = (matrix[f.file][f.rule] ?? 0) + 1;
  }
  return matrix;
}

async function loadBaseline() {
  try {
    const text = await fs.readFile(BASELINE_PATH, "utf8");
    return JSON.parse(text);
  } catch (err) {
    if (err.code === "ENOENT") return {};
    throw err;
  }
}

function diffMatrix(current, baseline) {
  /** @type {Array<{file: string; rule: string; delta: number}>} */
  const regressions = [];
  for (const [file, rules] of Object.entries(current)) {
    for (const [rule, count] of Object.entries(rules)) {
      const baselineCount = baseline[file]?.[rule] ?? 0;
      if (count > baselineCount) {
        regressions.push({ file, rule, delta: count - baselineCount });
      }
    }
  }
  return regressions;
}

async function main() {
  const { findings, scanned } = await collectFindings();
  const matrix = summarise(findings);

  if (updateBaseline) {
    await fs.writeFile(
      BASELINE_PATH,
      `${JSON.stringify(matrix, null, 2)}\n`,
      "utf8",
    );
    console.log(
      `ui-drift-audit: baseline updated — ${findings.length} finding(s) across ${scanned} file(s).`,
    );
    process.exit(0);
  }

  const baseline = await loadBaseline();
  const regressions = diffMatrix(matrix, baseline);

  if (regressions.length === 0) {
    const baselineTotal = Object.values(baseline).reduce(
      (sum, rules) => sum + Object.values(rules).reduce((a, b) => a + b, 0),
      0,
    );
    console.log(
      `ui-drift-audit: scanned ${scanned} file(s) — ${findings.length} known drift, ${baselineTotal} baselined. No regressions.`,
    );
    process.exit(0);
  }

  console.error(
    `ui-drift-audit: ${regressions.length} regression(s) — new drift introduced:\n`,
  );
  for (const r of regressions) {
    const offenders = findings.filter((f) => f.file === r.file && f.rule === r.rule);
    const baselineCount = baseline[r.file]?.[r.rule] ?? 0;
    console.error(`  ${r.file}  ${r.rule}  ${baselineCount} → ${baselineCount + r.delta}`);
    for (const f of offenders.slice(baselineCount)) {
      const truncated =
        f.snippet.length > 120 ? `${f.snippet.slice(0, 117)}…` : f.snippet;
      console.error(`    line ${f.line}  ${truncated}`);
    }
  }
  console.error(
    `\nFix the regressions, or — if the new drift is intentional and there's a migration plan in flight — run \`pnpm gate:ui-drift -- --update\` to refresh the baseline.`,
  );
  process.exit(1);
}

main().catch((err) => {
  console.error(`ui-drift-audit: ${err.message}`);
  process.exit(2);
});
