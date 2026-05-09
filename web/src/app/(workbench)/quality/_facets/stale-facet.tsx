"use client";

import { useEffect, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { Clock } from "lucide-react";
import { toast } from "@/components/ui/toast";
import { Button } from "@/components/ui/button";
import { BulkActionBar } from "@/components/ui/bulk-action-bar";
import { DataTable, type ColumnDef } from "@/components/ui/data-table";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import {
  FieldLabelText,
  FormSelect,
  FormTextarea,
} from "@/components/ui/form-input";
import { Modal } from "@/components/ui/modal";
import { SkeletonTable } from "@/components/ui/skeleton";
import {
  StatusBadge,
  type StatusTone,
} from "@/components/ui/status-badge";
import { useChromeFilters } from "@/components/workbench/workbench-page-shell";
import { useTableUrlState } from "@/hooks/use-table-url-state";

import {
  useBulkDecideStaleProposals,
  useDecideStaleProposal,
  useStaleProposals,
} from "@/hooks/api/use-quality";
import type { BindingEditOp } from "@/lib/api/binding-suggestions";
import { submitOntologyEdits } from "@/lib/api/edit-ops";
import {
  listTypeCandidates,
  type TypeCandidate,
} from "@/lib/api/quality";
import type { StaleConceptProposal } from "@/types/api";

type StatusFilter = "pending" | "decided" | "all";

const STATUS_TONE: Record<StaleConceptProposal["decision"], StatusTone> = {
  pending: "warning",
  approved: "brand",
  dismissed: "neutral",
};

const VALID_STATUS = new Set<StatusFilter>(["pending", "decided", "all"]);

export function StaleFacet() {
  const t = useTranslations("settings.quality.stale");
  const tCommon = useTranslations("common");
  const url = useTableUrlState({ filters: ["status"] });
  const statusFilter: StatusFilter = (() => {
    const raw = url.filters.status;
    return raw && VALID_STATUS.has(raw as StatusFilter)
      ? (raw as StatusFilter)
      : "pending";
  })();

  const [editing, setEditing] = useState<StaleConceptProposal | null>(null);
  const [picker, setPicker] = useState<{
    proposal: StaleConceptProposal;
    candidates: TypeCandidate[];
  } | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  // biome-ignore lint/correctness/useExhaustiveDependencies: filter drives reset
  useEffect(() => {
    setSelectedIds(new Set());
  }, [statusFilter]);

  const allRowsQuery = useStaleProposals(true);
  const allRows = allRowsQuery.data ?? [];
  const counts = useMemo(() => {
    const pending = allRows.filter((r) => r.decision === "pending").length;
    const decided = allRows.length - pending;
    return { pending, decided, all: allRows.length };
  }, [allRows]);

  const visibleRows = useMemo(() => {
    if (statusFilter === "pending") {
      return allRows.filter((r) => r.decision === "pending");
    }
    if (statusFilter === "decided") {
      return allRows.filter((r) => r.decision !== "pending");
    }
    return allRows;
  }, [allRows, statusFilter]);

  const bulkDismiss = useBulkDecideStaleProposals();
  const handleBulkDismiss = () => {
    const ids = Array.from(selectedIds);
    if (ids.length === 0) return;
    bulkDismiss.mutate(
      { ids, decision: "dismissed" },
      {
        onSuccess: ({ decided }) => {
          setSelectedIds(new Set());
          toast.success(t("toast.bulkDismissed", { count: decided }));
        },
        onError: (err) =>
          toast.error(err instanceof Error ? err.message : t("toast.failed")),
      },
    );
  };

  const emitDeprecate = async (
    proposal: StaleConceptProposal,
    candidate: TypeCandidate,
    reason?: string,
  ) => {
    const expected = Number.parseInt(candidate.current_version, 10);
    if (!Number.isFinite(expected)) {
      toast.error(t("toast.emitFailed"));
      return;
    }
    const kind = normaliseKind(proposal.type_kind);
    const op: BindingEditOp =
      kind === "edge"
        ? { op: "deprecate_edge_type", id: proposal.type_id }
        : { op: "deprecate_node_type", id: proposal.type_id };
    try {
      await submitOntologyEdits(candidate.ontology_id, {
        expected_version: expected,
        operations: [op],
        message: reason
          ? `deprecated via stale-proposal approval: ${reason}`
          : "deprecated via stale-proposal approval",
      });
      toast.success(
        t("toast.deprecated", { ontology: candidate.ontology_name }),
      );
    } catch (e) {
      toast.error(
        e instanceof Error ? e.message : t("toast.emitFailed"),
      );
    }
  };

  const decide = useDecideStaleProposal({
    onSuccess: async (row, vars) => {
      toast.success(
        row.decision === "approved"
          ? t("toast.approved")
          : t("toast.dismissed"),
      );
      setEditing(null);
      if (row.decision !== "approved") return;

      const kind = normaliseKind(row.type_kind);
      let candidates: TypeCandidate[] = [];
      try {
        candidates = await listTypeCandidates(row.type_id, kind);
      } catch {
        toast.error(t("toast.lookupFailed"));
        return;
      }
      const usable = candidates.filter((c) => !c.deprecated_at);
      if (usable.length === 0) {
        if (candidates.length > 0) {
          toast.info(t("toast.alreadyDeprecated"));
        } else {
          toast.info(t("toast.noCandidate"));
        }
        return;
      }
      if (usable.length === 1) {
        await emitDeprecate(row, usable[0], vars.reason);
        return;
      }
      setPicker({ proposal: row, candidates: usable });
    },
    onError: (err) =>
      toast.error(err instanceof Error ? err.message : t("toast.failed")),
  });

  const chromeFilters = useChromeFilters(
    <FormSelect
      density="settings"
      aria-label={t("filter.statusLabel")}
      value={statusFilter}
      onChange={(e) =>
        url.setFilter(
          "status",
          e.target.value === "pending" ? null : e.target.value,
        )
      }
      className="w-auto"
    >
      <option value="pending">
        {t("filter.pending", { count: counts.pending })}
      </option>
      <option value="decided">
        {t("filter.decided", { count: counts.decided })}
      </option>
      <option value="all">
        {t("filter.all", { count: counts.all })}
      </option>
    </FormSelect>,
  );

  const columns = useMemo<ColumnDef<StaleConceptProposal, unknown>[]>(
    () => [
      {
        id: "type_id",
        header: t("columns.type"),
        accessorKey: "type_id",
        cell: ({ getValue }) => (
          <span className="font-mono">{getValue<string>()}</span>
        ),
      },
      {
        id: "type_kind",
        header: t("columns.kind"),
        accessorKey: "type_kind",
        cell: ({ getValue }) => (
          <span className="text-foreground-muted">{getValue<string>()}</span>
        ),
      },
      {
        id: "days_since_last_use",
        header: t("columns.daysSince"),
        accessorKey: "days_since_last_use",
        cell: ({ getValue }) => (
          <span className="tabular-nums">{getValue<number>()}</span>
        ),
      },
      {
        id: "proposed_at",
        header: t("columns.proposedAt"),
        accessorKey: "proposed_at",
        cell: ({ getValue }) => (
          <span className="text-foreground-muted">
            {new Date(getValue<string>()).toLocaleDateString()}
          </span>
        ),
      },
      {
        id: "decision",
        header: t("columns.decision"),
        accessorKey: "decision",
        cell: ({ row }) => (
          <StatusBadge tone={STATUS_TONE[row.original.decision]} size="sm">
            {t(`decisionLabel.${row.original.decision}`)}
          </StatusBadge>
        ),
      },
      {
        id: "reason",
        header: t("columns.reason"),
        accessorKey: "reason",
        enableSorting: false,
        cell: ({ getValue }) => (
          <span className="text-foreground-muted">
            {getValue<string | null>() ?? "—"}
          </span>
        ),
      },
      {
        id: "actions",
        header: t("columns.actions"),
        enableSorting: false,
        meta: { headerClass: "text-end", cellClass: "text-end" },
        cell: ({ row }) =>
          row.original.decision === "pending" ? (
            <Button
              variant="ghost"
              size="xs"
              onClick={() => setEditing(row.original)}
            >
              {t("actions.review")}
            </Button>
          ) : null,
      },
    ],
    [t],
  );

  return (
    <div className="flex flex-col gap-4">
      {chromeFilters}

      {allRowsQuery.isLoading && <SkeletonTable rows={4} cols={7} />}

      {allRowsQuery.isError && (
        <ErrorState
          title={tCommon("loadError.title")}
          description={tCommon("loadError.description")}
          onRetry={() => allRowsQuery.refetch()}
          retryLabel={tCommon("retry")}
        />
      )}

      {!allRowsQuery.isLoading && !allRowsQuery.isError && (
        <DataTable<StaleConceptProposal>
          columns={columns}
          data={visibleRows}
          rowId={(row) => row.id}
          sort={url.sort}
          onSortChange={url.setSort}
          selectedIds={selectedIds}
          onSelectionChange={setSelectedIds}
          isRowSelectable={(row) => row.decision === "pending"}
          selectionLabels={{
            selectAll: t("selectAll"),
            selectRow: t("selectAllRow"),
          }}
          ariaLabel={t("title")}
          emptyState={
            <EmptyState
              kind={statusFilter === "all" ? "no-data" : "no-results"}
              icon={Clock}
              title={t(`empty.${statusFilter}`)}
            />
          }
        />
      )}

      <BulkActionBar
        count={selectedIds.size}
        countLabel={t("bulkSelectedCount", { count: selectedIds.size })}
        clearLabel={t("bulkClear")}
        ariaLabel={t("bulkBarLabel")}
        actions={[
          {
            key: "dismiss",
            label: t("bulkDismiss"),
            variant: "primary",
            onClick: handleBulkDismiss,
          },
        ]}
        onClear={() => setSelectedIds(new Set())}
        pending={bulkDismiss.isPending}
      />

      <DecisionModal
        proposal={editing}
        busy={decide.isPending}
        onCancel={() => setEditing(null)}
        onSubmit={(decision, reason) =>
          editing && decide.mutate({ id: editing.id, decision, reason })
        }
      />

      <CandidatePickerModal
        picker={picker}
        onCancel={() => setPicker(null)}
        onPick={async (candidate) => {
          if (!picker) return;
          const snapshot = picker;
          setPicker(null);
          await emitDeprecate(snapshot.proposal, candidate);
        }}
      />
    </div>
  );
}

function DecisionModal({
  proposal,
  busy,
  onSubmit,
  onCancel,
}: {
  proposal: StaleConceptProposal | null;
  busy: boolean;
  onSubmit: (decision: "approved" | "dismissed", reason?: string) => void;
  onCancel: () => void;
}) {
  const t = useTranslations("settings.quality.stale.modal");
  const [reason, setReason] = useState("");

  useEffect(() => {
    if (proposal) setReason("");
  }, [proposal]);

  const open = proposal !== null;
  return (
    <Modal
      open={open}
      onOpenChange={(next) => {
        if (!next) onCancel();
      }}
      title={t("title")}
      description={
        proposal
          ? `${proposal.type_id} · ${t("daysSince", {
              days: proposal.days_since_last_use,
            })}`
          : undefined
      }
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onCancel}>
            {t("cancel")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              onSubmit("dismissed", reason.trim() || undefined)
            }
            disabled={busy}
          >
            {t("dismiss")}
          </Button>
          <Button
            variant="primary"
            size="sm"
            onClick={() =>
              onSubmit("approved", reason.trim() || undefined)
            }
            disabled={busy}
            loading={busy}
          >
            {t("approve")}
          </Button>
        </>
      }
    >
      <label htmlFor="stale-reason" className="mb-1 block">
        <FieldLabelText label={t("reasonLabel")} />
      </label>
      <FormTextarea
        id="stale-reason"
        value={reason}
        onChange={(e) => setReason(e.target.value)}
        rows={3}
        placeholder={t("reasonPlaceholder")}
        density="settings"
      />
    </Modal>
  );
}

