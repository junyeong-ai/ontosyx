"use client";

// Master-detail page for vocabulary admin surfaces (CodeSystem,
// ValueSet, ConceptMap, NotationPattern, Mapping, Rule). Built on
// `EntityWorkbench` so every vocabulary tab carries the same
// list-pane + always-visible editor + usage-pane layout that the
// glossary surface uses. Industry pattern (Linear settings, Stripe
// Dashboard, Sanity Studio) — modal/dialog flows are reserved for
// destructive confirms; CRUD authoring stays inline.
//
// The detail editor is the schema-driven `StructuredEntityEditor` —
// every tab ships its own typed schema. The page itself does not
// manage form state.

import { type ReactNode, useMemo, useState } from "react";
import {
  DRAFT_ID,
  useMasterDetailSelection,
} from "@/hooks/use-master-detail-selection";
import { useTranslations } from "next-intl";
import { useRouter } from "next/navigation";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { FormInput } from "@/components/ui/form-input";
import { SkeletonList } from "@/components/ui/skeleton";
import { toast } from "@/components/ui/toast";
import { useConfirm } from "@/components/providers/confirm-provider";
import { StructuredEntityEditor } from "@/components/forms/structured-entity-editor";
import { EntityWorkbench } from "@/components/workbench/entity-workbench";
import {
  WORKBENCH_GUTTER,
  WORKBENCH_GUTTER_X,
} from "@/components/workbench/workbench-page-shell";
import { useWorkspaceOntology } from "@/hooks/api/use-workspace-ontology";
import { useApplyOntologyEdits } from "@/hooks/api/use-ontology-edits";
import type { OntologyEditOp } from "@/lib/api/edit-ops";
import type { EntitySchema } from "@/lib/forms/field-schema";
import { cn } from "@/lib/cn";
import type { OntologyIR } from "@/types/ontology";

import { Heading } from "@/components/ui/heading";
export interface MasterDetailEntityPageLabels {
  /** Page title. */
  title: string;
  /** "No committed ontology yet" prerequisite-empty title. */
  noOntology: string;
  /** Description below `noOntology` — explains the recovery (Design mode). */
  noOntologyDescription: string;
  /** Action label that opens Design mode from the prerequisite-empty state. */
  openDesign: string;
  /** List-pane heading — "코드 시스템 N개" / "Code system" pattern. */
  listHeading: (count: number) => string;
  /** Top-right "Add" button label. */
  createButton: string;
  /** Detail-header "Delete" button label. */
  deleteButton: string;
  /** Empty state. */
  emptyTitle: string;
  emptyDescription: string;
  /** Confirm dialog copy. `name` placeholder filled with item id. */
  confirmDeleteTitle: string;
  confirmDeleteDescription: (name: string) => string;
  /** Toasts. */
  createdToast: string;
  createFailedToast: (error: string) => string;
  updatedToast: string;
  updateFailedToast: (error: string) => string;
  deletedToast: string;
  deleteFailedToast: (error: string) => string;
  /** Edit-log messages — `name` resolves to the item id. */
  createdMessage: (name: string) => string;
  updatedMessage: (name: string) => string;
  deletedMessage: (name: string) => string;
  /** Header title shown when the detail pane is in draft-create mode. */
  createDialogTitle: string;
  /** Failure ErrorState copy. */
  loadErrorTitle: string;
  loadErrorDescription: string;
  retryLabel: string;
}

export interface MasterDetailEntityPageProps<T extends { id?: string }> {
  labels: MasterDetailEntityPageLabels;
  /** Resolve the collection slice from the ontology IR. */
  selectItems: (ir: OntologyIR) => T[];
  /** Stable id for an item — used for selection + deep-linking. */
  itemId: (item: T) => string;
  /** Render the row body in the list pane. */
  renderRow: (item: T) => ReactNode;
  /** Build the create / update / delete OntologyEditOps. */
  buildCreateOp: (def: T) => OntologyEditOp;
  buildUpdateOp: (id: string, def: T) => OntologyEditOp;
  buildDeleteOp: (id: string) => OntologyEditOp;
  /** Optional reverse-pointer renderer — drives the right pane. */
  renderUsage?: (item: T, ontology: OntologyIR) => ReactNode;
  /** Schema-driven structured editor. */
  schema: EntitySchema<T>;
  /**
   * URL search-param key used to round-trip selection. Defaults to
   * `id` — vocabulary tabs share a single page surface so the
   * default avoids per-tab divergence.
   */
  selectionParam?: string;
}

