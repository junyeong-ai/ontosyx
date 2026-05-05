"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";

import { useOntologyDetail } from "@/hooks/api/use-ontologies";
import type {
  ColumnLineage,
  EdgeTypeDef,
  NodeTypeDef,
  OntologyDetail,
  QueryDiagnostic,
  QueryProvenance,
} from "@/types/api";
import { arr } from "@/lib/ir-collections";
import { useDiagnosticResolver } from "@/lib/diagnostic";
import { localize } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";

/**
 * Compact "response basis" panel. Reads `arr(QueryResult.metadata.provenance)`
 * (Π-3) and shows the identity facts the LLM / admin UI needs to justify
 * a result: ontology + version, temporal pivot, touched data sources and
 * types, filter summary.
 *
 * When `provenance.ontology_id` is present the component fetches the
 * ontology detail (TanStack-cached) so type_ids render as human labels
 * with the internal id in a tooltip. An unresolvable type_id falls back
 * to the raw id — the fetch is best-effort and never blocks rendering.
 *
 * Each field is optional — the component only renders rows whose data
 * is present. When the full provenance is absent (legacy execution,
 * system-bypass path), the component returns `null` so it disappears
 * from the layout.
 */
export interface ResponseBasisProps {
  provenance: QueryProvenance | null | undefined;
  /**
   * Non-blocking advisory diagnostics from the Cypher validator
   * pipeline. Rendered as a severity-coloured list beneath the
   * provenance grid. Empty / undefined hides the section.
   *
   * Structured — the component picks the tone from `level` and
   * keys the icon off `validator`. Callers don't format anything.
   */
  warnings?: QueryDiagnostic[] | null;
  className?: string;
}

interface ResolvedType {
  id: string;
  /** Display label resolved via the ontology, or `null` when the id
   *  did not match any node/edge type (stale / cross-workspace /
   *  ontology fetch in flight). */
  label: string | null;
  /** Short human description, surfaced via the pill's `title` so a
   *  hover tells the viewer what this type represents. `null` when
   *  the type had no authored description. */
  description: string | null;
}

export function ResponseBasis({
  provenance,
  warnings,
  className,
}: ResponseBasisProps) {
  const t = useTranslations("widget.responseBasis");
  const resolveDiagnostic = useDiagnosticResolver();
  const localeChain = useLocaleChain();

  // Hook hygiene — call useOntologyDetail unconditionally and gate
  // via the `enabled` flag the hook already exposes. Passing `null`
  // parks the query in idle without firing a request.
  const { data: ontologyDetail } = useOntologyDetail(
    provenance?.ontology_id ?? null,
  );

  const resolvedTypes = useMemo(
    () => resolveTypeIds(provenance?.type_ids, ontologyDetail, localeChain),
    [provenance?.type_ids, ontologyDetail, localeChain],
  );

  const activeWarnings = (warnings ?? [])
    .filter((w) => w)
    .map((w) => ({
      ...w,
      // `useDiagnosticResolver` walks the catalogue tree to detect
      // presence, then either renders via ICU MessageFormat or falls
      // back to the diagnostic's English `message`. Empty resolved
      // strings collapse the row.
      resolvedMessage: resolveDiagnostic(w.message),
    }))
    .filter((w) => w.resolvedMessage.trim().length > 0);
  const hasProvenance =
    !!provenance &&
    !!(
      provenance.ontology_id ||
      provenance.ontology_version ||
      provenance.as_of ||
      (provenance.source_ids?.length ?? 0) > 0 ||
      resolvedTypes.length > 0 ||
      provenance.filter_summary ||
      (provenance.column_lineage?.length ?? 0) > 0
    );

  if (!hasProvenance && activeWarnings.length === 0) return null;

  return (
    <section
      aria-label={t("title")}
      className={
        "rounded-lg border border-divider bg-surface-raised p-3 text-xs/40 " +
        (className ?? "")
      }
    >
      <h3 className="mb-2 text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
        {t("title")}
      </h3>
      {activeWarnings.length > 0 && (
        <ul
          className="mb-2 space-y-1 rounded-md border border-warning-border bg-warning-surface px-3 py-2 text-2xs text-warning-foreground"
          aria-label={t("warningsLabel")}
        >
          {activeWarnings.map((w, i) => (
            <li
              key={i}
              className={`font-mono leading-snug break-words ${diagnosticLevelClass(w.level)}`}
            >
              <span className="me-1 font-semibold uppercase tracking-wide">
                {w.validator} {w.level}:
              </span>
              {w.resolvedMessage}
            </li>
          ))}
        </ul>
      )}
      {hasProvenance && provenance && (
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5">
        {provenance.ontology_id && (
          <Row label={t("ontologyLabel")}>
            <code className="font-mono text-2xs">{provenance.ontology_id}</code>
          </Row>
        )}
        {provenance.ontology_version && (
          <Row label={t("versionLabel")}>
            <span>{provenance.ontology_version}</span>
          </Row>
        )}
        {provenance.as_of && (
          <Row label={t("asOfLabel")}>
            <time dateTime={provenance.as_of} className="font-mono text-2xs">
              {provenance.as_of}
            </time>
          </Row>
        )}
        {provenance.source_ids && provenance.source_ids.length > 0 && (
          <Row label={t("sourcesLabel")}>
            <IdPillList items={provenance.source_ids} tone="emerald" />
          </Row>
        )}
        {resolvedTypes.length > 0 && (
          <Row label={t("typesLabel")}>
            <TypePillList items={resolvedTypes} />
          </Row>
        )}
        {provenance.filter_summary && (
          <Row label={t("filterLabel")}>
            <code className="whitespace-pre-wrap break-all font-mono text-2xs">
              {provenance.filter_summary}
            </code>
          </Row>
        )}
        {provenance.column_lineage && provenance.column_lineage.length > 0 && (
          <Row label={t("lineageLabel")}>
            <ColumnLineageList rows={provenance.column_lineage} />
          </Row>
        )}
      </dl>
      )}
    </section>
  );
}

