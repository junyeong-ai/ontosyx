import type { NextConfig } from "next";
import path from "node:path";
import createNextIntlPlugin from "next-intl/plugin";

const withNextIntl = createNextIntlPlugin("./src/i18n/request.ts");

const nextConfig: NextConfig = {
  // Why: monorepo has lockfiles at both repo root and `web/`; pin Turbopack's
  // root so it doesn't silently pick the repo root and then fail to resolve
  // `next/package.json` from `src/app`.
  turbopack: {
    root: path.resolve(__dirname),
  },
  // Avatar sources that `<Avatar>` accepts. OIDC providers host profile
  // pictures on these CDNs; adding them here lets `next/image` proxy them
  // safely and lets the component render without `unoptimized` for
  // additional hosts in the future.
  images: {
    remotePatterns: [
      { protocol: "https", hostname: "lh3.googleusercontent.com" },
      { protocol: "https", hostname: "secure.gravatar.com" },
      { protocol: "https", hostname: "avatars.githubusercontent.com" },
    ],
  },
};

export default withNextIntl(nextConfig);
