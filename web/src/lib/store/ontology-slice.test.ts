import { describe, it, expect, beforeEach } from "vitest";
import { createStore } from "zustand";
import type { AppStore } from "./types";
import { createOntologySlice } from "./ontology-slice";
import { createChatSlice } from "./chat-slice";
import { createChromeSlice } from "./chrome-slice";
import { createSelectionSlice } from "./selection-slice";
import { createDashboardSlice } from "./dashboard-slice";
import { createProjectSlice } from "./project-slice";
import { createVerificationSlice } from "./verification-slice";
import type { OntologyIR } from "@/types/api";

function createTestStore() {
  return createStore<AppStore>()((...a) => ({
    ...createOntologySlice(...a),
    ...createChatSlice(...a),
    ...createProjectSlice(...a),
    ...createChromeSlice(...a),
    ...createSelectionSlice(...a),
    ...createDashboardSlice(...a),
    ...createVerificationSlice(...a),
  }));
}

function makeOntology(overrides?: Partial<OntologyIR>): OntologyIR {
  return {
    id: "test-ont",
    name: "Test",
    version: 1,
    node_types: [
      {
        id: "n1",
        label: "Person",
        properties: [
          { id: "p1", name: "name", property_type: { type: "string" }, nullable: false },
        ],
        constraints: [],
      },
    ],
    edge_types: [],
    ...overrides,
  };
}

describe("OntologySlice", () => {
  let store: ReturnType<typeof createTestStore>;

  beforeEach(() => {
    store = createTestStore();
  });

  it("setOntology stores and retrieves ontology", () => {
    const ont = makeOntology();
    store.getState().setOntology(ont);
    expect(store.getState().ontology).toEqual(ont);
  });

  it("applyCommand: add_node creates a node and pushes undo", () => {
    store.getState().setOntology(makeOntology());
    store.getState().applyCommand({
      op: "add_node",
      id: "n2",
      label: "Product",
    });

    const state = store.getState();
    expect(state.ontology!.node_types).toHaveLength(2);
    expect(state.ontology!.node_types[1].label).toBe("Product");
    expect(state.commandStack).toHaveLength(1);
    expect(state.redoStack).toHaveLength(0);
  });

  it("undo reverses the last command", () => {
    store.getState().setOntology(makeOntology());
    store.getState().applyCommand({
      op: "add_node",
      id: "n2",
      label: "Product",
    });

    expect(store.getState().ontology!.node_types).toHaveLength(2);

    store.getState().undo();

    expect(store.getState().ontology!.node_types).toHaveLength(1);
    expect(store.getState().redoStack).toHaveLength(1);
    expect(store.getState().commandStack).toHaveLength(0);
  });

  it("redo re-applies the undone command", () => {
    store.getState().setOntology(makeOntology());
    store.getState().applyCommand({
      op: "add_node",
      id: "n2",
      label: "Product",
    });
    store.getState().undo();
    store.getState().redo();

    expect(store.getState().ontology!.node_types).toHaveLength(2);
    expect(store.getState().commandStack).toHaveLength(1);
    expect(store.getState().redoStack).toHaveLength(0);
  });

  it("applyCommand: rename_node changes label", () => {
    store.getState().setOntology(makeOntology());
    store.getState().applyCommand({
      op: "rename_node",
      node_id: "n1",
      new_label: "Individual",
    });

    expect(store.getState().ontology!.node_types[0].label).toBe("Individual");
  });

  it("applyCommand: delete_node removes node", () => {
    store.getState().setOntology(makeOntology());
    store.getState().applyCommand({ op: "delete_node", node_id: "n1" });
    expect(store.getState().ontology!.node_types).toHaveLength(0);
  });

  it("clearCommandStack resets undo/redo", () => {
    store.getState().setOntology(makeOntology());
    store.getState().applyCommand({
      op: "add_node",
      id: "n2",
      label: "Product",
    });
    store.getState().clearCommandStack();

    expect(store.getState().commandStack).toHaveLength(0);
    expect(store.getState().redoStack).toHaveLength(0);
  });

  it("multiple undo/redo maintains consistency", () => {
    store.getState().setOntology(makeOntology());

    // Apply 3 commands
    store.getState().applyCommand({ op: "add_node", id: "n2", label: "Product" });
    store.getState().applyCommand({ op: "add_node", id: "n3", label: "Order" });
    store.getState().applyCommand({
      op: "rename_node",
      node_id: "n1",
      new_label: "Customer",
    });

    expect(store.getState().ontology!.node_types).toHaveLength(3);
    expect(store.getState().commandStack).toHaveLength(3);

    // Undo all
    store.getState().undo();
    store.getState().undo();
    store.getState().undo();

    expect(store.getState().ontology!.node_types).toHaveLength(1);
    expect(store.getState().ontology!.node_types[0].label).toBe("Person");
    expect(store.getState().redoStack).toHaveLength(3);

    // Redo 2
    store.getState().redo();
    store.getState().redo();

    expect(store.getState().ontology!.node_types).toHaveLength(3);
    expect(store.getState().commandStack).toHaveLength(2);
    expect(store.getState().redoStack).toHaveLength(1);
  });

  it("new command after undo clears redo stack", () => {
    store.getState().setOntology(makeOntology());
    store.getState().applyCommand({ op: "add_node", id: "n2", label: "Product" });
    store.getState().undo();

    expect(store.getState().redoStack).toHaveLength(1);

    // New command should clear redo
    store.getState().applyCommand({ op: "add_node", id: "n3", label: "Order" });
    expect(store.getState().redoStack).toHaveLength(0);
  });
});
