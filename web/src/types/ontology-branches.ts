import type { components } from "./api.generated";
import type { OntologyDraft } from "./ontology-drafts";

export type OntologyVersionEntry = components["schemas"]["OntologyVersionEntry"];
export type OntologyVersionsResponse = components["schemas"]["OntologyVersionsResponse"];

export type DiffAddedNode = components["schemas"]["NodeTypeDef"];
export type DiffAddedEdge = components["schemas"]["EdgeTypeDef"];
export type DiffLocalizedText = components["schemas"]["LocalizedText"];
export type PropertyChange = components["schemas"]["PropertyChange"];
export type NodeChange = components["schemas"]["NodeChange"];
export type EdgeChange = components["schemas"]["EdgeChange"];
export type DiffModifiedNode = components["schemas"]["NodeDiff"];
export type DiffModifiedEdge = components["schemas"]["EdgeDiff"];
export type OntologyDiffSummary = components["schemas"]["OntologyDiff"];

export type RebaseAnalysis = components["schemas"]["RebaseAnalysis"];
export type RebasePreviewResponse = components["schemas"]["RebasePreviewResponse"];
export type RebaseConflict = components["schemas"]["RebaseConflict"];
export type ConflictAxis = components["schemas"]["ConflictAxis"];
export type PropertyConflictAtom = components["schemas"]["PropertyConflictAxis"];

export type RebaseDraftResponse = Omit<
  components["schemas"]["RebaseOntologyDraftResponse"],
  "project"
> & {
  project: OntologyDraft;
};
