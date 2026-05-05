"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { Link04Icon, Home01Icon } from "@hugeicons/core-free-icons";

export default function Expired() {
  const t = useTranslations("page.sharedDashboard");
  return (
    <main id="main" className="flex min-h-dvh items-center justify-center bg-surface-raised px-4">
      <div className="w-full max-w-md rounded-xl border border-divider bg-surface-base p-6 text-center shadow-1">
        <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-full bg-warning-surface/30">
          <HugeiconsIcon
            icon={Link04Icon}
            className="h-5 w-5 text-warning-foreground"
            size="100%"
          />
        </div>
        <h1 className="mt-4 text-base font-semibold text-foreground-strong">
          {t("expiredTitle")}
        </h1>
        <p className="mt-1 text-xs text-foreground-muted">
          {t("expiredSubtitle")}
        </p>

        <p className="mt-4 text-sm text-foreground">
          {t("expiredBody")}
        </p>

        <div className="mt-6 flex flex-wrap items-center justify-center gap-2">
          <Link
            href="/"
            className="inline-flex items-center gap-1.5 rounded-md border border-divider bg-surface-base px-4 py-2 text-xs font-medium text-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-raised-muted"
          >
            <HugeiconsIcon
              icon={Home01Icon}
              className="h-3.5 w-3.5"
              size="100%"
            />
            {t("home")}
          </Link>
        </div>
      </div>
    </main>
  );
}
