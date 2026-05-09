import { arr } from "@/lib/ir-collections";
import type {
  EdgeTypeDef,
  LocalizedText,
  NodeTypeDef,
  OntologyCommand,
  PropertyBinding,
  PropertyDef,
  PropertyPatch,
  PropertyType,
  PropertyValue,
  SourceLineage,
} from "@/types/api";
import type { components } from "@/types/api.generated";

export type WireOntologyCommand = components["schemas"]["OntologyCommand"];

const EMPTY_TEXT: LocalizedText = { default: "" };

function localizedText(value: LocalizedText | null | undefined): LocalizedText {
  return value ?? EMPTY_TEXT;
}

function propertyType(value: components["schemas"]["PropertyType"]): PropertyType {
  if (value.type === "list") {
    return { type: value.type, element: propertyType(value.element) };
  }
  return { type: value.type };
}

function propertyValue(value: components["schemas"]["PropertyValue"]): PropertyValue {
  switch (value.type) {
    case "list":
      return { type: value.type, value: value.value.map(propertyValue) };
    case "map":
      return {
        type: value.type,
        value: Object.fromEntries(
          Object.entries(value.value).map(([key, item]) => [key, propertyValue(item)]),
        ),
      };
    default:
      return value;
  }
}

function sourceLineage(
  value: components["schemas"]["SourceLineage"] | null | undefined,
): SourceLineage | undefined {
  if (!value) {
    return undefined;
  }
  return {
    ...value,
    source_id: value.source_id ?? undefined,
  };
}

function propertyBinding(
  binding: components["schemas"]["PropertyBinding"],
): PropertyBinding {
  switch (binding.kind) {
    case "value_set":
    case "code_system":
      return {
        ...binding,
        concept_map_id: binding.concept_map_id ?? undefined,
        valid_from: binding.valid_from ?? undefined,
        valid_to: binding.valid_to ?? undefined,
      };
    case "notation_pattern":
    case "value_range":
    case "concept":
      return {
        ...binding,
        valid_from: binding.valid_from ?? undefined,
        valid_to: binding.valid_to ?? undefined,
      };
  }
}

function property(def: components["schemas"]["PropertyDef"]): PropertyDef {
  return {
    ...def,
    id: def.id,
    name: def.name,
    property_type: propertyType(def.property_type),
    default_value:
      def.default_value == null ? undefined : propertyValue(def.default_value),
    description: localizedText(def.description),
    source_column: def.source_column ?? undefined,
    classification: def.classification ?? undefined,
    bindings: def.bindings?.map(propertyBinding),
  };
}

function propertyPatch(patch: components["schemas"]["PropertyPatch"]): PropertyPatch {
  return {
    ...patch,
    name: patch.name ?? undefined,
    nullable: patch.nullable ?? undefined,
    default_value:
      patch.default_value == null ? undefined : propertyValue(patch.default_value),
    property_type:
      patch.property_type == null ? undefined : propertyType(patch.property_type),
    description: patch.description ?? undefined,
  };
}

function nodeType(node: components["schemas"]["NodeTypeDef"]): NodeTypeDef {
  return {
    ...node,
    id: node.id,
    label: node.label,
    description: localizedText(node.description),
    source_lineage: sourceLineage(node.source_lineage),
    properties: arr(node.properties).map(property),
    concept_id: node.concept_id ?? undefined,
  };
}

function edgeType(edge: components["schemas"]["EdgeTypeDef"]): EdgeTypeDef {
  return {
    ...edge,
    id: edge.id,
    label: edge.label,
    description: localizedText(edge.description),
    source_node_id: edge.source_node_id,
    target_node_id: edge.target_node_id,
    properties: arr(edge.properties).map(property),
    concept_id: edge.concept_id ?? undefined,
  };
}

export function normalizeOntologyCommand(cmd: WireOntologyCommand): OntologyCommand {
  switch (cmd.op) {
    case "create_node_type":
      return { op: cmd.op, node: nodeType(cmd.node) };
    case "create_edge_type":
      return { op: cmd.op, edge: edgeType(cmd.edge) };
    case "add_property":
      return { op: cmd.op, owner: cmd.owner, property: property(cmd.property) };
    case "update_property":
      return {
        op: cmd.op,
        owner: cmd.owner,
        property_id: cmd.property_id,
        patch: propertyPatch(cmd.patch),
      };
    case "batch":
      return {
        op: cmd.op,
        description: cmd.description,
        commands: cmd.commands.map(normalizeOntologyCommand),
      };
    case "add_node":
    case "delete_node":
    case "rename_node":
    case "update_node_description":
    case "add_edge":
    case "delete_edge":
    case "rename_edge":
    case "update_edge_cardinality":
    case "update_edge_description":
    case "delete_property":
    case "add_constraint":
    case "remove_constraint":
    case "add_index":
    case "remove_index":
    case "create_object_mapping":
    case "update_object_mapping":
    case "delete_object_mapping":
    case "create_link_mapping":
    case "update_link_mapping":
    case "delete_link_mapping":
      return cmd;
  }
}

export function normalizeOntologyCommands(
  commands: readonly WireOntologyCommand[],
): OntologyCommand[] {
  return commands.map(normalizeOntologyCommand);
}
