"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
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
  const { summary } = diff;

  if (summary.total_changes === 0) {
    return (
      <div className="rounded-lg border border-zinc-200 bg-zinc-50/50 p-3 text-xs dark:border-zinc-700 dark:bg-zinc-900/50">
        <div className="flex items-center justify-between">
          <h4 className="font-semibold text-zinc-700 dark:text-zinc-300">
            {t("noChanges")}
          </h4>
          <button
            onClick={onDismiss}
            className="text-muted-foreground hover:text-zinc-600 dark:hover:text-zinc-300"
          >
            &times;
          </button>
        </div>
        <p className="mt-1 text-zinc-500 dark:text-muted-foreground">
          {t("noChangesDescription", { baseLabel, targetLabel })}
        </p>
      </div>
    );
  }

  return (
    <div className="rounded-lg border border-zinc-200 bg-white p-3 text-xs dark:border-zinc-700 dark:bg-zinc-900">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h4 className="font-semibold text-zinc-700 dark:text-zinc-300">
          {t("header", { baseLabel, targetLabel })}
        </h4>
        <button
          onClick={onDismiss}
          className="text-muted-foreground hover:text-zinc-600 dark:hover:text-zinc-300"
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
              <ModifiedNodeItem key={n.node_id} node={n} t={t} />
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
              <ModifiedEdgeItem key={e.edge_id} edge={e} t={t} />
            ))}
          </DiffSection>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function SummaryBadge({ color, label }: { color: "emerald" | "red" | "amber"; label: string }) {
  return (
    <span
      className={cn(
        "rounded px-1.5 py-0.5 text-[10px] font-semibold",
        color === "emerald" && "bg-emerald-100 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300",
        color === "red" && "bg-red-100 text-red-700 dark:bg-red-950/40 dark:text-red-300",
        color === "amber" && "bg-amber-100 text-amber-700 dark:bg-amber-950/40 dark:text-amber-300",
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
        color === "emerald" && "border-emerald-200 dark:border-emerald-900/60",
        color === "red" && "border-red-200 dark:border-red-900/60",
        color === "amber" && "border-amber-200 dark:border-amber-900/60",
      )}
    >
      <button
        onClick={() => setOpen(!open)}
        className={cn(
          "flex w-full items-center gap-1 text-left text-[11px] font-semibold",
          color === "emerald" && "text-emerald-700 dark:text-emerald-400",
          color === "red" && "text-red-700 dark:text-red-400",
          color === "amber" && "text-amber-700 dark:text-amber-400",
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
    <div className="rounded bg-emerald-50/50 px-2 py-1 dark:bg-emerald-950/20">
      <span className="font-medium text-emerald-700 dark:text-emerald-300">
        + {node.label}
      </span>
      {node.properties.length > 0 && (
        <span className="ml-1.5 text-muted-foreground">
          {t("propertiesCount", { count: node.properties.length })}
        </span>
      )}
    </div>
  );
}

function RemovedNodeItem({ node, t }: { node: NodeTypeDef; t: DiffTranslator }) {
  return (
    <div className="rounded bg-red-50/50 px-2 py-1 dark:bg-red-950/20">
      <span className="font-medium text-red-700 dark:text-red-300">
        - {node.label}
      </span>
      {node.properties.length > 0 && (
        <span className="ml-1.5 text-muted-foreground">
          {t("propertiesCount", { count: node.properties.length })}
        </span>
      )}
    </div>
  );
}

function ModifiedNodeItem({ node, t }: { node: NodeDiffEntry; t: DiffTranslator }) {
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <div className="rounded bg-amber-50/50 px-2 py-1 dark:bg-amber-950/20">
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center gap-1 text-left"
      >
        <span className="select-none text-muted-foreground">{isExpanded ? "\u25BE" : "\u25B8"}</span>
        <span className="font-medium text-amber-700 dark:text-amber-300">
          ~ {node.label}
        </span>
        <span className="ml-1 text-muted-foreground">
          {t("changes", { count: node.changes.length })}
        </span>
      </button>
      {isExpanded && (
        <div className="mt-1 ml-3 space-y-0.5">
          {node.changes.map((change, i) => (
            <NodeChangeItem key={i} change={change} t={t} />
          ))}
        </div>
      )}
    </div>
  );
}

function NodeChangeItem({ change, t }: { change: NodeChange; t: DiffTranslator }) {
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
          old={change.old ?? t("noneValue")}
          new_val={change.new ?? t("noneValue")}
        />
      );
    case "property_added":
      return (
        <div className="text-emerald-600 dark:text-emerald-400">
          + {t("propertyLabel")}: <span className="font-medium">{change.property.name}</span>
        </div>
      );
    case "property_removed":
      return (
        <div className="text-red-600 dark:text-red-400">
          - {t("propertyLabel")}: <span className="font-medium">{change.property.name}</span>
        </div>
      );
    case "property_modified":
      return (
        <div>
          <span className="text-amber-600 dark:text-amber-400">
            ~ {t("propertyLabel")}: <span className="font-medium">{change.property_name}</span>
          </span>
          <div className="ml-3 space-y-0.5">
            {change.changes.map((pc, i) => (
              <PropertyChangeItem key={i} change={pc} t={t} />
            ))}
          </div>
        </div>
      );
    case "constraint_added":
      return (
        <div className="text-emerald-600 dark:text-emerald-400">
          + {t("constraintLabel")}: <span className="font-mono">{change.constraint}</span>
        </div>
      );
    case "constraint_removed":
      return (
        <div className="text-red-600 dark:text-red-400">
          - {t("constraintLabel")}: <span className="font-mono">{change.constraint}</span>
        </div>
      );
  }
}

