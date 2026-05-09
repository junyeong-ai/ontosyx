"use client";

import { useId, useTransition } from "react";
import { useLocale, useTranslations } from "next-intl";

import { Select, SelectOption } from "@/components/ui/select";
import {
  LOCALE_LABELS,
  SUPPORTED_LOCALES,
  type Locale,
} from "@/i18n/config";
import { setLocaleAction } from "@/i18n/actions";

/**
 * Locale switcher — flips the `ontosyx_locale` cookie via a server
 * action and lets next.js re-render the tree with the new messages.
 *
 * Built on the design-system `Select` (Base UI Popover) so the menu
 * carries the same brand chrome as every other dropdown — native
 * `<select>` would render the OS popup at this scope, breaking the
 * dark-mode / brand-color register. Forms keep `<select>` for mobile
 * platform conventions; chrome dropdowns like this one go through the
 * tokenised primitive.
 */
export function LocaleSwitcher() {
  const current = useLocale() as Locale;
  const t = useTranslations("locale");
  const [isPending, startTransition] = useTransition();
  const labelId = useId();

  const handleChange = (next: string | null) => {
    if (!next || next === current) return;
    startTransition(async () => {
      await setLocaleAction(next as Locale);
      // Server action revalidates the layout; a hard reload also
      // drops the client router cache so every text node swaps in
      // a single paint.
      window.location.reload();
    });
  };

  return (
    <div className="px-3 py-2">
      <span
        id={labelId}
        className="mb-1 block text-2xs font-semibold uppercase tracking-wider text-foreground-muted"
      >
        {t("switcher")}
      </span>
      <Select
        value={current}
        onValueChange={handleChange}
        disabled={isPending}
        ariaLabelledBy={labelId}
      >
        {SUPPORTED_LOCALES.map((loc) => (
          <SelectOption key={loc} value={loc}>
            {LOCALE_LABELS[loc]}
          </SelectOption>
        ))}
      </Select>
    </div>
  );
}
