"use client";

import { useTranslations } from "next-intl";

import {
  SourceImportPanel,
  emptyImportValue,
} from "@/components/workbench/source-import-panel";

import { useBootstrap } from "../bootstrap-state";
import { bootstrapSourceToDataSourceSpec } from "../source-mapping";
import { StepShell } from "../step-shell";

export default function SelectTablesStep() {
  const t = useTranslations("bootstrap.step2b");
  const { state, update } = useBootstrap();

  const source = bootstrapSourceToDataSourceSpec(
    state.sourceKind,
    state.sourceConnection,
  );

  const value = {
    mode: state.analyzeMode,
    selectedTables: state.selectedTables,
  };

  const canAdvance = value.mode === "all" || value.selectedTables.length > 0;

  return (
    <StepShell
      stepKey="2b-select-tables"
      nextPath="/bootstrap/3-glossary"
      backPath="/bootstrap/2-source"
      canAdvance={canAdvance}
      title={t("title")}
      subtitle={t("subtitle")}
    >
      <SourceImportPanel
        source={source}
        value={value}
        onChange={(next) => {
          update({
            analyzeMode: next.mode,
            selectedTables: next.selectedTables,
          });
        }}
      />
    </StepShell>
  );
}

export { emptyImportValue };
