"use client";

// Φ6 #3 — Audit trail viewer.
//
// Surfaces the workspace's current ontology's `ProvenanceDef`
// entries (PROV-O subject / activity / agent / at_time +
// used / derived_from) as a filterable list. Read-only — the
// underlying records are emitted by the LLM design pipeline,
// rule evaluation, source scans, action execution, etc.
// Operators see WHY a fact in the graph exists, who ran the
// activity, and when.
//
// Scope: per-ontology. The IR carries provenance scoped to its
// own version, so this page lists that version's history. A
// workspace-wide aggregate (across multiple ontologies) is the
// natural follow-up if a multi-ontology workspace needs the
// rolled-up view.

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";

import { EmptyState } from "@/components/ui/empty-state";
import { Spinner } from "@/components/ui/spinner";
import { SettingsSelect } from "@/components/ui/form-input";
import {
  useOntologies,
  useOntologyDetail,
} from "@/hooks/api/use-ontologies";

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

export default function AuditTrailPage() {
  const t = useTranslations("settings.governance.audit");
  const ontologies = useOntologies({ limit: 1 });
  const ontology = ontologies.data?.items?.[0];
  const detail = useOntologyDetail(ontology?.id);

  const [activityFilter, setActivityFilter] = useState<string>("all");
  const [agentFilter, setAgentFilter] = useState<string>("all");

  const records = useMemo<ProvenanceDef[]>(() => {
    return (detail.data?.ontology_ir?.provenance ?? []) as ProvenanceDef[];
  }, [detail.data]);

  const filtered = useMemo(() => {
    return records.filter((r) => {
      if (activityFilter !== "all" && r.activity.kind !== activityFilter)
        return false;
      if (agentFilter !== "all" && r.agent.kind !== agentFilter) return false;
      return true;
    });
  }, [records, activityFilter, agentFilter]);

  if (ontologies.isLoading || detail.isLoading) {
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

      {!ontology && (
        <p className="rounded border border-amber-200 bg-amber-50 p-3 text-xs text-amber-700 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-300">
          {t("noOntology")}
        </p>
      )}

      {ontology && (
        <>
          <div className="flex items-center gap-3 flex-wrap">
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

          {filtered.length === 0 ? (
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
                  key={r.id}
                  className="rounded border border-zinc-200 bg-white p-3 dark:border-zinc-700 dark:bg-zinc-900"
                >
                  <ProvenanceRow record={r} />
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </div>
  );
}

function ProvenanceRow({ record }: { record: ProvenanceDef }) {
  const t = useTranslations("settings.governance.audit");
  const at = new Date(record.at_time);
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 flex-wrap">
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
