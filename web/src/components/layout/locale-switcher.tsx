"use client";

import { useLocale, useTranslations } from "next-intl";
import { useTransition } from "react";

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
 * Rendered inside the user menu popover. Small footprint so it can sit
 * beside account actions without crowding them.
 */
export function LocaleSwitcher() {
  const current = useLocale() as Locale;
  const t = useTranslations("locale");
  const [isPending, startTransition] = useTransition();

  const handleChange = (next: Locale) => {
    if (next === current) return;
    startTransition(async () => {
      await setLocaleAction(next);
      // The server action revalidates the root layout, but the client
      // router cache also needs to drop — a hard reload is the
      // least-surprising way to swap every text node at once.
      window.location.reload();
    });
  };

  return (
    <div className="px-3 py-2">
      <p className="mb-1 text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
        {t("switcher")}
      </p>
      <div className="flex gap-1">
        {SUPPORTED_LOCALES.map((loc) => {
          const active = loc === current;
          return (
            <button
              key={loc}
              type="button"
              disabled={isPending}
              aria-pressed={active}
              onClick={() => handleChange(loc)}
              className={`flex-1 rounded-md border px-2 py-1 text-xs transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
                active
                  ? "border-brand-foreground bg-brand-surface text-brand-foreground-strong"
                  : "border-divider bg-surface-base text-foreground hover:bg-surface-raised"
              } disabled:opacity-50`}
            >
              {LOCALE_LABELS[loc]}
            </button>
          );
        })}
      </div>
    </div>
  );
}
