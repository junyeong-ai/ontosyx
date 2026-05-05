"use client";

// PROV-O audit trail viewer.
//
// Workspace-wide stream of `ProvenanceDef` entries — every source
// scan, rule evaluation, action execution, LLM draft, etc. emitted
// by the platform. Read-only: operators see WHY a fact in the
// graph exists, who ran the activity, and when.
//
// Backed by `GET /api/governance/audit` — the server resolves the
// stream from the content-addressed entity store, applies jsonb-
// path filters at the SQL layer, and cursor-paginates. The page
// drives one infinite query against that endpoint; per-ontology
// scoping is a query-string parameter, not a client-side fan-out.

import { useState, useMemo } from "react";
import { useTranslations } from "next-intl";

import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { SettingsSelect } from "@/components/ui/form-input";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { useOntologies } from "@/hooks/api/use-ontologies";
import { useFormatters } from "@/hooks/use-formatters";
import { useAuditTrail } from "@/hooks/api/use-audit-trail";
import {
  type AgentRef,
  type AuditFilter,
  type AuditRecord,
  type EntityRef,
  type ProvenanceActivityKind,
  ACTIVITY_KINDS,
  AGENT_KINDS,
} from "@/types/audit";

export function ProvenanceAuditFacet() {
  const t = useTranslations("settings.governance.audit.provenance");
  const tCommon = useTranslations("common");
  const ontologies = useOntologies({ limit: 200 });
  const items = ontologies.data?.items ?? [];

  const [ontologyFilter, setOntologyFilter] = useState<string>("all");
  const [activityFilter, setActivityFilter] = useState<string>("all");
  const [agentFilter, setAgentFilter] = useState<string>("all");

  const filter = useMemo<AuditFilter>(
    () => ({
      ontology_id: ontologyFilter === "all" ? undefined : ontologyFilter,
      activity_kind:
        activityFilter === "all"
          ? undefined
          : (activityFilter as ProvenanceActivityKind["kind"]),
      agent_kind:
        agentFilter === "all"
          ? undefined
          : (agentFilter as AgentRef["kind"]),
    }),
    [ontologyFilter, activityFilter, agentFilter],
  );

  const trail = useAuditTrail(filter);
  const records = useMemo<AuditRecord[]>(
    () => trail.data?.pages.flatMap((p) => p.items) ?? [],
    [trail.data],
  );

  const showOntologyBadge = ontologyFilter === "all" && items.length > 1;

  const pageState: PageState = ontologies.isLoading
    ? { kind: "loading" }
    : { kind: "data" };

  return (
    <PageStateView state={pageState} skeleton={<SkeletonList count={6} />}>
      <div className="flex flex-col gap-4">

      {items.length === 0 && (
        <p className="rounded border border-warning-border bg-warning-surface p-3 text-xs text-warning-foreground">
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
              <option value="all">{t("activityKinds.all")}</option>
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
              <option value="all">{t("agentKinds.all")}</option>
              {AGENT_KINDS.map((k) => (
                <option key={k} value={k}>
                  {t(`agentKinds.${k}`)}
                </option>
              ))}
            </SettingsSelect>
            <span className="ms-auto text-2xs text-foreground-muted">
              {t("counts", { count: records.length })}
            </span>
          </div>

          {showOntologyBadge && (
            <p className="text-2xs text-foreground-muted">
              {t("aggregatedHint", { count: items.length })}
            </p>
          )}

          {trail.isLoading ? (
            <SkeletonList count={6} />
          ) : trail.isError ? (
            <ErrorState
              title={t("error.title")}
              description={t("error.description")}
              onRetry={() => trail.refetch()}
              retryLabel={tCommon("retry")}
            />
          ) : records.length === 0 ? (
            <EmptyState
              title={
                isFilterActive(filter)
                  ? t("empty.noMatch")
                  : t("empty.noRecords")
              }
              description={
                isFilterActive(filter)
                  ? t("empty.noMatchHint")
                  : t("empty.noRecordsHint")
              }
            />
          ) : (
            <>
              <ul className="flex flex-col gap-2">
                {records.map((r) => (
                  <li
                    key={`${r.ontology_id}:${r.provenance.id}`}
                    className="rounded border border-divider bg-surface-base p-3"
                  >
                    <ProvenanceRow
                      record={r}
                      showOntology={showOntologyBadge}
                    />
                  </li>
                ))}
              </ul>

              {trail.hasNextPage && (
                <div className="flex justify-center pt-2">
                  <button
                    type="button"
                    onClick={() => trail.fetchNextPage()}
                    disabled={trail.isFetchingNextPage}
                    className="rounded-md border border-divider bg-surface-base px-3 py-1.5 text-xs font-medium text-foreground hover:bg-surface-raised disabled:cursor-not-allowed disabled:opacity-50-strong"
                  >
                    {trail.isFetchingNextPage
                      ? t("loadingMore")
                      : t("loadMore")}
                  </button>
                </div>
              )}
            </>
          )}
        </>
      )}
      </div>
    </PageStateView>
  );
}

function isFilterActive(filter: AuditFilter): boolean {
  return (
    filter.ontology_id !== undefined ||
    filter.activity_kind !== undefined ||
    filter.agent_kind !== undefined ||
    filter.since !== undefined ||
    filter.until !== undefined
  );
}

function ProvenanceRow({
  record,
  showOntology,
}: {
  record: AuditRecord;
  showOntology: boolean;
}) {
  const t = useTranslations("settings.governance.audit.provenance");
  const fmt = useFormatters();
  const p = record.provenance;
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 flex-wrap">
            {showOntology && (
              <span
                className="max-w-[180px] truncate rounded bg-concept-surface px-2 py-0.5 text-2xs font-medium uppercase tracking-wider text-concept-foreground"
                title={record.ontology_name}
              >
                {record.ontology_name}
              </span>
            )}
            <span className="rounded bg-surface-inset px-2 py-0.5 text-2xs font-medium uppercase tracking-wider text-foreground-strong">
              {p.activity.kind}
            </span>
            <span className="rounded bg-info-surface px-2 py-0.5 text-2xs font-medium uppercase tracking-wider text-info-foreground">
              {p.agent.kind}
            </span>
            <span className="text-2xs text-foreground-muted">
              {fmt.date(record.at_time)}
            </span>
          </div>
          <p className="mt-1 font-mono text-2xs text-foreground">
            {summariseSubject(p.subject)}
          </p>
          <p className="mt-0.5 text-2xs text-foreground-muted">
            {summariseActivity(p.activity)}
            {" · "}
            {summariseAgent(p.agent)}
          </p>
          {(p.used?.length ?? 0) > 0 && (
            <p className="mt-1 text-2xs text-foreground-subtle">
              {t("usedLabel")}:{" "}
              {(p.used ?? []).slice(0, 5).map(summariseSubject).join(", ")}
              {(p.used?.length ?? 0) > 5 &&
                ` +${(p.used?.length ?? 0) - 5}`}
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
