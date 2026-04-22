"use client";

import { useTranslations } from "next-intl";
import { useBootstrap } from "../bootstrap-state";
import { StepShell } from "../step-shell";

const KINDS = ["postgresql", "mysql", "bigquery", "csv", "json"] as const;

export default function SourceStep() {
  const t = useTranslations("bootstrap.step2");
  const { state, update } = useBootstrap();

  return (
    <StepShell
      stepKey="2-source"
      nextPath="/bootstrap/3-glossary"
      backPath="/bootstrap/1-pilot"
      canAdvance={!!state.sourceKind}
      title={t("title")}
      subtitle={t("subtitle")}
    >
      <fieldset className="grid grid-cols-1 gap-2 md:grid-cols-3" aria-label={t("kindLabel")}>
        {KINDS.map((k) => (
          <label
            key={k}
            className={`cursor-pointer rounded border px-3 py-3 text-xs ${
              state.sourceKind === k
                ? "border-violet-500 bg-violet-50 text-violet-700 dark:bg-violet-950/40 dark:text-violet-300"
                : "border-zinc-200 bg-white text-muted-foreground hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:hover:bg-zinc-800"
            }`}
          >
            <input
              type="radio"
              name="sourceKind"
              value={k}
              checked={state.sourceKind === k}
              onChange={() => update({ sourceKind: k })}
              className="sr-only"
            />
            <p className="font-medium">{t(`kinds.${k}.label`)}</p>
            <p className="mt-0.5 text-[10px] text-muted-foreground">
              {t(`kinds.${k}.hint`)}
            </p>
          </label>
        ))}
      </fieldset>

      <div>
        <label
          htmlFor="connection"
          className="mb-1 block text-xs font-medium text-zinc-700 dark:text-zinc-300"
        >
          {t("connectionLabel")}
        </label>
        <input
          id="connection"
          value={state.sourceConnection}
          onChange={(e) => update({ sourceConnection: e.target.value })}
          placeholder={t("connectionPlaceholder")}
          className="w-full rounded border border-zinc-300 bg-white px-3 py-2 font-mono text-xs dark:border-zinc-600 dark:bg-zinc-900"
        />
        <p className="mt-1 text-[11px] text-muted-foreground">
          {t("connectionHint")}
        </p>
      </div>
    </StepShell>
  );
}
