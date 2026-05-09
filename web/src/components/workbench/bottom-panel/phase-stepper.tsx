"use client";

import { Fragment } from "react";
import { useTranslations } from "next-intl";
import { Check } from "lucide-react";
import { cn } from "@/lib/cn";

/**
 * Phase indicator for the design lifecycle.
 *
 * Renders the same `analyze → design → complete` progress strip
 * everywhere the operator needs to see "where am I in the flow".
 * The pre-project state (no project loaded yet, just a create form)
 * sits at index `-1` — every step is rendered as pending and the
 * connector tracks stay grey until step 0 ("analyze") becomes
 * active. After a project exists, the indicator follows
 * `OntologyDraft.status` via the integer in `currentStepIndex`.
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
    <ol
      className={cn("flex items-start px-2", className)}
      aria-label={t("phaseAria")}
    >
      {STEPS.map((step, i) => {
        const isActive = i === currentStepIndex;
        const isComplete = i < currentStepIndex;
        const isPending = i > currentStepIndex;
        return (
          <Fragment key={step}>
            {i > 0 && (
              <Connector active={i <= currentStepIndex} />
            )}
            <li
              className="flex flex-col items-center gap-1"
              aria-current={isActive ? "step" : undefined}
            >
              <div
                className={cn(
                  "flex h-6 w-6 items-center justify-center rounded-full text-2xs font-semibold transition-colors duration-[var(--duration-quick)]",
                  isActive &&
                    "bg-brand-solid text-foreground-onbrand ring-2 ring-brand-foreground/30 ring-offset-2 ring-offset-surface-base",
                  isComplete && "bg-brand-solid text-foreground-onbrand",
                  isPending && "bg-surface-inset text-foreground-muted",
                )}
              >
                {isComplete ? <CheckGlyph /> : i + 1}
              </div>
              <span
                className={cn(
                  "text-2xs font-medium",
                  i <= currentStepIndex
                    ? "text-brand-foreground"
                    : "text-foreground-muted",
                )}
              >
                {t(LABEL_KEY[step])}
              </span>
            </li>
          </Fragment>
        );
      })}
    </ol>
  );
}

function Connector({ active }: { active: boolean }) {
  return (
    <div
      aria-hidden="true"
      className={cn(
        // The connector sits at the circle's vertical centre. Step
        // column is `gap-1` (4px) between circle (24px) and label,
        // so 12px from the column top puts the connector through the
        // middle of the dot.
        "mt-3 h-0.5 flex-1 transition-colors duration-[var(--duration-quick)]",
        active ? "bg-brand-solid" : "bg-surface-inset",
      )}
    />
  );
}

function CheckGlyph() {
  return (
    <Check className="h-3 w-3" strokeWidth={1.5} />
  );
}
