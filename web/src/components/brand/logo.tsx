import Link from "next/link";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";

// ---------------------------------------------------------------------------
// Brand identity primitives — single source of truth for every surface
// that renders the Ontosyx mark or wordmark.
// ---------------------------------------------------------------------------
//
// Three exports cover every chrome / marketing / favicon / OG path:
//
//   BrandMark      — the SVG-only graph-triple mark. Inherits text colour
//                    via `currentColor` so it retypes through both
//                    `text-brand-foreground` (ambient) and
//                    `text-foreground-onbrand` (when sitting inside a
//                    brand-solid tile).
//   BrandWordmark  — the styled "Ontosyx" wordmark. Reads the brand
//                    name from the i18n catalogue at
//                    `chrome.header.appTitle` so locale catalogues
//                    own the literal — never hardcoded in JSX.
//   BrandLogo      — the lockup (mark + wordmark) with optional
//                    anchor link. Used for marketing / login / error
//                    surfaces; the sidebar renders the pieces
//                    separately so its brand-solid tile can sit
//                    around the mark only.
//
// Static SVG copies of the mark live at `app/icon.svg` and
// `app/apple-icon.svg`; edit the geometry here + mirror it there in
// the same commit.
// ---------------------------------------------------------------------------

/**
 * Bare geometric mark — three nodes at the vertices of an equilateral
 * triangle joined by three edges, the smallest motif that reads
 * unambiguously as "knowledge graph" at favicon scale.
 */
export function BrandMark({ size = 24, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      role="presentation"
      aria-hidden="true"
      className={cn("text-brand-foreground", className)}
    >
      <line x1="12" y1="6" x2="5" y2="18" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      <line x1="12" y1="6" x2="19" y2="18" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      <line x1="5" y1="18" x2="19" y2="18" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      <circle cx="12" cy="6" r="2.6" fill="currentColor" />
      <circle cx="5" cy="18" r="2.6" fill="currentColor" />
      <circle cx="19" cy="18" r="2.6" fill="currentColor" />
    </svg>
  );
}

/**
 * Styled "Ontosyx" wordmark — Geist 600 with tightened tracking. Reads
 * the literal from the i18n catalogue so the same name flows through
 * `<title>`, header chrome, sidebar tile, and OG image.
 */
export function BrandWordmark({
  size = 14,
  className,
}: {
  /** Pixel size of the wordmark's body text. */
  size?: number;
  className?: string;
}) {
  const t = useTranslations("chrome.header");
  return (
    <span
      className={cn(
        "font-semibold tracking-[-0.018em] text-foreground-strong leading-none",
        className,
      )}
      style={{ fontSize: `${size}px` }}
    >
      {t("appTitle")}
    </span>
  );
}

/**
 * Lockup — mark + wordmark, optionally wrapped in a `next/link`
 * anchor. Used by marketing / login / error surfaces. Chrome
 * (sidebar / header) renders [`BrandMark`] and [`BrandWordmark`]
 * separately so a brand-solid tile can frame the mark alone.
 */
export function BrandLogo({
  href,
  wordmark = true,
  size = 18,
  className,
  ariaLabel,
}: {
  href?: string;
  wordmark?: boolean;
  size?: number;
  className?: string;
  ariaLabel?: string;
}) {
  const t = useTranslations("chrome.header");
  const name = t("appTitle");
  const content = (
    <span
      className={cn(
        "inline-flex items-center gap-2 leading-none",
        className,
      )}
    >
      <BrandMark size={size} />
      {wordmark && <BrandWordmark size={Math.round(size * 0.85)} />}
    </span>
  );

  if (href) {
    return (
      <Link
        href={href}
        aria-label={ariaLabel ?? name}
        className="rounded-sm outline-none transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:opacity-80 focus-visible:ring-2 focus-visible:ring-brand-foreground/40 focus-visible:ring-offset-1"
      >
        {content}
      </Link>
    );
  }
  return content;
}
