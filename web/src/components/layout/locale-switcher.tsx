"use client";

import { useLocale, useTranslations } from "next-intl";
import { useId, useTransition } from "react";

import { FormSelect } from "@/components/ui/form-input";
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
 * Uses a compact `<select>` so the dropdown scales to any number of
 * locales — adding a new entry to `SUPPORTED_LOCALES` lights it up
 * here without rewriting layout. Inline button groups would visibly
 * crowd the popover at 4+ locales.
 */
export function LocaleSwitcher() {
  const current = useLocale() as Locale;
  const t = useTranslations("locale");
  const [isPending, startTransition] = useTransition();
  const labelId = useId();

  const handleChange = (next: Locale) => {
    if (next === current) return;
    startTransition(async () => {
      await setLocaleAction(next);
      // Server action revalidates the layout; the client router
      // cache also drops with a hard reload — least-surprising way
      // to swap every text node at once.
      window.location.reload();
    });
  };

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
        value={current}
        disabled={isPending}
        onChange={(e) => handleChange(e.target.value as Locale)}
      >
        {SUPPORTED_LOCALES.map((loc) => (
          <option key={loc} value={loc}>
            {LOCALE_LABELS[loc]}
          </option>
        ))}
      </FormSelect>
    </div>
  );
}
