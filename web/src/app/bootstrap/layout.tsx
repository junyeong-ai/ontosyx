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
      <div className="flex min-h-screen bg-zinc-50 dark:bg-zinc-950">
        <aside className="flex w-64 flex-col border-r border-zinc-200 bg-white px-5 py-6 dark:border-zinc-800 dark:bg-zinc-950">
          <Link
            href="/"
            className="mb-6 text-xs font-medium text-muted-foreground hover:text-zinc-700 dark:hover:text-zinc-300"
          >
            ← {t("exit")}
          </Link>

          <h1 className="mb-1 text-sm font-semibold text-zinc-900 dark:text-zinc-100">
            {t("title")}
          </h1>
          <p className="mb-6 text-xs text-muted-foreground">
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
                        ? "bg-violet-50 text-violet-700 dark:bg-violet-950/40 dark:text-violet-300"
                        : done
                          ? "text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
                          : "text-muted-foreground hover:bg-zinc-50 dark:hover:bg-zinc-900"
                    }`}
                  >
                    <span
                      aria-hidden
                      className={`mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border text-[10px] font-semibold ${
                        done
                          ? "border-emerald-500 bg-emerald-500 text-white"
                          : active
                            ? "border-violet-500 bg-violet-500 text-white"
                            : "border-zinc-300 text-muted-foreground dark:border-zinc-700"
                      }`}
                    >
                      {done ? "✓" : idx + 1}
                    </span>
                    <div className="min-w-0">
                      <p className="font-medium">
                        {t(`steps.${step.key}.title`)}
                      </p>
                      <p className="mt-0.5 text-[10px] text-muted-foreground">
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
            className="mt-auto rounded px-2 py-1.5 text-[11px] text-muted-foreground hover:bg-zinc-100 dark:hover:bg-zinc-800"
          >
            {t("saveExit")}
          </button>
        </aside>

        <main id="main" className="flex flex-1 flex-col overflow-auto">
          <div className="mx-auto w-full max-w-3xl px-8 py-10">{children}</div>
        </main>
      </div>
    </BootstrapProvider>
  );
}
