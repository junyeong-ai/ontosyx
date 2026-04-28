"use client";

import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";

/**
 * Phase indicator for the design lifecycle.
 *
 * Renders the same `analyze → design → complete` progress strip
 * everywhere the operator needs to see "where am I in the flow".
 * The pre-project state (no project loaded yet, just a create form)
 * sits at index `-1` — every step is rendered as pending and the
 * connector lines stay grey until step 0 ("analyze") becomes
 * active. After a project exists, the indicator follows
 * `DesignProject.status` via the integer in `currentStepIndex`.
 *
 * Stepper labels live in `workbench.bottomPanel.workflow.{step*}`
 * so they pick up the same translations the project-workflow panel
 * already uses — adding a new lifecycle step means one constant +
 * one i18n key, not a parallel copy in every consumer.
 */
const STEPS = ["analyze", "design", "complete"] as const;
type StepId = (typeof STEPS)[number];

const LABEL_KEY: Record<StepId, "stepAnalyze" | "stepDesign" | "stepComplete"> = {
  analyze: "stepAnalyze",
  design: "stepDesign",
  complete: "stepComplete",
};

export function PhaseStepper({
  currentStepIndex,
  className,
}: {
  /**
   * `-1` for the pre-project state (every step rendered as pending);
   * `0` analyze, `1` design, `2` complete.
   */
  currentStepIndex: number;
  className?: string;
}) {
  const t = useTranslations("workbench.bottomPanel.workflow");

  return (
    <div className={cn("flex items-center justify-between px-2", className)}>
      {STEPS.map((step, i) => (
        <div key={step} className="flex items-center">
          <div className="flex flex-col items-center gap-1">
            <div
              className={cn(
                "flex h-5 w-5 items-center justify-center rounded-full text-[9px] font-bold",
                i <= currentStepIndex
                  ? "bg-emerald-500 text-white"
                  : "bg-zinc-200 text-muted-foreground dark:bg-zinc-700",
              )}
            >
              {i + 1}
            </div>
            <span
              className={cn(
                "text-[9px] font-medium capitalize",
                i <= currentStepIndex
                  ? "text-emerald-600 dark:text-emerald-400"
                  : "text-muted-foreground",
              )}
            >
              {t(LABEL_KEY[step])}
            </span>
          </div>
          {i < STEPS.length - 1 && (
            <div
              className={cn(
                "mx-2 h-px w-8",
                i < currentStepIndex
                  ? "bg-emerald-400"
                  : "bg-zinc-200 dark:bg-zinc-700",
              )}
            />
          )}
        </div>
      ))}
    </div>
  );
}
