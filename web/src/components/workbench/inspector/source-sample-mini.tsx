"use client";

// ---------------------------------------------------------------------------
// SourceSampleMini — live-data preview inside the Design Inspector.
//
// When the selected node carries a `source_lineage.table` and the active
// project has a `source_profile`, render a compact 5-row sample plus the
// per-column distribution summary (distinct / null / min-max) that the
// introspection kernel already collected.
//
// This is the Foundry-grade "Design ↔ Live" feedback loop the previous
// architecture review flagged as missing: the operator sees the column
// distribution alongside the property they're shaping, without leaving
// Design mode.
// ---------------------------------------------------------------------------

import { useMemo } from "react";
import { useTranslations } from "next-intl";

import { useAppStore } from "@/lib/store";
import { useFormatters } from "@/hooks/use-formatters";
import type { ColumnStats, SourceProfile, TableProfile } from "@/types/api";

interface Props {
  /** Source-side table the node maps to. */
  tableName: string;
}

const MAX_SAMPLE_ROWS = 5;

/// Pick the table profile from the active project's introspection
/// snapshot. The project may have no source_profile (Text origin,
/// CodeRepository, BaseOntology) — return `null` so the caller hides
/// the panel cleanly instead of rendering an empty shell.
function resolveTableProfile(
  profile: SourceProfile | null | undefined,
  tableName: string,
): TableProfile | null {
  if (!profile?.table_profiles) return null;
  return profile.table_profiles.find((t) => t.table_name === tableName) ?? null;
}

/// Project the column statistics into a row-major 5×N preview matrix.
/// Sample arrays may be ragged (some columns sample fewer values than
/// others); fill missing cells with `""` so the table renders aligned.
function buildSampleRows(stats: ColumnStats[]): string[][] {
  const rowCount = Math.min(
    MAX_SAMPLE_ROWS,
    Math.max(...stats.map((s) => s.sample_values.length), 0),
  );
  if (rowCount === 0) return [];
  const rows: string[][] = [];
  for (let r = 0; r < rowCount; r++) {
    const row = stats.map((s) => s.sample_values[r] ?? "");
    rows.push(row);
  }
  return rows;
}

export function SourceSampleMini({ tableName }: Props) {
  const t = useTranslations("inspector.sourceSample");
  const fmt = useFormatters();
  const project = useAppStore((s) => s.activeOntologyDraft);
  const profile = (project?.source_profile ?? null) as SourceProfile | null;
  const tableProfile = useMemo(
    () => resolveTableProfile(profile, tableName),
    [profile, tableName],
  );

  if (!tableProfile) {
    return null;
  }

  const stats = tableProfile.column_stats;
  const rows = buildSampleRows(stats);
  const rowCount = tableProfile.row_count;

  return (
    <details
      className="rounded border border-divider bg-surface-base text-xs"
      open
    >
      <summary className="cursor-pointer select-none border-b border-divider-soft px-2 py-1 text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
        {t("title", {
          table: tableName,
          rowCount: fmt.number(rowCount),
        })}
      </summary>

      {rows.length === 0 ? (
        <p className="px-2 py-2 text-2xs text-foreground-muted">
          {t("noSamples")}
        </p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-2xs">
            <thead className="bg-surface-raised">
              <tr>
                {stats.map((c) => (
                  <th
                    key={c.column_name}
                    className="border-b border-divider px-2 py-1 text-start font-mono font-medium text-foreground-muted"
                  >
                    {c.column_name}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, i) => (
                <tr key={i} className="even:bg-surface-raised:bg-surface-base/50">
                  {row.map((cell, j) => (
                    <td
                      key={j}
                      className="border-b border-divider-soft px-2 py-1 font-mono text-foreground-subtle"
                    >
                      {cell || (
                        <span className="text-foreground-subtle italic">∅</span>
                      )}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Per-column distribution summary — distinct + null counts let
          the operator spot a near-key column or a high-null one
          without leaving the inspector. PII-suspect columns surface
          a "Redacted" badge in place of the distribution detail so
          the operator sees both that the column was flagged and
          that no raw values entered the profile. */}
      <ul className="border-t border-divider-soft px-2 py-1">
        {stats.map((c) => (
          <li
            key={c.column_name}
            className="flex items-center justify-between gap-2 py-0.5 text-2xs"
          >
            <span className="font-mono text-foreground-muted">
              {c.column_name}
            </span>
            {c.pii_redacted ? (
              <span
                className="rounded bg-danger-surface px-1.5 py-0.5 font-medium text-danger-foreground"
                title={t("piiRedactedTooltip")}
              >
                {t("piiRedactedBadge", {
                  kind: c.pii_redacted.kind,
                })}
              </span>
            ) : (
              <span className="text-foreground-muted">
                {t("distribution", {
                  distinct: fmt.number(c.distinct_count),
                  nulls: fmt.number(c.null_count),
                })}
              </span>
            )}
          </li>
        ))}
      </ul>
    </details>
  );
}
