"use client";

// Final step — render a summary card and let the operator either
// "Finish" (navigate to the design workbench with their collected
// intent, which the workbench can seed from localStorage) or restart.
// The wizard does NOT persist to backend yet — the workbench picks
// up the localStorage state and wires it into the first project
// creation call. That keeps the wizard fully client-side and
// restart-safe.

import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useMemo } from "react";
import { toast } from "sonner";

import { useBootstrap } from "../bootstrap-state";
import { StepShell } from "../step-shell";

export default function ValidateStep() {
  const t = useTranslations("bootstrap.step6");
  const router = useRouter();
  const { state, reset, markComplete } = useBootstrap();

  const glossaryCount = useMemo(
    () => state.glossaryDraft.split("\n").filter((l) => l.trim().length > 0).length,
    [state.glossaryDraft],
  );

  const ruleCount = useMemo(
    () => state.rulesDraft.split("\n").filter((l) => l.trim().length > 0).length,
    [state.rulesDraft],
  );

  const onFinish = () => {
    markComplete("6-validate");
    toast.success(t("toast.finished", { name: state.pilotName || "(unnamed)" }));
    // Land on the design workbench — the /(workbench) segment reads
    // the bootstrap state from localStorage if present.
    router.push("/design");
  };

  const handleRestart = () => {
    reset();
    router.push("/bootstrap/1-pilot");
  };

  return (
    <StepShell
      stepKey="6-validate"
      nextPath={null}
      backPath="/bootstrap/5-map"
      canAdvance
      onFinish={onFinish}
      title={t("title")}
      subtitle={t("subtitle")}
    >
      <div className="rounded-lg border border-violet-200 bg-violet-50 p-5 dark:border-violet-900/50 dark:bg-violet-950/30">
        <h3 className="mb-3 text-sm font-semibold text-violet-900 dark:text-violet-200">
          {t("summary.title", { name: state.pilotName || t("summary.unnamed") })}
        </h3>
        <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
          <SummaryRow
            label={t("summary.scopeLabel")}
            value={state.pilotScope || t("summary.notSet")}
          />
          <SummaryRow
            label={t("summary.sourceLabel")}
            value={state.sourceKind || t("summary.notSet")}
          />
          <SummaryRow
            label={t("summary.glossaryLabel")}
            value={t("summary.count", { n: glossaryCount })}
          />
          <SummaryRow
            label={t("summary.rulesLabel")}
            value={t("summary.count", { n: ruleCount })}
          />
        </dl>
        <p className="mt-4 text-[11px] text-violet-700 dark:text-violet-300">
          {t("summary.nextStepHint")}
        </p>
      </div>

      <div className="mt-4 flex items-center gap-2 text-xs">
        <button
          type="button"
          onClick={handleRestart}
          className="rounded px-3 py-1.5 text-muted-foreground hover:bg-zinc-100 dark:hover:bg-zinc-800"
        >
          {t("restart")}
        </button>
      </div>
    </StepShell>
  );
}

function SummaryRow({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt className="font-medium text-violet-900 dark:text-violet-200">{label}</dt>
      <dd className="text-violet-700 dark:text-violet-300">{value}</dd>
    </>
  );
}
