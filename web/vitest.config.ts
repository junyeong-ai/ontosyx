import { defineConfig } from "vitest/config";
import path from "path";

export default defineConfig({
  test: {
    environment: "jsdom",
    include: [
      "src/**/*.test.{ts,tsx}",
      // The i18n auditor lives under `scripts/` so it ships with the
      // tool rather than under src/ (which is Next.js's build input).
      // Keeping its test alongside the lib keeps tooling coherent.
      "scripts/**/*.test.{ts,tsx,mts,mjs}",
    ],
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
    },
  },
});