function CandidatePickerModal({
  picker,
  onPick,
  onCancel,
}: {
  picker: {
    proposal: StaleConceptProposal;
    candidates: TypeCandidate[];
  } | null;
  onPick: (candidate: TypeCandidate) => void;
  onCancel: () => void;
}) {
  const t = useTranslations("settings.quality.stale.picker");
  const open = picker !== null;
  return (
    <Modal
      open={open}
      onOpenChange={(next) => {
        if (!next) onCancel();
      }}
      title={t("title")}
      description={
        picker
          ? `${picker.proposal.type_id} · ${t("countHit", {
              count: picker.candidates.length,
            })}`
          : undefined
      }
      footer={
        <Button variant="ghost" size="sm" onClick={onCancel}>
          {t("skip")}
        </Button>
      }
    >
      <ul className="-mx-5 max-h-72 divide-y divide-divider-soft overflow-y-auto">
        {picker?.candidates.map((c) => (
          <li key={`${c.ontology_id}-${c.current_version}`}>
            <button
              type="button"
              onClick={() => onPick(c)}
              className="flex w-full items-start justify-between gap-3 px-5 py-3 text-start text-xs hover:bg-concept-surface"
            >
              <div className="min-w-0">
                <p className="truncate font-medium text-foreground-strong">
                  {c.ontology_name}
                </p>
                <p className="mt-0.5 truncate text-2xs text-foreground-muted">
                  <span className="font-mono">{c.label}</span>
                  {" · "}v{c.current_version}
                </p>
              </div>
              <span className="rounded bg-brand-surface px-2 py-0.5 text-2xs text-brand-foreground-strong">
                {t("pick")}
              </span>
            </button>
          </li>
        ))}
      </ul>
    </Modal>
  );
}

function normaliseKind(raw: string): "node" | "edge" {
  const lc = raw.toLowerCase();
  if (lc === "edge" || lc === "edgetype" || lc === "edge_type") return "edge";
  return "node";
}
