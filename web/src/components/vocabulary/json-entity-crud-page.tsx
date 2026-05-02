"use client";

// Shared scaffold for vocabulary admin pages that ride on the
// `JsonEntityEditor` (CodeSystem, ValueSet, NotationPattern,
// ConceptMap, Mapping). Lists the collection with create / edit /
// delete affordances; every mutation flows through
// `useApplyOntologyEdits` so each entity rides the same
// validate-then-commit pipeline as the rest of the admin surface.
//
// Per-kind specialisation (tree / 2D table / component builder)
// can land alongside this scaffold incrementally — each editor
// surfaces above the JSON view rather than replacing it, keeping
// the power-user fallback intact.

import { type ReactNode, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Modal } from "@/components/ui/modal";
import { Spinner } from "@/components/ui/spinner";
import { useConfirm } from "@/components/providers/confirm-provider";
import {
  useOntologies,
  useOntologyDetail,
} from "@/hooks/api/use-ontologies";
import { useApplyOntologyEdits } from "@/hooks/api/use-ontology-edits";
import type { OntologyEditOp } from "@/lib/api/edit-ops";
import type { OntologyIR } from "@/types/ontology";

import {
  JsonEntityEditor,
  type JsonEntityEditorLabels,
} from "./json-entity-editor";

export interface JsonEntityCrudPageLabels extends JsonEntityEditorLabels {
  /** Page title. */
  title: string;
  /** One-line subtitle below the title. */
  subtitle: string;
  /** "No committed ontology yet" amber banner copy. */
  noOntology: string;
  /** Top-right "Add" button label. */
  createButton: string;
  /** Per-row "Edit" button label. */
  editButton: string;
  /** Per-row "Delete" button label. */
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
  /** Create dialog copy. */
  createDialogTitle: string;
  createDialogDescription: string;
}

export interface JsonEntityCrudPageProps<T extends { id?: string }> {
  /** All user-facing copy resolved by the caller from
   *  `useTranslations`. */
  labels: JsonEntityCrudPageLabels;
  /** Placeholder schema hint shown inside the JSON textarea. */
  schemaHint: string;
  /** Resolve the collection slice from the ontology IR. */
  selectItems: (ir: OntologyIR) => T[];
  /** Stable id for an item — used as the row key + edit-state
   *  discriminator. */
  itemId: (item: T) => string;
  /** Render the row body (everything besides the trailing edit
   *  / delete buttons). Receives the typed item. */
  renderRow: (item: T) => ReactNode;
  /** Build the create / update / delete OntologyEditOps. */
  buildCreateOp: (def: T) => OntologyEditOp;
  buildUpdateOp: (id: string, def: T) => OntologyEditOp;
  buildDeleteOp: (id: string) => OntologyEditOp;
}

