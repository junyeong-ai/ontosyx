import type { StateCreator } from "zustand";
import type { OntologyIR, OntologyCommand, PropertyPatch, Cardinality } from "@/types/api";
import type { AppStore, OntologySlice } from "./types";
import { type OntologyIndex, buildOntologyIndex } from "@/lib/ontology-index";

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
            id: cmd.id!,
            label: cmd.label!,
            description: cmd.description,
            source_table: cmd.source_table,
            properties: [],
          },
        ],
      };
      return {
        ontology: newOntology,
        inverse: { op: "delete_node", node_id: cmd.id! },
      };
    }

    case "delete_node": {
      const node = ontology.node_types.find((n) => n.id === cmd.node_id);
      if (!node) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const removedEdges = ontology.edge_types.filter(
        (e) => e.source_node_id === cmd.node_id || e.target_node_id === cmd.node_id,
      );
      const removedIndexes = (ontology.indexes ?? []).filter(
        (idx) => idx.node_id === cmd.node_id,
      );
      const newOntology: OntologyIR = {
        ...ontology,
        node_types: ontology.node_types.filter((n) => n.id !== cmd.node_id),
        edge_types: ontology.edge_types.filter(
          (e) => e.source_node_id !== cmd.node_id && e.target_node_id !== cmd.node_id,
        ),
        indexes: (ontology.indexes ?? []).filter((idx) => idx.node_id !== cmd.node_id),
      };
      const inverseCommands: OntologyCommand[] = [
        {
          op: "add_node",
          id: node.id,
          label: node.label,
          description: node.description ?? undefined,
          source_table: node.source_table ?? undefined,
        },
        // Re-add properties
        ...node.properties.map((p) => ({
          op: "add_property" as const,
          owner_id: node.id,
          property: p,
        })),
        // Re-add constraints
        ...(node.constraints ?? []).map((c) => ({
          op: "add_constraint" as const,
          node_id: node.id,
          constraint: c,
        })),
        // Re-add connected edges + their properties
        ...removedEdges.flatMap((e) => [
          {
            op: "add_edge" as const,
            id: e.id,
            label: e.label,
            source_node_id: e.source_node_id,
            target_node_id: e.target_node_id,
            cardinality: e.cardinality ?? "many_to_many",
          },
          ...e.properties.map((p) => ({
            op: "add_property" as const,
            owner_id: e.id,
            property: p,
          })),
        ]),
        // Re-add indexes
        ...removedIndexes.map((idx) => ({
          op: "add_index" as const,
          index: idx,
        })),
      ];
      return {
        ontology: newOntology,
        inverse: { op: "batch", description: `Restore ${node.label}`, commands: inverseCommands },
      };
    }

    case "rename_node": {
      const node = ontology.node_types.find((n) => n.id === cmd.node_id);
      if (!node) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const oldLabel = node.label;
      const newOntology: OntologyIR = {
        ...ontology,
        node_types: ontology.node_types.map((n) =>
          n.id === cmd.node_id ? { ...n, label: cmd.new_label! } : n,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "rename_node", node_id: cmd.node_id!, new_label: oldLabel },
      };
    }

    case "update_node_description": {
      const node = ontology.node_types.find((n) => n.id === cmd.node_id);
      if (!node) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const oldDesc = node.description;
      const newOntology: OntologyIR = {
        ...ontology,
        node_types: ontology.node_types.map((n) =>
          n.id === cmd.node_id ? { ...n, description: cmd.description } : n,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "update_node_description", node_id: cmd.node_id!, description: oldDesc ?? undefined },
      };
    }

    case "add_edge": {
      const newOntology: OntologyIR = {
        ...ontology,
        edge_types: [
          ...ontology.edge_types,
          {
            id: cmd.id!,
            label: cmd.label!,
            source_node_id: cmd.source_node_id!,
            target_node_id: cmd.target_node_id!,
            properties: [],
            cardinality: cmd.cardinality as Cardinality,
          },
        ],
      };
      return {
        ontology: newOntology,
        inverse: { op: "delete_edge", edge_id: cmd.id! },
      };
    }

    case "delete_edge": {
      const edge = ontology.edge_types.find((e) => e.id === cmd.edge_id);
      if (!edge) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const newOntology: OntologyIR = {
        ...ontology,
        edge_types: ontology.edge_types.filter((e) => e.id !== cmd.edge_id),
      };
      const inverseCommands: OntologyCommand[] = [
        {
          op: "add_edge",
          id: edge.id,
          label: edge.label,
          source_node_id: edge.source_node_id,
          target_node_id: edge.target_node_id,
          cardinality: edge.cardinality ?? "many_to_many",
        },
        // Re-add edge properties
        ...edge.properties.map((p) => ({
          op: "add_property" as const,
          owner_id: edge.id,
          property: p,
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
      const edge = ontology.edge_types.find((e) => e.id === cmd.edge_id);
      if (!edge) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const oldLabel = edge.label;
      const newOntology: OntologyIR = {
        ...ontology,
        edge_types: ontology.edge_types.map((e) =>
          e.id === cmd.edge_id ? { ...e, label: cmd.new_label! } : e,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "rename_edge", edge_id: cmd.edge_id!, new_label: oldLabel },
      };
    }

    case "update_edge_cardinality": {
      const edge = ontology.edge_types.find((e) => e.id === cmd.edge_id);
      if (!edge) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const oldCard = edge.cardinality;
      const newOntology: OntologyIR = {
        ...ontology,
        edge_types: ontology.edge_types.map((e) =>
          e.id === cmd.edge_id ? { ...e, cardinality: cmd.cardinality as Cardinality } : e,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "update_edge_cardinality", edge_id: cmd.edge_id!, cardinality: oldCard ?? "many_to_many" },
      };
    }

    case "update_edge_description": {
      const edge = ontology.edge_types.find((e) => e.id === cmd.edge_id);
      if (!edge) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const oldDesc = edge.description;
      const newOntology: OntologyIR = {
        ...ontology,
        edge_types: ontology.edge_types.map((e) =>
          e.id === cmd.edge_id ? { ...e, description: cmd.description } : e,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "update_edge_description", edge_id: cmd.edge_id!, description: oldDesc ?? undefined },
      };
    }

    case "add_property": {
      const newOntology = mapOwner(ontology, cmd.owner_id!, (owner) => ({
        ...owner,
        properties: [...owner.properties, cmd.property!],
      }));
      return {
        ontology: newOntology,
        inverse: { op: "delete_property", owner_id: cmd.owner_id!, property_id: cmd.property!.id },
      };
    }

    case "delete_property": {
      const owner = findOwner(ontology, cmd.owner_id!);
      const prop = owner?.properties.find((p) => p.id === cmd.property_id);
      if (!owner || !prop) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const newOntology = mapOwner(ontology, cmd.owner_id!, (o) => ({
        ...o,
        properties: o.properties.filter((p) => p.id !== cmd.property_id),
      }));
      return {
        ontology: newOntology,
        inverse: { op: "add_property", owner_id: cmd.owner_id!, property: prop },
      };
    }

    case "update_property": {
      const owner = findOwner(ontology, cmd.owner_id);
      const prop = owner?.properties.find((p) => p.id === cmd.property_id);
      if (!owner || !prop) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const inversePatch: PropertyPatch = {};
      const { patch } = cmd;
      if (patch.name !== undefined) inversePatch.name = prop.name;
      if (patch.property_type !== undefined) inversePatch.property_type = prop.property_type;
      if (patch.nullable !== undefined) inversePatch.nullable = prop.nullable;
      if (patch.description !== undefined) inversePatch.description = prop.description ?? null;

      const newOntology = mapOwner(ontology, cmd.owner_id, (o) => ({
        ...o,
        properties: o.properties.map((p) =>
          p.id === cmd.property_id
            ? {
                ...p,
                ...(patch.name !== undefined && { name: patch.name }),
                ...(patch.property_type !== undefined && { property_type: patch.property_type }),
                ...(patch.nullable !== undefined && { nullable: patch.nullable }),
                ...(patch.description !== undefined && { description: patch.description ?? undefined }),
              }
            : p,
        ),
      }));
      return {
        ontology: newOntology,
        inverse: { op: "update_property", owner_id: cmd.owner_id, property_id: cmd.property_id, patch: inversePatch },
      };
    }

    case "add_constraint": {
      const newOntology: OntologyIR = {
        ...ontology,
        node_types: ontology.node_types.map((n) =>
          n.id === cmd.node_id
            ? { ...n, constraints: [...(n.constraints ?? []), cmd.constraint!] }
            : n,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "remove_constraint", node_id: cmd.node_id!, constraint_id: cmd.constraint!.id },
      };
    }

    case "remove_constraint": {
      const node = ontology.node_types.find((n) => n.id === cmd.node_id);
      const constraint = node?.constraints?.find((c) => c.id === cmd.constraint_id);
      if (!constraint) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const newOntology: OntologyIR = {
        ...ontology,
        node_types: ontology.node_types.map((n) =>
          n.id === cmd.node_id
            ? { ...n, constraints: (n.constraints ?? []).filter((c) => c.id !== cmd.constraint_id) }
            : n,
        ),
      };
      return {
        ontology: newOntology,
        inverse: { op: "add_constraint", node_id: cmd.node_id!, constraint: constraint },
      };
    }

    case "add_index": {
      const newOntology: OntologyIR = {
        ...ontology,
        indexes: [...(ontology.indexes ?? []), cmd.index!],
      };
      return {
        ontology: newOntology,
        inverse: { op: "remove_index", index_id: cmd.index!.id },
      };
    }

    case "remove_index": {
      const idx = (ontology.indexes ?? []).find((i) => i.id === cmd.index_id);
      if (!idx) return { ontology, inverse: { op: "batch", description: "noop", commands: [] } };
      const newOntology: OntologyIR = {
        ...ontology,
        indexes: (ontology.indexes ?? []).filter((i) => i.id !== cmd.index_id),
      };
      return {
        ontology: newOntology,
        inverse: { op: "add_index", index: idx },
      };
    }

    case "batch": {
      let current = ontology;
      const inverses: OntologyCommand[] = [];
      for (const sub of cmd.commands ?? []) {
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
    ontology.node_types.find((n) => n.id === ownerId) ??
    ontology.edge_types.find((e) => e.id === ownerId)
  );
}

/** Map over the owner (node or edge) that matches ownerId */
function mapOwner(
  ontology: OntologyIR,
  ownerId: string,
  fn: (owner: OntologyIR["node_types"][number] | OntologyIR["edge_types"][number]) =>
    OntologyIR["node_types"][number] | OntologyIR["edge_types"][number],
): OntologyIR {
  // Fast path: determine node vs edge from index
  const isNode =
    cachedIndex && cachedOntologyRef === ontology
      ? cachedIndex.nodeById.has(ownerId)
      : ontology.node_types.some((n) => n.id === ownerId);
  if (isNode) {
    return {
      ...ontology,
      node_types: ontology.node_types.map((n) =>
        n.id === ownerId ? (fn(n) as OntologyIR["node_types"][number]) : n,
      ),
    };
  }
  return {
    ...ontology,
    edge_types: ontology.edge_types.map((e) =>
      e.id === ownerId ? (fn(e) as OntologyIR["edge_types"][number]) : e,
    ),
  };
}

// ---------------------------------------------------------------------------
// Slice creator
// ---------------------------------------------------------------------------

export const createOntologySlice: StateCreator<AppStore, [], [], OntologySlice> = (set, get) => ({
  ontology: null,
  setOntology: (ontology) => {
    invalidateIndex();
    if (ontology) ensureIndex(ontology);
    set({ ontology, commandStack: [], redoStack: [] });
  },

  commandStack: [],
  redoStack: [],
  applyCommand: (command) => {
    const { ontology, commandStack } = get();
    if (!ontology) return;
    ensureIndex(ontology);
    const { ontology: newOntology, inverse } = applyCommandToOntology(ontology, command);
    invalidateIndex();
    const newStack = [...commandStack, { command, inverse, before: ontology }];
    set({
      ontology: newOntology,
      commandStack: capStack(newStack),
      redoStack: [],
    });
  },
  undo: () => {
    const { commandStack } = get();
    if (commandStack.length === 0) return;
    const last = commandStack[commandStack.length - 1];
    const newRedoStack = [...get().redoStack, last];
    set({
      ontology: last.before,
      commandStack: capStack(commandStack.slice(0, -1)),
      redoStack: capStack(newRedoStack),
    });
  },
  redo: () => {
    const { redoStack, ontology } = get();
    if (redoStack.length === 0 || !ontology) return;
    const entry = redoStack[redoStack.length - 1];
    const { ontology: newOntology, inverse } = applyCommandToOntology(ontology, entry.command);
    const newStack = [...get().commandStack, { command: entry.command, inverse, before: ontology }];
    set({
      ontology: newOntology,
      commandStack: capStack(newStack),
      redoStack: capStack(redoStack.slice(0, -1)),
    });
  },
  clearCommandStack: () => set({ commandStack: [], redoStack: [] }),
  resetOntology: () => set({ ontology: null, commandStack: [], redoStack: [] }),
  loadSavedOntology: (ontology) =>
    set({ ontology, commandStack: [], redoStack: [] }),

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
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
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
