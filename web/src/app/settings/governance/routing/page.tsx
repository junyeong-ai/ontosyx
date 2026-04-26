"use client";

// Φ6 #1 — admin UI for the ChangeRoutingRule matrix.
//
// Renders every visible row (global defaults + workspace
// overrides), groups by change_type so an admin sees the
// effective rule for each ontology edit category at a glance,
// and offers a "revert to global" action on rows that carry a
// workspace override. Editing the routing variant is structured
// (kind dropdown surfaces the four variants); the inner
// predicate / role-list bodies stay JSON-edit for now because
// the predicate vocabulary keeps growing and a hand-rolled
// form would lag the canonical definition.

import { useMemo } from "react";
import { useTranslations } from "next-intl";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Spinner } from "@/components/ui/spinner";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { SettingsSelect } from "@/components/ui/form-input";
import {
  deleteRoutingRule,
  listRoutingRules,
  upsertRoutingRule,
  type ApprovalRouting,
  type ChangeRoutingRule,
  type RiskLevel,
} from "@/lib/api/governance-routing";

const ROUTING_KINDS: ApprovalRouting["kind"][] = [
  "auto_approve",
  "auto_approve_with_notification",
  "approval_required_unless",
  "approval_required",
];

const RISK_LEVELS: RiskLevel[] = ["low", "medium", "high"];

const RISK_BADGE: Record<RiskLevel, string> = {
  low: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300",
  medium:
    "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300",
  high: "bg-rose-100 text-rose-700 dark:bg-rose-900/30 dark:text-rose-300",
};

export default function GovernanceRoutingPage() {
  const t = useTranslations("settings.governance.routing");
  const qc = useQueryClient();
  const confirm = useConfirm();

  const rulesQuery = useQuery({
    queryKey: ["governance-routing"],
    queryFn: listRoutingRules,
  });

  // Group rules by change_type so the workspace override (if any)
  // sits next to its global default. Effective rule per kind is the
  // higher-priority row (workspace overrides ship with priority
  // 100, defaults with 0).
  const grouped = useMemo(() => {
    const m = new Map<string, ChangeRoutingRule[]>();
    for (const r of rulesQuery.data ?? []) {
      const list = m.get(r.change_type) ?? [];
      list.push(r);
      m.set(r.change_type, list);
    }
    // Sort rows within each group: workspace override first.
    for (const list of m.values()) {
      list.sort((a, b) => Number(b.workspace_scoped) - Number(a.workspace_scoped));
    }
    return [...m.entries()].sort(([a], [b]) => a.localeCompare(b));
  }, [rulesQuery.data]);

  const upsert = useMutation({
    mutationFn: ({
      changeType,
      kind,
      risk,
    }: {
      changeType: string;
      kind: ApprovalRouting["kind"];
      risk: RiskLevel;
    }) => {
      // For variants that carry a body, default to an empty list —
      // the operator refines later through a richer editor (or by
      // hand-editing the JSON until that editor lands).
      const routing: ApprovalRouting =
        kind === "auto_approve"
          ? { kind: "auto_approve" }
          : kind === "auto_approve_with_notification"
            ? { kind: "auto_approve_with_notification", notify_roles: [] }
            : kind === "approval_required_unless"
              ? { kind: "approval_required_unless", skip_predicates: [] }
              : { kind: "approval_required" };
      return upsertRoutingRule(changeType, { routing, risk_level: risk });
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["governance-routing"] });
      toast.success(t("toast.upserted"));
    },
    onError: (e: Error) => toast.error(t("toast.upsertFailed", { error: e.message })),
  });

  const remove = useMutation({
    mutationFn: (changeType: string) => deleteRoutingRule(changeType),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["governance-routing"] });
      toast.success(t("toast.reverted"));
    },
    onError: (e: Error) => toast.error(t("toast.revertFailed", { error: e.message })),
  });

  const handleRevert = async (changeType: string) => {
    const ok = await confirm({
      title: t("confirm.revertTitle"),
      description: t("confirm.revertDescription", { changeType }),
      confirmLabel: t("confirm.revertConfirm"),
      cancelLabel: t("confirm.cancel"),
      variant: "warning",
    });
    if (ok) remove.mutate(changeType);
  };

  if (rulesQuery.isLoading) {
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

      {grouped.length === 0 ? (
        <EmptyState
          title={t("empty.title")}
          description={t("empty.description")}
        />
      ) : (
        <ul className="flex flex-col gap-3">
          {grouped.map(([changeType, rows]) => {
            const effective = rows[0]; // workspace override if present, else default
            const hasOverride = rows.some((r) => r.workspace_scoped);
            return (
              <li
                key={changeType}
                className="rounded border border-zinc-200 bg-white p-3 dark:border-zinc-700 dark:bg-zinc-900"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="font-mono text-sm font-medium text-zinc-900 dark:text-zinc-100">
                        {changeType}
                      </span>
                      {hasOverride && (
                        <span className="rounded bg-violet-100 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-violet-700 dark:bg-violet-900/30 dark:text-violet-300">
                          {t("badges.override")}
                        </span>
                      )}
                      <span
                        className={`rounded px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider ${RISK_BADGE[effective.risk_level]}`}
                      >
                        {effective.risk_level}
                      </span>
                    </div>
                    <p className="mt-1 text-[11px] text-zinc-600 dark:text-zinc-400">
                      {t("currentLabel")}:{" "}
                      <span className="font-mono text-[11px]">
                        {effective.routing.kind}
                      </span>
                    </p>
                  </div>
                  <div className="flex items-center gap-2 flex-wrap">
                    <SettingsSelect
                      label={t("editLabel")}
                      hideLabel
                      defaultValue={effective.routing.kind}
                      onChange={(e) =>
                        upsert.mutate({
                          changeType,
                          kind: e.target.value as ApprovalRouting["kind"],
                          risk: effective.risk_level,
                        })
                      }
                      className="w-56"
                    >
                      {ROUTING_KINDS.map((k) => (
                        <option key={k} value={k}>
                          {t(`routingKinds.${k}`)}
                        </option>
                      ))}
                    </SettingsSelect>
                    <SettingsSelect
                      label={t("riskLabel")}
                      hideLabel
                      defaultValue={effective.risk_level}
                      onChange={(e) =>
                        upsert.mutate({
                          changeType,
                          kind: effective.routing.kind,
                          risk: e.target.value as RiskLevel,
                        })
                      }
                      className="w-28"
                    >
                      {RISK_LEVELS.map((r) => (
                        <option key={r} value={r}>
                          {r}
                        </option>
                      ))}
                    </SettingsSelect>
                    {hasOverride && (
                      <Button
                        variant="ghost"
                        size="xs"
                        onClick={() => handleRevert(changeType)}
                        disabled={remove.isPending}
                      >
                        {t("revertButton")}
                      </Button>
                    )}
                  </div>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
