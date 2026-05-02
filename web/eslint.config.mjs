import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";
import nextTs from "eslint-config-next/typescript";

const RAW_PALETTE_REGEX =
  "(?:^|\\s)(?:text|bg|border|ring|fill|stroke|from|to|via|divide|outline|placeholder|caret|accent|decoration|shadow)-(?:slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)-[0-9]";

const designRules = [
  {
    selector: `Literal[value=/${RAW_PALETTE_REGEX}/]`,
    message:
      "Raw palette colour. Use a semantic token (text-foreground, text-foreground-muted, text-muted-foreground, text-foreground-strong; bg-surface-base, bg-surface-raised, bg-surface-inset; border-divider, border-divider-soft; text-brand-foreground, bg-brand-surface, bg-brand-solid; bg-success-surface, bg-warning-surface, bg-danger-surface, bg-info-surface, bg-concept-surface).",
  },
  {
    selector: `TemplateElement[value.raw=/${RAW_PALETTE_REGEX}/]`,
    message: "Raw palette colour. Use a semantic token.",
  },
  {
    selector: "Literal[value=/text-\\[(?:[0-9]|10)px\\]/]",
    message:
      "Sub-11px text is below readability + WCAG limits. Use `text-2xs` (11px) or larger.",
  },
  {
    selector:
      "Literal[value=/(?:^|\\s)w-\\[(?:2[5-9]|[3-9][0-9]|[1-9][0-9]{2,})px\\]/]",
    message:
      "Magic pixel widths are forbidden. Use a width token (w-rail / w-sidebar-narrow / w-sidebar / w-inspector / w-panel-narrow / w-panel / w-panel-wide).",
  },
  {
    selector: "JSXAttribute[name.name=/^is(Open|Visible)$/]",
    message:
      "Use bare adjective prop names: `open`, `visible` — never `isOpen` / `isVisible`.",
  },
  {
    selector:
      "Literal[value=/(?:text|bg|border|ring|fill|stroke|divide|outline)-[a-z][\\w-]*\\/[0-9]+\\/[0-9]+/]",
    message:
      "Double opacity / line-height modifier is invalid Tailwind syntax (e.g. `bg-X/50/20`). Likely codemod side-effect — collapse to a single modifier.",
  },
  {
    selector:
      "TemplateElement[value.raw=/(?:text|bg|border|ring|fill|stroke|divide|outline)-[a-z][\\w-]*\\/[0-9]+\\/[0-9]+/]",
    message:
      "Double opacity / line-height modifier is invalid Tailwind syntax. Likely codemod side-effect.",
  },
];

const eslintConfig = defineConfig([
  ...nextVitals,
  ...nextTs,
  {
    rules: {
      "react-hooks/set-state-in-effect": "warn",
      "@typescript-eslint/no-explicit-any": "warn",
      "no-console": ["error", { allow: ["warn", "error"] }],
    },
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    ignores: [
      "src/components/ui/**",
      "src/components/motion/**",
      // Chart / data-visualisation surface — raw palette is the
      // semantic of categorical color encoding (recharts, force-
      // graph). Tokens map to status meaning, not to a 12-way
      // categorical scale.
      "src/components/widgets/**",
      "src/components/workbench/dashboard*/**",
      "src/components/workbench/dashboard-*.tsx",
      "src/lib/logger.ts",
      "src/types/api.generated.ts",
      "src/**/__tests__/**",
      "src/**/*.test.ts",
      "src/**/*.test.tsx",
    ],
    rules: {
      "no-restricted-syntax": ["error", ...designRules],
    },
  },
  {
    files: ["src/lib/store/selectors.ts"],
    rules: {
      "no-restricted-syntax": [
        "error",
        {
          selector:
            "ExportNamedDeclaration > VariableDeclaration > VariableDeclarator[id.name=/^selectAction/]",
          message:
            "Action selector wrappers are forbidden. Read actions inline at the call site: useAppStore((s) => s.fooBar).",
        },
        {
          selector: "ExportSpecifier[exported.name=/^selectAction/]",
          message:
            "Action selector wrappers are forbidden. Read actions inline at the call site: useAppStore((s) => s.fooBar).",
        },
      ],
    },
  },
  globalIgnores([
    ".next/**",
    "out/**",
    "build/**",
    "next-env.d.ts",
  ]),
]);

export default eslintConfig;
