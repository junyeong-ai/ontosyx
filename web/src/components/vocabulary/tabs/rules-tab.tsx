"use client";

// Rules tab — master-detail rules editor. Shares the
// `EntityWorkbench` shell with the other vocabulary tabs so the
// SHACL rule list, the structural editor, and a future usage map
// stay aligned with CodeSystem / ValueSet / ConceptMap layouts.
// `RuleForm` is the central editor and stays modal-free; the only
// dialog still in the flow is the destructive delete confirmation
// (industry pattern — Linear / Stripe Dashboard reserve modals for
// confirmations only).

import { useCallback, useEffect, useMemo, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { FormInput } from "@/components/ui/form-input";
import { SkeletonList } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { useConfirm } from "@/components/providers/confirm-provider";
import { EntityWorkbench } from "@/components/workbench/entity-workbench";
import { RuleForm } from "@/components/vocabulary/rule-form";
import { useWorkspaceOntology } from "@/hooks/api/use-workspace-ontology";
import { useApplyOntologyEdits } from "@/hooks/api/use-ontology-edits";
import type { OntologyEditOp, RuleDef } from "@/lib/api/edit-ops";
import { localizePresent } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";
import { cn } from "@/lib/cn";

const DRAFT_ID = "__new__";
const SELECTION_PARAM = "rule";

function freshRuleId(): string {
  return `rule-${crypto.randomUUID().slice(0, 8)}`;
}

export function RulesTab() {
  const t = useTranslations("settings.vocabulary.rules");
  const tWorkbench = useTranslations("settings.vocabulary.workbench");
  const tCommon = useTranslations("common");
  const router = useRouter();
  const searchParams = useSearchParams();
  const detail = useWorkspaceOntology();
  const ontology = detail.data ?? null;
  const apply = useApplyOntologyEdits(ontology?.id);
  const confirm = useConfirm();

  const rules = useMemo<RuleDef[]>(
    () => (detail.data?.ontology_ir?.rules as RuleDef[] | undefined) ?? [],
    [detail.data?.ontology_ir?.rules],
  );
  const expectedVersion =
    Number(detail.data?.current_version?.version ?? "0") || 0;

  const [search, setSearch] = useState("");
  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    if (!needle) return rules;
    return rules.filter((r) => r.id.toLowerCase().includes(needle));
  }, [rules, search]);

  const selectedId = searchParams.get(SELECTION_PARAM);
  const isDraft = selectedId === DRAFT_ID;
  const selected =
    selectedId && !isDraft
      ? rules.find((r) => r.id === selectedId) ?? null
      : null;

  const setSelection = useCallback(
    (id: string | null) => {
      const next = new URLSearchParams(searchParams);
      if (id === null) next.delete(SELECTION_PARAM);
      else next.set(SELECTION_PARAM, id);
      const qs = next.toString();
      router.replace(qs ? `?${qs}` : "?");
    },
    [router, searchParams],
  );

  // Auto-select first item on initial load so the editor is never blank.
  useEffect(() => {
    if (selectedId === null && rules.length > 0 && !isDraft) {
      setSelection(rules[0].id);
    }
  }, [selectedId, rules, isDraft, setSelection]);

  const submit = (operations: OntologyEditOp[], message: string) =>
    apply.mutateAsync({
      operations,
      expected_version: expectedVersion,
      message,
    });

  const handleCreate = async (def: RuleDef) => {
    if (!ontology?.id) return;
    const withId: RuleDef = { ...def, id: def.id || freshRuleId() };
    try {
      await submit(
        [{ op: "create_rule", def: withId }],
        t("messages.created", { id: withId.id }),
      );
      toast.success(t("toast.created", { id: withId.id }));
      setSelection(withId.id);
    } catch (err) {
      toast.error(t("toast.createFailed", { error: (err as Error).message }));
    }
  };

  const handleUpdate = async (def: RuleDef) => {
    if (!ontology?.id || !selected) return;
    try {
      await submit(
        [{ op: "update_rule", id: selected.id, def }],
        t("messages.updated", { id: selected.id }),
      );
      toast.success(t("toast.updated", { id: selected.id }));
    } catch (err) {
      toast.error(t("toast.updateFailed", { error: (err as Error).message }));
    }
  };

  const handleDelete = async () => {
    if (!ontology?.id || !selected) return;
    const ok = await confirm({
      title: t("confirm.deleteTitle"),
      description: t("confirm.deleteDescription", { name: selected.id }),
      variant: "danger",
    });
    if (!ok) return;
    try {
      await submit(
        [{ op: "delete_rule", id: selected.id }],
        t("messages.deleted", { name: selected.id }),
      );
      toast.success(t("toast.deleted", { name: selected.id }));
      setSelection(null);
    } catch (err) {
      toast.error(t("toast.deleteFailed", { error: (err as Error).message }));
    }
  };

  if (detail.isLoading) {
    return <SkeletonList count={4} />;
  }

  if (detail.isError) {
    return (
      <ErrorState
        title={tCommon("loadError.title")}
        description={tCommon("loadError.description")}
        onRetry={() => detail.refetch()}
        retryLabel={tCommon("retry")}
      />
    );
  }

  if (!ontology) {
    return (
      <p className="rounded border border-warning-border bg-warning-surface p-3 text-xs text-warning-foreground">
        {t("noOntology")}
      </p>
    );
  }

  return (
    <EntityWorkbench<RuleDef>
      listPane={
        <ListPane
          rules={filtered}
          selectedId={selectedId}
          onSelect={setSelection}
          onCreate={() => setSelection(DRAFT_ID)}
          search={search}
          onSearch={setSearch}
          createLabel={t("createButton")}
          searchPlaceholder={tWorkbench("searchPlaceholder")}
          emptyTitle={t("empty.title")}
          emptyDescription={t("empty.description")}
          busy={apply.isPending}
        />
      }
      detailPane={
        <DetailPane
          key={isDraft ? "__draft__" : selected?.id ?? "__empty__"}
          isDraft={isDraft}
          selected={selected}
          ontologyId={ontology.id}
          onCreate={handleCreate}
          onUpdate={handleUpdate}
          onDelete={handleDelete}
          onCancelDraft={() => setSelection(null)}
          pending={apply.isPending}
          deleteLabel={t("deleteButton")}
          createTitle={t("createDialog.title")}
          nothingTitle={tWorkbench("nothingSelected.title")}
          nothingDescription={tWorkbench("nothingSelected.description")}
        />
      }
      selected={selected}
    />
  );
}

