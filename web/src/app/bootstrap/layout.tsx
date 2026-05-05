"use client";

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useCallback } from "react";

import { BootstrapProvider } from "./bootstrap-state";

const STEPS = [
  { key: "1-pilot", path: "/bootstrap/1-pilot" },
  { key: "2-source", path: "/bootstrap/2-source" },
  { key: "3-glossary", path: "/bootstrap/3-glossary" },
  { key: "4-rules", path: "/bootstrap/4-rules" },
  { key: "5-map", path: "/bootstrap/5-map" },
  { key: "6-validate", path: "/bootstrap/6-validate" },
] as const;

export default function BootstrapLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const t = useTranslations("bootstrap.sidebar");
  const pathname = usePathname();
  const router = useRouter();
  const currentIdx = STEPS.findIndex((s) => pathname?.startsWith(s.path));

  const handleExit = useCallback(() => {
    router.push("/");
  }, [router]);

  return (
    <BootstrapProvider>
      <div className="flex min-h-screen bg-surface-raised">
        <aside className="flex w-64 flex-col border-e border-divider bg-surface-base px-5 py-6">
          <Link
            href="/"
            className="mb-6 text-xs font-medium text-foreground-muted hover:text-foreground-muted"
          >
            ← {t("exit")}
          </Link>

          <h1 className="mb-1 text-base font-semibold text-foreground-strong">
            {t("title")}
          </h1>
          <p className="mb-6 text-xs text-foreground-muted">
            {t("subtitle")}
          </p>

          <ol className="flex flex-col gap-1 text-xs">
            {STEPS.map((step, idx) => {
              const done = idx < currentIdx;
              const active = idx === currentIdx;
              return (
                <li key={step.key}>
                  <Link
                    href={step.path}
                    className={`flex items-start gap-3 rounded px-2 py-2 ${
                      active
                        ? "bg-concept-surface text-concept-foreground"
                        : done
                          ? "text-foreground hover:bg-surface-inset"
                          : "text-foreground-muted hover:bg-surface-raised"
                    }`}
                  >
                    <span
                      aria-hidden="true"
                      className={`mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border text-2xs font-semibold ${
                        done
                          ? "border-brand-foreground bg-brand-solid text-foreground-onbrand"
                          : active
                            ? "border-concept-foreground bg-concept-foreground text-foreground-onbrand"
                            : "border-divider text-foreground-muted"
                      }`}
                    >
                      {done ? "✓" : idx + 1}
                    </span>
                    <div className="min-w-0">
                      <p className="font-medium">
                        {t(`steps.${step.key}.title`)}
                      </p>
                      <p className="mt-0.5 text-2xs text-foreground-muted">
                        {t(`steps.${step.key}.summary`)}
                      </p>
                    </div>
                  </Link>
                </li>
              );
            })}
          </ol>

          <button
            type="button"
            onClick={handleExit}
            className="mt-auto rounded px-2 py-1.5 text-2xs text-foreground-muted hover:bg-surface-inset"
          >
            {t("saveExit")}
          </button>
        </aside>

        <main
          id="main"
          tabIndex={0}
          className="flex flex-1 flex-col overflow-auto outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/50 focus-visible:ring-inset"
        >
          <div className="mx-auto w-full max-w-3xl px-8 py-10">{children}</div>
        </main>
      </div>
    </BootstrapProvider>
  );
}
