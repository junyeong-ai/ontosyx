use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Domain models for app state persistence
// ---------------------------------------------------------------------------

/// A single query execution record: NL question → QueryIR → compiled → results.
///
/// Ontology reproducibility: when `ontology_id` is set, the ontology identity
/// is referenced directly and the caller resolves a concrete version through
/// `OntologyVersionStore`. Draft / unsaved executions store `ontology_snapshot`
/// inline so an ad-hoc query that never gets committed still round-trips.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QueryExecution {
    pub id: Uuid,
    pub user_id: String,
    pub question: String,
    /// Lineage identifier (OntologyIR.id) at execution time. Stable
    /// across revisions of the same ontology — used for grouping
    /// executions under one ontology even when its version changes.
    pub ontology_lineage_id: String,
    pub ontology_version: i32,
    /// FK to `ontologies.id` — the committed ontology identity this query
    /// ran against. When set, `ontology_snapshot` is NULL.
    pub ontology_id: Option<Uuid>,
    /// Full OntologyIR snapshot (NULL when ontology_id is set)
    pub ontology_snapshot: Option<serde_json::Value>,
    pub query_ir: serde_json::Value,
    /// Compiler target language (e.g., "cypher")
    pub compiled_target: String,
    pub compiled_query: String,
    pub results: serde_json::Value,
    pub widget: Option<serde_json::Value>,
    pub explanation: String,
    /// LLM model used
    pub model: String,
    pub execution_time_ms: i64,
    /// Resolved query bindings for graph highlighting (binding-aware provenance)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_bindings: Option<serde_json::Value>,
    /// User feedback on query accuracy: "positive" or "negative"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Lightweight projection for list endpoints (excludes large JSONB blobs).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QueryExecutionSummary {
    pub id: Uuid,
    pub question: String,
    pub ontology_lineage_id: String,
    pub ontology_version: i32,
    pub compiled_target: String,
    pub model: String,
    pub execution_time_ms: i64,
    pub row_count: i64,
    pub has_widget: bool,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Λ Phase — Level 1 identity + version snapshot models.
//
// These mirror the migration-0016 tables `ontologies` and
// `ontology_version_snapshots`. They pair with
// `ox_ontology::storage::{ExtractedEntity, EntityKind}` to form
// the full commit / load pipeline the new
// `OntologyVersionStore` trait exposes.
// ---------------------------------------------------------------------------

/// A logical ontology identity (pre-version). Authored once per
/// business ontology; carries the stable `lineage_id` that
/// downstream systems (quality rules, saved queries, mappings)
/// reference by string.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OntologyRow {
    pub id: Uuid,
    pub lineage_id: String,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One immutable version snapshot of an ontology. The version's
/// *content* lives in the Level 2 entity store; this row records
/// "version V of ontology O was committed by U at time T" plus
/// the bitemporal window metadata.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OntologyVersionSnapshot {
    pub id: Uuid,
    pub ontology_id: Uuid,
    pub version: String,
    pub valid_from: DateTime<Utc>,
    pub valid_to: Option<DateTime<Utc>>,
    pub sys_from: DateTime<Utc>,
    pub sys_to: Option<DateTime<Utc>>,
    pub parent_version_id: Option<Uuid>,
    pub committed_by: String,
    pub commit_message: String,
    pub created_at: DateTime<Utc>,
    pub workspace_id: Uuid,
}

/// A single entity in the Level 2 content-addressed store. The
/// tuple `(entity_kind, logical_id)` identifies the entity across
/// versions; `entity_hash` changes when the entity's content
/// changes and stays stable when the author edits an unrelated
/// entity.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OntologyEntityRow {
    pub entity_hash: String,
    pub entity_kind: String,
    pub content: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Version → entity pointer row. Joins a version snapshot to
/// every entity that belongs to it.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OntologyVersionEntityRow {
    pub version_id: Uuid,
    pub entity_kind: String,
    pub entity_logical_id: String,
    pub entity_hash: String,
    pub workspace_id: Uuid,
    pub created_at: DateTime<Utc>,
}

/// Incremental change of one entity between two version
/// snapshots. Produced by `OntologyVersionStore::diff_versions`;
/// used by the admin UI's version-diff panel and by
/// `TemporalRewriter` to walk rename chains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityChange {
    pub entity_kind: String,
    pub entity_logical_id: String,
    pub kind: EntityChangeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntityChangeKind {
    /// Entity existed in `from` but not in `to`.
    Removed { from_hash: String },
    /// Entity exists in `to` but not in `from`.
    Added { to_hash: String },
    /// Entity present in both but with different content.
    Modified { from_hash: String, to_hash: String },
}

