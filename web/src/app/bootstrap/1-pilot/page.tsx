"use client";

import { useTranslations } from "next-intl";
import { FormInput, FormTextarea } from "@/components/ui/form-input";
import { useBootstrap } from "../bootstrap-state";
import { StepShell } from "../step-shell";

export default function PilotStep() {
  const t = useTranslations("bootstrap.step1");
  const { state, update } = useBootstrap();

  return (
    <StepShell
      stepKey="1-pilot"
      nextPath="/bootstrap/2-source"
      canAdvance={state.pilotName.trim().length > 0}
      title={t("title")}
      subtitle={t("subtitle")}
    >
      <div>
        <label
          htmlFor="pilot-name"
          className="mb-1 block text-xs font-medium text-foreground"
        >
          {t("nameLabel")}
        </label>
        <FormInput
          id="pilot-name"
          value={state.pilotName}
          onChange={(e) => update({ pilotName: e.target.value })}
          placeholder={t("namePlaceholder")}
        />
      </div>

      <div>
        <label
          htmlFor="pilot-scope"
          className="mb-1 block text-xs font-medium text-foreground"
        >
          {t("scopeLabel")}
        </label>
        <FormTextarea
          id="pilot-scope"
          rows={4}
          value={state.pilotScope}
          onChange={(e) => update({ pilotScope: e.target.value })}
          placeholder={t("scopePlaceholder")}
        />
        <p className="mt-1 text-2xs text-foreground-muted">{t("scopeHint")}</p>
      </div>
    </StepShell>
  );
}
