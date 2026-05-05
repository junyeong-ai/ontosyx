"use client";

import { useTranslations } from "next-intl";
import { FormTextarea } from "@/components/ui/form-input";
import { useBootstrap } from "../bootstrap-state";
import { StepShell } from "../step-shell";

export default function GlossaryStep() {
  const t = useTranslations("bootstrap.step3");
  const { state, update } = useBootstrap();

  return (
    <StepShell
      stepKey="3-glossary"
      nextPath="/bootstrap/4-rules"
      backPath="/bootstrap/2-source"
      canAdvance
      title={t("title")}
      subtitle={t("subtitle")}
    >
      <div>
        <label
          htmlFor="glossary-draft"
          className="mb-1 block text-xs font-medium text-foreground"
        >
          {t("draftLabel")}
        </label>
        <FormTextarea
          id="glossary-draft"
          rows={8}
          value={state.glossaryDraft}
          onChange={(e) => update({ glossaryDraft: e.target.value })}
          placeholder={t("draftPlaceholder")}
          className="font-mono"
        />
        <p className="mt-1 text-2xs text-foreground-muted">{t("draftHint")}</p>
      </div>
    </StepShell>
  );
}
