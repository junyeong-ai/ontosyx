import { defineConfig, globalIgnores } from "eslint/config";
import nextVitals from "eslint-config-next/core-web-vitals";
import nextTs from "eslint-config-next/typescript";

const eslintConfig = defineConfig([
  ...nextVitals,
  ...nextTs,
  {
    rules: {
      // setState in useEffect is intentional for initialization patterns (dark mode, media queries)
      "react-hooks/set-state-in-effect": "warn",
      // Allow explicit any in component props spreading and API response handling
      "@typescript-eslint/no-explicit-any": "warn",
      // Block ad-hoc debug logs; use `lib/logger.ts` instead. `warn`/`error`
      // stay permitted for exceptional paths (error boundaries, validation).
      "no-console": ["error", { allow: ["warn", "error"] }],
    },
  },
  {
    files: ["src/lib/store/selectors.ts"],
    rules: {
      // Action handles are read inline at the call site
      // (`useAppStore((s) => s.fooBar)`); selector wrappers add no
      // memoization value over Zustand's stable action references
      // and split the canonical pattern across two files. Both export
      // shapes (`export const selectActionFoo = ...` and
      // `export { selectActionFoo }`) are blocked.
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
  // Override default ignores of eslint-config-next.
  globalIgnores([
    // Default ignores of eslint-config-next:
    ".next/**",
    "out/**",
    "build/**",
    "next-env.d.ts",
  ]),
]);

export default eslintConfig;
