import { describe, expect, it } from "vitest";
import {
  normalizeOntologyCommand,
  type WireOntologyCommand,
} from "./ontology-command-normalize";

describe("normalizeOntologyCommand", () => {
  it("normalizes full node payloads for the FE optimistic mirror", () => {
    const command: WireOntologyCommand = {
      op: "create_node_type",
      node: {
        id: "node-customer",
        label: "Customer",
        source_lineage: {
          table: "customers",
          source_id: null,
        },
        properties: [
          {
            id: "prop-name",
            name: "name",
            property_type: { type: "string" },
            default_value: {
              type: "map",
              value: {
                tier: { type: "string", value: "gold" },
                scores: {
                  type: "list",
                  value: [{ type: "int", value: 10 }],
                },
              },
            },
            classification: null,
            source_column: null,
            bindings: [
              {
                kind: "value_set",
                id: "customer_status",
                concept_map_id: null,
                valid_from: null,
                valid_to: null,
              },
            ],
          },
        ],
      },
    };

    const normalized = normalizeOntologyCommand(command);

    expect(normalized).toEqual({
      op: "create_node_type",
      node: {
        id: "node-customer",
        label: "Customer",
        description: { default: "" },
        source_lineage: {
          table: "customers",
          source_id: undefined,
        },
        properties: [
          {
            id: "prop-name",
            name: "name",
            property_type: { type: "string" },
            default_value: {
              type: "map",
              value: {
                tier: { type: "string", value: "gold" },
                scores: {
                  type: "list",
                  value: [{ type: "int", value: 10 }],
                },
              },
            },
            description: { default: "" },
            source_column: undefined,
            classification: undefined,
            bindings: [
              {
                kind: "value_set",
                id: "customer_status",
                concept_map_id: undefined,
                valid_from: undefined,
                valid_to: undefined,
              },
            ],
          },
        ],
        concept_id: undefined,
      },
    });
  });

  it("normalizes nested batch commands recursively", () => {
    const command: WireOntologyCommand = {
      op: "batch",
      description: "apply suggestions",
      commands: [
        {
          op: "add_property",
          owner: { kind: "node", type_id: "node-customer" },
          property: {
            id: "prop-age",
            name: "age",
            property_type: { type: "int" },
          },
        },
        {
          op: "update_property",
          owner: { kind: "node", type_id: "node-customer" },
          property_id: "prop-age",
          patch: { property_type: { type: "float" } },
        },
      ],
    };

    const normalized = normalizeOntologyCommand(command);

    expect(normalized).toMatchObject({
      op: "batch",
      commands: [
        {
          op: "add_property",
          property: {
            property_type: { type: "int" },
            description: { default: "" },
          },
        },
        {
          op: "update_property",
          patch: { property_type: { type: "float" } },
        },
      ],
    });
  });
});