/**
 * Render `column_lineage` grouped by `source_id`. Each output column
 * gets one line: `out ← source.column [transform]`. Transforms
 * (ConceptMap rewrites, SQL expressions, JSON paths) appear as a
 * muted suffix so the human can spot non-identity mappings quickly.
 *
 * Density policy: when there are at most three lineage rows, render
 * inline (the common case — single-source result with a few
 * projected columns). Beyond that, collapse the list behind a native
 * `<details>` so the panel doesn't dominate the response surface.
 * The summary states the source count + total row count so the user
 * can decide whether to expand.
 */
function ColumnLineageList({ rows }: { rows: ColumnLineage[] }) {
  const t = useTranslations("widget.responseBasis");
  const grouped = new Map<string, ColumnLineage[]>();
  for (const row of rows) {
    const bucket = grouped.get(row.source_id);
    if (bucket) bucket.push(row);
    else grouped.set(row.source_id, [row]);
  }
  const body = (
    <ul className="flex flex-col gap-1.5">
      {Array.from(grouped.entries()).map(([sourceId, lines]) => (
        <li key={sourceId} className="flex flex-col gap-0.5">
          <span className="font-mono text-2xs text-foreground-muted">
            {sourceId}
          </span>
          <ul className="flex flex-col gap-0.5 ps-3">
            {lines.map((row, idx) => (
              <li
                key={`${row.output_column}-${idx}`}
                className="font-mono text-2xs leading-snug"
              >
                <span className="text-foreground">
                  {row.output_column}
                </span>
                <span className="mx-1 text-foreground-muted">←</span>
                <span className="text-foreground-muted">
                  {row.source_column}
                </span>
                {row.transform && (
                  <span className="ms-2 italic text-warning-foreground">
                    [{row.transform}]
                  </span>
                )}
              </li>
            ))}
          </ul>
        </li>
      ))}
    </ul>
  );

  if (rows.length <= LINEAGE_INLINE_THRESHOLD) {
    return body;
  }

  return (
    <details className="group/lineage">
      <summary className="cursor-pointer text-2xs text-foreground-muted hover:text-foreground-muted">
        <span className="font-mono">
          {t("lineageSummary", { sources: grouped.size, rows: rows.length })}
        </span>
        <span className="ms-1 text-foreground-muted group-open/lineage:hidden">
          ▸
        </span>
        <span className="ms-1 text-foreground-muted hidden group-open/lineage:inline">
          ▾
        </span>
      </summary>
      <div className="mt-1.5">{body}</div>
    </details>
  );
}