/// Hydration join row — one output of the SELECT behind
/// `OntologyVersionStore::get_ontology_ir`. Packs the pointer row
/// with the resolved entity content in a single round trip.
/// Not surfaced on the public API — internal to the PostgreSQL
/// implementation.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OntologyEntityJoinRow {
    pub entity_kind: String,
    pub entity_logical_id: String,
    pub entity_hash: String,
    pub content: serde_json::Value,
}

/// Diff row — internal SELECT output for
/// `OntologyVersionStore::diff_versions`. Mapped into
/// [`EntityChange`] once PostgreSQL has joined the two sides.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DiffRow {
    pub entity_kind: String,
    pub entity_logical_id: String,
    pub from_hash: Option<String>,
    pub to_hash: Option<String>,
}

// ---------------------------------------------------------------------------
// Λ-11 — Progressive Disclosure navigation results.
//
// These rows surface from the `OntologyNavigationStore` queries
// and feed the admin UI's schema browser + the LLM prompt
// builder's subgraph-extraction path.
// ---------------------------------------------------------------------------

// Navigation result types (EntitySearchHit, EntityNeighbor, HierarchyRow)
// moved to `crate::navigation`. The trait methods there consume / return
// structured `Subgraph` + `EntitySearchHit` instead of the three former
// row-shape DTOs.

