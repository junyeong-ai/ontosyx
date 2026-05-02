"use client";

// Shared step chrome — title + subtitle + child form + footer row
// (Back / Skip / Next). Each step page owns its form fields; the
// shell handles navigation + marking the step complete in shared
// state.

import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useCallback } from "react";

import { useBootstrap } from "./bootstrap-state";

export interface StepShellProps {
  stepKey: string;
  /** Route to jump to when the user clicks Next. `null` means the
   * final "Finish" action, which fires `onFinish` instead. */
  nextPath: string | null;
  backPath?: string;
  /** `true` when the step's inputs satisfy the minimum bar for
   * pressing Next (not Skip). Skip is always available to support
   * the "re-entry later" requirement. */
  canAdvance: boolean;
  /** Called on the last step when the user commits — typically
   * navigates to the Design workbench with the project id. */
  onFinish?: () => void;
  children: React.ReactNode;
  title: string;
  subtitle: string;
}

export function StepShell(props: StepShellProps) {
  const { stepKey, nextPath, backPath, canAdvance, onFinish, children, title, subtitle } =
    props;
  const router = useRouter();
  const t = useTranslations("bootstrap.step");
  const { markComplete } = useBootstrap();

  const handleNext = useCallback(() => {
    markComplete(stepKey);
    if (nextPath) {
      router.push(nextPath);
    } else {
      onFinish?.();
    }
  }, [markComplete, nextPath, onFinish, router, stepKey]);

  const handleSkip = useCallback(() => {
    if (nextPath) {
      router.push(nextPath);
    }
  }, [nextPath, router]);

  return (
    <section>
      <header className="mb-6">
        <h2 className="text-lg font-semibold text-foreground-strong">
          {title}
        </h2>
        <p className="mt-1 text-xs text-muted-foreground">{subtitle}</p>
      </header>

      <div className="space-y-4">{children}</div>

      <footer className="mt-8 flex items-center justify-between border-t border-divider pt-4">
        <button
          type="button"
          onClick={() => (backPath ? router.push(backPath) : router.back())}
          className="rounded px-3 py-1.5 text-xs text-muted-foreground hover:bg-surface-inset"
        >
          {t("back")}
        </button>
        <div className="flex items-center gap-2">
          {nextPath && (
            <button
              type="button"
              onClick={handleSkip}
              className="rounded px-3 py-1.5 text-xs text-muted-foreground hover:bg-surface-inset"
            >
              {t("skip")}
            </button>
          )}
          <button
            type="button"
            onClick={handleNext}
            disabled={!canAdvance && !!nextPath}
            className="rounded bg-concept-foreground px-3 py-1.5 text-xs font-medium text-white hover:bg-concept-foreground disabled:opacity-50"
          >
            {nextPath ? t("next") : t("finish")}
          </button>
        </div>
      </footer>
    </section>
  );
}
