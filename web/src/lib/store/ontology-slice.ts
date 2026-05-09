import type { StateCreator } from "zustand";
import type {
  OntologyCommand,
  OntologyIR,
  PropertyOwner,
  PropertyPatch,
} from "@/types/api";
import type { AppStore, CommandEntry, OntologySlice } from "./types";
import { type OntologyIndex, buildOntologyIndex } from "@/lib/ontology-index";
import { toast } from "@/components/ui/toast";
import { arr } from "@/lib/ir-collections";
import { getI18nBridge } from "@/lib/i18n-bridge";

const MAX_UNDO_DEPTH = 50;

/** Cap a stack to MAX_UNDO_DEPTH, keeping the most recent entries. */
function capStack<T>(stack: T[]): T[] {
  return stack.length > MAX_UNDO_DEPTH
    ? stack.slice(stack.length - MAX_UNDO_DEPTH)
    : stack;
}

// Module-level index cache — rebuilt only when ontology reference changes.
// This avoids O(N) lookups in findOwner/mapOwner during command application.
let cachedIndex: OntologyIndex | null = null;
let cachedOntologyRef: OntologyIR | null = null;

/** Get or build the O(1) lookup index for the current ontology. */
export function ensureIndex(ontology: OntologyIR): OntologyIndex {
  if (cachedOntologyRef !== ontology || !cachedIndex) {
    cachedIndex = buildOntologyIndex(ontology);
    cachedOntologyRef = ontology;
  }
  return cachedIndex;
}

/** Invalidate the index when ontology changes (called after mutations). */
function invalidateIndex() {
  cachedIndex = null;
  cachedOntologyRef = null;
}

/** Track whether the undo cap warning has been shown for the current stack. */
let capWarningShown = false;

function propertyOwnerId(owner: PropertyOwner): string {
  return owner.type_id;
}

// ---------------------------------------------------------------------------
// Optimistic command application (FE mirror of Rust OntologyCommand)
// ---------------------------------------------------------------------------

