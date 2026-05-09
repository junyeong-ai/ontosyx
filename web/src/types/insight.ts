import type { LocalizedText } from "./ontology";
import type { ClientPage } from "./pagination";
import type { components } from "./api.generated";

export type InsightQueryIR = components["schemas"]["QueryIR"];
export type InsightProvenance = components["schemas"]["QueryProvenance"];

export type InsightDef = Omit<
  components["schemas"]["InsightDef"],
  "concept_anchors" | "description" | "original_provenance" | "tags"
> & {
  concept_anchors: string[];
  description: LocalizedText;
  original_provenance?: InsightProvenance | null;
  tags: string[];
};

export type CreateInsightRequest = components["schemas"]["CreateInsightRequest"];
export type UpdateInsightRequest = components["schemas"]["UpdateInsightRequest"];

export type InsightListPage = ClientPage<
  Omit<components["schemas"]["CursorPage_InsightDef"], "items"> & {
  items: InsightDef[];
  }
>;
