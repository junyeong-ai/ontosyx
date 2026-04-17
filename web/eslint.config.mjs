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
