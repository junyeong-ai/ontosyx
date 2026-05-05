// Locale → text direction.
//
// Today's bundles are `ko` and `en`, both LTR. The lookup is
// here so future locales (Arabic, Hebrew, Persian, Urdu) flip
// `dir="rtl"` on `<html>` automatically when their bundle lands —
// every CSS rule that uses logical properties (`start` / `end`,
// `inline-start` / `inline-end`) flips on the same toggle without
// per-component changes.
//
// The lookup is intentionally a closed map. A locale that arrives
// without an explicit row falls through to `ltr`, which is correct
// for every locale outside the small RTL set; if a future RTL
// locale is missed, the bug surfaces as wrong-direction text and
// gets caught by the locale-rollout review rather than silently
// rendering RTL content as LTR.

export type TextDirection = "ltr" | "rtl";

const RTL_LOCALES: ReadonlySet<string> = new Set([
  "ar",
  "fa",
  "he",
  "ur",
  // Add the BCP-47 region-tagged forms eagerly so callers can pass
  // `ar-EG`, `he-IL`, etc. without re-deriving the base subtag.
  "ar-EG",
  "ar-SA",
  "ar-AE",
  "fa-IR",
  "he-IL",
  "ur-PK",
]);

/**
 * Resolve the text direction for a locale tag. Accepts BCP-47
 * shapes (`en`, `en-US`, `ar-EG`); returns `ltr` for unknown tags
 * because every workspace-shipped locale today is LTR and the
 * fallback should not destabilise rendering.
 */
export function directionForLocale(locale: string): TextDirection {
  if (RTL_LOCALES.has(locale)) return "rtl";
  // Try the base subtag — `ar-EG` falls through to `ar`.
  const base = locale.split("-")[0];
  if (RTL_LOCALES.has(base)) return "rtl";
  return "ltr";
}
