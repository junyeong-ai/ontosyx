"use client";

import { useState } from "react";
import type { DesignOptions, PiiDecision } from "@/types/api";
import { relationshipKey, columnKey } from "./design-panel-shared";

// ---------------------------------------------------------------------------
// Derive the five sub-states from a fresh `designOptions` snapshot. Lives
// outside the hook so the render-phase reset below can call it without a
// closure on any stale prop.
// ---------------------------------------------------------------------------

interface ResetState {
  confirmed: Record<string, boolean>;
  pii: Record<string, PiiDecision | "">;
  clarifications: Record<string, string>;
  excluded: Record<string, boolean>;
  allowPartial: boolean;
}

function deriveResetState(designOptions: DesignOptions): ResetState {
  const confirmed: Record<string, boolean> = {};
  designOptions.confirmed_relationships?.forEach((r) => {
    confirmed[relationshipKey(r)] = true;
  });
  const pii: Record<string, PiiDecision | ""> = {};
  designOptions.pii_decisions?.forEach((d) => {
    pii[columnKey(d.table, d.column)] = d.decision;
  });
  const clarifications: Record<string, string> = {};
  designOptions.column_clarifications?.forEach((c) => {
    clarifications[columnKey(c.table, c.column)] = c.hint;
  });
  const excluded: Record<string, boolean> = {};
  designOptions.excluded_tables?.forEach((t) => {
    excluded[t] = true;
  });
  return {
    confirmed,
    pii,
    clarifications,
    excluded,
    allowPartial: designOptions.allow_partial_source_analysis ?? false,
  };
}

// ---------------------------------------------------------------------------
// Decision state hook — shared between WorkflowActions and AnalysisReview
// ---------------------------------------------------------------------------

export interface DesignDecisions {
  confirmedRelationships: Record<string, boolean>;
  setConfirmedRelationships: React.Dispatch<React.SetStateAction<Record<string, boolean>>>;
  piiDecisions: Record<string, PiiDecision | "">;
  setPiiDecisions: React.Dispatch<React.SetStateAction<Record<string, PiiDecision | "">>>;
  clarifications: Record<string, string>;
  setClarifications: React.Dispatch<React.SetStateAction<Record<string, string>>>;
  excludedTables: Record<string, boolean>;
  setExcludedTables: React.Dispatch<React.SetStateAction<Record<string, boolean>>>;
  allowPartialAnalysis: boolean;
  setAllowPartialAnalysis: React.Dispatch<React.SetStateAction<boolean>>;
  unresolvedPiiCount: number;
  unresolvedClarificationCount: number;
  needsPartialAcknowledgement: boolean;
}

export function useDesignDecisions(designOptions: DesignOptions, report: {
  pii_findings: { table: string; column: string }[];
  /** `AmbiguityContext`-shaped rows (structured `column: {relation, column}`). */
  ambiguous_columns: { column: { relation: string; column: string } }[];
  analysis_completeness?: string;
} | null): DesignDecisions {
  const [confirmedRelationships, setConfirmedRelationships] = useState<Record<string, boolean>>(
    () => deriveResetState(designOptions).confirmed,
  );
  const [piiDecisions, setPiiDecisions] = useState<Record<string, PiiDecision | "">>(
    () => deriveResetState(designOptions).pii,
  );
  const [clarifications, setClarifications] = useState<Record<string, string>>(
    () => deriveResetState(designOptions).clarifications,
  );
  const [excludedTables, setExcludedTables] = useState<Record<string, boolean>>(
    () => deriveResetState(designOptions).excluded,
  );
  const [allowPartialAnalysis, setAllowPartialAnalysis] = useState(
    () => deriveResetState(designOptions).allowPartial,
  );

  // Derived-state-on-prop-change — the React 19 idiomatic replacement for
  // a `useEffect(() => { setLocal(deriveFromProp(prop)); }, [prop])` reset
  // cascade. React explicitly supports render-phase setState when it
  // matches the "last-seen prop" pattern documented at
  // https://react.dev/reference/react/useState#storing-information-from-previous-renders.
  // Same end result as the old effect but no cascading render trip.
  const [prevDesignOptions, setPrevDesignOptions] = useState(designOptions);
  if (prevDesignOptions !== designOptions) {
    const reset = deriveResetState(designOptions);
    setPrevDesignOptions(designOptions);
    setConfirmedRelationships(reset.confirmed);
    setPiiDecisions(reset.pii);
    setClarifications(reset.clarifications);
    setExcludedTables(reset.excluded);
    setAllowPartialAnalysis(reset.allowPartial);
  }

  // Derived counts
  const unresolvedPiiCount = report
    ? report.pii_findings.filter(
        (f) => !piiDecisions[columnKey(f.table, f.column)],
      ).length
    : 0;

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
    piiDecisions,
    setPiiDecisions,
    clarifications,
    setClarifications,
    excludedTables,
    setExcludedTables,
    allowPartialAnalysis,
    setAllowPartialAnalysis,
    unresolvedPiiCount,
    unresolvedClarificationCount,
    needsPartialAcknowledgement,
  };
}
