import { useMemo } from "react";
import type { OntologyIR, NodeTypeDef, EdgeTypeDef } from "@/types/api";
import type { PatternNode } from "./ir-builder";

export interface Suggestion {
  edge: EdgeTypeDef;
  direction: "outgoing" | "incoming";
  targetNode: NodeTypeDef;
  alreadyInPattern: boolean;
}

export function useSuggestions(
  selectedNodeLabel: string | null,
  patternNodes: PatternNode[],
  ontology: OntologyIR | null,
): Suggestion[] {
  return useMemo(() => {
    if (!selectedNodeLabel || !ontology) return [];

    const selectedNodeType = ontology.node_types.find(
      (nt) => nt.label === selectedNodeLabel,
    );
    if (!selectedNodeType) return [];

    const addedLabels = new Set(patternNodes.map((n) => n.label));
    const suggestions: Suggestion[] = [];

    for (const edge of ontology.edge_types) {
      if (edge.source_node_id === selectedNodeType.id) {
        const target = ontology.node_types.find(
          (nt) => nt.id === edge.target_node_id,
        );
        if (target) {
          suggestions.push({
            edge,
            direction: "outgoing",
            targetNode: target,
            alreadyInPattern: addedLabels.has(target.label),
          });
        }
      }
      if (edge.target_node_id === selectedNodeType.id) {
        const source = ontology.node_types.find(
          (nt) => nt.id === edge.source_node_id,
        );
        if (source) {
          suggestions.push({
            edge,
            direction: "incoming",
            targetNode: source,
            alreadyInPattern: addedLabels.has(source.label),
          });
        }
      }
    }

    // Not-in-pattern first, then alphabetical by edge label
    return suggestions.sort((a, b) => {
      if (a.alreadyInPattern !== b.alreadyInPattern)
        return a.alreadyInPattern ? 1 : -1;
      return a.edge.label.localeCompare(b.edge.label);
    });
  }, [selectedNodeLabel, patternNodes, ontology]);
}
