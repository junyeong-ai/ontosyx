// ---------------------------------------------------------------------------
// Shared i18n constants
// ---------------------------------------------------------------------------
//
// Kept in a plain module (no `next/headers`, no `server-only`) so both
// server-side `getRequestConfig` and client-side components can import
// the same source of truth for the supported locale set.
// ---------------------------------------------------------------------------

export const SUPPORTED_LOCALES = ["ko", "en"] as const;
export type Locale = (typeof SUPPORTED_LOCALES)[number];

export const DEFAULT_LOCALE: Locale = "ko";

/**
 * Human-readable labels for the locale switcher UI. Kept alongside the
 * locale list so adding a new locale stays a single-file change.
 */
export const LOCALE_LABELS: Record<Locale, string> = {
  ko: "한국어",
  en: "English",
};

/** Cookie key used by `request.ts` and the switcher. Centralised so a
 *  future rename (e.g. for a SaaS rebrand) hits one place. */
export const LOCALE_COOKIE = "ontosyx_locale";

/**
 * Pinned IANA time zone for date / number formatting. Set explicitly so
 * `next-intl` doesn't fall back to the host runtime's TZ on the server
 * (which can differ from the user's browser TZ in deployed envs and
 * dev machines). Hydration mismatches in any `<FormattedDate />` or
 * `Date.toLocaleString()` would otherwise re-render entire subtrees.
 *
 * Korea-first product → KST. If multi-region support lands, store the
 * preferred TZ on the user / workspace and resolve it in
 * `i18n/request.ts` instead of changing this default.
 */
export const DEFAULT_TIME_ZONE = "Asia/Seoul";
