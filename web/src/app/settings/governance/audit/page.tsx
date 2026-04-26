"use client";

// Φ6 #3 — Audit trail viewer.
//
// Surfaces the workspace's `ProvenanceDef` entries (PROV-O subject
// / activity / agent / at_time + used / derived_from) as a
// filterable list. Read-only — the underlying records are emitted
// by the LLM design pipeline, rule evaluation, source scans,
// action execution, etc. Operators see WHY a fact in the graph
// exists, who ran the activity, and when.
//
// Scope: workspace-wide (default) or single-ontology. The PROV-O
// records are stored on each `OntologyIR.provenance`; the page
// either picks a single ontology's history or fans out across all
// committed ontologies in the workspace and merges the streams,
// attributing each row to its source ontology so a multi-ontology
// workspace can see the rolled-up view at a glance.

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { useQueries } from "@tanstack/react-query";

import { EmptyState } from "@/components/ui/empty-state";
import { Spinner } from "@/components/ui/spinner";
import { SettingsSelect } from "@/components/ui/form-input";
import {
  ontologiesKeys,
  useOntologies,
} from "@/hooks/api/use-ontologies";
import { getOntologyDetail } from "@/lib/api/ontology";
import type { OntologyDetail } from "@/types/api";

// PROV-O shapes mirrored from `crates/ox-ontology/src/provenance.rs`.
// We narrow inside the component so a malformed record falls back to
// "raw payload" rather than crashing the page.
type EntityRef =
  | { kind: "node_instance"; node_type_id: string; element_id: string }
  | { kind: "edge_instance"; edge_type_id: string; element_id: string }
  | {
      kind: "property_value";
      node_type_id: string;
      element_id: string;
      property_id: string;
    }
  | { kind: "arbitrary"; label: string };

type ProvenanceActivityKind =
  | { kind: "source_scan"; source_id: string; mapping_id: string }
  | { kind: "function_eval"; function_id: string }
  | {
      kind: "rule_validate";
      rule_id: string;
      outcome: "pass" | "warn" | "fail";
    }
  | {
      kind: "action_execute";
      action_id: string;
      idempotency_key?: string | null;
    }
  | { kind: "ontology_edit"; command_summary: string }
  | {
      kind: "draft_proposal";
      prompt_name: string;
      prompt_version: string;
      model_id: string;
    }
  | { kind: "cache_refresh"; mapping_id: string }
  | { kind: "enrichment"; enrichment_id: string }
  | { kind: "import"; format: string; source_uri?: string | null }
  | { kind: "export"; format: string; destination_uri?: string | null };

type AgentRef =
  | { kind: "user"; user_id: string }
  | { kind: "service"; service_id: string }
  | { kind: "llm_model"; model_id: string }
  | { kind: "system" };

interface ProvenanceDef {
  id: string;
  subject: EntityRef;
  activity: ProvenanceActivityKind;
  agent: AgentRef;
  at_time: string;
  used?: EntityRef[];
  derived_from?: EntityRef[];
  ontology_valid_at?: string | null;
  data_valid_at?: string | null;
}

const ACTIVITY_KINDS: Array<ProvenanceActivityKind["kind"] | "all"> = [
  "all",
  "source_scan",
  "function_eval",
  "rule_validate",
  "action_execute",
  "ontology_edit",
  "draft_proposal",
  "cache_refresh",
  "enrichment",
  "import",
  "export",
];

const AGENT_KINDS: Array<AgentRef["kind"] | "all"> = [
  "all",
  "user",
  "service",
  "llm_model",
  "system",
];

/** A PROV-O record annotated with the source ontology so the
 *  cross-ontology view can attribute each row. */
interface AttributedRecord extends ProvenanceDef {
  ontology_id: string;
  ontology_name: string;
}

/** Constant value reused by `useQueries` so React-Query cache
 *  hits work across renders. */
const ONTOLOGY_LIST_LIMIT = 100;