function AddedEdgeItem({ edge }: { edge: EdgeTypeDef }) {
  return (
    <div className="rounded bg-emerald-50/50 px-2 py-1 dark:bg-emerald-950/20">
      <span className="font-medium text-emerald-700 dark:text-emerald-300">
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
    <div className="rounded bg-red-50/50 px-2 py-1 dark:bg-red-950/20">
      <span className="font-medium text-red-700 dark:text-red-300">
        - {edge.label}
      </span>
      <span className="ml-1.5 text-muted-foreground">
        ({edge.source_node_id} &rarr; {edge.target_node_id})
      </span>
    </div>
  );
}

function ModifiedEdgeItem({ edge, t }: { edge: EdgeDiffEntry; t: DiffTranslator }) {
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <div className="rounded bg-amber-50/50 px-2 py-1 dark:bg-amber-950/20">
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center gap-1 text-left"
      >
        <span className="select-none text-muted-foreground">{isExpanded ? "\u25BE" : "\u25B8"}</span>
        <span className="font-medium text-amber-700 dark:text-amber-300">
          ~ {edge.label}
        </span>
        <span className="ml-1 text-muted-foreground">
          {t("changes", { count: edge.changes.length })}
        </span>
      </button>
      {isExpanded && (
        <div className="mt-1 ml-3 space-y-0.5">
          {edge.changes.map((change, i) => (
            <EdgeChangeItem key={i} change={change} t={t} />
          ))}
        </div>
      )}
    </div>
  );
}

function EdgeChangeItem({ change, t }: { change: EdgeChange; t: DiffTranslator }) {
  switch (change.type) {
    case "label_changed":
      return <ChangeRow label={t("labelTxt")} old={change.old} new_val={change.new} />;
    case "description_changed":
      return (
        <ChangeRow
          label={t("descriptionTxt")}
          old={change.old ?? t("noneValue")}
          new_val={change.new ?? t("noneValue")}
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
        <div className="text-emerald-600 dark:text-emerald-400">
          + {t("propertyLabel")}: <span className="font-medium">{change.property.name}</span>
        </div>
      );
    case "property_removed":
      return (
        <div className="text-red-600 dark:text-red-400">
          - {t("propertyLabel")}: <span className="font-medium">{change.property.name}</span>
        </div>
      );
    case "property_modified":
      return (
        <div>
          <span className="text-amber-600 dark:text-amber-400">
            ~ {t("propertyLabel")}: <span className="font-medium">{change.property_name}</span>
          </span>
          <div className="ml-3 space-y-0.5">
            {change.changes.map((pc, i) => (
              <PropertyChangeItem key={i} change={pc} t={t} />
            ))}
          </div>
        </div>
      );
  }
}

function PropertyChangeItem({ change, t }: { change: PropertyChange; t: DiffTranslator }) {
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
          old={change.old ?? t("noneValue")}
          new_val={change.new ?? t("noneValue")}
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
    <div className="flex items-baseline gap-1 text-zinc-600 dark:text-muted-foreground">
      <span className="font-medium text-muted-foreground">{label}:</span>
      <span className="line-through text-red-500/70 dark:text-red-400/70">{old}</span>
      <span className="text-muted-foreground dark:text-zinc-600">&rarr;</span>
      <span className="text-emerald-600 dark:text-emerald-400">{new_val}</span>
    </div>
  );
}