export function JsonEntityCrudPage<T extends { id?: string }>({
  labels,
  schemaHint,
  selectItems,
  itemId,
  renderRow,
  buildCreateOp,
  buildUpdateOp,
  buildDeleteOp,
}: JsonEntityCrudPageProps<T>) {
  const ontologiesQuery = useOntologies({ limit: 1 });
  const ontology = ontologiesQuery.data?.items?.[0];
  const detail = useOntologyDetail(ontology?.id);
  const apply = useApplyOntologyEdits(ontology?.id);
  const confirm = useConfirm();

  const [createOpen, setCreateOpen] = useState(false);
  const [editing, setEditing] = useState<T | null>(null);

  // `selectItems` is a per-page identity-stable callback; React 19's
  // compiler-aware lint forbids the manual `useMemo` because it can't
  // verify the dependency list. The cost of recomputing the slice
  // each render is negligible (the IR is already in memory).
  const items: T[] = detail.data?.ontology_ir
    ? selectItems(detail.data.ontology_ir)
    : [];
  const expectedVersion =
    Number(detail.data?.current_version?.version ?? "0") || 0;

  const submit = (operations: OntologyEditOp[], message: string) =>
    apply.mutateAsync({
      operations,
      expected_version: expectedVersion,
      message,
    });

  const handleCreate = async (def: T) => {
    if (!ontology?.id) return;
    try {
      await submit([buildCreateOp(def)], labels.createdMessage(itemId(def)));
      toast.success(labels.createdToast);
      setCreateOpen(false);
    } catch (err) {
      toast.error(labels.createFailedToast((err as Error).message));
    }
  };

  const handleUpdate = async (def: T) => {
    if (!ontology?.id || !editing) return;
    const id = itemId(editing);
    try {
      await submit([buildUpdateOp(id, def)], labels.updatedMessage(id));
      toast.success(labels.updatedToast);
      setEditing(null);
    } catch (err) {
      toast.error(labels.updateFailedToast((err as Error).message));
    }
  };

  const handleDelete = async (item: T) => {
    if (!ontology?.id) return;
    const id = itemId(item);
    const ok = await confirm({
      title: labels.confirmDeleteTitle,
      description: labels.confirmDeleteDescription(id),
      variant: "danger",
    });
    if (!ok) return;
    try {
      await submit([buildDeleteOp(id)], labels.deletedMessage(id));
      toast.success(labels.deletedToast);
    } catch (err) {
      toast.error(labels.deleteFailedToast((err as Error).message));
    }
  };

  if (ontologiesQuery.isLoading || detail.isLoading) {
    return (
      <div className="flex items-center justify-center py-10">
        <Spinner />
      </div>
    );
  }

  if (!ontology) {
    return (
      <div className="flex flex-col gap-4">
        <p className="text-xs text-foreground-muted">{labels.subtitle}</p>
        <p className="rounded-md border border-warning-border bg-warning-surface p-3 text-xs text-warning-foreground">
          {labels.noOntology}
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex items-start justify-between gap-4">
        <p className="text-xs text-foreground-muted">{labels.subtitle}</p>
        <Button
          variant="primary"
          size="sm"
          onClick={() => setCreateOpen(true)}
          disabled={apply.isPending}
        >
          {labels.createButton}
        </Button>
      </header>

      {items.length === 0 ? (
        <EmptyState
          title={labels.emptyTitle}
          description={labels.emptyDescription}
        />
      ) : (
        <ul className="flex flex-col gap-2">
          {items.map((item) => {
            const id = itemId(item);
            return (
              <li
                key={id}
                className="rounded border border-divider bg-surface-base p-3"
              >
                {editing && itemId(editing) === id ? (
                  <JsonEntityEditor
                    key={id}
                    initial={item}
                    schemaHint={schemaHint}
                    labels={labels}
                    onSubmit={handleUpdate}
                    onCancel={() => setEditing(null)}
                    pending={apply.isPending}
                  />
                ) : (
                  <Row
                    body={renderRow(item)}
                    onEdit={() => setEditing(item)}
                    onDelete={() => handleDelete(item)}
                    busy={apply.isPending}
                    editLabel={labels.editButton}
                    deleteLabel={labels.deleteButton}
                  />
                )}
              </li>
            );
          })}
        </ul>
      )}

      <Modal
        open={createOpen}
        onOpenChange={setCreateOpen}
        title={labels.createDialogTitle}
        description={labels.createDialogDescription}
        size="lg"
      >
        <JsonEntityEditor
          schemaHint={schemaHint}
          labels={labels}
          onSubmit={handleCreate}
          onCancel={() => setCreateOpen(false)}
          pending={apply.isPending}
        />
      </Modal>
    </div>
  );
}

function Row({
  body,
  onEdit,
  onDelete,
  busy,
  editLabel,
  deleteLabel,
}: {
  body: ReactNode;
  onEdit: () => void;
  onDelete: () => void;
  busy: boolean;
  editLabel: string;
  deleteLabel: string;
}) {
  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0 flex-1">{body}</div>
      <div className="flex items-center gap-1.5">
        <Button variant="ghost" size="xs" onClick={onEdit} disabled={busy}>
          {editLabel}
        </Button>
        <Button variant="ghost" size="xs" onClick={onDelete} disabled={busy}>
          {deleteLabel}
        </Button>
      </div>
    </div>
  );
}
