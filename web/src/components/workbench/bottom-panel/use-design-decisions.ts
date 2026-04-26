"use client";

import { useState } from "react";
import type { DesignOptions, PiiKind } from "@/types/api";
import { relationshipKey, columnKey } from "./design-panel-shared";

// ---------------------------------------------------------------------------
// Decision state hook — shared between WorkflowActions and AnalysisReview.
// Tracks per-column PII annotations + per-column exclusions alongside the
// existing relationship / clarification / table-exclusion state.
// ---------------------------------------------------------------------------

export interface PiiAnnotationEntry {
  table: string;
  column: string;
  kind: PiiKind;
}

interface ResetState {
  confirmed: Record<string, boolean>;
  piiAnnotations: Record<string, PiiAnnotationEntry>;
  excludedColumns: Record<string, { table: string; column: string }>;
  clarifications: Record<string, string>;
  excludedTables: Record<string, boolean>;
  allowPartial: boolean;
}

function deriveResetState(designOptions: DesignOptions): ResetState {
  const confirmed: Record<string, boolean> = {};
  designOptions.confirmed_relationships?.forEach((r) => {
    confirmed[relationshipKey(r)] = true;
  });
  const piiAnnotations: Record<string, PiiAnnotationEntry> = {};
  designOptions.pii_annotations?.forEach((a) => {
    piiAnnotations[columnKey(a.table, a.column)] = {
      table: a.table,
      column: a.column,
      kind: a.kind,
    };
  });
  const excludedColumns: Record<string, { table: string; column: string }> = {};
  designOptions.excluded_columns?.forEach((c) => {
    excludedColumns[columnKey(c.table, c.column)] = {
      table: c.table,
      column: c.column,
    };
  });
  const clarifications: Record<string, string> = {};
  designOptions.column_clarifications?.forEach((c) => {
    clarifications[columnKey(c.table, c.column)] = c.hint;
  });
  const excludedTables: Record<string, boolean> = {};
  designOptions.excluded_tables?.forEach((t) => {
    excludedTables[t] = true;
  });
  return {
    confirmed,
    piiAnnotations,
    excludedColumns,
    clarifications,
    excludedTables,
    allowPartial: designOptions.allow_partial_source_analysis ?? false,
  };
}

export interface DesignDecisions {
  confirmedRelationships: Record<string, boolean>;
  setConfirmedRelationships: React.Dispatch<React.SetStateAction<Record<string, boolean>>>;
  piiAnnotations: Record<string, PiiAnnotationEntry>;
  setPiiAnnotations: React.Dispatch<
    React.SetStateAction<Record<string, PiiAnnotationEntry>>
  >;
  excludedColumns: Record<string, { table: string; column: string }>;
  setExcludedColumns: React.Dispatch<
    React.SetStateAction<Record<string, { table: string; column: string }>>
  >;
  clarifications: Record<string, string>;
  setClarifications: React.Dispatch<React.SetStateAction<Record<string, string>>>;
  excludedTables: Record<string, boolean>;
  setExcludedTables: React.Dispatch<React.SetStateAction<Record<string, boolean>>>;
  allowPartialAnalysis: boolean;
  setAllowPartialAnalysis: React.Dispatch<React.SetStateAction<boolean>>;
  unresolvedClarificationCount: number;
  needsPartialAcknowledgement: boolean;
}

export function useDesignDecisions(designOptions: DesignOptions, report: {
  ambiguous_columns: { column: { relation: string; column: string } }[];
  analysis_completeness?: string;
} | null): DesignDecisions {
  const [confirmedRelationships, setConfirmedRelationships] = useState<Record<string, boolean>>(
    () => deriveResetState(designOptions).confirmed,
  );
  const [piiAnnotations, setPiiAnnotations] = useState<Record<string, PiiAnnotationEntry>>(
    () => deriveResetState(designOptions).piiAnnotations,
  );
  const [excludedColumns, setExcludedColumns] = useState<
    Record<string, { table: string; column: string }>
  >(() => deriveResetState(designOptions).excludedColumns);
  const [clarifications, setClarifications] = useState<Record<string, string>>(
    () => deriveResetState(designOptions).clarifications,
  );
  const [excludedTables, setExcludedTables] = useState<Record<string, boolean>>(
    () => deriveResetState(designOptions).excludedTables,
  );
  const [allowPartialAnalysis, setAllowPartialAnalysis] = useState(
    () => deriveResetState(designOptions).allowPartial,
  );

  const [prevDesignOptions, setPrevDesignOptions] = useState(designOptions);
  if (prevDesignOptions !== designOptions) {
    const reset = deriveResetState(designOptions);
    setPrevDesignOptions(designOptions);
    setConfirmedRelationships(reset.confirmed);
    setPiiAnnotations(reset.piiAnnotations);
    setExcludedColumns(reset.excludedColumns);
    setClarifications(reset.clarifications);
    setExcludedTables(reset.excludedTables);
    setAllowPartialAnalysis(reset.allowPartial);
  }

  const unresolvedClarificationCount = report
    ? report.ambiguous_columns.filter(
        (c) => !clarifications[columnKey(c.column.relation, c.column.column)]?.trim(),
      ).length
    : 0;

  const needsPartialAcknowledgement =
    report?.analysis_completeness === "partial" && !allowPartialAnalysis;

  return {
    confirmedRelationships,
    setConfirmedRelationships,
    piiAnnotations,
    setPiiAnnotations,
    excludedColumns,
    setExcludedColumns,
    clarifications,
    setClarifications,
    excludedTables,
    setExcludedTables,
    allowPartialAnalysis,
    setAllowPartialAnalysis,
    unresolvedClarificationCount,
    needsPartialAcknowledgement,
  };
}