export function MasterDetailEntityPage<T extends { id?: string }>({
  labels,
  selectItems,
  itemId,
  renderRow,
  buildCreateOp,
  buildUpdateOp,
  buildDeleteOp,
  renderUsage,
  schema,
  selectionParam = "id",
}: MasterDetailEntityPageProps<T>) {
  const t = useTranslations("settings.vocabulary.workbench");
  const confirm = useConfirm();
  const router = useRouter();
  const openDesign = () => router.push("/design");

  const detail = useWorkspaceOntology();
  const ontology = detail.data ?? null;
  const apply = useApplyOntologyEdits(ontology?.id);

  const items: T[] = detail.data?.ontology_ir
    ? selectItems(detail.data.ontology_ir)
    : [];
  const expectedVersion =
    Number(detail.data?.current_version?.version ?? "0") || 0;

  const [search, setSearch] = useState("");
  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    if (!needle) return items;
    return items.filter((item) => itemId(item).toLowerCase().includes(needle));
  }, [items, search, itemId]);

  // Selection round-trips through the URL — see the hook for the
  // full deep-link / back-button / draft-state contract.
  const { selectedId, selected, isDraft, setSelection } =
    useMasterDetailSelection({
      items,
      itemId,
      selectionParam,
    });

  const submit = (operations: OntologyEditOp[], message: string) =>
    apply.mutateAsync({
      operations,
      expected_version: expectedVersion,
      message,
    });

  const handleCreate = async (def: T) => {
    if (!ontology?.id) return;
    try {
      const id = itemId(def);
      await submit([buildCreateOp(def)], labels.createdMessage(id));
      toast.success(labels.createdToast);
      setSelection(id);
    } catch (err) {
      toast.error(labels.createFailedToast((err as Error).message));
    }
  };

  const handleUpdate = async (def: T) => {
    if (!ontology?.id || !selected) return;
    const id = itemId(selected);
    try {
      await submit([buildUpdateOp(id, def)], labels.updatedMessage(id));
      toast.success(labels.updatedToast);
    } catch (err) {
      toast.error(labels.updateFailedToast((err as Error).message));
    }
  };

  const handleDelete = async () => {
    if (!ontology?.id || !selected) return;
    const id = itemId(selected);
    const ok = await confirm({
      title: labels.confirmDeleteTitle,
      description: labels.confirmDeleteDescription(id),
      variant: "danger",
    });
    if (!ok) return;
    try {
      await submit([buildDeleteOp(id)], labels.deletedMessage(id));
      toast.success(labels.deletedToast);
      setSelection(null);
    } catch (err) {
      toast.error(labels.deleteFailedToast((err as Error).message));
    }
  };

  if (detail.isLoading) {
    return (
      <div className={WORKBENCH_GUTTER}>
        <SkeletonList count={5} />
      </div>
    );
  }

  if (detail.isError) {
    return (
      <div className={cn("flex h-full items-center justify-center", WORKBENCH_GUTTER_X)}>
        <ErrorState
          title={labels.loadErrorTitle}
          description={labels.loadErrorDescription}
          onRetry={() => detail.refetch()}
          retryLabel={labels.retryLabel}
        />
      </div>
    );
  }

  if (!ontology) {
    return (
      <div className={cn(WORKBENCH_GUTTER, "flex flex-col")}>
        <EmptyState
          kind="prerequisite"
          title={labels.noOntology}
          description={labels.noOntologyDescription}
          action={{ label: labels.openDesign, onClick: openDesign }}
        />
      </div>
    );
  }

  const listPane = (
    <ListPane
      items={filtered}
      itemId={itemId}
      renderRow={renderRow}
      selectedId={selectedId}
      onSelect={setSelection}
      onCreate={() => setSelection(DRAFT_ID)}
      search={search}
      onSearch={setSearch}
      heading={labels.listHeading(items.length)}
      labels={{
        createButton: labels.createButton,
        emptyTitle: labels.emptyTitle,
        emptyDescription: labels.emptyDescription,
        searchPlaceholder: t("searchPlaceholder"),
      }}
      busy={apply.isPending}
    />
  );

  const detailPane = (
    <DetailPane
      key={isDraft ? "__draft__" : selected ? itemId(selected) : "__empty__"}
      isDraft={isDraft}
      selected={selected}
      schema={schema}
      labels={labels}
      onCreate={handleCreate}
      onUpdate={handleUpdate}
      onDelete={handleDelete}
      onCancelDraft={() => setSelection(null)}
      pending={apply.isPending}
    />
  );

  const auxPane =
    renderUsage && selected && detail.data?.ontology_ir
      ? (
          <div className="flex h-full flex-col">
            <Heading level={3} size={6} className="border-b border-divider px-3 py-2">
              {t("usageHeader")}
            </Heading>
            <div className="flex-1 overflow-y-auto px-3 py-2">
              {renderUsage(selected, detail.data.ontology_ir)}
            </div>
          </div>
        )
      : undefined;

  return (
    <EntityWorkbench<T>
      listPane={listPane}
      detailPane={detailPane}
      auxPane={auxPane}
      auxToggleLabel={t("toggleUsage")}
      selected={selected}
    />
  );
}

