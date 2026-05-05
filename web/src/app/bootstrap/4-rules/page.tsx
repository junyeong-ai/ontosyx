"use client";

import { useTranslations } from "next-intl";
import { FormTextarea } from "@/components/ui/form-input";
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
        <FormTextarea
          id="rules-draft"
          rows={8}
          value={state.rulesDraft}
          onChange={(e) => update({ rulesDraft: e.target.value })}
          placeholder={t("draftPlaceholder")}
          className="font-mono"
        />
        <p className="mt-1 text-2xs text-foreground-muted">{t("draftHint")}</p>
      </div>
    </StepShell>
  );
}
