use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
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
    #[schema(value_type = Option<ox_ontology::ir::OntologyIR>)]
    pub ontology_snapshot: Option<serde_json::Value>,
    #[schema(value_type = ox_query_ir::query::QueryIR)]
    pub query_ir: serde_json::Value,
    /// Compiler target language (e.g., "cypher")
    pub compiled_target: String,
    pub compiled_query: String,
    #[schema(value_type = ox_query_ir::query::QueryResult)]
    pub results: serde_json::Value,
    #[schema(value_type = Option<ox_query_ir::widget::WidgetHint>)]
    pub widget: Option<serde_json::Value>,
    pub explanation: String,
    /// LLM provider used.
    pub model_provider: String,
    /// LLM model used.
    pub model: String,
    pub execution_time_ms: i64,
    /// Resolved query bindings for graph highlighting (binding-aware provenance)
    #[schema(value_type = ox_query_ir::bindings::ResolvedQueryBindings)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_bindings: Option<serde_json::Value>,
    /// User feedback on query accuracy: "positive" or "negative"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Lightweight projection for list endpoints (excludes large JSONB blobs).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct QueryExecutionSummary {
    pub id: Uuid,
    pub question: String,
    pub ontology_lineage_id: String,
    pub ontology_version: i32,
    pub compiled_target: String,
    pub model_provider: String,
    pub model: String,
    pub execution_time_ms: i64,
    pub row_count: i64,
    pub has_widget: bool,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Λ Phase — Level 1 identity + version snapshot models.
