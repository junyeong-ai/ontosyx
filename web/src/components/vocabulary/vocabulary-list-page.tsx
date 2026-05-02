"use client";

// Shared scaffold for vocabulary admin lists: collection rows + a
// delete affordance, routed through `useApplyOntologyEdits`. Each
// caller supplies its own row renderer and delete-op builder.

import type { ReactNode } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Spinner } from "@/components/ui/spinner";
import { useConfirm } from "@/components/providers/confirm-provider";
import {
  useOntologies,
  useOntologyDetail,
} from "@/hooks/api/use-ontologies";
import { useApplyOntologyEdits } from "@/hooks/api/use-ontology-edits";
import type { OntologyEditOp } from "@/lib/api/edit-ops";
import type { OntologyIR } from "@/types/ontology";

interface VocabularyListPageProps<T> {
  /** Page title rendered in the heading. */
  title: string;
  /** One-line subtitle shown under the title. */
  subtitle: string;
  /** Empty-state title. */
  emptyTitle: string;
  /** Empty-state body. */
  emptyDescription: string;
  /** "No committed ontology yet" amber banner copy. */
  noOntologyMessage: string;
  /** Confirm-dialog title for delete. */
  confirmDeleteTitle: string;
  /** Confirm-dialog body. Receives the row's display name as `{name}`. */
  confirmDeleteDescription: (name: string) => string;
  /** Delete button label. */
  deleteLabel: string;
  /** Toast on successful delete; receives `{name}`. */
  deletedToast: (name: string) => string;
  /** Toast on failed delete; receives `{error}`. */
  deleteFailedToast: (error: string) => string;
  /** Edit-log message; receives `{name}`. */
  deleteMessage: (name: string) => string;
  /** Resolve the collection slice from the ontology IR. */
  selectItems: (ir: OntologyIR) => T[];
  /** Stable id for an item, used as the row key + delete op target. */
  itemId: (item: T) => string;
  /** Human-readable name for confirm dialogs / toasts. */
  itemName: (item: T) => string;
  /** Build the delete OntologyEditOp for the given item. */
  buildDeleteOp: (id: string) => OntologyEditOp;
  /** Render the row body (everything besides the trailing delete
   *  button). Receives the typed item. */
  renderRow: (item: T) => ReactNode;
}

export function VocabularyListPage<T>({
  title,
  subtitle,
  emptyTitle,
  emptyDescription,
  noOntologyMessage,
  confirmDeleteTitle,
  confirmDeleteDescription,
  deleteLabel,
  deletedToast,
  deleteFailedToast,
  deleteMessage,
  selectItems,
  itemId,
  itemName,
  buildDeleteOp,
  renderRow,
}: VocabularyListPageProps<T>) {
  const ontologiesQuery = useOntologies({ limit: 1 });
  const ontology = ontologiesQuery.data?.items?.[0];
  const detail = useOntologyDetail(ontology?.id);
  const apply = useApplyOntologyEdits(ontology?.id);
  const confirm = useConfirm();

  const items = detail.data?.ontology_ir
    ? selectItems(detail.data.ontology_ir)
    : [];

  const expectedVersion =
    Number(detail.data?.current_version?.version ?? "0") || 0;

  const handleDelete = async (item: T) => {
    if (!ontology?.id) return;
    const name = itemName(item);
    const ok = await confirm({
      title: confirmDeleteTitle,
      description: confirmDeleteDescription(name),
      variant: "danger",
    });
    if (!ok) return;
    apply.mutate(
      {
        operations: [buildDeleteOp(itemId(item))],
        expected_version: expectedVersion,
        message: deleteMessage(name),
      },
      {
        onSuccess: () => toast.success(deletedToast(name)),
        onError: (err) => toast.error(deleteFailedToast(err.message)),
      },
    );
  };

  if (ontologiesQuery.isLoading || detail.isLoading) {
    return (
      <div className="flex items-center justify-center py-10">
        <Spinner />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <header>
        <h1 className="text-xl font-semibold text-foreground-strong">
          {title}
        </h1>
        <p className="mt-1 text-xs text-foreground-muted">
          {subtitle}
        </p>
      </header>

      {!ontology && (
        <p className="rounded border border-warning-border bg-warning-surface p-3 text-xs text-warning-foreground">
          {noOntologyMessage}
        </p>
      )}

      {ontology && items.length === 0 && (
        <EmptyState title={emptyTitle} description={emptyDescription} />
      )}

      {items.length > 0 && (
        <ul className="flex flex-col gap-2">
          {items.map((item) => (
            <li
              key={itemId(item)}
              className="rounded border border-divider bg-surface-base p-3"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0 flex-1">{renderRow(item)}</div>
                <Button
                  variant="ghost"
                  size="xs"
                  onClick={() => handleDelete(item)}
                  disabled={apply.isPending}
                >
                  {deleteLabel}
                </Button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
