import { describe, it, expect } from "vitest";

import type { OntologyCommand, OntologyIR, PropertyDef } from "@/types/api";

import { commandOpBadge, formatCommand } from "./command-format";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function person(): NonNullable<OntologyIR["node_types"]>[number] {
  return {
    id: "node-person-uuid",
    label: "Person",
    description: { default: "" },
    properties: [
      {
        id: "prop-email-uuid",
        name: "email",
description: { default: "" },
        property_type: { type: "string" },
        nullable: false,
      },
    ],
  };
}

function company(): NonNullable<OntologyIR["node_types"]>[number] {
  return {
    id: "node-company-uuid",
    label: "Company",
    description: { default: "" },
    properties: [],
  };
}

function worksAt(): NonNullable<OntologyIR["edge_types"]>[number] {
  return {
    id: "edge-worksat-uuid",
    label: "WORKS_AT",
    description: { default: "" },
    source_node_id: "node-person-uuid",
    target_node_id: "node-company-uuid",
    properties: [],
    cardinality: "many_to_one",
  };
}

function ontology(): OntologyIR {
  return {
    id: "ont-1",
    name: "test",
description: { default: "" },
    version: { number: 1 },
    node_types: [person(), company()],
    edge_types: [worksAt()],
  };
}

// ---------------------------------------------------------------------------
// formatCommand — label resolution against the ontology
// ---------------------------------------------------------------------------

describe("formatCommand — label resolution", () => {
  it("resolves node ids to their label on delete_node", () => {
    const cmd: OntologyCommand = { op: "delete_node", node_id: "node-person-uuid" };
    expect(formatCommand(cmd, ontology())).toEqual({
      key: "deleteNode",
      params: { label: "Person" },
    });
  });

  it("resolves source + target labels on add_edge", () => {
    const cmd: OntologyCommand = {
      op: "add_edge",
      id: "new",
      label: "FOUNDED",
      source_node_id: "node-person-uuid",
      target_node_id: "node-company-uuid",
      cardinality: "one_to_many",
    };
    expect(formatCommand(cmd, ontology())).toEqual({
      key: "addEdge",
      params: { label: "FOUNDED", source: "Person", target: "Company" },
    });
  });

  it("resolves a property name on update_property", () => {
    const cmd: OntologyCommand = {
      op: "update_property",
      owner_id: "node-person-uuid",
      property_id: "prop-email-uuid",
      patch: { nullable: true },
    };
    expect(formatCommand(cmd, ontology())).toEqual({
      key: "updateProperty",
      params: { name: "email", owner: "Person" },
    });
  });

  it("falls back to a truncated UUID when the id isn't in the ontology", () => {
    const cmd: OntologyCommand = {
      op: "delete_node",
      node_id: "00000000-aaaa-bbbb-cccc-deadbeef1234",
    };
    expect(formatCommand(cmd, ontology())).toEqual({
      key: "deleteNode",
      params: { label: "00000000…" },
    });
  });

  it("does not truncate short ids (12-char threshold)", () => {
    const cmd: OntologyCommand = { op: "delete_node", node_id: "short-id" };
    expect(formatCommand(cmd, ontology())).toEqual({
      key: "deleteNode",
      params: { label: "short-id" },
    });
  });

  it("works when the ontology is null (no context available)", () => {
    const cmd: OntologyCommand = { op: "delete_node", node_id: "node-person-uuid" };
    expect(formatCommand(cmd, null)).toEqual({
      key: "deleteNode",
      params: { label: "node-per…" },
    });
  });
});

describe("formatCommand — per-op rendering", () => {
  it("renders add_node with the declared label verbatim", () => {
    const cmd: OntologyCommand = {
      op: "add_node",
      id: "new",
      label: "Customer",
      description: { default: "A paying user" },
    };
    expect(formatCommand(cmd)).toEqual({
      key: "addNode",
      params: { label: "Customer" },
    });
  });

  it("renders rename_node with both labels", () => {
    const cmd: OntologyCommand = {
      op: "rename_node",
      node_id: "node-person-uuid",
      new_label: "Contact",
    };
    expect(formatCommand(cmd, ontology())).toEqual({
      key: "renameNode",
      params: { label: "Person", newLabel: "Contact" },
    });
  });

  it("emits batch count as a numeric param so the renderer can pluralise via ICU", () => {
    const one: OntologyCommand = {
      op: "batch",
      description: "x",
      commands: [{ op: "delete_node", node_id: "n" }],
    };
    const two: OntologyCommand = {
      op: "batch",
      description: "x",
      commands: [
        { op: "delete_node", node_id: "n" },
        { op: "delete_node", node_id: "m" },
      ],
    };
    expect(formatCommand(one)).toEqual({ key: "batch", params: { count: 1 } });
    expect(formatCommand(two)).toEqual({ key: "batch", params: { count: 2 } });
  });

  it("renders add_property with the property name taken from the command", () => {
    const prop: PropertyDef = {
      id: "prop-new",
      name: "phone",
description: { default: "" },
      property_type: { type: "string" },
    };
    const cmd: OntologyCommand = {
      op: "add_property",
      owner_id: "node-person-uuid",
      property: prop,
    };
    expect(formatCommand(cmd, ontology())).toEqual({
      key: "addProperty",
      params: { name: "phone", owner: "Person" },
    });
  });
});

// ---------------------------------------------------------------------------
// commandOpBadge — colour grouping
// ---------------------------------------------------------------------------

describe("commandOpBadge", () => {
  it("groups additions as green ADD", () => {
    for (const op of [
      { op: "add_node", id: "x", label: "X" } as const,
      {
        op: "add_edge",
        id: "x",
        label: "X",
        source_node_id: "a",
        target_node_id: "b",
        cardinality: "one_to_one",
      } as const,
      {
        op: "add_property",
        owner_id: "a",
        property: { id: "p", name: "x",
description: { default: "" }, property_type: { type: "string" } },
      } as const,
    ]) {
      const badge = commandOpBadge(op as OntologyCommand);
      expect(badge).toEqual({ label: "ADD", color: "green" });
    }
  });

  it("groups deletions / removals as red DEL", () => {
    for (const op of [
      { op: "delete_node", node_id: "n" } as const,
      { op: "delete_edge", edge_id: "e" } as const,
      { op: "delete_property", owner_id: "o", property_id: "p" } as const,
      { op: "remove_constraint", node_id: "n", constraint_id: "c" } as const,
      { op: "remove_index", index_id: "i" } as const,
    ]) {
      expect(commandOpBadge(op as OntologyCommand)).toEqual({
        label: "DEL",
        color: "red",
      });
    }
  });

  it("groups updates as blue UPD", () => {
    const cmd: OntologyCommand = {
      op: "rename_node",
      node_id: "n",
      new_label: "new",
    };
    expect(commandOpBadge(cmd)).toEqual({ label: "UPD", color: "blue" });
  });
});