export default function AuditTrailPage() {
  const t = useTranslations("settings.governance.audit");
  // Pull the full first page so the aggregate view has every
  // committed ontology to fan out across. A workspace with > 100
  // ontologies is rare today; the next-cursor pass slots in
  // trivially when it becomes load-bearing.
  const ontologies = useOntologies({ limit: ONTOLOGY_LIST_LIMIT });
  const items = ontologies.data?.items ?? [];

  // "all" → cross-ontology aggregate; otherwise the specific
  // ontology id. Default to "all" so a multi-ontology workspace
  // immediately sees the rolled-up view; single-ontology workspaces
  // see exactly the same thing they used to.
  const [ontologyFilter, setOntologyFilter] = useState<string>("all");
  const [activityFilter, setActivityFilter] = useState<string>("all");
  const [agentFilter, setAgentFilter] = useState<string>("all");

  // Parallel detail fetch — one query per ontology. React-Query
  // caches each by detail key, so opening this page after viewing
  // any single ontology elsewhere reuses the cache.
  const targets =
    ontologyFilter === "all"
      ? items
      : items.filter((o) => o.id === ontologyFilter);

  const detailQueries = useQueries({
    queries: targets.map((o) => ({
      queryKey: ontologiesKeys.detail(o.id),
      queryFn: () => getOntologyDetail(o.id),
    })),
  });

  const records = useMemo<AttributedRecord[]>(() => {
    const merged: AttributedRecord[] = [];
    detailQueries.forEach((q, i) => {
      const target = targets[i];
      if (!target) return;
      const detail = q.data as OntologyDetail | undefined;
      const provenance = (detail?.ontology_ir?.provenance ?? []) as ProvenanceDef[];
      provenance.forEach((p) =>
        merged.push({
          ...p,
          ontology_id: target.id,
          ontology_name: target.name,
        }),
      );
    });
    // Newest first — the underlying IR doesn't guarantee ordering
    // and the cross-ontology merge interleaves multiple streams.
    merged.sort((a, b) => b.at_time.localeCompare(a.at_time));
    return merged;
  }, [detailQueries, targets]);

  const filtered = useMemo(() => {
    return records.filter((r) => {
      if (activityFilter !== "all" && r.activity.kind !== activityFilter)
        return false;
      if (agentFilter !== "all" && r.agent.kind !== agentFilter) return false;
      return true;
    });
  }, [records, activityFilter, agentFilter]);

  const someDetailLoading = detailQueries.some((q) => q.isLoading);

  if (ontologies.isLoading) {
    return (
      <div className="flex items-center justify-center py-10">
        <Spinner />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <header>
        <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
          {t("pageTitle")}
        </h1>
        <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">
          {t("pageSubtitle")}
        </p>
      </header>

      {items.length === 0 && (
        <p className="rounded border border-amber-200 bg-amber-50 p-3 text-xs text-amber-700 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-300">
          {t("noOntology")}
        </p>
      )}

      {items.length > 0 && (
        <>
          <div className="flex items-center gap-3 flex-wrap">
            <SettingsSelect
              label={t("filter.ontology")}
              value={ontologyFilter}
              onChange={(e) => setOntologyFilter(e.target.value)}
              className="w-56"
            >
              <option value="all">
                {t("ontologyAll")} ({items.length})
              </option>
              {items.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.name}
                </option>
              ))}
            </SettingsSelect>
            <SettingsSelect
              label={t("filter.activity")}
              value={activityFilter}
              onChange={(e) => setActivityFilter(e.target.value)}
              className="w-48"
            >
              {ACTIVITY_KINDS.map((k) => (
                <option key={k} value={k}>
                  {t(`activityKinds.${k}`)}
                </option>
              ))}
            </SettingsSelect>
            <SettingsSelect
              label={t("filter.agent")}
              value={agentFilter}
              onChange={(e) => setAgentFilter(e.target.value)}
              className="w-40"
            >
              {AGENT_KINDS.map((k) => (
                <option key={k} value={k}>
                  {t(`agentKinds.${k}`)}
                </option>
              ))}
            </SettingsSelect>
            <span className="ml-auto text-[11px] text-muted-foreground">
              {t("counts", {
                shown: filtered.length,
                total: records.length,
              })}
            </span>
          </div>

          {ontologyFilter === "all" && items.length > 1 && (
            <p className="text-[11px] text-muted-foreground">
              {t("aggregatedHint", { count: items.length })}
            </p>
          )}

          {someDetailLoading ? (
            <div className="flex items-center justify-center py-10">
              <Spinner />
            </div>
          ) : filtered.length === 0 ? (
            <EmptyState
              title={
                records.length === 0
                  ? t("empty.noRecords")
                  : t("empty.noMatch")
              }
              description={
                records.length === 0
                  ? t("empty.noRecordsHint")
                  : t("empty.noMatchHint")
              }
            />
          ) : (
            <ul className="flex flex-col gap-2">
              {filtered.map((r) => (
                <li
                  key={`${r.ontology_id}:${r.id}`}
                  className="rounded border border-zinc-200 bg-white p-3 dark:border-zinc-700 dark:bg-zinc-900"
                >
                  <ProvenanceRow
                    record={r}
                    showOntology={ontologyFilter === "all" && items.length > 1}
                  />
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </div>
  );
}

function ProvenanceRow({
  record,
  showOntology = false,
}: {
  record: AttributedRecord;
  /** Surface the source ontology badge — only useful in the
   *  cross-ontology aggregate view. Single-ontology view hides it
   *  to avoid visual noise. */
  showOntology?: boolean;
}) {
  const t = useTranslations("settings.governance.audit");
  const at = new Date(record.at_time);
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 flex-wrap">
            {showOntology && (
              <span
                className="max-w-[180px] truncate rounded bg-violet-100 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-violet-700 dark:bg-violet-900/40 dark:text-violet-300"
                title={record.ontology_name}
              >
                {record.ontology_name}
              </span>
            )}
            <span className="rounded bg-zinc-100 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-zinc-700 dark:bg-zinc-800 dark:text-zinc-200">
              {record.activity.kind}
            </span>
            <span className="rounded bg-sky-100 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-sky-700 dark:bg-sky-900/40 dark:text-sky-300">
              {record.agent.kind}
            </span>
            <span className="text-[10px] text-muted-foreground">
              {at.toLocaleString()}
            </span>
          </div>
          <p className="mt-1 font-mono text-[11px] text-zinc-700 dark:text-zinc-300">
            {summariseSubject(record.subject)}
          </p>
          <p className="mt-0.5 text-[11px] text-zinc-600 dark:text-zinc-400">
            {summariseActivity(record.activity)}
            {" · "}
            {summariseAgent(record.agent)}
          </p>
          {(record.used?.length ?? 0) > 0 && (
            <p className="mt-1 text-[10px] text-zinc-500 dark:text-zinc-500">
              {t("usedLabel")}:{" "}
              {(record.used ?? [])
                .slice(0, 5)
                .map(summariseSubject)
                .join(", ")}
              {(record.used?.length ?? 0) > 5 &&
                ` +${(record.used?.length ?? 0) - 5}`}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}

function summariseSubject(s: EntityRef): string {
  switch (s.kind) {
    case "node_instance":
      return `${s.node_type_id} #${s.element_id}`;
    case "edge_instance":
      return `${s.edge_type_id} #${s.element_id}`;
    case "property_value":
      return `${s.node_type_id} #${s.element_id}.${s.property_id}`;
    case "arbitrary":
      return s.label;
  }
}

function summariseActivity(a: ProvenanceActivityKind): string {
  switch (a.kind) {
    case "source_scan":
      return `source ${a.source_id} · mapping ${a.mapping_id}`;
    case "function_eval":
      return `function ${a.function_id}`;
    case "rule_validate":
      return `rule ${a.rule_id} → ${a.outcome}`;
    case "action_execute":
      return `action ${a.action_id}${a.idempotency_key ? ` · key ${a.idempotency_key}` : ""}`;
    case "ontology_edit":
      return `edit · ${a.command_summary}`;
    case "draft_proposal":
      return `${a.model_id} · ${a.prompt_name}@${a.prompt_version}`;
    case "cache_refresh":
      return `cache refresh · mapping ${a.mapping_id}`;
    case "enrichment":
      return `enrichment ${a.enrichment_id}`;
    case "import":
      return `import ${a.format}${a.source_uri ? ` from ${a.source_uri}` : ""}`;
    case "export":
      return `export ${a.format}${a.destination_uri ? ` to ${a.destination_uri}` : ""}`;
  }
}

function summariseAgent(a: AgentRef): string {
  switch (a.kind) {
    case "user":
      return `user ${a.user_id}`;
    case "service":
      return `service ${a.service_id}`;
    case "llm_model":
      return `model ${a.model_id}`;
    case "system":
      return "system";
  }
}
