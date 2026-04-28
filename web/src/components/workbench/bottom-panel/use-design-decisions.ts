"use client";

import { useState } from "react";
import type { DesignOptions, PiiKind } from "@/types/api";
import { relationshipKey, columnKey } from "./design-panel-shared";

// ---------------------------------------------------------------------------
// Decision state hook — shared between WorkflowActions and AnalysisReview.
// Tracks per-column PII annotations + per-column exclusions alongside the
// existing relationship / clarification / table-exclusion state.
//
// Names mirror the wire shape (`design_options.<field>`) in camelCase so
// the local form state and the persisted server state stay aligned —
// every field here lowers directly into the `DesignOptions` payload.
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
  partialAnalysisAcknowledged: boolean;
  largeSchemaAcknowledged: boolean;
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
    partialAnalysisAcknowledged: designOptions.partial_analysis_acknowledged ?? false,
    largeSchemaAcknowledged: designOptions.large_schema_acknowledged ?? false,
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
  partialAnalysisAcknowledged: boolean;
  setPartialAnalysisAcknowledged: React.Dispatch<React.SetStateAction<boolean>>;
  largeSchemaAcknowledged: boolean;
  setLargeSchemaAcknowledged: React.Dispatch<React.SetStateAction<boolean>>;
  unresolvedClarificationCount: number;
}

export function useDesignDecisions(designOptions: DesignOptions, report: {
  ambiguous_columns: { column: { relation: string; column: string } }[];
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
  const [partialAnalysisAcknowledged, setPartialAnalysisAcknowledged] = useState(
    () => deriveResetState(designOptions).partialAnalysisAcknowledged,
  );
  const [largeSchemaAcknowledged, setLargeSchemaAcknowledged] = useState(
    () => deriveResetState(designOptions).largeSchemaAcknowledged,
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
    setPartialAnalysisAcknowledged(reset.partialAnalysisAcknowledged);
    setLargeSchemaAcknowledged(reset.largeSchemaAcknowledged);
  }

  const unresolvedClarificationCount = report
    ? report.ambiguous_columns.filter(
        (c) => !clarifications[columnKey(c.column.relation, c.column.column)]?.trim(),
      ).length
    : 0;

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
    partialAnalysisAcknowledged,
    setPartialAnalysisAcknowledged,
    largeSchemaAcknowledged,
    setLargeSchemaAcknowledged,
    unresolvedClarificationCount,
  };
}
