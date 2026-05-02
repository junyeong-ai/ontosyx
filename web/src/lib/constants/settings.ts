/**
 * Settings page width policy — opt-out narrow.
 *
 * Default: `max-w-7xl` (1280px). Generous on wide monitors, comfortable
 * on laptops. Lists, tables, dashboards, multi-column grids inherit this.
 *
 * Narrow opt-out: pure single-column form pages keep `max-w-3xl` so input
 * rows stay within the 50–75 character reading width that web typography
 * research treats as the form-readability sweet spot.
 *
 * To opt a new page into narrow, add its pathname here. New data-display
 * pages inherit the wide default automatically — no list maintenance.
 */
export const NARROW_SETTINGS_PAGES = new Set([
  "/settings/profile",
  "/settings/workspace",
  "/settings/system",
]);
