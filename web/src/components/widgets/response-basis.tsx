"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";

import { useOntologyDetail } from "@/hooks/api/use-ontologies";
import type {
  EdgeTypeDef,
  NodeTypeDef,
  OntologyDetail,
  QueryDiagnostic,
  QueryProvenance,
} from "@/types/api";
import { arr } from "@/lib/ir-collections";
import { defaultText } from "@/lib/locale/localize";

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

  // Hook hygiene — call useOntologyDetail unconditionally and gate
  // via the `enabled` flag the hook already exposes. Passing `null`
  // parks the query in idle without firing a request.
  const { data: ontologyDetail } = useOntologyDetail(
    provenance?.ontology_id ?? null,
  );

  const resolvedTypes = useMemo(
    () => resolveTypeIds(provenance?.type_ids, ontologyDetail),
    [provenance?.type_ids, ontologyDetail],
  );

  const activeWarnings =
    warnings?.filter((w) => w && w.message.trim().length > 0) ?? [];
  const hasProvenance =
    !!provenance &&
    !!(
      provenance.ontology_id ||
      provenance.ontology_version ||
      provenance.as_of ||
      (provenance.source_ids?.length ?? 0) > 0 ||
      resolvedTypes.length > 0 ||
      provenance.filter_summary
    );

  if (!hasProvenance && activeWarnings.length === 0) return null;

  return (
    <section
      aria-label={t("title")}
      className={
        "rounded-lg border border-zinc-200 bg-zinc-50/60 p-3 text-xs dark:border-zinc-800 dark:bg-zinc-900/40 " +
        (className ?? "")
      }
    >
      <h3 className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
        {t("title")}
      </h3>
      {activeWarnings.length > 0 && (
        <ul
          className="mb-2 space-y-1 rounded-md border border-amber-200 bg-amber-50/70 px-3 py-2 text-[11px] text-amber-800 dark:border-amber-900/50 dark:bg-amber-950/30 dark:text-amber-300"
          aria-label={t("warningsLabel")}
        >
          {activeWarnings.map((w, i) => (
            <li
              key={i}
              className={`font-mono leading-snug break-words ${diagnosticLevelClass(w.level)}`}
            >
              <span className="mr-1 font-semibold uppercase tracking-wide">
                {w.validator} {w.level}:
              </span>
              {w.message}
            </li>
          ))}
        </ul>
      )}
      {hasProvenance && provenance && (
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5">
        {provenance.ontology_id && (
          <Row label={t("ontologyLabel")}>
            <code className="font-mono text-[11px]">{provenance.ontology_id}</code>
          </Row>
        )}
        {provenance.ontology_version && (
          <Row label={t("versionLabel")}>
            <span>{provenance.ontology_version}</span>
          </Row>
        )}
        {provenance.as_of && (
          <Row label={t("asOfLabel")}>
            <time dateTime={provenance.as_of} className="font-mono text-[11px]">
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
            <code className="whitespace-pre-wrap break-all font-mono text-[11px]">
              {provenance.filter_summary}
            </code>
          </Row>
        )}
      </dl>
      )}
    </section>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <>
      <dt className="text-[11px] font-medium text-muted-foreground">{label}</dt>
      <dd className="min-w-0 break-words text-zinc-700 dark:text-zinc-300">{children}</dd>
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
      return { id, label: node.label, description: defaultText(node.description) || null };
    }
    const edge = edgeById.get(id);
    if (edge) {
      return { id, label: edge.label, description: defaultText(edge.description) || null };
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
          className={`inline-flex rounded-full px-2 py-0.5 font-mono text-[10px] ${toneClass}`}
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
              className={`inline-flex rounded-full px-2 py-0.5 font-mono text-[10px] ${toneClass}`}
            >
              {item.id}
            </span>
          );
        }
        return (
          <span
            key={item.id}
            title={title}
            className={`inline-flex cursor-help rounded-full px-2 py-0.5 text-[10px] font-medium ${toneClass}`}
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
      return "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400";
    case "indigo":
      return "bg-indigo-100 text-indigo-700 dark:bg-indigo-900/30 dark:text-indigo-400";
  }
}

/** Severity-driven text colour for a diagnostic row. The amber
 *  container stays; the row colour escalates for error-level
 *  strict-pass diagnostics so they stand out from warnings. */
function diagnosticLevelClass(level: QueryDiagnostic["level"]): string {
  switch (level) {
    case "error":
      return "text-red-800 dark:text-red-300";
    case "warning":
      return "text-amber-800 dark:text-amber-300";
    case "info":
      return "text-sky-800 dark:text-sky-300";
  }
}
