"use client";

import { useTranslations } from "next-intl";

import { Modal } from "@/components/ui/modal";
import { Heading } from "@/components/ui/heading";
import { useDraftDiffAgainstCanonical } from "@/hooks/api/use-ontology-branches";
import type {
  DiffAddedEdge,
  DiffAddedNode,
  DiffModifiedEdge,
  DiffModifiedNode,
} from "@/types/ontology-branches";

interface DiffModalProps {
  draftId: string | null;
  draftTitle: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/**
 * Section header + chip-list shell shared by every category
 * row. Hidden when the list is empty so the modal collapses
 * around whatever did change.
 */
function DiffSection<T>({
  title,
  items,
  renderItem,
  variant,
}: {
  title: string;
  items: T[];
  renderItem: (item: T, idx: number) => string;
  variant: "added" | "removed" | "modified";
}) {
  if (items.length === 0) return null;
  const tone = {
    added:
      "bg-success-surface text-success-foreground ring-1 ring-success-border",
    removed:
      "bg-danger-surface text-danger-foreground ring-1 ring-danger-border",
    modified: "bg-info-surface text-info-foreground ring-1 ring-info-border",
  }[variant];
  return (
    <section className="mb-4">
      <Heading level={3} size={6}>
        {title} · {items.length}
      </Heading>
      <ul className="mt-2 flex flex-wrap gap-1.5">
        {items.map((item, idx) => {
          // BE diff entries are positional and have no stable
          // identity beyond the per-row rendered label; concat
          // index + rendered label so duplicates (same label
          // appearing twice) still get distinct keys.
          const rendered = renderItem(item, idx);
          return (
            <li
              key={`${idx}-${rendered}`}
              className={`rounded-full px-2 py-0.5 text-2xs font-medium ${tone}`}
            >
              {rendered}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

export function DiffModal({
  draftId,
  draftTitle,
  open,
  onOpenChange,
}: DiffModalProps) {
  const t = useTranslations("workbench.branches");
  const tCommon = useTranslations("common");
  const diff = useDraftDiffAgainstCanonical(open ? draftId : null);

  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={t("diffModal.title", { title: draftTitle })}
      description={t("diffModal.description")}
      size="xl"
    >
      {diff.isLoading ? (
        <p className="text-sm text-foreground-muted">{tCommon("loading")}</p>
      ) : diff.isError ? (
        <p className="text-sm text-danger-foreground">
          {tCommon("loadError.title")}
        </p>
      ) : !diff.data ? null : diff.data.summary.total_changes === 0 ? (
        <p className="text-sm text-foreground-muted">{t("diffNoChanges")}</p>
      ) : (
        <div>
          <p className="mb-4 text-xs text-foreground-muted">
            {t("diffSummary", {
              added: diff.data.summary.added_count,
              removed: diff.data.summary.removed_count,
              modified: diff.data.summary.modified_count,
            })}
          </p>

          <DiffSection<DiffAddedNode>
            title={t("diffModal.addedNodes")}
            items={diff.data.added_nodes}
            renderItem={(n) => n.label || n.id}
            variant="added"
          />
          <DiffSection<DiffAddedNode>
            title={t("diffModal.removedNodes")}
            items={diff.data.removed_nodes}
            renderItem={(n) => n.label || n.id}
            variant="removed"
          />
          <DiffSection<DiffModifiedNode>
            title={t("diffModal.modifiedNodes")}
            items={diff.data.modified_nodes}
            renderItem={(n) =>
              `${n.label || n.node_id} (${n.changes.length})`
            }
            variant="modified"
          />
          <DiffSection<DiffAddedEdge>
            title={t("diffModal.addedEdges")}
            items={diff.data.added_edges}
            renderItem={(e) => e.label || e.id}
            variant="added"
          />
          <DiffSection<DiffAddedEdge>
            title={t("diffModal.removedEdges")}
            items={diff.data.removed_edges}
            renderItem={(e) => e.label || e.id}
            variant="removed"
          />
          <DiffSection<DiffModifiedEdge>
            title={t("diffModal.modifiedEdges")}
            items={diff.data.modified_edges}
            renderItem={(e) =>
              `${e.label || e.edge_id} (${e.changes.length})`
            }
            variant="modified"
          />
        </div>
      )}
    </Modal>
  );
}
