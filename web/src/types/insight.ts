// Wire shape for `ox_query_ir::insight::InsightDef`. Server owns
// `id`, `created_at`, `updated_at` — clients read them, never set.

import type { LocalizedText } from "./ontology";

export interface InsightDef {
  id: string;
  question: LocalizedText;
  description: LocalizedText;
  tags: string[];
  /** `GlossaryTermId` strings — typed concept anchors per the
   *  1-pager's "용어 사전이 다리" axis. Cross-team filtering by
   *  concept stays consistent even as `tags` shorthand drifts. */
  concept_anchors: string[];
  /** Logical query — wire shape is canonical `QueryIR` JSON. */
  query_ir: unknown;
  /** Provenance the insight was originally computed against —
   *  ontology + registry version + column-lineage trail. */
  original_provenance?: unknown;
  author_id: string;
  expires_at?: string | null;
  created_at: string;
  updated_at: string;
}

/** POST /api/insights — server stamps id + timestamps.
 *  `description` is required (mirrors the canonical `InsightDef`
 *  shape). Send `{ default: "" }` when the user leaves it blank. */
export interface CreateInsightRequest {
  question: LocalizedText;
  description: LocalizedText;
  tags?: string[];
  concept_anchors?: string[];
  query_ir: unknown;
  original_provenance?: unknown;
  expires_at?: string | null;
}

/** PUT /api/insights/{id} — `expected_updated_at` is the
 *  optimistic-CAS handle. Stale writes return 409. */
export interface UpdateInsightRequest {
  question: LocalizedText;
  description: LocalizedText;
  tags?: string[];
  concept_anchors?: string[];
  query_ir: unknown;
  original_provenance?: unknown;
  expires_at?: string | null;
  expected_updated_at: string;
}

export interface InsightListPage {
  items: InsightDef[];
  next_cursor?: string | null;
}