/// A pinned query result for quick access.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PinboardItem {
    pub id: Uuid,
    pub query_execution_id: Uuid,
    pub user_id: String,
    pub widget_spec: serde_json::Value,
    pub title: Option<String>,
    pub pinned_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Workspaces — multi-tenant isolation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub owner_id: Uuid,
    pub settings: serde_json::Value,
    pub created_at: DateTime<Utc>,
    /// Workspace's canonical UI / LLM locale (BCP 47). Always lowercase;
    /// enforced by `workspaces_primary_locale_check` at the DB layer.
    pub primary_locale: String,
    /// Ordered fallback chain (JSON array of BCP 47 tags, non-empty) used
    /// by `LocalizedText::resolve`. Validated by
    /// `workspaces_locale_fallback_check`.
    pub locale_fallback: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceMember {
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

/// Workspace with member count for list endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceSummary {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub owner_id: Uuid,
    pub role: String,
    pub member_count: i64,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Users — OIDC-authenticated identities
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
    pub provider: String,
    pub provider_sub: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Design projects — ontology design lifecycle persistence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DesignProject {
    pub id: Uuid,
    /// "analyzed", "designed", "completed"
    pub status: String,
    /// Monotonically increasing on every mutation; used for optimistic concurrency.
    pub revision: i32,
    pub user_id: String,
    pub title: Option<String>,
    /// SourceConfig JSON (source_type, schema_name — no secrets)
    pub source_config: serde_json::Value,
    /// Canonical source identity derived via
    /// `SourceId::from_source_config` at project creation. Stable
    /// across restarts; federation plan-cache keys, query
    /// provenance, and ambiguity lookups all refer to this id so
    /// the same request under the same source shape replays
    /// deterministically.
    pub source_id: String,
    /// Raw source data (text/csv/json input; null for postgresql)
    pub source_data: Option<String>,
    /// SourceSchema snapshot from analysis
    pub source_schema: Option<serde_json::Value>,
    /// SourceProfile snapshot from analysis
    pub source_profile: Option<serde_json::Value>,
    /// SourceAnalysisReport snapshot from analysis
    pub analysis_report: Option<serde_json::Value>,
    /// User decisions (DesignOptions)
    pub design_options: serde_json::Value,
    /// Generated OntologyIR. Canonical `object_mappings` (node→table,
    /// property→column bindings) live inside this value; there is
    /// no separate persisted blob.
    pub ontology: Option<serde_json::Value>,
    /// OntologyQualityReport
    pub quality_report: Option<serde_json::Value>,
    /// FK to `ontologies.id` — the logical ontology identity this project
    /// was completed into. `None` until the design is completed.
    pub ontology_id: Option<Uuid>,
    /// History of data sources added to this project
    pub source_history: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub analyzed_at: Option<DateTime<Utc>>,
}

/// Lightweight projection for list endpoints (excludes large JSONB blobs).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DesignProjectSummary {
    pub id: Uuid,
    pub status: String,
    pub revision: i32,
    pub user_id: String,
    pub title: Option<String>,
    pub source_config: serde_json::Value,
    pub ontology_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub analyzed_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Ontology snapshots — revision history for design projects
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OntologySnapshot {
    pub id: Uuid,
    pub project_id: Uuid,
    pub workspace_id: Uuid,
    pub revision: i32,
    /// OntologyIR JSON — includes canonical `object_mappings`.
    pub ontology: serde_json::Value,
    pub quality_report: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// Lightweight projection for listing snapshots (excludes large JSONB blobs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologySnapshotSummary {
    pub id: Uuid,
    pub revision: i32,
    pub created_at: DateTime<Utc>,
    pub node_count: i64,
    pub edge_count: i64,
}

// ---------------------------------------------------------------------------
// System configuration — runtime-tunable settings from DB
// ---------------------------------------------------------------------------

/// A single configuration row from the `system_config` table.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SystemConfigRow {
    pub category: String,
    pub key: String,
    pub value: String,
    pub data_type: String,
    pub description: String,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Workbench perspectives — per-user graph canvas state
// ---------------------------------------------------------------------------

/// A saved workbench perspective: node positions, viewport, filters, etc.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkbenchPerspective {
    pub id: Uuid,
    pub user_id: String,
    pub workspace_id: Uuid,
    pub lineage_id: String,
    pub topology_signature: String,
    pub project_id: Option<Uuid>,
    pub name: String,
    pub positions: serde_json::Value,
    pub viewport: serde_json::Value,
    pub filters: serde_json::Value,
    pub collapsed_groups: serde_json::Value,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Dashboard {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub layout: serde_json::Value,
    pub is_public: bool,
    pub share_token: Option<String>,
    pub shared_at: Option<DateTime<Utc>>,
    /// When the share token expires. `None` means never (legacy rows);
    /// new shares always set this via `update_dashboard_share_token`.
    pub share_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// AnalysisRecipe — reusable data analysis algorithm
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AnalysisRecipe {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: String,
    pub algorithm_type: String,
    pub code_template: String,
    pub parameters: serde_json::Value,
    pub required_columns: serde_json::Value,
    pub output_description: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub version: i32,
    /// "draft", "approved", "deprecated"
    pub status: String,
    /// Previous version's ID (for version chain)
    pub parent_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// AnalysisResult — cached/versioned recipe execution output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AnalysisResult {
    pub id: Uuid,
    pub recipe_id: Option<Uuid>,
    pub ontology_lineage_id: Option<String>,
    pub input_hash: String,
    pub output: serde_json::Value,
    pub duration_ms: i64,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ScheduledTask — cron-based recipe execution schedule
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ScheduledTask {
    pub id: Uuid,
    pub recipe_id: Uuid,
    pub ontology_lineage_id: Option<String>,
    pub cron_expression: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: DateTime<Utc>,
    pub last_status: Option<String>,
    pub webhook_url: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// DashboardWidget — a saved query/analysis bound to a dashboard
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DashboardWidget {
    pub id: Uuid,
    pub dashboard_id: Uuid,
    pub workspace_id: Uuid,
    pub title: String,
    pub widget_type: String,
    pub query: Option<String>,
    pub widget_spec: serde_json::Value,
    pub position: serde_json::Value,
    pub refresh_interval_secs: Option<i32>,
    pub last_result: Option<serde_json::Value>,
    pub last_refreshed: Option<DateTime<Utc>>,
    pub thresholds: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// SavedReport — parameterized query template for reusable analytics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SavedReport {
    pub id: Uuid,
    pub user_id: String,
    pub ontology_lineage_id: String,
    pub title: String,
    pub description: Option<String>,
    pub query_template: String,
    pub parameters: serde_json::Value,
    pub widget_type: Option<String>,
    pub is_public: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// SavedQueryPattern — canvas-editable PatternIR with layout + zoom preserved
// ---------------------------------------------------------------------------
//
// Unlike `SavedReport` (server-side Cypher templates), the payload here is
// the raw PatternIR that the visual query builder edits. The compiled
// QueryIR is reconstructable on demand from `pattern_ir.compile()` so
// we store only the editable form.

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SavedQueryPattern {
    pub id: Uuid,
    pub user_id: String,
    pub ontology_lineage_id: String,
    pub name: String,
    pub description: Option<String>,
    pub pattern_ir: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// PendingEmbedding — retry queue for failed embedding operations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PendingEmbedding {
    pub id: Uuid,
    pub content: String,
    pub metadata: serde_json::Value,
    pub retry_count: i32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Agent sessions — execution context for replay and audit
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentSession {
    pub id: Uuid,
    pub user_id: String,
    pub ontology_lineage_id: Option<String>,
    pub prompt_hash: String,
    pub tool_schema_hash: String,
    pub model_id: String,
    pub model_config: serde_json::Value,
    pub user_message: String,
    pub final_text: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentEvent {
    pub id: Uuid,
    pub session_id: Uuid,
    pub workspace_id: Uuid,
    pub sequence: i32,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Prompt templates — versioned prompt management
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PromptTemplateRow {
    pub id: Uuid,
    pub name: String,
    /// Semantic version. Stored as TEXT in Postgres (with a CHECK constraint
    /// in migration 0006) and decoded via `TryFrom<String>` so the row
    /// always carries a parsed `PromptVersion` rather than a free-form
    /// string. Prevents the `"v10" < "v9"` lexicographic-sort surprise.
    #[sqlx(try_from = "String")]
    pub version: ox_core::PromptVersion,
    pub content: String,
    pub variables: serde_json::Value,
    pub metadata: serde_json::Value,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub is_active: bool,
    /// Workspace this override belongs to. `null` on the wire = global
    /// template (visible to every workspace as the fallback). The field
    /// is always emitted — a missing field and `null` mean different
    /// things to generated clients, and the OpenAPI schema (see
    /// `ox_api::openapi::PromptTemplateRow`) declares this as
    /// `Option<Uuid>` present in every response.
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// Element verifications — per-element verification tracking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ElementVerification {
    pub id: Uuid,
    pub ontology_lineage_id: String,
    pub element_id: String,
    pub element_kind: String,
    pub verified_by: Uuid,
    /// Resolved user display name (from users JOIN). Not stored in DB.
    #[sqlx(default)]
    pub verified_by_name: Option<String>,
    pub review_notes: Option<String>,
    pub invalidated_at: Option<DateTime<Utc>>,
    pub invalidation_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Tool approvals — HITL tool review decisions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ToolApproval {
    pub id: Uuid,
    pub session_id: Uuid,
    pub workspace_id: Uuid,
    pub tool_call_id: String,
    pub approved: bool,
    pub reason: Option<String>,
    pub modified_input: Option<serde_json::Value>,
    pub user_id: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Audit Log — append-only event log for CRUD operations
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// API Key — DB-backed identity for programmatic access
// ---------------------------------------------------------------------------

/// A long-lived API key. The plaintext key is never stored — only the
/// SHA-256 hash. The `label` is surfaced in audit logs (e.g.
/// `Principal::id = "apikey:ci-deploy"`).
///
/// Workspace-scoped keys (`workspace_id = Some(...)`) obey RLS and can
/// only see data in their workspace. Global keys (`workspace_id = None`)
/// require the bearer to be a workspace admin to use them across
/// workspaces — usually reserved for CI/admin scripts.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub label: String,
    /// SHA-256 hash of the plaintext key. Excluded from serialization
    /// so this struct can be safely returned through HTTP/JSON without
    /// leaking the offline-attackable hash.
    #[serde(skip_serializing)]
    pub key_hash: Vec<u8>,
    pub created_by: String,
    pub workspace_id: Option<Uuid>,
    /// Role granted to any caller presenting this key. Enforced at the
    /// DB layer (CHECK constraint) to exactly one of `admin`, `designer`,
    /// or `viewer`. The auth middleware copies this into the synthetic
    /// JWT claim instead of the previous hard-coded `"admin"`.
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub workspace_id: Uuid,
    /// Workspace impacted by the action. Differs from `workspace_id`
    /// only for SYSTEM_BYPASS maintenance tasks that operate across
    /// workspaces. When `None`, the affected workspace equals
    /// `workspace_id` (the common case).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected_workspace_id: Option<Uuid>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Usage Records — cost metering for LLM, compute, storage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UsageRecord {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Option<Uuid>,
    pub resource_type: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub operation: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub duration_ms: i64,
    pub cost_usd: f64,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Aggregated usage summary for a time period.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UsageSummary {
    pub resource_type: String,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cost_usd: f64,
    pub request_count: i64,
}

// ---------------------------------------------------------------------------
// Approval Requests — configurable gates for schema deployment & migration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub requester_id: Uuid,
    pub action_type: String,
    pub resource_type: String,
    pub resource_id: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub reviewer_id: Option<Uuid>,
    pub review_notes: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// One entry in the comment thread attached to an approval request.
/// The `review_notes` field on the parent row still records the
/// reviewer's rationale at decision time, but the thread is the
/// canonical surface — `review_approval` mirrors that note here as
/// the decision-time entry.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApprovalComment {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub approval_id: Uuid,
    pub author_id: Uuid,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Data Quality — declarative quality rules with evaluation results
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QualityRule {
    pub id: Uuid,
    pub workspace_id: Uuid,
    /// Lineage the rule is scoped to. Two ontologies in the same
    /// workspace that share a label (e.g., both have `Person`) keep
    /// their quality rules distinct because the evaluator filters by
    /// this field before running each sweep.
    pub ontology_lineage_id: String,
    pub name: String,
    pub description: Option<String>,
    pub rule_type: String,
    pub target_label: String,
    pub target_property: Option<String>,
    pub threshold: f64,
    pub cypher_check: Option<String>,
    pub severity: String,
    pub is_active: bool,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QualityResult {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub rule_id: Uuid,
    pub passed: bool,
    pub actual_value: Option<f64>,
    pub details: serde_json::Value,
    pub evaluated_at: DateTime<Utc>,
}

/// Dashboard-oriented view: each rule + its latest evaluation result.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QualityDashboardEntry {
    pub rule_id: Uuid,
    pub name: String,
    pub rule_type: String,
    pub target_label: String,
    pub severity: String,
    pub threshold: f64,
    pub latest_passed: Option<bool>,
    pub latest_value: Option<f64>,
    pub latest_evaluated_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Data Lineage — provenance tracking for graph data
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LineageEntry {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Option<Uuid>,
    pub graph_label: String,
    pub graph_element_type: String,
    pub source_type: String,
    pub source_name: String,
    pub source_table: Option<String>,
    pub source_columns: Option<Vec<String>>,
    pub load_plan_hash: Option<String>,
    /// Column-level property mappings: source_column -> graph_property + transform.
    /// Stored as JSON array of `{source_column, graph_property, transform?}`.
    pub property_mappings: Option<serde_json::Value>,
    pub record_count: i64,
    pub loaded_by: Option<Uuid>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: String,
    pub error_message: Option<String>,
}

/// Summary of lineage per graph label.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LineageSummary {
    pub graph_label: String,
    pub graph_element_type: String,
    pub source_count: i64,
    pub total_records: i64,
    pub last_loaded_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// ACL Policies — fine-grained attribute-based access control
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AclPolicy {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub subject_type: String,
    pub subject_value: String,
    pub resource_type: String,
    pub resource_value: Option<String>,
    pub action: String,
    pub properties: Option<Vec<String>>,
    pub mask_pattern: Option<String>,
    pub priority: i32,
    pub is_active: bool,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Model Configs — runtime LLM model configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ModelConfig {
    pub id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub name: String,
    pub provider: String,
    pub model_id: String,
    pub max_tokens: i32,
    pub temperature: Option<f32>,
    pub timeout_secs: i32,
    pub cost_per_1m_input: Option<f64>,
    pub cost_per_1m_output: Option<f64>,
    pub daily_budget_usd: Option<f64>,
    pub priority: i32,
    pub enabled: bool,
    pub api_key_env: Option<String>,
    pub region: Option<String>,
    pub base_url: Option<String>,
    pub provider_meta: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewModelConfig {
    pub workspace_id: Option<Uuid>,
    pub name: String,
    pub provider: String,
    pub model_id: String,
    pub max_tokens: Option<i32>,
    pub temperature: Option<f32>,
    pub timeout_secs: Option<i32>,
    pub cost_per_1m_input: Option<f64>,
    pub cost_per_1m_output: Option<f64>,
    pub daily_budget_usd: Option<f64>,
    pub priority: Option<i32>,
    pub api_key_env: Option<String>,
    pub region: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigUpdate {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub max_tokens: Option<i32>,
    pub temperature: Option<f32>,
    pub timeout_secs: Option<i32>,
    pub cost_per_1m_input: Option<f64>,
    pub cost_per_1m_output: Option<f64>,
    pub daily_budget_usd: Option<f64>,
    pub priority: Option<i32>,
    pub enabled: Option<bool>,
    pub api_key_env: Option<String>,
    pub region: Option<String>,
    pub base_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Model Routing Rules — operation-based model selection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ModelRoutingRule {
    pub id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub operation: String,
    pub model_config_id: Uuid,
    pub priority: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRoutingRule {
    pub workspace_id: Option<Uuid>,
    pub operation: String,
    pub model_config_id: Uuid,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRuleUpdate {
    pub operation: Option<String>,
    pub model_config_id: Option<Uuid>,
    pub priority: Option<i32>,
    pub enabled: Option<bool>,
}

// ---------------------------------------------------------------------------
// Load Checkpoints — watermark-based incremental loading state
// ---------------------------------------------------------------------------

/// Tracks the last successfully loaded watermark value for incremental (delta) loads.
/// Each checkpoint is unique per (workspace, project, source_table, graph_label).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LoadCheckpoint {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Uuid,
    pub source_table: String,
    pub graph_label: String,
    pub watermark_column: String,
    pub watermark_value: String,
    pub record_count: i64,
    pub loaded_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// NotificationChannel — configured notification destination
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NotificationChannel {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub channel_type: String,
    pub config: serde_json::Value,
    pub events: Vec<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// NotificationLog — delivery log entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct NotificationLog {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub channel_id: Uuid,
    pub event_type: String,
    pub subject: String,
    pub body: String,
    pub status: String,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Knowledge Base — failure-driven learning entries
// ---------------------------------------------------------------------------

/// A knowledge entry: correction from query failure or admin-created hint.
///
/// Knowledge is workspace-scoped and ontology-version-aware:
/// - `ontology_name` spans versions (knowledge survives across
///   ontology commits that keep the name + workspace stable)
/// - `affected_labels` enables label-based GIN lookup and staleness detection
/// - `version_checked` tracks the last ontology version where validity was confirmed
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KnowledgeEntry {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub ontology_name: String,
    pub ontology_version_min: i32,
    pub ontology_version_max: Option<i32>,
    pub kind: String,
    pub status: String,
    pub confidence: f64,
    pub title: String,
    pub content: String,
    pub structured_data: serde_json::Value,
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    pub version_checked: i32,
    pub content_hash: String,
    pub source_execution_ids: Vec<Uuid>,
    pub source_session_id: Option<Uuid>,
    pub affected_labels: Vec<String>,
    pub affected_properties: Vec<String>,
    pub created_by: String,
    pub reviewed_by: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub review_notes: Option<String>,
    pub use_count: i64,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One registered federation (VOL) data-source adapter.
///
/// Rows in this table are the durable form of every `source_id` the
/// `ox-federation` planner might resolve at query time. The admin
/// CRUD routes write here; the AppState bootstrap (slice W3b) will
/// stream the rows back into `InMemoryAdapterResolver` at server
/// start so a restart does not drop registrations.
///
/// `config` is adapter-specific JSON — for CSV/JSON it carries a
/// `data` field holding the inline payload, for future Postgres /
/// Snowflake adapters it will carry a `connection_string` or a
/// secret-manager reference. Keeping it typeless here means new
/// adapter kinds land without another migration.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DataSource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub source_id: String,
    pub kind: String,
    pub config: serde_json::Value,
    /// Last cached `ox_source::AnalysisResult` for this source. `None`
    /// until the first analyze_* call lands. The shape is opaque at
    /// the store layer; ox-api round-trips it through serde.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_analysis_snapshot: Option<serde_json::Value>,
    /// Per-table fingerprint map produced by
    /// [`ox_core::source_schema::SchemaFingerprint`]. JSON shape:
    /// `{ "<table>": { "hash": "<hex>", "computed_at": "<iso>" } }`.
    /// Driven by the kernel's drift detection — UI re-scan compares
    /// the live fingerprint against this map and only re-introspects
    /// tables whose hash differs.
    #[serde(default = "default_empty_object")]
    pub schema_fingerprints: serde_json::Value,
    /// Wall-clock timestamp of the most-recent successful analyze_*
    /// call. `None` when nothing has been analysed yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_analyzed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_empty_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}
