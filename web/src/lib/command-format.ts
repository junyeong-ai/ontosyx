import type { OntologyCommand, OntologyIR } from "@/types/api";

/** Resolve a node/edge ID to its label using the ontology. Falls back to truncated ID. */
function resolveLabel(
  ontology: OntologyIR | null | undefined,
  id: string,
  kind: "node" | "edge" | "any",
): string {
  if (ontology) {
    if (kind !== "edge") {
      const node = ontology.node_types.find((n) => n.id === id);
      if (node) return node.label;
    }
    if (kind !== "node") {
      const edge = ontology.edge_types.find((e) => e.id === id);
      if (edge) return edge.label;
    }
  }
  // Fallback: truncated UUID
  return id.length > 12 ? `${id.slice(0, 8)}…` : id;
}

function resolveProperty(
  ontology: OntologyIR | null | undefined,
  ownerId: string,
  propertyId: string,
): string {
  if (ontology) {
    const owner =
      ontology.node_types.find((n) => n.id === ownerId) ??
      ontology.edge_types.find((e) => e.id === ownerId);
    if (owner) {
      const prop = owner.properties.find((p) => p.id === propertyId);
      if (prop) return prop.name;
    }
  }
  return propertyId.length > 12 ? `${propertyId.slice(0, 8)}…` : propertyId;
}

/**
 * Format an OntologyCommand as a human-readable one-liner for the preview panel.
 * Pass the current ontology to resolve IDs to labels.
 */
export function formatCommand(
  cmd: OntologyCommand,
  ontology?: OntologyIR | null,
): string {
  switch (cmd.op) {
    case "add_node":
      return `Add node: ${cmd.label}`;
    case "delete_node":
      return `Delete node: ${resolveLabel(ontology, cmd.node_id, "node")}`;
    case "rename_node":
      return `Rename node: ${resolveLabel(ontology, cmd.node_id, "node")} → ${cmd.new_label}`;
    case "update_node_description":
      return `Update description: ${resolveLabel(ontology, cmd.node_id, "node")}`;
    case "add_edge":
      return `Add edge: ${cmd.label} (${resolveLabel(ontology, cmd.source_node_id, "node")} → ${resolveLabel(ontology, cmd.target_node_id, "node")})`;
    case "delete_edge":
      return `Delete edge: ${resolveLabel(ontology, cmd.edge_id, "edge")}`;
    case "rename_edge":
      return `Rename edge: ${resolveLabel(ontology, cmd.edge_id, "edge")} → ${cmd.new_label}`;
    case "update_edge_cardinality":
      return `Update cardinality: ${resolveLabel(ontology, cmd.edge_id, "edge")} → ${cmd.cardinality}`;
    case "update_edge_description":
      return `Update description: ${resolveLabel(ontology, cmd.edge_id, "edge")}`;
    case "add_property":
      return `Add property: ${cmd.property.name} to ${resolveLabel(ontology, cmd.owner_id, "any")}`;
    case "delete_property":
      return `Delete property: ${resolveProperty(ontology, cmd.owner_id, cmd.property_id)} from ${resolveLabel(ontology, cmd.owner_id, "any")}`;
    case "update_property":
      return `Update property: ${resolveProperty(ontology, cmd.owner_id, cmd.property_id)} on ${resolveLabel(ontology, cmd.owner_id, "any")}`;
    case "add_constraint":
      return `Add constraint on ${resolveLabel(ontology, cmd.node_id, "node")}`;
    case "remove_constraint":
      return `Remove constraint from ${resolveLabel(ontology, cmd.node_id, "node")}`;
    case "add_index":
      return `Add index on ${resolveLabel(ontology, cmd.index.node_id, "node")}`;
    case "remove_index":
      return `Remove index: ${cmd.index_id.length > 12 ? `${cmd.index_id.slice(0, 8)}…` : cmd.index_id}`;
    case "batch":
      return `Batch: ${cmd.commands.length} command${cmd.commands.length === 1 ? "" : "s"}`;
    default:
      return `Unknown command`;
  }
}

/**
 * Return a short badge label for the operation type.
 */
export function commandOpBadge(cmd: OntologyCommand): {
  label: string;
  color: "green" | "red" | "blue";
} {
  switch (cmd.op) {
    case "add_node":
    case "add_edge":
    case "add_property":
    case "add_constraint":
    case "add_index":
      return { label: "ADD", color: "green" };
    case "delete_node":
    case "delete_edge":
    case "delete_property":
    case "remove_constraint":
    case "remove_index":
      return { label: "DEL", color: "red" };
    default:
      return { label: "UPD", color: "blue" };
  }
}