//
// These mirror the baseline tables `ontologies` and
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
    /// Locale-aware human label. `LocalizedText` shape:
    /// `{ default, translations? }`. Empty when the ontology was
    /// created without a display label — consumers fall back to
    /// `name`.
    pub display_name: serde_json::Value,
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
    /// PROV-O activity record produced by `commit_version` —
    /// resolves to a `provenance_records` row carrying agent,
    /// derivation chain, and (for LLM-driven commits) the
    /// `prompt_render_hash` plan reference. Always present —
    /// the schema FK is `NOT NULL`.
    pub provenance_id: Uuid,
    /// sha256 over the tokenizer-relevant glossary state at
    /// commit time. Diffed against the previous snapshot's
    /// fingerprint to gate the lindera user-dictionary rebuild
    /// + retokenized backfill on retrieval surfaces.
    pub glossary_tokenizer_fingerprint: String,
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
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct PinboardItem {
    pub id: Uuid,
    pub query_execution_id: Uuid,
    pub user_id: String,
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub widget_spec: serde_json::Value,
    pub title: Option<String>,
    pub pinned_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Workspaces — multi-tenant isolation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub owner_id: Uuid,
    pub settings: serde_json::Value,
    pub created_at: DateTime<Utc>,
    /// Workspace's canonical authoring locale (BCP 47). Always lowercase;
    /// enforced by `workspaces_primary_locale_check` at the DB layer.
    pub primary_locale: String,
    /// Ordered fallback chain (BCP 47 tags, non-empty) the admin /
    /// operator UI walks when resolving translations. Validated by
    /// `workspaces_admin_locale_fallback_check`.
    pub admin_locale_fallback: Vec<String>,
    /// Ordered fallback chain (JSON array of BCP 47 tags, non-empty) the
    /// agent / Brain prompts and tool-result contexts walk. Distinct from
    /// `admin_locale_fallback` so a Korean-first admin surface can pair
    /// with an English-first LLM context. Validated by
    /// `workspaces_llm_locale_fallback_check`.
    pub llm_locale_fallback: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceMember {
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub joined_at: DateTime<Utc>,
    /// Joined from `users.email`. Always present for members because
    /// the column is `NOT NULL` on the users table; modelled as
    /// `String` rather than `Option<String>` so the FE doesn't have
    /// to fall back on `user_id.slice()` for a label.
    pub email: String,
    /// Joined from `users.name` — display name on the OAuth provider.
    /// `None` when the provider didn't surface one (some legacy /
    /// API-key users).
    pub name: Option<String>,
    /// Joined from `users.picture` — profile avatar URL. `None` when
    /// the provider didn't surface one.
    pub picture: Option<String>,
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
    /// Bulk JWT invalidation counter. Issued tokens carry this value
    /// as the `tv` claim; an increment retires every prior token in
    /// one round-trip. Pairs with the per-token `revoked_jwts` list
    /// for fine-grained revocation.
    pub token_version: i64,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

/// One revoked JWT entry. The pair `(jti, expires_at)` is enough to
/// keep the table bounded — once `now() > expires_at` the underlying
/// token is unusable regardless of revocation state and the row can
/// be dropped by the cleanup cron.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RevokedJwt {
    pub jti: Uuid,
    pub revoked_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_by_user_id: Option<Uuid>,
    pub reason: Option<String>,
}

/// One Idempotency-Key middleware record. The middleware reads
/// `request_hash` to confirm a replay is genuinely the same request
/// (not the same key reused for a different payload — Stripe's
/// behaviour) and replays `response_status` / `response_body` /
/// `response_content_type` byte-for-byte to the client.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IdempotencyRecord {
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub method: String,
    pub path: String,
    pub key: String,
    pub request_hash: Vec<u8>,
    pub response_status: i16,
    pub response_body: Vec<u8>,
    pub response_content_type: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Design projects — ontology design lifecycle persistence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct OntologyDraft {
    pub id: Uuid,
    /// "analyzed", "designed", "completed"
    pub status: String,
    /// Monotonically increasing on every mutation; used for optimistic concurrency.
    pub revision: i32,
    pub user_id: String,
    pub title: Option<String>,
    /// SourceConfig JSON (source_type, schema_name — no secrets)
    #[schema(value_type = ox_ontology::ontology_draft::SourceConfig)]
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
    #[schema(value_type = Option<ox_core::source_schema::SourceSchema>)]
    pub source_schema: Option<serde_json::Value>,
    /// SourceProfile snapshot from analysis
    #[schema(value_type = Option<ox_core::source_schema::SourceProfile>)]
    pub source_profile: Option<serde_json::Value>,
    /// SourceAnalysisReport snapshot from analysis
    #[schema(value_type = Option<ox_ontology::source_analysis::SourceAnalysisReport>)]
    pub analysis_report: Option<serde_json::Value>,
    /// User decisions (DesignOptions)
    #[schema(value_type = ox_ontology::source_analysis::DesignOptions)]
    pub design_options: serde_json::Value,
    /// Draft-lifecycle [`ox_core::source_scope::AnalysisScope`] — included
    /// tables, deferred tables (with reason + revisit), policy-
    /// excluded tables, per-table fingerprints, and the timestamp of
    /// the last introspection. Accumulates across every analyze /
    /// extend / reanalyze pass so the operator's "which tables are
    /// modeled / deferred / drifted" view survives the full project
    /// lifecycle.
    #[schema(value_type = ox_core::source_scope::AnalysisScope)]
    pub analysis_scope: serde_json::Value,
    /// Generated OntologyIR. Canonical `object_mappings` (node→table,
    /// property→column bindings) live inside this value; there is
    /// no separate persisted blob.
    #[schema(value_type = Option<ox_ontology::ir::OntologyIR>)]
    pub ontology: Option<serde_json::Value>,
    /// OntologyQualityReport
    #[schema(value_type = Option<ox_ontology::quality::OntologyQualityReport>)]
    pub quality_report: Option<serde_json::Value>,
    /// FK to `ontology_version_snapshots.id` — the canonical version
    /// the project's in-flight `ontology` JSONB was branched from.
    /// `complete_ontology_draft` compares this against the canonical
    /// head and refuses the commit if they diverge, forcing the
    /// operator to rebase before retry. `None` for greenfield
    /// projects whose first commit creates the canonical's first
    /// version. Workspace × ontology is 1:1, so the canonical's
    /// identity is always the workspace's; the parent here pins the
    /// *version* axis.
    pub parent_version_id: Option<Uuid>,
    /// FK to `ontology_version_snapshots.id` — the snapshot this
    /// draft was committed into via `complete_ontology_draft`.
    /// `None` while the draft is in `analyzed` / `designed` status;
    /// set on transition to `completed`. Operators follow the link
    /// from a completed draft straight to "the version this
    /// produced" without resolving via lineage.
    pub committed_version_id: Option<Uuid>,
    /// History of data sources added to this draft
    #[schema(value_type = Vec<ox_ontology::ontology_draft::SourceHistoryEntry>)]
    pub source_history: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub analyzed_at: Option<DateTime<Utc>>,
}

/// Lightweight projection for list endpoints (excludes large JSONB blobs).
///
/// Both `parent_version_id` (fork point) and `committed_version_id`
/// (commit target) ride on the projection because the branching
/// dashboard renders both endpoints inline without per-row hydration.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct OntologyDraftSummary {
    pub id: Uuid,
    pub status: String,
    pub revision: i32,
    pub user_id: String,
    pub title: Option<String>,
    #[schema(value_type = ox_ontology::ontology_draft::SourceConfig)]
    pub source_config: serde_json::Value,
    /// Canonical version this draft was branched from. `None` for
    /// greenfield drafts whose first commit creates the workspace's
    /// first canonical version.
    pub parent_version_id: Option<Uuid>,
    /// Canonical version this draft committed into. `None` until
    /// the draft is completed.
    pub committed_version_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub analyzed_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Ontology snapshots — revision history for design projects
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct OntologySnapshot {
    pub id: Uuid,
    pub ontology_draft_id: Uuid,
    pub workspace_id: Uuid,
    pub revision: i32,
    /// OntologyIR JSON — includes canonical `object_mappings`.
    #[schema(value_type = ox_ontology::ir::OntologyIR)]
    pub ontology: serde_json::Value,
    #[schema(value_type = Option<ox_ontology::quality::OntologyQualityReport>)]
    pub quality_report: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// Lightweight projection for listing snapshots (excludes large JSONB blobs).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WorkbenchPerspective {
    pub id: Uuid,
    pub user_id: String,
    pub workspace_id: Uuid,
    pub lineage_id: String,
    pub topology_signature: String,
    pub ontology_draft_id: Option<Uuid>,
    pub name: String,
    pub positions: std::collections::BTreeMap<String, CanvasPosition>,
    pub viewport: CanvasViewport,
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub filters: serde_json::Value,
    pub collapsed_groups: Vec<String>,
    pub is_default: bool,
    /// Retrieval profile (Φ10) the perspective pins for GraphRAG.
    /// Different perspectives can pin different shapes — a
    /// Customer-centric perspective wants different edge weights
    /// than a Product-centric one. `None` falls back to the
    /// workspace's `default` profile (Φ10.5 auto-seed). FK is
    /// `ON DELETE SET NULL` so a profile delete cleans dangling
    /// references without dropping the perspective.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval_profile_id: Option<ox_ontology::RetrievalProfileId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CanvasPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CanvasViewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Dashboard {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub layout: Vec<DashboardLayoutItem>,
    pub is_public: bool,
    pub share_token: Option<String>,
    pub shared_at: Option<DateTime<Utc>>,
    /// When the share token expires. `None` means never (legacy rows);
    /// new shares always set this via `update_dashboard_share_token`.
    pub share_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DashboardLayoutItem {
    pub widget_id: Uuid,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

// ---------------------------------------------------------------------------
// AnalysisRecipe — reusable data analysis algorithm
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AnalysisRecipe {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: String,
    pub algorithm_type: String,
    pub code_template: String,
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub parameters: serde_json::Value,
    #[schema(value_type = Vec<String>)]
    pub required_columns: serde_json::Value,
    pub output_description: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub version: i32,
    pub status: RecipeStatus,
    /// Previous version's ID (for version chain)
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecipeStatus {
    Draft,
    Approved,
    Deprecated,
}

impl RecipeStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Approved => "approved",
            Self::Deprecated => "deprecated",
        }
    }
}

impl std::str::FromStr for RecipeStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "approved" => Ok(Self::Approved),
            "deprecated" => Ok(Self::Deprecated),
            other => Err(format!("unknown recipe status: {other}")),
        }
    }
}

