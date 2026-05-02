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
  // Native page-transition cross-fades on supported browsers — the
  // route swap reuses a snapshot of the previous page during the
  // outgoing fade so the eye lands on the new content without a
  // hard cut. Falls back to instant swap on browsers without the
  // View Transitions API; no user-facing failure.
  experimental: {
    viewTransition: true,
  },
};

export default withNextIntl(nextConfig);
