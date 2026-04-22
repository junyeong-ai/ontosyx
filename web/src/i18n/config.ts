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
