"use client";

import { useId } from "react";
import { useTranslations } from "next-intl";

import { FormSelect } from "@/components/ui/form-input";
import { useThemePreference, type ThemePreference } from "@/hooks/use-theme";

const PREFERENCES: readonly ThemePreference[] = ["system", "light", "dark"];

/**
 * Theme preference switcher — chrome-mounted control that lets the
 * operator override the OS color scheme. Lives next to the locale
 * switcher in the user-menu popover so the two preferences read as
 * a single "personalisation" panel.
 *
 * Matches `LocaleSwitcher` in shape — compact `<select>` with an
 * uppercase eyebrow label — so adding more chrome preferences
 * (timezone, density, motion) lands in the same visual register.
 */
export function ThemeSwitcher() {
  const t = useTranslations("theme");
  const { preference, setPreference } = useThemePreference();
  const labelId = useId();

  return (
    <div className="px-3 py-2">
      <label
        id={labelId}
        htmlFor={`${labelId}-select`}
        className="mb-1 block text-2xs font-semibold uppercase tracking-wider text-foreground-muted"
      >
        {t("switcher")}
      </label>
      <FormSelect
        id={`${labelId}-select`}
        density="compact"
        aria-labelledby={labelId}
        value={preference}
        onChange={(e) => setPreference(e.target.value as ThemePreference)}
      >
        {PREFERENCES.map((pref) => (
          <option key={pref} value={pref}>
            {t(`option.${pref}`)}
          </option>
        ))}
      </FormSelect>
    </div>
  );
}