function applyCommandToOntology(
  ontology: OntologyIR,
  cmd: OntologyCommand,
): { ontology: OntologyIR; inverse: OntologyCommand } {
  switch (cmd.op) {
    case "add_node": {
      const newOntology: OntologyIR = {
        ...ontology,
        node_types: [
          ...ontology.node_types,
          {
            id: cmd.id,
            label: cmd.label,
            description: cmd.description ?? { default: "" },
            properties: [],
          },
        ],
      };
      return {
        ontology: newOntology,
        inverse: { op: "delete_node", node_id: cmd.id },
      };
    }

    case "create_node_type": {
      const newOntology: OntologyIR = {
        ...ontology,
        node_types: [...arr(ontology.node_types), cmd.node],
      };
      return {
        ontology: newOntology,
        inverse: { op: "delete_node", node_id: cmd.node.id },
      };
    }

    case "delete_node": {
      const node = arr(ontology.node_types).find((n) => n.id === cmd.node_id);
      if (!node) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const removedEdges = arr(ontology.edge_types).filter(
        (e) => e.source_node_id === cmd.node_id || e.target_node_id === cmd.node_id,
      );
      const removedEdgeIds = new Set(removedEdges.map((e) => e.id));
      const removedObjectMappings = arr(ontology.object_mappings).filter(
        (m) => m.node_type_id === cmd.node_id,
      );
      const removedLinkMappings = arr(ontology.link_mappings).filter((m) =>
        removedEdgeIds.has(m.edge_type_id),
      );
      const removedIndexes = arr(ontology.indexes).filter(
        (idx) => idx.node_id === cmd.node_id,
      );
      const newOntology: OntologyIR = {
        ...ontology,
        node_types: arr(ontology.node_types).filter((n) => n.id !== cmd.node_id),
        edge_types: arr(ontology.edge_types).filter(
          (e) => e.source_node_id !== cmd.node_id && e.target_node_id !== cmd.node_id,
        ),
        indexes: arr(ontology.indexes).filter((idx) => idx.node_id !== cmd.node_id),
        object_mappings: arr(ontology.object_mappings).filter(
          (m) => m.node_type_id !== cmd.node_id,
        ),
        link_mappings: arr(ontology.link_mappings).filter(
          (m) => !removedEdgeIds.has(m.edge_type_id),
        ),
      };
      const inverseCommands: OntologyCommand[] = [
        {
          op: "create_node_type",
          node,
        },
        ...removedEdges.map((edge) => ({
          op: "create_edge_type" as const,
          edge,
        })),
        // Re-add indexes
        ...removedIndexes.map((idx) => ({
          op: "add_index" as const,
          index: idx,
        })),
        ...removedObjectMappings.map((mapping) => ({
          op: "create_object_mapping" as const,
          mapping,
        })),
        ...removedLinkMappings.map((mapping) => ({
          op: "create_link_mapping" as const,
          mapping,
        })),
      ];
      return {
        ontology: newOntology,
        inverse: { op: "batch", description: `Restore ${node.label}`, commands: inverseCommands },
      };
    }

    case "rename_node": {
      const node = arr(ontology.node_types).find((n) => n.id === cmd.node_id);
      if (!node) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const oldLabel = node.label;
      const newOntology: OntologyIR = {
        ...ontology,
        node_types: arr(ontology.node_types).map((n) =>
          n.id === cmd.node_id ? { ...n, label: cmd.new_label } : n,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "rename_node", node_id: cmd.node_id, new_label: oldLabel },
      };
    }

    case "update_node_description": {
      const node = arr(ontology.node_types).find((n) => n.id === cmd.node_id);
      if (!node) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const oldDesc = node.description;
      const newOntology: OntologyIR = {
        ...ontology,
        node_types: arr(ontology.node_types).map((n) =>
          n.id === cmd.node_id
            ? { ...n, description: cmd.description ?? { default: "" } }
            : n,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "update_node_description", node_id: cmd.node_id, description: oldDesc ?? undefined },
      };
    }

    case "add_edge": {
      const newOntology: OntologyIR = {
        ...ontology,
        edge_types: [
          ...ontology.edge_types,
          {
            id: cmd.id,
            label: cmd.label,
            description: { default: "" },
            source_node_id: cmd.source_node_id,
            target_node_id: cmd.target_node_id,
            properties: [],
            cardinality: cmd.cardinality,
          },
        ],
      };
      return {
        ontology: newOntology,
        inverse: { op: "delete_edge", edge_id: cmd.id },
      };
    }

    case "create_edge_type": {
      const newOntology: OntologyIR = {
        ...ontology,
        edge_types: [...arr(ontology.edge_types), cmd.edge],
      };
      return {
        ontology: newOntology,
        inverse: { op: "delete_edge", edge_id: cmd.edge.id },
      };
    }

    case "delete_edge": {
      const edge = arr(ontology.edge_types).find((e) => e.id === cmd.edge_id);
      if (!edge) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const removedLinkMappings = arr(ontology.link_mappings).filter(
        (m) => m.edge_type_id === cmd.edge_id,
      );
      const newOntology: OntologyIR = {
        ...ontology,
        edge_types: arr(ontology.edge_types).filter((e) => e.id !== cmd.edge_id),
        link_mappings: arr(ontology.link_mappings).filter((m) => m.edge_type_id !== cmd.edge_id),
      };
      const inverseCommands: OntologyCommand[] = [
        {
          op: "create_edge_type",
          edge,
        },
        ...removedLinkMappings.map((mapping) => ({
          op: "create_link_mapping" as const,
          mapping,
        })),
      ];
      return {
        ontology: newOntology,
        inverse: inverseCommands.length === 1
          ? inverseCommands[0]
          : { op: "batch", description: `Restore edge ${edge.label}`, commands: inverseCommands },
      };
    }

    case "rename_edge": {
      const edge = arr(ontology.edge_types).find((e) => e.id === cmd.edge_id);
      if (!edge) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const oldLabel = edge.label;
      const newOntology: OntologyIR = {
        ...ontology,
        edge_types: arr(ontology.edge_types).map((e) =>
          e.id === cmd.edge_id ? { ...e, label: cmd.new_label } : e,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "rename_edge", edge_id: cmd.edge_id, new_label: oldLabel },
      };
    }

    case "update_edge_cardinality": {
      const edge = arr(ontology.edge_types).find((e) => e.id === cmd.edge_id);
      if (!edge) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const oldCard = edge.cardinality;
      const newOntology: OntologyIR = {
        ...ontology,
        edge_types: arr(ontology.edge_types).map((e) =>
          e.id === cmd.edge_id ? { ...e, cardinality: cmd.cardinality } : e,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "update_edge_cardinality", edge_id: cmd.edge_id, cardinality: oldCard ?? "many_to_many" },
      };
    }

    case "update_edge_description": {
      const edge = arr(ontology.edge_types).find((e) => e.id === cmd.edge_id);
      if (!edge) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const oldDesc = edge.description;
      const newOntology: OntologyIR = {
        ...ontology,
        edge_types: arr(ontology.edge_types).map((e) =>
          e.id === cmd.edge_id
            ? { ...e, description: cmd.description ?? { default: "" } }
            : e,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "update_edge_description", edge_id: cmd.edge_id, description: oldDesc ?? undefined },
      };
    }

    case "add_property": {
      const newOntology = mapPropertyOwner(ontology, cmd.owner, (owner) => ({
        ...owner,
        properties: [...owner.properties, cmd.property],
      }));
      return {
        ontology: newOntology,
        inverse: { op: "delete_property", owner: cmd.owner, property_id: cmd.property.id },
      };
    }

    case "delete_property": {
      const ownerId = propertyOwnerId(cmd.owner);
      const owner = findOwner(ontology, ownerId);
      const prop = arr(owner?.properties).find((p) => p.id === cmd.property_id);
      if (!owner || !prop) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const nodeOwner = cmd.owner.kind === "node"
        ? arr(ontology.node_types).find((n) => n.id === ownerId)
        : undefined;
      const ownerIsNode = Boolean(nodeOwner);
      const removedConstraints = nodeOwner
        ? arr(nodeOwner.constraints).filter((constraint) => {
            const ids =
              "property_ids" in constraint
                ? constraint.property_ids
                : [constraint.property_id];
            return arr(ids).includes(cmd.property_id);
          })
        : [];
      const removedIndexes = arr(ontology.indexes).filter(
        (index) =>
          index.node_id === ownerId &&
          ("property_ids" in index
            ? arr(index.property_ids).includes(cmd.property_id)
            : index.property_id === cmd.property_id),
      );
      const previousMappings = ownerIsNode
        ? arr(ontology.object_mappings).filter(
            (mapping) =>
              mapping.node_type_id === ownerId &&
              arr(mapping.property_mappings).some((m) => m.property_id === cmd.property_id),
          )
        : [];
      const withoutProperty: OntologyIR = nodeOwner
        ? {
            ...ontology,
            node_types: arr(ontology.node_types).map((node) =>
              node.id === ownerId
                ? {
                    ...node,
                    properties: arr(node.properties).filter((p) => p.id !== cmd.property_id),
                    constraints: arr(node.constraints).filter(
                      (constraint) =>
                        !removedConstraints.some((removed) => removed.id === constraint.id),
                    ),
                  }
                : node,
            ),
          }
        : mapPropertyOwner(ontology, cmd.owner, (o) => ({
            ...o,
            properties: arr(o.properties).filter((p) => p.id !== cmd.property_id),
          }));
      const newOntology: OntologyIR = ownerIsNode
        ? {
            ...withoutProperty,
            indexes: arr(withoutProperty.indexes).filter(
              (index) => !removedIndexes.some((removed) => removed.id === index.id),
            ),
            object_mappings: arr(withoutProperty.object_mappings).map((mapping) =>
              mapping.node_type_id === ownerId
                ? {
                    ...mapping,
                    property_mappings: arr(mapping.property_mappings).filter(
                      (m) => m.property_id !== cmd.property_id,
                    ),
                  }
                : mapping,
            ),
          }
        : withoutProperty;
      const addProperty: OntologyCommand = {
        op: "add_property",
        owner: cmd.owner,
        property: prop,
      };
      const restoreConstraints: OntologyCommand[] = ownerIsNode
        ? removedConstraints.map((constraint) => ({
            op: "add_constraint" as const,
            node_id: ownerId,
            constraint,
          }))
        : [];
      const restoreIndexes: OntologyCommand[] = removedIndexes.map((index) => ({
        op: "add_index" as const,
        index,
      }));
      const restoreMappings: OntologyCommand[] = previousMappings.map((mapping) => ({
        op: "update_object_mapping" as const,
        id: mapping.id,
        mapping,
      }));
      const inverseCommands = [
        addProperty,
        ...restoreConstraints,
        ...restoreIndexes,
        ...restoreMappings,
      ];
      return {
        ontology: newOntology,
        inverse:
          inverseCommands.length === 1
            ? addProperty
            : {
                op: "batch",
                description: `Restore property ${prop.name}`,
                commands: inverseCommands,
              },
      };
    }

    case "update_property": {
      const ownerId = propertyOwnerId(cmd.owner);
      const owner = findOwner(ontology, ownerId);
      const prop = arr(owner?.properties).find((p) => p.id === cmd.property_id);
      if (!owner || !prop) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const inversePatch: PropertyPatch = {};
      const { patch } = cmd;
      if (patch.name !== undefined) inversePatch.name = prop.name;
      if (patch.property_type !== undefined) inversePatch.property_type = prop.property_type;
      if (patch.nullable !== undefined) inversePatch.nullable = prop.nullable;
      if (patch.description !== undefined) inversePatch.description = prop.description;

      const newOntology = mapPropertyOwner(ontology, cmd.owner, (o) => ({
        ...o,
        properties: arr(o.properties).map((p) =>
          p.id === cmd.property_id
            ? {
                ...p,
                ...(patch.name !== undefined && { name: patch.name }),
                ...(patch.property_type !== undefined && { property_type: patch.property_type }),
                ...(patch.nullable !== undefined && { nullable: patch.nullable }),
                ...(patch.description !== undefined && { description: patch.description }),
              }
            : p,
        ),
      }));
      return {
        ontology: newOntology,
        inverse: { op: "update_property", owner: cmd.owner, property_id: cmd.property_id, patch: inversePatch },
      };
    }

    case "add_constraint": {
      const newOntology: OntologyIR = {
        ...ontology,
        node_types: arr(ontology.node_types).map((n) =>
          n.id === cmd.node_id
            ? { ...n, constraints: [...arr(n.constraints), cmd.constraint] }
            : n,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "remove_constraint", node_id: cmd.node_id, constraint_id: cmd.constraint.id },
      };
    }

    case "remove_constraint": {
      const node = arr(ontology.node_types).find((n) => n.id === cmd.node_id);
      const constraint = node?.constraints?.find((c) => c.id === cmd.constraint_id);
      if (!constraint) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const newOntology: OntologyIR = {
        ...ontology,
        node_types: arr(ontology.node_types).map((n) =>
          n.id === cmd.node_id
            ? { ...n, constraints: arr(n.constraints).filter((c) => c.id !== cmd.constraint_id) }
            : n,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "add_constraint", node_id: cmd.node_id, constraint },
      };
    }

    case "add_index": {
      const newOntology: OntologyIR = {
        ...ontology,
        indexes: [...arr(ontology.indexes), cmd.index],
      };
      return {
        ontology: newOntology,
        inverse: { op: "remove_index", index_id: cmd.index.id },
      };
    }

    case "remove_index": {
      const idx = arr(ontology.indexes).find((i) => i.id === cmd.index_id);
      if (!idx) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const newOntology: OntologyIR = {
        ...ontology,
        indexes: arr(ontology.indexes).filter((i) => i.id !== cmd.index_id),
      };
      return {
        ontology: newOntology,
        inverse: { op: "add_index", index: idx },
      };
    }

    case "create_object_mapping": {
      const existing = arr(ontology.object_mappings);
      const newOntology: OntologyIR = {
        ...ontology,
        object_mappings: [...existing, cmd.mapping],
      };
      return {
        ontology: newOntology,
        inverse: { op: "delete_object_mapping", id: cmd.mapping.id },
      };
    }

    case "update_object_mapping": {
      const existing = arr(ontology.object_mappings);
      const previous = existing.find((m) => m.id === cmd.id);
      if (!previous)
        return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const newOntology: OntologyIR = {
        ...ontology,
        object_mappings: existing.map((m) =>
          m.id === cmd.id ? cmd.mapping : m,
        ),
      };
      return {
        ontology: newOntology,
        inverse: {
          op: "update_object_mapping",
          id: cmd.id,
          mapping: previous,
        },
      };
    }

    case "delete_object_mapping": {
      const existing = arr(ontology.object_mappings);
      const removed = existing.find((m) => m.id === cmd.id);
      if (!removed)
        return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const newOntology: OntologyIR = {
        ...ontology,
        object_mappings: existing.filter((m) => m.id !== cmd.id),
      };
      return {
        ontology: newOntology,
        inverse: { op: "create_object_mapping", mapping: removed },
      };
    }

    case "create_link_mapping": {
      const existing = arr(ontology.link_mappings);
      const newOntology: OntologyIR = {
        ...ontology,
        link_mappings: [...existing, cmd.mapping],
      };
      return {
        ontology: newOntology,
        inverse: { op: "delete_link_mapping", id: cmd.mapping.id },
      };
    }

    case "update_link_mapping": {
      const existing = arr(ontology.link_mappings);
      const previous = existing.find((m) => m.id === cmd.id);
      if (!previous)
        return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const newOntology: OntologyIR = {
        ...ontology,
        link_mappings: existing.map((m) => (m.id === cmd.id ? cmd.mapping : m)),
      };
      return {
        ontology: newOntology,
        inverse: {
          op: "update_link_mapping",
          id: cmd.id,
          mapping: previous,
        },
      };
    }

    case "delete_link_mapping": {
      const existing = arr(ontology.link_mappings);
      const removed = existing.find((m) => m.id === cmd.id);
      if (!removed)
        return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const newOntology: OntologyIR = {
        ...ontology,
        link_mappings: existing.filter((m) => m.id !== cmd.id),
      };
      return {
        ontology: newOntology,
        inverse: { op: "create_link_mapping", mapping: removed },
      };
    }

    case "batch": {
      let current = ontology;
      const inverses: OntologyCommand[] = [];
      for (const sub of cmd.commands) {
        const result = applyCommandToOntology(current, sub);
        current = result.ontology;
        inverses.push(result.inverse);
      }
      return {
        ontology: current,
        inverse: { op: "batch", description: `Undo: ${cmd.description}`, commands: inverses.reverse() },
      };
    }

    default:
      return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
  }
}

