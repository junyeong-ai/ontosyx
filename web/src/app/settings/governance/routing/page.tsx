"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { EmptyState } from "@/components/ui/empty-state";
import { Spinner } from "@/components/ui/spinner";
import { useConfirm } from "@/components/providers/confirm-provider";
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
  low: "bg-brand-surface-strong text-brand-foreground-strong-strong",
  medium:
    "bg-warning-surface text-warning-foreground",
  high: "bg-danger-surface text-danger-foreground dark:bg-danger-foreground/30 dark:text-danger-foreground",
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
    <SettingsPageShell title={t("pageTitle")} subtitle={t("pageSubtitle")}>
      <div className="flex flex-col gap-4">

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
                className="rounded border border-divider bg-surface-base p-3"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="font-mono text-sm font-medium text-foreground-strong">
                        {changeType}
                      </span>
                      {hasOverride && (
                        <span className="rounded bg-concept-surface px-2 py-0.5 text-2xs font-medium uppercase tracking-wider text-concept-foreground">
                          {t("badges.override")}
                        </span>
                      )}
                      <span
                        className={`rounded px-2 py-0.5 text-2xs font-medium uppercase tracking-wider ${RISK_BADGE[effective.risk_level]}`}
                      >
                        {effective.risk_level}
                      </span>
                    </div>
                    <p className="mt-1 text-[11px] text-foreground-muted">
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
    </SettingsPageShell>
  );
}