// ---------------------------------------------------------------------------
// List pane — search + create button + scrollable rows.
// ---------------------------------------------------------------------------

interface ListPaneProps<T> {
  items: T[];
  itemId: (item: T) => string;
  renderRow: (item: T) => ReactNode;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
  search: string;
  onSearch: (value: string) => void;
  heading: string;
  labels: {
    createButton: string;
    emptyTitle: string;
    emptyDescription: string;
    searchPlaceholder: string;
  };
  busy: boolean;
}

function ListPane<T>({
  items,
  itemId,
  renderRow,
  selectedId,
  onSelect,
  onCreate,
  search,
  onSearch,
  heading,
  labels,
  busy,
}: ListPaneProps<T>) {
  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-divider px-3 py-2">
        <Heading level={2} size={6} className="flex-1">
          {heading}
        </Heading>
        <button
          type="button"
          onClick={onCreate}
          disabled={busy}
          aria-label={labels.createButton}
          className="inline-flex items-center gap-1 rounded bg-brand-solid px-2 py-1 text-2xs font-semibold uppercase tracking-wider text-foreground-onbrand shadow-1 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-brand-solid disabled:opacity-50"
        >
          <span aria-hidden className="text-sm leading-none">
            +
          </span>
          {labels.createButton}
        </button>
      </div>
      <div className="border-b border-divider px-3 py-2">
        <FormInput
          type="search"
          value={search}
          onChange={(e) => onSearch(e.target.value)}
          placeholder={labels.searchPlaceholder}
          density="compact"
        />
      </div>
      {items.length === 0 ? (
        <div className="flex flex-1 items-center justify-center px-4 py-6">
          <EmptyState
            title={labels.emptyTitle}
            description={labels.emptyDescription}
          />
        </div>
      ) : (
        <ul className="flex-1 overflow-y-auto py-1">
          {items.map((item) => {
            const id = itemId(item);
            const isSelected = id === selectedId;
            return (
              <li key={id}>
                <button
                  type="button"
                  onClick={() => onSelect(id)}
                  className={cn(
                    "flex w-full flex-col items-start px-3 py-2 text-start text-xs transition-colors duration-[var(--duration-quick)]",
                    isSelected
                      ? "bg-brand-surface text-brand-foreground"
                      : "text-foreground hover:bg-surface-hover",
                  )}
                >
                  {renderRow(item)}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Detail pane — header (id badge + title + delete) + editor body.
// ---------------------------------------------------------------------------

interface DetailPaneProps<T extends { id?: string }> {
  isDraft: boolean;
  selected: T | null;
  schema: EntitySchema<T>;
  labels: MasterDetailEntityPageLabels;
  onCreate: (def: T) => Promise<void>;
  onUpdate: (def: T) => Promise<void>;
  onDelete: () => Promise<void>;
  onCancelDraft: () => void;
  pending: boolean;
}

function DetailPane<T extends { id?: string }>({
  isDraft,
  selected,
  schema,
  labels,
  onCreate,
  onUpdate,
  onDelete,
  onCancelDraft,
  pending,
}: DetailPaneProps<T>) {
  const t = useTranslations("settings.vocabulary.workbench");

  if (!isDraft && !selected) {
    return (
      <div className="flex h-full items-center justify-center px-6 py-12">
        <EmptyState
          title={t("nothingSelected.title")}
          description={t("nothingSelected.description")}
        />
      </div>
    );
  }

  const title = isDraft
    ? labels.createDialogTitle
    : selected
      ? selected.id ?? ""
      : "";

  return (
    <div className="flex h-full min-w-0 flex-col">
      <header className="flex items-center gap-3 border-b border-divider px-4 py-3">
        <Heading level={2} size={6} className="flex-1 truncate font-mono">
          {title}
        </Heading>
        {!isDraft && selected && (
          <Button
            variant="danger"
            size="sm"
            onClick={onDelete}
            disabled={pending}
          >
            {labels.deleteButton}
          </Button>
        )}
      </header>
      <div className="flex-1 overflow-y-auto px-4 py-4">
        <StructuredEntityEditor
          schema={schema}
          initial={isDraft ? undefined : selected ?? undefined}
          onSubmit={isDraft ? onCreate : onUpdate}
          onCancel={isDraft ? onCancelDraft : () => undefined}
          pending={pending}
        />
      </div>
    </div>
  );
}
