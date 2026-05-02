"use client";

import { useTranslations } from "next-intl";
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
        <input
          id="pilot-name"
          value={state.pilotName}
          onChange={(e) => update({ pilotName: e.target.value })}
          placeholder={t("namePlaceholder")}
          className="w-full rounded border border-divider bg-surface-base px-3 py-2 text-sm"
        />
      </div>

      <div>
        <label
          htmlFor="pilot-scope"
          className="mb-1 block text-xs font-medium text-foreground"
        >
          {t("scopeLabel")}
        </label>
        <textarea
          id="pilot-scope"
          rows={4}
          value={state.pilotScope}
          onChange={(e) => update({ pilotScope: e.target.value })}
          placeholder={t("scopePlaceholder")}
          className="w-full rounded border border-divider bg-surface-base px-3 py-2 text-sm"
        />
        <p className="mt-1 text-[11px] text-muted-foreground">{t("scopeHint")}</p>
      </div>
    </StepShell>
  );
}