impl std::fmt::Display for RecipeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// RecipeExecutionResult — cached/versioned recipe execution output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct RecipeExecutionResult {
    pub id: Uuid,
    pub recipe_id: Option<Uuid>,
    pub ontology_lineage_id: Option<String>,
    pub input_hash: String,
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub output: serde_json::Value,
    pub duration_ms: i64,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ScheduledTask — cron-based recipe execution schedule
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DashboardWidget {
    pub id: Uuid,
    pub dashboard_id: Uuid,
    pub workspace_id: Uuid,
    pub title: String,
    pub widget_type: String,
    pub query: Option<String>,
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub widget_spec: serde_json::Value,
    pub position: DashboardWidgetPosition,
    pub refresh_interval_secs: Option<i32>,
    #[schema(value_type = Option<std::collections::HashMap<String, Object>>, additional_properties)]
    pub last_result: Option<serde_json::Value>,
    pub last_refreshed: Option<DateTime<Utc>>,
    pub thresholds: Option<DashboardWidgetThresholds>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DashboardWidgetPosition {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DashboardWidgetThresholds {
    pub warning: Option<f64>,
    pub critical: Option<f64>,
    pub direction: Option<ThresholdDirection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdDirection {
    Above,
    Below,
}

// ---------------------------------------------------------------------------
// SavedReport — parameterized query template for reusable analytics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SavedReportParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: SavedReportParameterType,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SavedReportParameterType {
    String,
    Number,
    Boolean,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct SavedReport {
    pub id: Uuid,
    pub user_id: String,
    pub ontology_lineage_id: String,
    pub title: String,
    pub description: Option<String>,
    pub query_template: String,
    #[schema(value_type = Vec<SavedReportParameter>)]
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

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentSession {
    pub id: Uuid,
    pub user_id: String,
    pub ontology_lineage_id: Option<String>,
    pub prompt_hash: String,
    pub tool_schema_hash: String,
    pub model_id: String,
    pub model_config: AgentSessionModelConfig,
    pub user_message: String,
    pub final_text: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentSessionModelConfig {
    pub execution_mode: AgentExecutionMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentExecutionMode {
    Auto,
    Plan,
    Supervised,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentEvent {
    pub id: Uuid,
    pub session_id: Uuid,
    pub workspace_id: Uuid,
    pub sequence: i32,
    pub event_type: String,
    pub payload: AgentEventPayload,
    pub created_at: DateTime<Utc>,
}

/// Canonical persisted agent timeline payload. The stream adapter,
/// audit trail, and session reconstruction all use this shape so
/// replay does not depend on branchforge's private serde layout.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEventPayload {
    Text {
        delta: String,
    },
    Thinking {
        content: String,
    },
    ToolStart {
        id: String,
        name: String,
        #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
        input: serde_json::Value,
    },
    ToolComplete {
        id: String,
        name: String,
        #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
        output: serde_json::Value,
        is_error: bool,
        duration_ms: Option<i64>,
    },
    ToolProgress {
        tool_call_id: String,
        step: String,
        status: String,
        duration_ms: Option<i64>,
        #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
        metadata: serde_json::Value,
    },
    ToolBlocked {
        id: String,
        name: String,
        reason: String,
    },
    ToolReview {
        id: String,
        name: String,
        #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
        input: serde_json::Value,
    },
    TurnUsage {
        input_tokens: i64,
        output_tokens: i64,
    },
    Complete {
        session_id: String,
        text: String,
        #[schema(value_type = Vec<std::collections::HashMap<String, Object>>, additional_properties)]
        tool_calls: serde_json::Value,
        iterations: u32,
    },
}

impl AgentEventPayload {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Text { .. } => "text",
            Self::Thinking { .. } => "thinking",
            Self::ToolStart { .. } => "tool_start",
            Self::ToolComplete { .. } => "tool_complete",
            Self::ToolProgress { .. } => "tool_progress",
            Self::ToolBlocked { .. } => "tool_blocked",
            Self::ToolReview { .. } => "tool_review",
            Self::TurnUsage { .. } => "turn_usage",
            Self::Complete { .. } => "complete",
        }
    }
}

// ---------------------------------------------------------------------------
// Prompt templates — versioned prompt management
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PromptTemplateRow {
    pub id: Uuid,
    pub name: String,
    /// Semantic version. Stored as TEXT in Postgres (with a CHECK constraint
    /// in the schema baseline) and decoded via `TryFrom<String>` so the row
    /// always carries a parsed `PromptVersion` rather than a free-form
    /// string. Prevents the `"v10" < "v9"` lexicographic-sort surprise.
    #[sqlx(try_from = "String")]
    pub version: ox_core::PromptVersion,
    pub content: String,
    pub variables: Vec<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
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
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Usage Records — cost metering for LLM, compute, storage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
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
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Aggregated usage summary for a time period.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub requester_id: Uuid,
    pub requester_name: Option<String>,
    pub action_type: String,
    pub resource_type: String,
    pub resource_id: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub reviewer_id: Option<Uuid>,
    pub reviewer_name: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// One entry in the comment thread attached to an approval request.
/// The reviewer's decision-time rationale is the first comment; any
/// pre-/post-decision discussion follows in the same thread.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ApprovalComment {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub approval_id: Uuid,
    pub author_id: Uuid,
    pub author_name: Option<String>,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Data Quality — declarative quality rules with evaluation results
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct QualityResult {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub rule_id: Uuid,
    pub passed: bool,
    pub actual_value: Option<f64>,
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub details: serde_json::Value,
    pub evaluated_at: DateTime<Utc>,
}

/// Dashboard-oriented view: each rule + its latest evaluation result.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct LineageEntry {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub ontology_draft_id: Option<Uuid>,
    pub graph_label: String,
    pub graph_element_type: String,
    pub source_type: String,
    pub source_name: String,
    pub source_table: Option<String>,
    pub source_columns: Option<Vec<String>>,
    pub load_plan_hash: Option<String>,
    /// Column-level property mappings: source_column -> graph_property + transform.
    /// Stored as JSON array of `{source_column, graph_property, transform?}`.
    #[schema(value_type = Option<Vec<std::collections::HashMap<String, Object>>>, additional_properties)]
    pub property_mappings: Option<serde_json::Value>,
    pub record_count: i64,
    pub loaded_by: Option<Uuid>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: String,
    pub error_message: Option<String>,
}

/// Summary of lineage per graph label.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
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
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub provider_meta: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
    pub enabled: Option<bool>,
    pub api_key_env: Option<String>,
    pub region: Option<String>,
    pub base_url: Option<String>,
    #[serde(default = "default_empty_object")]
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub provider_meta: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub provider_meta: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Model Routing Rules — operation-based model selection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ModelRoutingRule {
    pub id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub operation: String,
    pub model_config_id: Uuid,
    pub priority: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NewRoutingRule {
    pub workspace_id: Option<Uuid>,
    pub operation: String,
    pub model_config_id: Uuid,
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RoutingRuleUpdate {
    pub operation: Option<String>,
    pub model_config_id: Option<Uuid>,
    pub priority: Option<i32>,
    pub enabled: Option<bool>,
}

// ---------------------------------------------------------------------------
// Load Checkpoints — watermark-based incremental loading state
// ---------------------------------------------------------------------------

/// Tracks the last successfully loaded watermark value for
/// incremental (delta) loads. Unique per
/// `(workspace, project, source_table, graph_label)`.
///
/// `id` and `workspace_id` reflect persistence state: `None` on a
/// freshly-authored checkpoint (the store mints `id` via the
/// column DEFAULT and stamps `workspace_id` from the active
/// task-local on insert), `Some(_)` on a checkpoint read back from
/// the store. Use [`Self::draft`] to author fresh entries.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct LoadCheckpoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    pub ontology_draft_id: Uuid,
    pub source_table: String,
    pub graph_label: String,
    pub watermark_column: String,
    pub watermark_value: String,
    pub record_count: i64,
    pub loaded_at: DateTime<Utc>,
}

impl LoadCheckpoint {
    /// Author a fresh checkpoint with the persistence-side fields
    /// (`id`, `workspace_id`) left for the store to populate.
    pub fn draft(
        ontology_draft_id: Uuid,
        source_table: String,
        graph_label: String,
        watermark_column: String,
        watermark_value: String,
        record_count: i64,
    ) -> Self {
        Self {
            id: None,
            workspace_id: None,
            ontology_draft_id,
            source_table,
            graph_label,
            watermark_column,
            watermark_value,
            record_count,
            loaded_at: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// NotificationChannel — configured notification destination
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NotificationChannel {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub channel_type: NotificationChannelType,
    pub config: WebhookNotificationConfig,
    pub events: Vec<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannelType {
    SlackWebhook,
    GenericWebhook,
}

impl NotificationChannelType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SlackWebhook => "slack_webhook",
            Self::GenericWebhook => "generic_webhook",
        }
    }
}

impl std::str::FromStr for NotificationChannelType {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "slack_webhook" => Ok(Self::SlackWebhook),
            "generic_webhook" => Ok(Self::GenericWebhook),
            other => Err(format!("unknown notification channel type: {other}")),
        }
    }
}

impl std::fmt::Display for NotificationChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WebhookNotificationConfig {
    pub url: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub headers: std::collections::BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------
// NotificationLog — delivery log entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct KnowledgeEntry {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub ontology_name: String,
    pub ontology_version_min: i32,
    pub ontology_version_max: Option<i32>,
    pub kind: KnowledgeKind,
    pub status: KnowledgeStatus,
    pub confidence: f64,
    pub title: String,
    pub content: String,
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub structured_data: serde_json::Value,
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
    /// Morphologically-tokenized projection of `title + content`,
    /// stamped at write time using the workspace's lindera+user-dict
    /// tokenizer. Drives the `searchable_tsv` GENERATED column +
    /// hybrid retrieval RRF lexical rank.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tokenized_text: String,
    /// sha256 of the workspace tokenizer dict at write time.
    /// Backfill cron retokenizes any row whose fingerprint is
    /// stale relative to the current canonical commit.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tokenizer_dict_fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeKind {
    Correction,
    Hint,
}

impl KnowledgeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Correction => "correction",
            Self::Hint => "hint",
        }
    }
}

impl std::str::FromStr for KnowledgeKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "correction" => Ok(Self::Correction),
            "hint" => Ok(Self::Hint),
            other => Err(format!("unknown knowledge kind: {other}")),
        }
    }
}

