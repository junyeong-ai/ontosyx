"use client";

// Bootstrap step 2b — selective table picker.
//
// Sits between step 2 (source kind + connection) and step 3 (glossary
// draft) so the operator can scope the upcoming introspection to a
// subset of the source's tables. Default mode is "all" — clicking
// straight through gets the whole-source sweep.
//
// The actual UI lives in `SourceImportPanel` so the same component
// services Design-mode's "Import Tables" action without divergence.

import { useTranslations } from "next-intl";

import {
  SourceImportPanel,
  emptyImportValue,
} from "@/components/workbench/source-import-panel";

import { useBootstrap } from "../bootstrap-state";
import { bootstrapSourceToProjectSource } from "../source-mapping";
import { StepShell } from "../step-shell";

export default function SelectTablesStep() {
  const t = useTranslations("bootstrap.step2b");
  const { state, update } = useBootstrap();

  const source = bootstrapSourceToProjectSource(
    state.sourceKind,
    state.sourceConnection,
  );

  const value = {
    mode: state.analyzeMode,
    selectedTables: state.selectedTables,
  };

  // Step advances when:
  // - mode is "all" (no selection required), OR
  // - mode is "subset" / "staged" and at least one table is selected
  //   (both modes carry the same "list ≥ 1" precondition; the
  //   defer-the-rest distinction lives post-introspection in the
  //   AnalysisScope, not at the picker boundary).
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

// Re-export so existing tests / pages that imported this constant
// keep their reference. Kept minimal — the bootstrap module owns
// step state, not panel state.
export { emptyImportValue };
