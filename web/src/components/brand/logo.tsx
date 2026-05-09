import Link from "next/link";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";

// ---------------------------------------------------------------------------
// BrandLogo — single source of truth for the Ontosyx wordmark + mark.
// ---------------------------------------------------------------------------
//
// The mark is a triangle of three nodes joined by edges — the smallest
// motif that reads unambiguously as "knowledge graph" at favicon scale.
// Stroke + fill use `currentColor` so the same SVG retypes from
// `text-brand-foreground` (chrome) to `text-foreground-onbrand` (when
// laid over a brand-solid surface) without separate variants.
//
// The wordmark sits in the active sans-serif stack (Geist on Latin,
// Pretendard / Noto Sans KR on Korean) at semibold 600 with tightened
// tracking. Header chrome is not a content heading — `BrandLogo` never
// renders an `<h1>`; routes own their own page heading.
//
// Static SVG copies of the mark live next to this file at
// `app/icon.svg` and `app/apple-icon.svg`. Edit the geometry here +
// mirror it there in the same commit; the parity test in
// `__tests__/brand-asset-parity.test.tsx` pins the four anchor points
// so a one-sided drift fails the FE gate.
// ---------------------------------------------------------------------------

export interface BrandLogoProps {
  /**
   * Render as a `next/link` anchor pointing at the workspace home.
   * Pass `undefined` to render the mark + wordmark as raw inline
   * content (login splash, error pages, marketing surfaces).
   */
  href?: string;
  /**
   * Show the "Ontosyx" wordmark next to the mark. Defaults to `true`.
   * Pass `false` for compact surfaces (mobile chrome, breadcrumbs).
   */
  wordmark?: boolean;
  /**
   * Pixel side of the mark. The wordmark scales against this value
   * so the lockup proportions stay constant across sizes.
   */
  size?: number;
  className?: string;
  /**
   * Accessible label when rendered as a link. Pure decorative
   * mark + wordmark inline content does not need this — the
   * surrounding chrome already names the destination.
   */
  ariaLabel?: string;
}

export function BrandLogo({
  href,
  wordmark = true,
  size = 18,
  className,
  ariaLabel,
}: BrandLogoProps) {
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
      {wordmark && (
        <span
          className="font-semibold tracking-[-0.018em] text-foreground-strong"
          style={{ fontSize: `${size * 0.85}px` }}
        >
          {name}
        </span>
      )}
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

/**
 * Bare geometric mark — three nodes at the vertices of an equilateral
 * triangle joined by three edges. `currentColor` retypes through the
 * caller's text colour. Exported for surfaces that want the icon
 * alone (e.g. spinner adornment, empty-state accent).
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
      {/* Edges first so the nodes draw on top — the join reads as
          a node-on-line interruption, not a stroke-cap collision. */}
      <line x1="12" y1="6" x2="5" y2="18" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      <line x1="12" y1="6" x2="19" y2="18" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      <line x1="5" y1="18" x2="19" y2="18" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      <circle cx="12" cy="6" r="2.6" fill="currentColor" />
      <circle cx="5" cy="18" r="2.6" fill="currentColor" />
      <circle cx="19" cy="18" r="2.6" fill="currentColor" />
    </svg>
  );
}
