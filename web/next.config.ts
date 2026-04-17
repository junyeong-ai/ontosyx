import type { NextConfig } from "next";
import path from "node:path";

const nextConfig: NextConfig = {
  // Why: monorepo has lockfiles at both repo root and `web/`; pin Turbopack's
  // root so it doesn't silently pick the repo root and then fail to resolve
  // `next/package.json` from `src/app`.
  turbopack: {
    root: path.resolve(__dirname),
  },
};

export default nextConfig;
