"use client";

import { useEffect, useState } from "react";
import type { DesignOptions, PiiDecision } from "@/types/api";
import { relationshipKey, columnKey } from "./design-panel-shared";

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
  ambiguous_columns: { table: string; column: string }[];
  analysis_completeness?: string;
} | null): DesignDecisions {
  const [confirmedRelationships, setConfirmedRelationships] = useState<Record<string, boolean>>(() => {
    const init: Record<string, boolean> = {};
    designOptions.confirmed_relationships?.forEach((r) => {
      init[relationshipKey(r)] = true;
    });
    return init;
  });
  const [piiDecisions, setPiiDecisions] = useState<Record<string, PiiDecision | "">>(() => {
    const init: Record<string, PiiDecision | ""> = {};
    designOptions.pii_decisions?.forEach((d) => {
      init[columnKey(d.table, d.column)] = d.decision;
    });
    return init;
  });
  const [clarifications, setClarifications] = useState<Record<string, string>>(() => {
    const init: Record<string, string> = {};
    designOptions.column_clarifications?.forEach((c) => {
      init[columnKey(c.table, c.column)] = c.hint;
    });
    return init;
  });
  const [excludedTables, setExcludedTables] = useState<Record<string, boolean>>(() => {
    const init: Record<string, boolean> = {};
    designOptions.excluded_tables?.forEach((t) => {
      init[t] = true;
    });
    return init;
  });
  const [allowPartialAnalysis, setAllowPartialAnalysis] = useState(
    designOptions.allow_partial_source_analysis ?? false,
  );

  // Re-derive local state when design_options changes
  useEffect(() => {
    const nextConfirmed: Record<string, boolean> = {};
    designOptions.confirmed_relationships?.forEach((r) => {
      nextConfirmed[relationshipKey(r)] = true;
    });
    setConfirmedRelationships(nextConfirmed);

    const nextPii: Record<string, PiiDecision | ""> = {};
    designOptions.pii_decisions?.forEach((d) => {
      nextPii[columnKey(d.table, d.column)] = d.decision;
    });
    setPiiDecisions(nextPii);

    const nextClarifications: Record<string, string> = {};
    designOptions.column_clarifications?.forEach((c) => {
      nextClarifications[columnKey(c.table, c.column)] = c.hint;
    });
    setClarifications(nextClarifications);

    const nextExcluded: Record<string, boolean> = {};
    designOptions.excluded_tables?.forEach((t) => {
      nextExcluded[t] = true;
    });
    setExcludedTables(nextExcluded);

    setAllowPartialAnalysis(designOptions.allow_partial_source_analysis ?? false);
  }, [designOptions]);

  // Derived counts
  const unresolvedPiiCount = report
    ? report.pii_findings.filter(
        (f) => !piiDecisions[columnKey(f.table, f.column)],
      ).length
    : 0;

  const unresolvedClarificationCount = report
    ? report.ambiguous_columns.filter(
        (c) => !clarifications[columnKey(c.table, c.column)]?.trim(),
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