/** Find node or edge by id (O(1) via index, O(N) fallback) */
function findOwner(ontology: OntologyIR, ownerId: string) {
  // Fast path: use cached index if available
  if (cachedIndex && cachedOntologyRef === ontology) {
    return cachedIndex.nodeById.get(ownerId) ?? cachedIndex.edgeById.get(ownerId);
  }
  return (
    arr(ontology.node_types).find((n) => n.id === ownerId) ??
    arr(ontology.edge_types).find((e) => e.id === ownerId)
  );
}

/** Map over the owner (node or edge) addressed by a typed property owner path. */
function mapPropertyOwner(
  ontology: OntologyIR,
  owner: PropertyOwner,
  fn: (owner: OntologyIR["node_types"][number] | OntologyIR["edge_types"][number]) =>
    OntologyIR["node_types"][number] | OntologyIR["edge_types"][number],
): OntologyIR {
  if (owner.kind === "node") {
    return {
      ...ontology,
      node_types: arr(ontology.node_types).map((n) =>
        n.id === owner.type_id
          ? {
              ...n,
              properties: fn(n).properties,
            }
          : n,
      ),
    };
  }
  return {
    ...ontology,
    edge_types: arr(ontology.edge_types).map((e) =>
      e.id === owner.type_id
        ? {
            ...e,
            properties: fn(e).properties,
          }
        : e,
    ),
  };
}

