"use client";

import { useTranslations } from "next-intl";

import { useAuditTrail } from "@/hooks/api/use-audit-trail";
import type {
  EdgeTypeDef,
  NodeTypeDef,
  OntologyIR,
} from "@/types/api";
import type { AuditRecord, ProvenanceDef } from "@/types/audit";

// ---------------------------------------------------------------------------
// ChangeLogFacet — PROV-O-aligned audit feed for the entity. The
// audit endpoint scopes by ontology (the per-entity filter is a
// client-side projection); we read 50 rows per page and walk the
// activity payloads to keep only those that touch this entity
// (subject id matches OR command_summary mentions the id).
// ---------------------------------------------------------------------------

interface ChangeLogFacetProps {
  ontology: OntologyIR;
  entity: NodeTypeDef | EdgeTypeDef;
  kind: "node" | "edge";
}

interface ChangeRow {
  at_time: string;
  agent: string;
  summary: string;
}

export function ChangeLogFacet({ ontology, entity, kind }: ChangeLogFacetProps) {
  const t = useTranslations("workbench.entityFacets.changelog");
  const audit = useAuditTrail({ ontology_id: ontology.id }, 50);

  const records: ChangeRow[] = (audit.data?.pages ?? [])
    .flatMap((page) => page.items as AuditRecord[])
    .map((record) => projectRecordForEntity(record, entity.id, kind))
    .filter((row): row is ChangeRow => row !== null);

  if (audit.isLoading && records.length === 0) {
    return <p className="text-[11px] italic text-muted-foreground">{t("loading")}</p>;
  }
  if (audit.isError) {
    return (
      <p className="text-[11px] text-rose-600 dark:text-rose-400">
        {t("loadError")}
      </p>
    );
  }
  if (records.length === 0) {
    return (
      <p className="text-[11px] italic text-muted-foreground">
        {t("emptyState")}
      </p>
    );
  }

  return (
    <ol className="space-y-2">
      {records.map((row, idx) => (
        <li
          key={`${row.at_time}-${idx}`}
          className="flex items-start gap-3 rounded border border-zinc-100 bg-zinc-50/40 px-3 py-2 dark:border-zinc-800/60 dark:bg-zinc-900/40"
        >
          <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
            {formatRelative(row.at_time)}
          </span>
          <span className="flex flex-1 flex-col gap-0.5">
            <span className="text-[11px] text-zinc-700 dark:text-zinc-200">
              {row.summary}
            </span>
            <span className="text-[10px] text-muted-foreground">
              {row.agent}
            </span>
          </span>
        </li>
      ))}
      {audit.hasNextPage && (
        <li>
          <button
            type="button"
            onClick={() => audit.fetchNextPage()}
            disabled={audit.isFetchingNextPage}
            className="text-[11px] text-violet-600 hover:underline disabled:opacity-50 dark:text-violet-400"
          >
            {audit.isFetchingNextPage ? t("loadingMore") : t("loadMore")}
          </button>
        </li>
      )}
    </ol>
  );
}

function projectRecordForEntity(
  record: AuditRecord,
  entityId: string,
  kind: "node" | "edge",
): ChangeRow | null {
  const prov = record.provenance as ProvenanceDef | undefined;
  if (!prov) return null;
  const subjectMatches = subjectTouchesEntity(prov, entityId, kind);
  const activityMentions =
    prov.activity.kind === "ontology_edit" &&
    prov.activity.command_summary.includes(entityId);
  if (!subjectMatches && !activityMentions) return null;
  return {
    at_time: record.at_time,
    agent: formatAgent(prov),
    summary: formatActivity(prov),
  };
}

function subjectTouchesEntity(
  prov: ProvenanceDef,
  entityId: string,
  kind: "node" | "edge",
): boolean {
  switch (prov.subject.kind) {
    case "node_instance":
      return kind === "node" && prov.subject.node_type_id === entityId;
    case "property_value":
      return kind === "node" && prov.subject.node_type_id === entityId;
    case "edge_instance":
      return kind === "edge" && prov.subject.edge_type_id === entityId;
    case "arbitrary":
      return false;
  }
}

function formatActivity(prov: ProvenanceDef): string {
  switch (prov.activity.kind) {
    case "ontology_edit":
      return prov.activity.command_summary;
    case "rule_validate":
      return `Rule validate (${prov.activity.outcome})`;
    case "source_scan":
      return `Source scan: ${prov.activity.mapping_id}`;
    case "function_eval":
      return `Function eval: ${prov.activity.function_id}`;
    case "action_execute":
      return `Action execute: ${prov.activity.action_id}`;
    case "draft_proposal":
      return `LLM draft (${prov.activity.model_id})`;
    case "cache_refresh":
      return `Cache refresh: ${prov.activity.mapping_id}`;
    case "enrichment":
      return `Enrichment: ${prov.activity.enrichment_id}`;
    case "import":
      return `Import (${prov.activity.format})`;
    case "export":
      return `Export (${prov.activity.format})`;
  }
}

function formatAgent(prov: ProvenanceDef): string {
  switch (prov.agent.kind) {
    case "user":
      return prov.agent.user_id;
    case "service":
      return `service:${prov.agent.service_id}`;
    case "llm_model":
      return `llm:${prov.agent.model_id}`;
    case "system":
      return "system";
  }
}

function formatRelative(iso: string): string {
  try {
    const d = new Date(iso);
    return `${d.toISOString().slice(0, 10)} ${d.toISOString().slice(11, 16)}`;
  } catch {
    return iso;
  }
}
