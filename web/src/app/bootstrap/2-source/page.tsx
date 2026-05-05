"use client";

import { useTranslations } from "next-intl";
import { FormInput } from "@/components/ui/form-input";
import { RadioCard } from "@/components/ui/radio";
import { useBootstrap } from "../bootstrap-state";
import { StepShell } from "../step-shell";

const KINDS = ["postgresql", "mysql", "bigquery", "csv", "json"] as const;

export default function SourceStep() {
  const t = useTranslations("bootstrap.step2");
  const { state, update } = useBootstrap();

  return (
    <StepShell
      stepKey="2-source"
      nextPath="/bootstrap/2b-select-tables"
      backPath="/bootstrap/1-pilot"
      canAdvance={!!state.sourceKind}
      title={t("title")}
      subtitle={t("subtitle")}
    >
      <fieldset className="grid grid-cols-1 gap-2 md:grid-cols-3" aria-label={t("kindLabel")}>
        {KINDS.map((k) => (
          <RadioCard
            key={k}
            name="sourceKind"
            value={k}
            checked={state.sourceKind === k}
            onChange={() => update({ sourceKind: k })}
            title={t(`kinds.${k}.label`)}
            hint={t(`kinds.${k}.hint`)}
          />
        ))}
      </fieldset>

      <div>
        <label
          htmlFor="connection"
          className="mb-1 block text-xs font-medium text-foreground"
        >
          {t("connectionLabel")}
        </label>
        <FormInput
          id="connection"
          value={state.sourceConnection}
          onChange={(e) => update({ sourceConnection: e.target.value })}
          placeholder={t("connectionPlaceholder")}
          className="font-mono"
        />
        <p className="mt-1 text-2xs text-foreground-muted">
          {t("connectionHint")}
        </p>
      </div>
    </StepShell>
  );
}
