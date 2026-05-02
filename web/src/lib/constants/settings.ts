/**
 * Settings page width policy — opt-out narrow.
 *
 * Default: `max-w-7xl` (1280px). Generous on wide monitors, comfortable on
 * laptops. Most settings pages — lists, tables, dashboards, multi-column
 * grids — benefit from breathing room and would feel cramped at 768px.
 *
 * Narrow opt-out: pure-form pages (single column of inputs) keep
 * `max-w-3xl` (768px) so input rows stay within the comfortable
 * 50–75 character reading width that web typography research
 * (Bringhurst, Nielsen) treats as the form-readability sweet spot.
 *
 * To opt a new page into narrow, add its pathname here. To opt out,
 * remove it. New data-display pages (the common case) automatically
 * inherit the wide default — no list maintenance needed.
 *
 * Industry reference: Vercel / Linear / Stripe / GitHub all use
 * wide-by-default for settings with narrow forms as the exception.
 */
export const NARROW_SETTINGS_PAGES = new Set([
  "/settings/profile",
  "/settings/workspace",
  "/settings/system",
  "/settings/prompts",
  "/settings/providers",
]);

/**
 * @deprecated Kept for any external consumer; prefer the
 * `NARROW_SETTINGS_PAGES` opt-out above. This Set is now empty
 * because every page is wide by default.
 */
export const WIDE_SETTINGS_PAGES = new Set<string>();