/** Inline up to this many rows; collapse beyond. Three covers the
 *  single-source / few-columns case (the common Cypher response
 *  shape) while preventing fan-out queries from dominating the
 *  Provenance panel. */
const LINEAGE_INLINE_THRESHOLD = 3;

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <>
      <dt className="text-2xs font-medium text-foreground-muted">{label}</dt>
      <dd className="min-w-0 break-words text-foreground">{children}</dd>
    </>
  );
}

/**
 * Try to resolve each opaque type id to a (label, description)
 * triple via the ontology IR. Never throws — ids that don't match
 * any node / edge come back with `label: null` and the caller
 * renders the raw id.
 */
function resolveTypeIds(
  ids: string[] | undefined,
  detail: OntologyDetail | undefined,
  chain: readonly string[],
): ResolvedType[] {
  if (!ids?.length) return [];

  const ir = detail?.ontology_ir;
  if (!ir) {
    return ids.map((id) => ({ id, label: null, description: null }));
  }

  const nodeById = new Map<string, NodeTypeDef>();
  for (const n of arr(ir.node_types)) nodeById.set(n.id, n);
  const edgeById = new Map<string, EdgeTypeDef>();
  for (const e of arr(ir.edge_types)) edgeById.set(e.id, e);

  return ids.map((id) => {
    const node = nodeById.get(id);
    if (node) {
      return { id, label: node.label, description: localize(node.description, chain) || null };
    }
    const edge = edgeById.get(id);
    if (edge) {
      return { id, label: edge.label, description: localize(edge.description, chain) || null };
    }
    return { id, label: null, description: null };
  });
}

/** Mono-font pill list for opaque identifier arrays (source_ids). */
function IdPillList({ items, tone }: { items: string[]; tone: "emerald" | "indigo" }) {
  const toneClass = toneClasses(tone);
  return (
    <div className="flex flex-wrap gap-1">
      {items.map((item) => (
        <span
          key={item}
          className={`inline-flex rounded-full px-2 py-0.5 font-mono text-2xs ${toneClass}`}
        >
          {item}
        </span>
      ))}
    </div>
  );
}

/**
 * Pill list that prefers a resolved label for each type id. Unknown
 * ids render mono-font (same visual as IdPillList) so the viewer can
 * still spot the raw handle; resolved ids render the label in
 * regular weight with the id + description in the `title` attribute
 * so a hover reveals the full context. `title` is the low-tech
 * affordance — the component stays standalone without Tooltip /
 * portal wiring.
 */
function TypePillList({ items }: { items: ResolvedType[] }) {
  const toneClass = toneClasses("indigo");
  return (
    <div className="flex flex-wrap gap-1">
      {items.map((item) => {
        const titleParts = [item.id];
        if (item.description) titleParts.push(item.description);
        const title = titleParts.join(" · ");
        if (item.label === null) {
          // Unresolved — preserve the mono style to signal "raw id".
          return (
            <span
              key={item.id}
              title={item.id}
              className={`inline-flex rounded-full px-2 py-0.5 font-mono text-2xs ${toneClass}`}
            >
              {item.id}
            </span>
          );
        }
        return (
          <span
            key={item.id}
            title={title}
            className={`inline-flex cursor-help rounded-full px-2 py-0.5 text-2xs font-medium ${toneClass}`}
          >
            {item.label}
          </span>
        );
      })}
    </div>
  );
}

function toneClasses(tone: "emerald" | "indigo"): string {
  switch (tone) {
    case "emerald":
      return "bg-success-surface text-success-foreground";
    case "indigo":
      return "bg-concept-surface text-concept-foreground";
  }
}

/** Severity-driven text colour for a diagnostic row. The amber
 *  container stays; the row colour escalates for error-level
 *  strict-pass diagnostics so they stand out from warnings. */
function diagnosticLevelClass(level: QueryDiagnostic["level"]): string {
  switch (level) {
    case "error":
      return "text-danger-foreground";
    case "warning":
      return "text-warning-foreground";
    case "info":
      return "text-info-foreground";
  }
}