impl std::fmt::Display for KnowledgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeStatus {
    Draft,
    Approved,
    Stale,
    Deprecated,
}

impl KnowledgeStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Approved => "approved",
            Self::Stale => "stale",
            Self::Deprecated => "deprecated",
        }
    }
}

impl std::str::FromStr for KnowledgeStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "approved" => Ok(Self::Approved),
            "stale" => Ok(Self::Stale),
            "deprecated" => Ok(Self::Deprecated),
            other => Err(format!("unknown knowledge status: {other}")),
        }
    }
}

impl std::fmt::Display for KnowledgeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One registered federation (VOL) data-source adapter.
///
/// Rows in this table are the durable form of every `source_id` the
/// `ox-federation` planner might resolve at query time. The admin
/// CRUD routes write here; the `AppState` bootstrap streams the
/// rows back into `InMemoryAdapterResolver` at server start so a
/// restart does not drop registrations.
///
/// `config` is adapter-specific JSON — for CSV/JSON it carries a
/// `data` field holding the inline payload, for future Postgres /
/// Snowflake adapters it will carry a `connection_string` or a
/// secret-manager reference. Keeping it typeless here means new
/// adapter kinds land without another migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub source_id: String,
    pub kind: String,
    pub config: serde_json::Value,
    /// Last cached `ox_source::RecipeExecutionResult` for this source. `None`
    /// until the first analyze_* call lands. The shape is opaque at
    /// the store layer; ox-api round-trips it through serde.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_analysis_snapshot: Option<serde_json::Value>,
    /// Per-table fingerprint map produced by
    /// [`ox_core::source_schema::SchemaFingerprint`].
    /// Driven by the kernel's drift detection — UI re-scan compares
    /// the live fingerprint against this map and only re-introspects
    /// tables whose hash differs.
    #[serde(default)]
    pub schema_fingerprints: BTreeMap<String, ox_core::SchemaFingerprint>,
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
