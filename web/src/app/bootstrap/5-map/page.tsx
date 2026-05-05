"use client";

import { useTranslations } from "next-intl";
import { FormTextarea } from "@/components/ui/form-input";
import { useBootstrap } from "../bootstrap-state";
import { StepShell } from "../step-shell";

export default function MapStep() {
  const t = useTranslations("bootstrap.step5");
  const { state, update } = useBootstrap();

  return (
    <StepShell
      stepKey="5-map"
      nextPath="/bootstrap/6-validate"
      backPath="/bootstrap/4-rules"
      canAdvance
      title={t("title")}
      subtitle={t("subtitle")}
    >
      <div>
        <label
          htmlFor="mapping-notes"
          className="mb-1 block text-xs font-medium text-foreground"
        >
          {t("notesLabel")}
        </label>
        <FormTextarea
          id="mapping-notes"
          rows={8}
          value={state.mappingNotes}
          onChange={(e) => update({ mappingNotes: e.target.value })}
          placeholder={t("notesPlaceholder")}
        />
        <p className="mt-1 text-2xs text-foreground-muted">{t("notesHint")}</p>
      </div>
    </StepShell>
  );
}
