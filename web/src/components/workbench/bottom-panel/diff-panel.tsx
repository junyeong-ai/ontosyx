"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import { Card } from "@/components/ui/card";
import type {
  OntologyDiff,
  NodeDiffEntry,
  EdgeDiffEntry,
  NodeChange,
  EdgeChange,
  PropertyChange,
  NodeTypeDef,
  EdgeTypeDef,
} from "@/types/api";
import { arr } from "@/lib/ir-collections";
import { localizePresent } from "@/lib/locale/localize";
import { useLocaleChain } from "@/hooks/use-locale-chain";

type DiffTranslator = ReturnType<typeof useTranslations<"workbench.bottomPanel.diff">>;

// ---------------------------------------------------------------------------
// DiffPanel — visual diff between two ontology versions
// ---------------------------------------------------------------------------

export function DiffPanel({
  diff,
  baseLabel,
  targetLabel,
  onDismiss,
}: {
  diff: OntologyDiff;
  baseLabel: string;
  targetLabel: string;
  onDismiss: () => void;
}) {
  const t = useTranslations("workbench.bottomPanel.diff");
  const chain = useLocaleChain("admin");
  const { summary } = diff;

  if (summary.total_changes === 0) {
    return (
      <Card variant="inset" padding="sm" className="text-xs">
        <div className="flex items-center justify-between">
          <h4 className="font-semibold text-foreground">
            {t("noChanges")}
          </h4>
          <button
            onClick={onDismiss}
            className="text-foreground-muted hover:text-foreground"
          >
            &times;
          </button>
        </div>
        <p className="mt-1 text-foreground-muted">
          {t("noChangesDescription", { baseLabel, targetLabel })}
        </p>
      </Card>
    );
  }

  return (
    <Card padding="sm" className="text-xs">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h4 className="font-semibold text-foreground">
          {t("heading", { baseLabel, targetLabel })}
        </h4>
        <button
          onClick={onDismiss}
          className="text-foreground-muted hover:text-foreground"
        >
          &times;
        </button>
      </div>

      {/* Summary bar */}
      <div className="mt-2 flex flex-wrap gap-2">
        {summary.nodes_added > 0 && (
          <SummaryBadge color="emerald" label={t("summaryNodesAdded", { count: summary.nodes_added })} />
        )}
        {summary.nodes_removed > 0 && (
          <SummaryBadge color="red" label={t("summaryNodesRemoved", { count: summary.nodes_removed })} />
        )}
        {summary.nodes_modified > 0 && (
          <SummaryBadge color="amber" label={t("summaryNodesModified", { count: summary.nodes_modified })} />
        )}
        {summary.edges_added > 0 && (
          <SummaryBadge color="emerald" label={t("summaryEdgesAdded", { count: summary.edges_added })} />
        )}
        {summary.edges_removed > 0 && (
          <SummaryBadge color="red" label={t("summaryEdgesRemoved", { count: summary.edges_removed })} />
        )}
        {summary.edges_modified > 0 && (
          <SummaryBadge color="amber" label={t("summaryEdgesModified", { count: summary.edges_modified })} />
        )}
        {summary.properties_added > 0 && (
          <SummaryBadge color="emerald" label={t("summaryPropertiesAdded", { count: summary.properties_added })} />
        )}
        {summary.properties_removed > 0 && (
          <SummaryBadge color="red" label={t("summaryPropertiesRemoved", { count: summary.properties_removed })} />
        )}
      </div>

      {/* Change details */}
      <div className="mt-3 space-y-2 max-h-[40vh] overflow-y-auto">
        {diff.added_nodes.length > 0 && (
          <DiffSection title={t("addedNodes")} color="emerald">
            {diff.added_nodes.map((n) => (
              <AddedNodeItem key={n.id} node={n} t={t} />
            ))}
          </DiffSection>
        )}

        {diff.removed_nodes.length > 0 && (
          <DiffSection title={t("removedNodes")} color="red">
            {diff.removed_nodes.map((n) => (
              <RemovedNodeItem key={n.id} node={n} t={t} />
            ))}
          </DiffSection>
        )}

        {diff.modified_nodes.length > 0 && (
          <DiffSection title={t("modifiedNodes")} color="amber">
            {diff.modified_nodes.map((n) => (
              <ModifiedNodeItem key={n.node_id} node={n} t={t} chain={chain} />
            ))}
          </DiffSection>
        )}

        {diff.added_edges.length > 0 && (
          <DiffSection title={t("addedEdges")} color="emerald">
            {diff.added_edges.map((e) => (
              <AddedEdgeItem key={e.id} edge={e} />
            ))}
          </DiffSection>
        )}

        {diff.removed_edges.length > 0 && (
          <DiffSection title={t("removedEdges")} color="red">
            {diff.removed_edges.map((e) => (
              <RemovedEdgeItem key={e.id} edge={e} />
            ))}
          </DiffSection>
        )}

        {diff.modified_edges.length > 0 && (
          <DiffSection title={t("modifiedEdges")} color="amber">
            {diff.modified_edges.map((e) => (
              <ModifiedEdgeItem key={e.edge_id} edge={e} t={t} chain={chain} />
            ))}
          </DiffSection>
        )}
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function SummaryBadge({ color, label }: { color: "emerald" | "red" | "amber"; label: string }) {
  return (
    <span
      className={cn(
        "rounded px-1.5 py-0.5 text-2xs font-semibold",
        color === "emerald" && "bg-brand-surface-strong text-brand-foreground-strong",
        color === "red" && "bg-danger-surface text-danger-foreground",
        color === "amber" && "bg-warning-surface text-warning-foreground",
      )}
    >
      {label}
    </span>
  );
}

function DiffSection({
  title,
  color,
  children,
}: {
  title: string;
  color: "emerald" | "red" | "amber";
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(true);

  return (
    <div
      className={cn(
        "rounded border p-2",
        color === "emerald" && "border-brand-border/60",
        color === "red" && "border-danger-border/60",
        color === "amber" && "border-warning-border/60",
      )}
    >
      <button
        onClick={() => setOpen(!open)}
        className={cn(
          "flex w-full items-center gap-1 text-left text-[11px] font-semibold",
          color === "emerald" && "text-brand-foreground",
          color === "red" && "text-danger-foreground",
          color === "amber" && "text-warning-foreground",
        )}
      >
        <span className="select-none">{open ? "\u25BE" : "\u25B8"}</span>
        {title}
      </button>
      {open && <div className="mt-1.5 space-y-1">{children}</div>}
    </div>
  );
}

function AddedNodeItem({ node, t }: { node: NodeTypeDef; t: DiffTranslator }) {
  return (
    <div className="rounded bg-brand-surface px-2 py-1">
      <span className="font-medium text-brand-foreground-strong">
        + {node.label}
      </span>
      {arr(node.properties).length > 0 && (
        <span className="ml-1.5 text-muted-foreground">
          {t("propertiesCount", { count: arr(node.properties).length })}
        </span>
      )}
    </div>
  );
}

function RemovedNodeItem({ node, t }: { node: NodeTypeDef; t: DiffTranslator }) {
  return (
    <div className="rounded bg-danger-surface px-2 py-1">
      <span className="font-medium text-danger-foreground">
        - {node.label}
      </span>
      {arr(node.properties).length > 0 && (
        <span className="ml-1.5 text-muted-foreground">
          {t("propertiesCount", { count: arr(node.properties).length })}
        </span>
      )}
    </div>
  );
}

function ModifiedNodeItem({
  node,
  t,
  chain,
}: {
  node: NodeDiffEntry;
  t: DiffTranslator;
  chain: readonly string[];
}) {
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <div className="rounded bg-warning-surface px-2 py-1">
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center gap-1 text-left"
      >
        <span className="select-none text-muted-foreground">{isExpanded ? "\u25BE" : "\u25B8"}</span>
        <span className="font-medium text-warning-foreground">
          ~ {node.label}
        </span>
        <span className="ml-1 text-muted-foreground">
          {t("changes", { count: node.changes.length })}
        </span>
      </button>
      {isExpanded && (
        <div className="mt-1 ml-3 space-y-0.5">
          {node.changes.map((change, i) => (
            <NodeChangeItem key={i} change={change} t={t} chain={chain} />
          ))}
        </div>
      )}
    </div>
  );
}

function NodeChangeItem({
  change,
  t,
  chain,
}: {
  change: NodeChange;
  t: DiffTranslator;
  chain: readonly string[];
}) {
  switch (change.type) {
    case "label_changed":
      return (
        <ChangeRow
          label={t("labelTxt")}
          old={change.old}
          new_val={change.new}
        />
      );
    case "description_changed":
      return (
        <ChangeRow
          label={t("descriptionTxt")}
          old={localizePresent(change.old, chain) ?? t("noneValue")}
          new_val={localizePresent(change.new, chain) ?? t("noneValue")}
        />
      );
    case "property_added":
      return (
        <div className="text-brand-foreground">
          + {t("propertyLabel")}: <span className="font-medium">{change.property.name}</span>
        </div>
      );
    case "property_removed":
      return (
        <div className="text-danger-foreground">
          - {t("propertyLabel")}: <span className="font-medium">{change.property.name}</span>
        </div>
      );
    case "property_modified":
      return (
        <div>
          <span className="text-warning-foreground">
            ~ {t("propertyLabel")}: <span className="font-medium">{change.property_name}</span>
          </span>
          <div className="ml-3 space-y-0.5">
            {change.changes.map((pc, i) => (
              <PropertyChangeItem key={i} change={pc} t={t} chain={chain} />
            ))}
          </div>
        </div>
      );
    case "constraint_added":
      return (
        <div className="text-brand-foreground">
          + {t("constraintLabel")}: <span className="font-mono">{change.constraint}</span>
        </div>
      );
    case "constraint_removed":
      return (
        <div className="text-danger-foreground">
          - {t("constraintLabel")}: <span className="font-mono">{change.constraint}</span>
        </div>
      );
  }
}

function AddedEdgeItem({ edge }: { edge: EdgeTypeDef }) {
  return (
    <div className="rounded bg-brand-surface px-2 py-1">
      <span className="font-medium text-brand-foreground-strong">
        + {edge.label}
      </span>
      <span className="ml-1.5 text-muted-foreground">
        ({edge.source_node_id} &rarr; {edge.target_node_id})
      </span>
    </div>
  );
}

function RemovedEdgeItem({ edge }: { edge: EdgeTypeDef }) {
  return (
    <div className="rounded bg-danger-surface px-2 py-1">
      <span className="font-medium text-danger-foreground">
        - {edge.label}
      </span>
      <span className="ml-1.5 text-muted-foreground">
        ({edge.source_node_id} &rarr; {edge.target_node_id})
      </span>
    </div>
  );
}

function ModifiedEdgeItem({
  edge,
  t,
  chain,
}: {
  edge: EdgeDiffEntry;
  t: DiffTranslator;
  chain: readonly string[];
}) {
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <div className="rounded bg-warning-surface px-2 py-1">
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center gap-1 text-left"
      >
        <span className="select-none text-muted-foreground">{isExpanded ? "\u25BE" : "\u25B8"}</span>
        <span className="font-medium text-warning-foreground">
          ~ {edge.label}
        </span>
        <span className="ml-1 text-muted-foreground">
          {t("changes", { count: edge.changes.length })}
        </span>
      </button>
      {isExpanded && (
        <div className="mt-1 ml-3 space-y-0.5">
          {edge.changes.map((change, i) => (
            <EdgeChangeItem key={i} change={change} t={t} chain={chain} />
          ))}
        </div>
      )}
    </div>
  );
}

function EdgeChangeItem({
  change,
  t,
  chain,
}: {
  change: EdgeChange;
  t: DiffTranslator;
  chain: readonly string[];
}) {
  switch (change.type) {
    case "label_changed":
      return <ChangeRow label={t("labelTxt")} old={change.old} new_val={change.new} />;
    case "description_changed":
      return (
        <ChangeRow
          label={t("descriptionTxt")}
          old={localizePresent(change.old, chain) ?? t("noneValue")}
          new_val={localizePresent(change.new, chain) ?? t("noneValue")}
        />
      );
    case "source_changed":
      return <ChangeRow label={t("sourceTxt")} old={change.old} new_val={change.new} />;
    case "target_changed":
      return <ChangeRow label={t("targetTxt")} old={change.old} new_val={change.new} />;
    case "cardinality_changed":
      return <ChangeRow label={t("cardinalityTxt")} old={change.old} new_val={change.new} />;
    case "property_added":
      return (
        <div className="text-brand-foreground">
          + {t("propertyLabel")}: <span className="font-medium">{change.property.name}</span>
        </div>
      );
    case "property_removed":
      return (
        <div className="text-danger-foreground">
          - {t("propertyLabel")}: <span className="font-medium">{change.property.name}</span>
        </div>
      );
    case "property_modified":
      return (
        <div>
          <span className="text-warning-foreground">
            ~ {t("propertyLabel")}: <span className="font-medium">{change.property_name}</span>
          </span>
          <div className="ml-3 space-y-0.5">
            {change.changes.map((pc, i) => (
              <PropertyChangeItem key={i} change={pc} t={t} chain={chain} />
            ))}
          </div>
        </div>
      );
  }
}

function PropertyChangeItem({
  change,
  t,
  chain,
}: {
  change: PropertyChange;
  t: DiffTranslator;
  chain: readonly string[];
}) {
  switch (change.type) {
    case "type_changed":
      return <ChangeRow label={t("typeTxt")} old={change.old} new_val={change.new} />;
    case "nullability_changed":
      return (
        <ChangeRow
          label={t("nullableTxt")}
          old={change.old ? t("trueValue") : t("falseValue")}
          new_val={change.new ? t("trueValue") : t("falseValue")}
        />
      );
    case "description_changed":
      return (
        <ChangeRow
          label={t("descriptionTxt")}
          old={localizePresent(change.old, chain) ?? t("noneValue")}
          new_val={localizePresent(change.new, chain) ?? t("noneValue")}
        />
      );
    case "default_value_changed":
      return (
        <ChangeRow
          label={t("defaultTxt")}
          old={change.old ?? t("noneValue")}
          new_val={change.new ?? t("noneValue")}
        />
      );
  }
}

function ChangeRow({
  label,
  old,
  new_val,
}: {
  label: string;
  old: string;
  new_val: string;
}) {
  return (
    <div className="flex items-baseline gap-1 text-foreground dark:text-muted-foreground">
      <span className="font-medium text-muted-foreground">{label}:</span>
      <span className="line-through text-danger-foreground/70">{old}</span>
      <span className="text-muted-foreground">&rarr;</span>
      <span className="text-brand-foreground">{new_val}</span>
    </div>
  );
}
