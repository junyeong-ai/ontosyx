import type { OntologyCommand, OntologyIR } from "@/types/api";
import { arr } from "@/lib/ir-collections";

/**
 * The set of i18n leaf keys `formatCommand` can emit — typed as a
 * literal union so a switch-case that returns an unrecognised key
 * fails compilation, not at runtime when the translator silently
 * falls back to the key string. Adding a new `OntologyCommand`
 * variant requires extending this union AND the
 * `workbench.canvas.commandPreview.command.<key>` namespace in both
 * `messages/{ko,en}.json`.
 */
export type FormattedCommandKey =
  | "addNode"
  | "deleteNode"
  | "renameNode"
  | "updateNodeDescription"
  | "setNodeGlossaryAnchors"
  | "addEdge"
  | "deleteEdge"
  | "renameEdge"
  | "updateEdgeCardinality"
  | "updateEdgeDescription"
  | "setEdgeGlossaryAnchors"
  | "addProperty"
  | "deleteProperty"
  | "updateProperty"
  | "addConstraint"
  | "removeConstraint"
  | "addIndex"
  | "removeIndex"
  | "createObjectMapping"
  | "updateObjectMapping"
  | "deleteObjectMapping"
  | "batch"
  | "unknown";

export interface FormattedCommand {
  key: FormattedCommandKey;
  params: Record<string, string | number>;
}

function resolveLabel(
  ontology: OntologyIR | null | undefined,
  id: string,
  kind: "node" | "edge" | "any",
): string {
  if (ontology) {
    if (kind !== "edge") {
      const node = arr(ontology.node_types).find((n) => n.id === id);
      if (node) return node.label;
    }
    if (kind !== "node") {
      const edge = arr(ontology.edge_types).find((e) => e.id === id);
      if (edge) return edge.label;
    }
  }
  return id.length > 12 ? `${id.slice(0, 8)}…` : id;
}

function resolveProperty(
  ontology: OntologyIR | null | undefined,
  ownerId: string,
  propertyId: string,
): string {
  if (ontology) {
    const owner =
      arr(ontology.node_types).find((n) => n.id === ownerId) ??
      arr(ontology.edge_types).find((e) => e.id === ownerId);
    if (owner) {
      const prop = arr(owner.properties).find((p) => p.id === propertyId);
      if (prop) return prop.name;
    }
  }
  return propertyId.length > 12 ? `${propertyId.slice(0, 8)}…` : propertyId;
}

function shortId(id: string): string {
  return id.length > 12 ? `${id.slice(0, 8)}…` : id;
}

/**
 * Resolve an ontology command into a translation key + params bundle.
 * Renderers translate via `t(formatted.key, formatted.params)` against
 * the `workbench.canvas.commandPreview.command` namespace, which lets
 * the wire-protocol stay language-neutral while UI surfaces stay localised.
 */
export function formatCommand(
  cmd: OntologyCommand,
  ontology?: OntologyIR | null,
): FormattedCommand {
  switch (cmd.op) {
    case "add_node":
      return { key: "addNode", params: { label: cmd.label } };
    case "delete_node":
      return {
        key: "deleteNode",
        params: { label: resolveLabel(ontology, cmd.node_id, "node") },
      };
    case "rename_node":
      return {
        key: "renameNode",
        params: {
          label: resolveLabel(ontology, cmd.node_id, "node"),
          newLabel: cmd.new_label,
        },
      };
    case "update_node_description":
      return {
        key: "updateNodeDescription",
        params: { label: resolveLabel(ontology, cmd.node_id, "node") },
      };
    case "set_node_glossary_anchors":
      return {
        key: "setNodeGlossaryAnchors",
        params: {
          label: resolveLabel(ontology, cmd.node_id, "node"),
          count: cmd.anchors.length,
        },
      };
    case "add_edge":
      return {
        key: "addEdge",
        params: {
          label: cmd.label,
          source: resolveLabel(ontology, cmd.source_node_id, "node"),
          target: resolveLabel(ontology, cmd.target_node_id, "node"),
        },
      };
    case "delete_edge":
      return {
        key: "deleteEdge",
        params: { label: resolveLabel(ontology, cmd.edge_id, "edge") },
      };
    case "rename_edge":
      return {
        key: "renameEdge",
        params: {
          label: resolveLabel(ontology, cmd.edge_id, "edge"),
          newLabel: cmd.new_label,
        },
      };
    case "update_edge_cardinality":
      return {
        key: "updateEdgeCardinality",
        params: {
          label: resolveLabel(ontology, cmd.edge_id, "edge"),
          cardinality: cmd.cardinality,
        },
      };
    case "update_edge_description":
      return {
        key: "updateEdgeDescription",
        params: { label: resolveLabel(ontology, cmd.edge_id, "edge") },
      };
    case "set_edge_glossary_anchors":
      return {
        key: "setEdgeGlossaryAnchors",
        params: {
          label: resolveLabel(ontology, cmd.edge_id, "edge"),
          count: cmd.anchors.length,
        },
      };
    case "add_property":
      return {
        key: "addProperty",
        params: {
          name: cmd.property.name,
          owner: resolveLabel(ontology, cmd.owner_id, "any"),
        },
      };
    case "delete_property":
      return {
        key: "deleteProperty",
        params: {
          name: resolveProperty(ontology, cmd.owner_id, cmd.property_id),
          owner: resolveLabel(ontology, cmd.owner_id, "any"),
        },
      };
    case "update_property":
      return {
        key: "updateProperty",
        params: {
          name: resolveProperty(ontology, cmd.owner_id, cmd.property_id),
          owner: resolveLabel(ontology, cmd.owner_id, "any"),
        },
      };
    case "add_constraint":
      return {
        key: "addConstraint",
        params: { label: resolveLabel(ontology, cmd.node_id, "node") },
      };
    case "remove_constraint":
      return {
        key: "removeConstraint",
        params: { label: resolveLabel(ontology, cmd.node_id, "node") },
      };
    case "add_index":
      return {
        key: "addIndex",
        params: { label: resolveLabel(ontology, cmd.index.node_id, "node") },
      };
    case "remove_index":
      return { key: "removeIndex", params: { id: shortId(cmd.index_id) } };
    case "create_object_mapping":
      return {
        key: "createObjectMapping",
        params: { relation: cmd.mapping.relation },
      };
    case "update_object_mapping":
      return {
        key: "updateObjectMapping",
        params: { relation: cmd.mapping.relation },
      };
    case "delete_object_mapping":
      return { key: "deleteObjectMapping", params: { id: shortId(cmd.id) } };
    case "batch":
      return { key: "batch", params: { count: cmd.commands.length } };
    default: {
      // Exhaustiveness check: when a new `OntologyCommand` variant
      // lands in `types/ontology.ts`, the assignment below stops
      // compiling — the discriminated-union narrowing leaves `cmd`
      // typed as the new variant rather than `never`, so the dev
      // is forced to add a matching `case` (and i18n key) before the
      // tree builds. The runtime fallback keeps the UI graceful if
      // the server emits an op the client doesn't recognise yet.
      const _exhaustive: never = cmd;
      void _exhaustive;
      return { key: "unknown", params: {} };
    }
  }
}

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
