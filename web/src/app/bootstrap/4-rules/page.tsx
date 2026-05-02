"use client";

import { useTranslations } from "next-intl";
import { useBootstrap } from "../bootstrap-state";
import { StepShell } from "../step-shell";

export default function RulesStep() {
  const t = useTranslations("bootstrap.step4");
  const { state, update } = useBootstrap();

  return (
    <StepShell
      stepKey="4-rules"
      nextPath="/bootstrap/5-map"
      backPath="/bootstrap/3-glossary"
      canAdvance
      title={t("title")}
      subtitle={t("subtitle")}
    >
      <div>
        <label
          htmlFor="rules-draft"
          className="mb-1 block text-xs font-medium text-foreground"
        >
          {t("draftLabel")}
        </label>
        <textarea
          id="rules-draft"
          rows={8}
          value={state.rulesDraft}
          onChange={(e) => update({ rulesDraft: e.target.value })}
          placeholder={t("draftPlaceholder")}
          className="w-full rounded border border-divider bg-surface-base px-3 py-2 font-mono text-xs"
        />
        <p className="mt-1 text-[11px] text-muted-foreground">{t("draftHint")}</p>
      </div>
    </StepShell>
  );
}
