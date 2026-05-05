"use client";

import { useState } from "react";
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

import { EdgeChangeList, NodeChangeList } from "./diff-change-list";

interface DiffModalProps {
  draftId: string | null;
  draftTitle: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/**
 * Modified-nodes section — disclosure list. The chip count
 * (`changes.length`) sits as the row preview; clicking the
 * row expands to render the typed `NodeChange` list. Multiple
 * rows can be expanded simultaneously so the operator
 * cross-references freely.
 */
function ModifiedNodeSection({ items }: { items: DiffModifiedNode[] }) {
  const t = useTranslations("workbench.branches");
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  if (items.length === 0) return null;
  const toggle = (key: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };
  return (
    <section className="mb-4">
      <Heading level={3} size={6}>
        {t("diffModal.modifiedNodes")} · {items.length}
      </Heading>
      <ul className="mt-2 space-y-1">
        {items.map((n) => {
          const isOpen = expanded.has(n.node_id);
          return (
            <li
              key={n.node_id}
              className="rounded-lg border border-divider bg-surface-base"
            >
              <button
                type="button"
                onClick={() => toggle(n.node_id)}
                className="flex w-full items-baseline justify-between gap-3 px-3 py-2 text-left text-xs hover:bg-surface-inset"
                aria-expanded={isOpen}
              >
                <span className="font-medium">{n.label || n.node_id}</span>
                <span className="rounded-full bg-info-surface px-2 py-0.5 text-2xs font-medium text-info-foreground">
                  {n.changes.length}
                </span>
              </button>
              {isOpen ? (
                <div className="border-t border-divider px-3 py-2">
                  <NodeChangeList changes={n.changes} />
                </div>
              ) : null}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/** Same disclosure pattern for modified edges. */
function ModifiedEdgeSection({ items }: { items: DiffModifiedEdge[] }) {
  const t = useTranslations("workbench.branches");
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  if (items.length === 0) return null;
  const toggle = (key: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };
  return (
    <section className="mb-4">
      <Heading level={3} size={6}>
        {t("diffModal.modifiedEdges")} · {items.length}
      </Heading>
      <ul className="mt-2 space-y-1">
        {items.map((e) => {
          const isOpen = expanded.has(e.edge_id);
          return (
            <li
              key={e.edge_id}
              className="rounded-lg border border-divider bg-surface-base"
            >
              <button
                type="button"
                onClick={() => toggle(e.edge_id)}
                className="flex w-full items-baseline justify-between gap-3 px-3 py-2 text-left text-xs hover:bg-surface-inset"
                aria-expanded={isOpen}
              >
                <span className="font-medium">{e.label || e.edge_id}</span>
                <span className="rounded-full bg-info-surface px-2 py-0.5 text-2xs font-medium text-info-foreground">
                  {e.changes.length}
                </span>
              </button>
              {isOpen ? (
                <div className="border-t border-divider px-3 py-2">
                  <EdgeChangeList changes={e.changes} />
                </div>
              ) : null}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/**
 * Section header + chip-list shell shared by every added/removed
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
              added:
                diff.data.summary.nodes_added +
                diff.data.summary.edges_added +
                diff.data.summary.properties_added,
              removed:
                diff.data.summary.nodes_removed +
                diff.data.summary.edges_removed +
                diff.data.summary.properties_removed,
              modified:
                diff.data.summary.nodes_modified +
                diff.data.summary.edges_modified,
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
          <ModifiedNodeSection items={diff.data.modified_nodes} />
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
          <ModifiedEdgeSection items={diff.data.modified_edges} />
        </div>
      )}
    </Modal>
  );
}
