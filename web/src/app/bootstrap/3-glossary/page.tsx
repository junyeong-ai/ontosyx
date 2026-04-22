"use client";

import { useTranslations } from "next-intl";
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
          className="mb-1 block text-xs font-medium text-zinc-700 dark:text-zinc-300"
        >
          {t("draftLabel")}
        </label>
        <textarea
          id="glossary-draft"
          rows={8}
          value={state.glossaryDraft}
          onChange={(e) => update({ glossaryDraft: e.target.value })}
          placeholder={t("draftPlaceholder")}
          className="w-full rounded border border-zinc-300 bg-white px-3 py-2 font-mono text-xs dark:border-zinc-600 dark:bg-zinc-900"
        />
        <p className="mt-1 text-[11px] text-muted-foreground">{t("draftHint")}</p>
      </div>
    </StepShell>
  );
}
