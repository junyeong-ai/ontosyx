"use client";

import { useId } from "react";
import { useTranslations } from "next-intl";

import { Select, SelectOption } from "@/components/ui/select";
import { useThemePreference, type ThemePreference } from "@/hooks/use-theme";

const PREFERENCES: readonly ThemePreference[] = ["system", "light", "dark"];

/**
 * Theme preference switcher — chrome-mounted control that lets the
 * operator override the OS color scheme. Lives next to the locale
 * switcher in the user-menu popover so the two preferences read as
 * a single "personalisation" panel.
 *
 * Built on the design-system `Select` so the popup matches the
 * platform chrome (shadow / radius / dark-mode pair). Native
 * `<select>` would punch through to OS-styled menus and break the
 * brand register.
 */
export function ThemeSwitcher() {
  const t = useTranslations("theme");
  const { preference, setPreference } = useThemePreference();
  const labelId = useId();

  return (
    <div className="px-3 py-2">
      <span
        id={labelId}
        className="mb-1 block text-2xs font-semibold uppercase tracking-wider text-foreground-muted"
      >
        {t("switcher")}
      </span>
      <Select
        value={preference}
        onValueChange={(v) => v && setPreference(v as ThemePreference)}
        ariaLabelledBy={labelId}
      >
        {PREFERENCES.map((pref) => (
          <SelectOption key={pref} value={pref}>
            {t(`option.${pref}`)}
          </SelectOption>
        ))}
      </Select>
    </div>
  );
}
