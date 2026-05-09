// Φ11 — Verified-query bank wire types.
//
// Manually mirrored from the Rust BE (`ox_ontology::VerifiedQueryDef`,
// `ox-api::routes::verified_queries`). Kept inline rather than
// re-exporting `components["schemas"]` so this surface stays stable
// across api.generated.ts regenerations driven by sibling features.
//
// Drift contract: any BE schema change here also lands a matching
// TS edit. The OpenAPI drift CI gate
// (`scripts/check-openapi-drift.sh`) catches the BE-side schema
// shift; this file is the manual counterpart for one feature.

/** snake_case wire string mirroring `ox_ontology::ComplexityClass`. */
export type ComplexityClass = "trivial" | "simple" | "composite" | "complex";

/** snake_case wire string mirroring `ox_ontology::VerifiedQueryStatus`. */
export type VerifiedQueryStatus =
  | "verified"
  | "under_review"
  | "deprecated"
  | "stale";

/** Stable identifier — opaque string, server-generated when omitted. */
export type VerifiedQueryId = string;

/** Author envelope mirroring `ox_ontology::AgentRef` discriminated union. */
export type VerifiedQueryAuthor =
  | { kind: "user"; user_id: string }
  | { kind: "service"; service_id: string };

/**
 * One verified-query row — server-authoritative shape.
 *
 * `embedding` is **never** round-tripped on list / detail responses
 * (the BE elides it from the SELECT column list — 1024 f32 × N rows
 * would be kbytes of payload the FE doesn't need). Carried in the
 * type for symmetry with the BE struct only; FE consumers should
 * treat it as always-`undefined`.
 */
export interface VerifiedQuery {
  id: VerifiedQueryId;
  workspace_id: string;
  question: string;
  question_hash: string;
  query_ir: Record<string, unknown>;
  complexity_class: ComplexityClass;
  status: VerifiedQueryStatus;
  author: VerifiedQueryAuthor;
  description?: string;
  verified_at: string;
  updated_at: string;
}

/**
 * `POST /api/verified-queries` request body. `id` and
 * `question_hash` are server-generated when omitted; explicit
 * values must match the canonicalisation rule the BE recomputes.
 */
export interface PromoteVerifiedQueryRequest {
  id?: string;
  question: string;
  query_ir: Record<string, unknown>;
  complexity_class: ComplexityClass;
  status?: VerifiedQueryStatus;
  description?: string;
}

/**
 * `POST /api/verified-queries/{id}/transition-status` request
 * body. The BE rejects illegal transitions (e.g. `Stale →
 * UnderReview` skips the explicit re-promotion).
 */
export interface TransitionVerifiedQueryStatusRequest {
  status: VerifiedQueryStatus;
}

/**
 * `GET /api/verified-queries` response — flat array under `rows`.
 * No cursor pagination yet (the bank is small enough that the
 * server's `limit` cap suffices); a future paged shape will
 * extend with `next_cursor` without breaking existing consumers.
 */
export interface VerifiedQueryListResponse {
  rows: VerifiedQuery[];
}
