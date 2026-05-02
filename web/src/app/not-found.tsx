/**
 * Root 404 page for the App Router.
 *
 * Rendered when Next.js cannot match a route, or when a component calls
 * `notFound()`. All copy is locale-aware via next-intl.
 */

import Link from "next/link";
import { getTranslations } from "next-intl/server";
import { HugeiconsIcon } from "@hugeicons/react";
import { Home01Icon, HelpCircleIcon, Search01Icon } from "@hugeicons/core-free-icons";

export default async function NotFound() {
  const t = await getTranslations("notFound");

  return (
    <main
      id="main"
      className="flex min-h-dvh items-center justify-center bg-surface-raised px-4"
    >
      <div className="w-full max-w-md text-center">
        <p className="text-xs font-semibold uppercase tracking-widest text-brand-foreground">
          {t("code")}
        </p>
        <h1 className="mt-2 text-2xl font-semibold text-foreground-strong">
          {t("title")}
        </h1>
        <p className="mt-2 text-sm text-foreground dark:text-muted-foreground">
          {t("description")}
        </p>
        <p className="mt-1 text-xs text-muted-foreground">{t("hint")}</p>

        <div className="mt-8 flex flex-wrap items-center justify-center gap-2">
          <Link
            href="/"
            aria-label={t("homeAria")}
            className="inline-flex items-center gap-1.5 rounded-md bg-brand-solid px-4 py-2 text-xs font-medium text-white transition-colors hover:bg-brand-solid"
          >
            <HugeiconsIcon icon={Home01Icon} className="h-3.5 w-3.5" size="100%" />
            {t("home")}
          </Link>
          <Link
            href="/?onboarding=1"
            aria-label={t("getStartedAria")}
            className="inline-flex items-center gap-1.5 rounded-md border border-divider bg-surface-base px-4 py-2 text-xs font-medium text-foreground transition-colors hover:bg-surface-raised-muted"
          >
            <HugeiconsIcon icon={HelpCircleIcon} className="h-3.5 w-3.5" size="100%" />
            {t("getStarted")}
          </Link>
        </div>

        <p className="mt-8 flex items-center justify-center gap-1.5 text-2xs text-muted-foreground">
          <HugeiconsIcon icon={Search01Icon} className="h-3 w-3" size="100%" />
          <span>
            {t("searchTipPrefix")}
            <kbd className="mx-1 rounded border border-divider bg-surface-base px-1 py-0.5 font-mono text-2xs">
              ⌘K
            </kbd>
            {t("searchTipSuffix")}
          </span>
        </p>
      </div>
    </main>
  );
}