interface ListPaneProps {
  rules: RuleDef[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
  search: string;
  onSearch: (value: string) => void;
  createLabel: string;
  searchPlaceholder: string;
  emptyTitle: string;
  emptyDescription: string;
  busy: boolean;
}

function ListPane({
  rules,
  selectedId,
  onSelect,
  onCreate,
  search,
  onSearch,
  createLabel,
  searchPlaceholder,
  emptyTitle,
  emptyDescription,
  busy,
}: ListPaneProps) {
  const t = useTranslations("settings.vocabulary.rules");
  const localeChain = useLocaleChain();
  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-divider px-3 py-2">
        <h2 className="flex-1 text-xs font-semibold text-foreground-strong">
          {t("listHeading", { count: rules.length })}
        </h2>
        <button
          type="button"
          onClick={onCreate}
          disabled={busy}
          aria-label={createLabel}
          className="inline-flex items-center gap-1 rounded bg-brand-solid px-2 py-1 text-2xs font-semibold uppercase tracking-wider text-foreground-onbrand shadow-1 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-brand-solid disabled:opacity-50"
        >
          <span aria-hidden className="text-sm leading-none">
            +
          </span>
          {createLabel}
        </button>
      </div>
      <div className="border-b border-divider px-3 py-2">
        <FormInput
          type="search"
          value={search}
          onChange={(e) => onSearch(e.target.value)}
          placeholder={searchPlaceholder}
          density="compact"
        />
      </div>
      {rules.length === 0 ? (
        <div className="flex flex-1 items-center justify-center px-4 py-6">
          <EmptyState title={emptyTitle} description={emptyDescription} />
        </div>
      ) : (
        <ul className="flex-1 overflow-y-auto py-1">
          {rules.map((rule) => {
            const isSelected = rule.id === selectedId;
            const name = rule.name
              ? localizePresent(rule.name, localeChain)
              : null;
            const isDerived = rule.origin?.kind === "derived_from_binding";
            return (
              <li key={rule.id}>
                <button
                  type="button"
                  onClick={() => onSelect(rule.id)}
                  className={cn(
                    "flex w-full flex-col items-start gap-0.5 px-3 py-2 text-left transition-colors duration-[var(--duration-quick)]",
                    isSelected
                      ? "bg-brand-surface text-brand-foreground"
                      : "text-foreground hover:bg-surface-hover",
                  )}
                >
                  <div className="flex w-full min-w-0 items-center gap-1.5">
                    <span className="truncate font-mono text-xs font-medium">
                      {rule.id}
                    </span>
                    {rule.severity && (
                      <span
                        className={cn(
                          "shrink-0 rounded px-1 py-0.5 text-2xs font-medium uppercase",
                          severityClass(rule.severity),
                        )}
                      >
                        {t(`severity.${rule.severity}`)}
                      </span>
                    )}
                    {isDerived && (
                      <span className="shrink-0 rounded bg-concept-surface px-1 py-0.5 text-2xs font-medium uppercase text-concept-foreground">
                        {t("derivedBadge")}
                      </span>
                    )}
                  </div>
                  <div className="mt-0.5 truncate text-2xs text-foreground-muted">
                    {name && name !== rule.id ? name + " · " : ""}
                    {t("constraintCount", {
                      count: rule.constraints?.length ?? 0,
                    })}
                  </div>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

interface DetailPaneProps {
  isDraft: boolean;
  selected: RuleDef | null;
  ontologyId: string | undefined;
  onCreate: (def: RuleDef) => Promise<void>;
  onUpdate: (def: RuleDef) => Promise<void>;
  onDelete: () => Promise<void>;
  onCancelDraft: () => void;
  pending: boolean;
  deleteLabel: string;
  createTitle: string;
  nothingTitle: string;
  nothingDescription: string;
}

function DetailPane({
  isDraft,
  selected,
  ontologyId,
  onCreate,
  onUpdate,
  onDelete,
  onCancelDraft,
  pending,
  deleteLabel,
  createTitle,
  nothingTitle,
  nothingDescription,
}: DetailPaneProps) {
  if (!isDraft && !selected) {
    return (
      <div className="flex h-full items-center justify-center px-6 py-12">
        <EmptyState title={nothingTitle} description={nothingDescription} />
      </div>
    );
  }
  const title = isDraft ? createTitle : selected?.id ?? "";
  const isDerived = selected?.origin?.kind === "derived_from_binding";
  return (
    <div className="flex h-full min-w-0 flex-col">
      <header className="flex items-center gap-3 border-b border-divider px-4 py-3">
        <h2 className="flex-1 truncate font-mono text-sm font-semibold text-foreground-strong">
          {title}
        </h2>
        {!isDraft && selected && !isDerived && (
          <Button
            variant="danger"
            size="sm"
            onClick={onDelete}
            disabled={pending}
          >
            {deleteLabel}
          </Button>
        )}
      </header>
      <div className="flex-1 overflow-y-auto px-4 py-4">
        <RuleForm
          initial={isDraft ? undefined : selected ?? undefined}
          ontologyId={ontologyId}
          onSubmit={isDraft ? onCreate : onUpdate}
          onCancel={isDraft ? onCancelDraft : () => undefined}
          pending={pending}
        />
      </div>
    </div>
  );
}

function severityClass(severity: RuleDef["severity"]): string {
  switch (severity) {
    case "violation":
      return "bg-danger-surface text-danger-foreground";
    case "warning":
      return "bg-warning-surface text-warning-foreground";
    case "info":
      return "bg-info-surface text-info-foreground";
    default:
      return "bg-surface-inset text-foreground-muted";
  }
}
