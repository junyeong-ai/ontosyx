"use client";

import { useState } from "react";
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
  const { summary } = diff;

  if (summary.total_changes === 0) {
    return (
      <div className="rounded-lg border border-zinc-200 bg-zinc-50/50 p-3 text-xs dark:border-zinc-700 dark:bg-zinc-900/50">
        <div className="flex items-center justify-between">
          <h4 className="font-semibold text-zinc-700 dark:text-zinc-300">
            No Changes
          </h4>
          <button
            onClick={onDismiss}
            className="text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300"
          >
            &times;
          </button>
        </div>
        <p className="mt-1 text-zinc-500 dark:text-zinc-400">
          {baseLabel} and {targetLabel} are identical.
        </p>
      </div>
    );
  }

  return (
    <div className="rounded-lg border border-zinc-200 bg-white p-3 text-xs dark:border-zinc-700 dark:bg-zinc-900">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h4 className="font-semibold text-zinc-700 dark:text-zinc-300">
          Diff: {baseLabel} &rarr; {targetLabel}
        </h4>
        <button
          onClick={onDismiss}
          className="text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300"
        >
          &times;
        </button>
      </div>

      {/* Summary bar */}
      <div className="mt-2 flex flex-wrap gap-2">
        {summary.nodes_added > 0 && (
          <SummaryBadge color="emerald" label={`+${summary.nodes_added} node${summary.nodes_added > 1 ? "s" : ""}`} />
        )}
        {summary.nodes_removed > 0 && (
          <SummaryBadge color="red" label={`-${summary.nodes_removed} node${summary.nodes_removed > 1 ? "s" : ""}`} />
        )}
        {summary.nodes_modified > 0 && (
          <SummaryBadge color="amber" label={`~${summary.nodes_modified} modified node${summary.nodes_modified > 1 ? "s" : ""}`} />
        )}
        {summary.edges_added > 0 && (
          <SummaryBadge color="emerald" label={`+${summary.edges_added} edge${summary.edges_added > 1 ? "s" : ""}`} />
        )}
        {summary.edges_removed > 0 && (
          <SummaryBadge color="red" label={`-${summary.edges_removed} edge${summary.edges_removed > 1 ? "s" : ""}`} />
        )}
        {summary.edges_modified > 0 && (
          <SummaryBadge color="amber" label={`~${summary.edges_modified} modified edge${summary.edges_modified > 1 ? "s" : ""}`} />
        )}
        {summary.properties_added > 0 && (
          <SummaryBadge color="emerald" label={`+${summary.properties_added} prop${summary.properties_added > 1 ? "s" : ""}`} />
        )}
        {summary.properties_removed > 0 && (
          <SummaryBadge color="red" label={`-${summary.properties_removed} prop${summary.properties_removed > 1 ? "s" : ""}`} />
        )}
      </div>

      {/* Change details */}
      <div className="mt-3 space-y-2 max-h-[40vh] overflow-y-auto">
        {/* Added Nodes */}
        {diff.added_nodes.length > 0 && (
          <DiffSection title="Added Nodes" color="emerald">
            {diff.added_nodes.map((n) => (
              <AddedNodeItem key={n.id} node={n} />
            ))}
          </DiffSection>
        )}

        {/* Removed Nodes */}
        {diff.removed_nodes.length > 0 && (
          <DiffSection title="Removed Nodes" color="red">
            {diff.removed_nodes.map((n) => (
              <RemovedNodeItem key={n.id} node={n} />
            ))}
          </DiffSection>
        )}

        {/* Modified Nodes */}
        {diff.modified_nodes.length > 0 && (
          <DiffSection title="Modified Nodes" color="amber">
            {diff.modified_nodes.map((n) => (
              <ModifiedNodeItem key={n.node_id} node={n} />
            ))}
          </DiffSection>
        )}

        {/* Added Edges */}
        {diff.added_edges.length > 0 && (
          <DiffSection title="Added Edges" color="emerald">
            {diff.added_edges.map((e) => (
              <AddedEdgeItem key={e.id} edge={e} />
            ))}
          </DiffSection>
        )}

        {/* Removed Edges */}
        {diff.removed_edges.length > 0 && (
          <DiffSection title="Removed Edges" color="red">
            {diff.removed_edges.map((e) => (
              <RemovedEdgeItem key={e.id} edge={e} />
            ))}
          </DiffSection>
        )}

        {/* Modified Edges */}
        {diff.modified_edges.length > 0 && (
          <DiffSection title="Modified Edges" color="amber">
            {diff.modified_edges.map((e) => (
              <ModifiedEdgeItem key={e.edge_id} edge={e} />
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

function AddedNodeItem({ node }: { node: NodeTypeDef }) {
  return (
    <div className="rounded bg-emerald-50/50 px-2 py-1 dark:bg-emerald-950/20">
      <span className="font-medium text-emerald-700 dark:text-emerald-300">
        + {node.label}
      </span>
      {node.properties.length > 0 && (
        <span className="ml-1.5 text-zinc-400 dark:text-zinc-500">
          ({node.properties.length} properties)
        </span>
      )}
    </div>
  );
}

function RemovedNodeItem({ node }: { node: NodeTypeDef }) {
  return (
    <div className="rounded bg-red-50/50 px-2 py-1 dark:bg-red-950/20">
      <span className="font-medium text-red-700 dark:text-red-300">
        - {node.label}
      </span>
      {node.properties.length > 0 && (
        <span className="ml-1.5 text-zinc-400 dark:text-zinc-500">
          ({node.properties.length} properties)
        </span>
      )}
    </div>
  );
}

function ModifiedNodeItem({ node }: { node: NodeDiffEntry }) {
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <div className="rounded bg-amber-50/50 px-2 py-1 dark:bg-amber-950/20">
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center gap-1 text-left"
      >
        <span className="select-none text-zinc-400">{isExpanded ? "\u25BE" : "\u25B8"}</span>
        <span className="font-medium text-amber-700 dark:text-amber-300">
          ~ {node.label}
        </span>
        <span className="ml-1 text-zinc-400 dark:text-zinc-500">
          ({node.changes.length} change{node.changes.length > 1 ? "s" : ""})
        </span>
      </button>
      {isExpanded && (
        <div className="mt-1 ml-3 space-y-0.5">
          {node.changes.map((change, i) => (
            <NodeChangeItem key={i} change={change} />
          ))}
        </div>
      )}
    </div>
  );
}

function NodeChangeItem({ change }: { change: NodeChange }) {
  switch (change.type) {
    case "label_changed":
      return (
        <ChangeRow
          label="Label"
          old={change.old}
          new_val={change.new}
        />
      );
    case "description_changed":
      return (
        <ChangeRow
          label="Description"
          old={change.old ?? "(none)"}
          new_val={change.new ?? "(none)"}
        />
      );
    case "property_added":
      return (
        <div className="text-emerald-600 dark:text-emerald-400">
          + Property: <span className="font-medium">{change.property.name}</span>
        </div>
      );
    case "property_removed":
      return (
        <div className="text-red-600 dark:text-red-400">
          - Property: <span className="font-medium">{change.property.name}</span>
        </div>
      );
    case "property_modified":
      return (
        <div>
          <span className="text-amber-600 dark:text-amber-400">
            ~ Property: <span className="font-medium">{change.property_name}</span>
          </span>
          <div className="ml-3 space-y-0.5">
            {change.changes.map((pc, i) => (
              <PropertyChangeItem key={i} change={pc} />
            ))}
          </div>
        </div>
      );
    case "constraint_added":
      return (
        <div className="text-emerald-600 dark:text-emerald-400">
          + Constraint: <span className="font-mono">{change.constraint}</span>
        </div>
      );
    case "constraint_removed":
      return (
        <div className="text-red-600 dark:text-red-400">
          - Constraint: <span className="font-mono">{change.constraint}</span>
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
      <span className="ml-1.5 text-zinc-400 dark:text-zinc-500">
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
      <span className="ml-1.5 text-zinc-400 dark:text-zinc-500">
        ({edge.source_node_id} &rarr; {edge.target_node_id})
      </span>
    </div>
  );
}

function ModifiedEdgeItem({ edge }: { edge: EdgeDiffEntry }) {
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <div className="rounded bg-amber-50/50 px-2 py-1 dark:bg-amber-950/20">
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="flex w-full items-center gap-1 text-left"
      >
        <span className="select-none text-zinc-400">{isExpanded ? "\u25BE" : "\u25B8"}</span>
        <span className="font-medium text-amber-700 dark:text-amber-300">
          ~ {edge.label}
        </span>
        <span className="ml-1 text-zinc-400 dark:text-zinc-500">
          ({edge.changes.length} change{edge.changes.length > 1 ? "s" : ""})
        </span>
      </button>
      {isExpanded && (
        <div className="mt-1 ml-3 space-y-0.5">
          {edge.changes.map((change, i) => (
            <EdgeChangeItem key={i} change={change} />
          ))}
        </div>
      )}
    </div>
  );
}

function EdgeChangeItem({ change }: { change: EdgeChange }) {
  switch (change.type) {
    case "label_changed":
      return <ChangeRow label="Label" old={change.old} new_val={change.new} />;
    case "description_changed":
      return (
        <ChangeRow
          label="Description"
          old={change.old ?? "(none)"}
          new_val={change.new ?? "(none)"}
        />
      );
    case "source_changed":
      return <ChangeRow label="Source" old={change.old} new_val={change.new} />;
    case "target_changed":
      return <ChangeRow label="Target" old={change.old} new_val={change.new} />;
    case "cardinality_changed":
      return <ChangeRow label="Cardinality" old={change.old} new_val={change.new} />;
    case "property_added":
      return (
        <div className="text-emerald-600 dark:text-emerald-400">
          + Property: <span className="font-medium">{change.property.name}</span>
        </div>
      );
    case "property_removed":
      return (
        <div className="text-red-600 dark:text-red-400">
          - Property: <span className="font-medium">{change.property.name}</span>
        </div>
      );
    case "property_modified":
      return (
        <div>
          <span className="text-amber-600 dark:text-amber-400">
            ~ Property: <span className="font-medium">{change.property_name}</span>
          </span>
          <div className="ml-3 space-y-0.5">
            {change.changes.map((pc, i) => (
              <PropertyChangeItem key={i} change={pc} />
            ))}
          </div>
        </div>
      );
  }
}

function PropertyChangeItem({ change }: { change: PropertyChange }) {
  switch (change.type) {
    case "type_changed":
      return <ChangeRow label="Type" old={change.old} new_val={change.new} />;
    case "nullability_changed":
      return (
        <ChangeRow
          label="Nullable"
          old={change.old ? "true" : "false"}
          new_val={change.new ? "true" : "false"}
        />
      );
    case "description_changed":
      return (
        <ChangeRow
          label="Description"
          old={change.old ?? "(none)"}
          new_val={change.new ?? "(none)"}
        />
      );
    case "default_value_changed":
      return (
        <ChangeRow
          label="Default"
          old={change.old ?? "(none)"}
          new_val={change.new ?? "(none)"}
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
    <div className="flex items-baseline gap-1 text-zinc-600 dark:text-zinc-400">
      <span className="font-medium text-zinc-500 dark:text-zinc-500">{label}:</span>
      <span className="line-through text-red-500/70 dark:text-red-400/70">{old}</span>
      <span className="text-zinc-400 dark:text-zinc-600">&rarr;</span>
      <span className="text-emerald-600 dark:text-emerald-400">{new_val}</span>
    </div>
  );
}