// ---------------------------------------------------------------------------
// Slice creator
// ---------------------------------------------------------------------------

export const createOntologySlice: StateCreator<AppStore, [], [], OntologySlice> = (set, get) => ({
  ontology: null,

  commandStack: [],
  redoStack: [],
  applyCommand: (command) => {
    const { ontology, commandStack } = get();
    if (!ontology) return;
    ensureIndex(ontology);
    const { ontology: newOntology, inverse } = applyCommandToOntology(ontology, command);
    invalidateIndex();
    const newStack = [...commandStack, { command, inverse }];
    const capped = capStack(newStack);
    if (capped.length < newStack.length && !capWarningShown) {
      toast.info(getI18nBridge().inspectorToast.undoLimit);
      capWarningShown = true;
    }
    set({
      ontology: newOntology,
      commandStack: capped,
      redoStack: [],
    });
  },
  undo: () => {
    const { commandStack, ontology } = get();
    if (commandStack.length === 0 || !ontology) return;
    const last = commandStack[commandStack.length - 1];
    ensureIndex(ontology);
    const { ontology: restored } = applyCommandToOntology(ontology, last.inverse);
    invalidateIndex();
    const newRedoStack = [...get().redoStack, last];
    set({
      ontology: restored,
      commandStack: capStack(commandStack.slice(0, -1)),
      redoStack: capStack(newRedoStack),
    });
  },
  redo: () => {
    const { redoStack, ontology } = get();
    if (redoStack.length === 0 || !ontology) return;
    const entry = redoStack[redoStack.length - 1];
    ensureIndex(ontology);
    const { ontology: newOntology, inverse } = applyCommandToOntology(ontology, entry.command);
    invalidateIndex();
    const newStack = [...get().commandStack, { command: entry.command, inverse }];
    set({
      ontology: newOntology,
      commandStack: capStack(newStack),
      redoStack: capStack(redoStack.slice(0, -1)),
    });
  },
  clearCommandStack: () => { capWarningShown = false; set({ commandStack: [], redoStack: [] }); },
  applyOntologyDraftSnapshot: (project) => set((state) => {
    capWarningShown = false;
    invalidateIndex();

    if (!project) {
      return {
        activeOntologyDraft: null,
        ontology: null,
        commandStack: [],
        redoStack: [],
      };
    }

    const baseOntology = project.ontology ?? null;
    const switchingDrafts = state.activeOntologyDraft?.id !== project.id;

    // Same-project refetch (e.g. cache invalidation after save):
    // replay unsaved commands on the new server snapshot so the
    // user's in-flight edits survive. Switching projects clears
    // the stack — the edits belong to the previous project.
    if (
      !switchingDrafts
      && state.commandStack.length > 0
      && baseOntology
    ) {
      let working = baseOntology;
      const replayed: CommandEntry[] = [];
      for (const entry of state.commandStack) {
        const result = applyCommandToOntology(working, entry.command);
        working = result.ontology;
        replayed.push({ command: entry.command, inverse: result.inverse });
      }
      ensureIndex(working);
      return {
        activeOntologyDraft: project,
        ontology: working,
        commandStack: replayed,
      };
    }

    if (baseOntology) ensureIndex(baseOntology);
    return {
      activeOntologyDraft: project,
      ontology: baseOntology,
      commandStack: [],
      redoStack: [],
    };
  }),
  loadStandaloneOntology: (ontology) => {
    capWarningShown = false;
    invalidateIndex();
    ensureIndex(ontology);
    set({
      // Standalone-mode invariant: never paired with a project.
      activeOntologyDraft: null,
      ontology,
      commandStack: [],
      redoStack: [],
    });
  },

  nodeGroups: {},
  restoreNodeGroups: (groups) => set({ nodeGroups: groups }),
  createGroup: (name, nodeIds) => {
    const id = `group-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    set((s) => ({
      nodeGroups: { ...s.nodeGroups, [id]: { name, nodeIds, collapsed: false } },
    }));
  },
  toggleGroupCollapse: (groupId) => {
    set((s) => {
      const group = s.nodeGroups[groupId];
      if (!group) return s;
      return {
        nodeGroups: {
          ...s.nodeGroups,
          [groupId]: { ...group, collapsed: !group.collapsed },
        },
      };
    });
  },
  removeGroup: (groupId) => {
    set((s) => {
      const { [groupId]: _, ...rest } = s.nodeGroups;
      return { nodeGroups: rest };
    });
  },
  renameGroup: (groupId, name) => {
    set((s) => {
      const group = s.nodeGroups[groupId];
      if (!group) return s;
      return {
        nodeGroups: {
          ...s.nodeGroups,
          [groupId]: { ...group, name },
        },
      };
    });
  },
});
