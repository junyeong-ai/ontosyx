"use client";

import { useTranslations } from "next-intl";
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
          className="mb-1 block text-xs font-medium text-zinc-700 dark:text-zinc-300"
        >
          {t("notesLabel")}
        </label>
        <textarea
          id="mapping-notes"
          rows={8}
          value={state.mappingNotes}
          onChange={(e) => update({ mappingNotes: e.target.value })}
          placeholder={t("notesPlaceholder")}
          className="w-full rounded border border-zinc-300 bg-white px-3 py-2 text-xs dark:border-zinc-600 dark:bg-zinc-900"
        />
        <p className="mt-1 text-[11px] text-muted-foreground">{t("notesHint")}</p>
      </div>
    </StepShell>
  );
}
