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
    <div className="flex min-h-dvh items-center justify-center bg-zinc-50 px-4 dark:bg-zinc-950">
      <div className="w-full max-w-md text-center">
        <p className="text-xs font-semibold uppercase tracking-widest text-emerald-500">
          {t("code")}
        </p>
        <h1 className="mt-2 text-2xl font-semibold text-zinc-900 dark:text-zinc-100">
          {t("title")}
        </h1>
        <p className="mt-2 text-sm text-zinc-600 dark:text-muted-foreground">
          {t("description")}
        </p>
        <p className="mt-1 text-xs text-muted-foreground">{t("hint")}</p>

        <div className="mt-8 flex flex-wrap items-center justify-center gap-2">
          <Link
            href="/"
            aria-label={t("homeAria")}
            className="inline-flex items-center gap-1.5 rounded-md bg-emerald-600 px-4 py-2 text-xs font-medium text-white transition-colors hover:bg-emerald-700"
          >
            <HugeiconsIcon icon={Home01Icon} className="h-3.5 w-3.5" size="100%" />
            {t("home")}
          </Link>
          <Link
            href="/?onboarding=1"
            aria-label={t("getStartedAria")}
            className="inline-flex items-center gap-1.5 rounded-md border border-zinc-200 bg-white px-4 py-2 text-xs font-medium text-zinc-700 transition-colors hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800"
          >
            <HugeiconsIcon icon={HelpCircleIcon} className="h-3.5 w-3.5" size="100%" />
            {t("getStarted")}
          </Link>
        </div>

        <p className="mt-8 flex items-center justify-center gap-1.5 text-[10px] text-muted-foreground">
          <HugeiconsIcon icon={Search01Icon} className="h-3 w-3" size="100%" />
          <span>
            {t("searchTipPrefix")}
            <kbd className="mx-1 rounded border border-zinc-200 bg-white px-1 py-0.5 font-mono text-[9px] dark:border-zinc-700 dark:bg-zinc-900">
              ⌘K
            </kbd>
            {t("searchTipSuffix")}
          </span>
        </p>
      </div>
    </div>
  );
}
