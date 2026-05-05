/**
 * Settings page width policy — opt-out narrow.
 *
 * Default: `max-w-7xl` (1280px). Generous on wide monitors, comfortable
 * on laptops. Lists, tables, dashboards, multi-column grids inherit this.
 *
 * Narrow opt-out: pure single-column form pages keep `max-w-3xl` so input
 * rows stay within the 50–75 character reading width that web typography
 * research treats as the form-readability sweet spot. Match is by prefix:
 * `/settings/workspace/general/foo` is also narrow so a deep-link into a
 * nested form section stays width-consistent with its parent.
 *
 * To opt a new page into narrow, add its pathname here. New data-display
 * pages inherit the wide default automatically — no list maintenance.
 */
export const NARROW_SETTINGS_PAGES = new Set(["/settings/workspace/general"]);

/** Match `pathname` against the narrow set, prefix-aware. */
export function isNarrowSettingsPage(pathname: string): boolean {
  for (const prefix of NARROW_SETTINGS_PAGES) {
    if (pathname === prefix || pathname.startsWith(`${prefix}/`)) {
      return true;
    }
  }
  return false;
}
