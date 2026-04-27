use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use chrono::{DateTime, Utc};
use ox_core::PromptVersion;
use ox_core::error::OxResult;

use crate::models::*;

// ---------------------------------------------------------------------------
// Cursor-based pagination
// ---------------------------------------------------------------------------

/// Cursor-based pagination parameters.
/// Cursor is an opaque compound string: "timestamp|uuid".
#[derive(Debug, Clone, Deserialize)]
pub struct CursorParams {
    /// Max items to return (default 50, max 100)
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Opaque cursor from a previous response's `next_cursor`
    pub cursor: Option<String>,
}

fn default_limit() -> u32 {
    50
}

impl CursorParams {
    /// Clamp limit to [1, 100].
    pub fn effective_limit(&self) -> i64 {
        self.limit.clamp(1, 100) as i64
    }

    /// Parse compound cursor "timestamp|uuid" into its parts.
    pub fn cursor_parts(&self) -> Option<(DateTime<Utc>, Uuid)> {
        let s = self.cursor.as_deref()?;
        let (ts_str, id_str) = s.split_once('|')?;
        let ts: DateTime<Utc> = ts_str.parse().ok().or_else(|| {
            tracing::warn!(cursor = s, "Malformed cursor: invalid timestamp");
            None
        })?;
        let id: Uuid = id_str.parse().ok().or_else(|| {
            tracing::warn!(cursor = s, "Malformed cursor: invalid UUID");
            None
        })?;
        Some((ts, id))
    }
}

impl Default for CursorParams {
    fn default() -> Self {
        Self {
            limit: 50,
            cursor: None,
        }
    }
}

/// Cursor-paginated result.
#[derive(Debug, Serialize)]
pub struct CursorPage<T: Serialize> {
    pub items: Vec<T>,
    /// Pass this value as `cursor` in the next request. `None` means no more pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

// ---------------------------------------------------------------------------
// AnalysisSnapshot — grouped parameters for replace_analysis_snapshot
// ---------------------------------------------------------------------------

pub struct AnalysisSnapshot {
    pub source_config: serde_json::Value,
    /// Canonical source identity recomputed from `source_config`
    /// via `SourceId::from_source_config`. Reanalyze rewrites this
    /// when the fingerprint shifts so federation caches invalidate
    /// naturally on source replacement.
    pub source_id: String,
    pub source_data: Option<String>,
    pub source_schema: serde_json::Value,
    pub source_profile: serde_json::Value,
    pub analysis_report: serde_json::Value,
    pub design_options: serde_json::Value,
}

// ---------------------------------------------------------------------------
// ExtendResult — grouped parameters for update_extend_result
// ---------------------------------------------------------------------------

pub struct ExtendResult {
    /// Canonical OntologyIR JSON — already carries object_mappings
    /// stamped with each source's SourceId, so no separate mapping
    /// blob travels alongside.
    pub ontology: serde_json::Value,
    pub quality_report: serde_json::Value,
    pub source_schema: serde_json::Value,
    pub source_profile: serde_json::Value,
    pub source_history: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Sub-traits — segregated store interfaces
// ---------------------------------------------------------------------------

#[async_trait]
pub trait QueryStore: Send + Sync {
    async fn create_query_execution(&self, execution: &QueryExecution) -> OxResult<()>;

    async fn get_query_execution(
        &self,
        user_id: &str,
        id: Uuid,
    ) -> OxResult<Option<QueryExecution>>;

    async fn list_query_executions(
        &self,
        user_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<QueryExecutionSummary>>;

    /// Update feedback on a query execution. Returns false if not found or not owned by user.
    async fn update_query_feedback(
        &self,
        id: Uuid,
        user_id: &str,
        feedback: Option<&str>,
    ) -> OxResult<bool>;
}

// ---------------------------------------------------------------------------
// Λ Phase — OntologyVersionStore.
//
// Commits extract an `OntologyIR` into content-addressed entities
// (via `ox_ontology::storage::extract_entities`), INSERT ON CONFLICT
// DO NOTHING into the Level 2 store, and write a new pointer set.
// Loads rehydrate a version by joining the pointer set with the
// entity store.
// ---------------------------------------------------------------------------

#[async_trait]
pub trait OntologyVersionStore: Send + Sync {
    /// Create a new logical ontology. Assigns a fresh lineage_id
    /// if `lineage_id` is `None`.
    async fn create_ontology(
        &self,
        name: &str,
        description: &serde_json::Value,
        lineage_id: Option<&str>,
    ) -> OxResult<crate::models::OntologyRow>;

    /// Look up a logical ontology by UUID. Workspace-scoped.
    async fn get_ontology(&self, id: Uuid) -> OxResult<Option<crate::models::OntologyRow>>;

    /// Paginated list of ontology identities visible to the
    /// current workspace. Ordered newest-first by `created_at`
    /// (then `id` for tie-break). Returns the identity row only;
    /// callers that need the current version or IR call
    /// `get_current_version` + `get_ontology_ir` per row.
    async fn list_ontologies(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<crate::models::OntologyRow>>;

    /// Look up a logical ontology by lineage id. The lineage id
    /// is the stable cross-version handle referenced by quality
    /// rules, saved queries, and external mappings.
    async fn find_ontology_by_lineage(
        &self,
        lineage_id: &str,
    ) -> OxResult<Option<crate::models::OntologyRow>>;

    /// Look up a logical ontology by its short name within the
    /// current workspace. Names are unique per workspace (see
    /// `ontologies_ws_name_uq`); the RLS policy scopes the query.
    async fn find_ontology_by_name(
        &self,
        name: &str,
    ) -> OxResult<Option<crate::models::OntologyRow>>;

    /// Commit a new immutable version of `ontology_id`.
    ///
    /// Pipeline:
    /// 1. Extract entities from `ir` via
    ///    `ox_ontology::storage::extract_entities`.
    /// 2. `INSERT ... ON CONFLICT (entity_hash) DO NOTHING`
    ///    into `ontology_entity_versions` — automatic dedup of
    ///    unchanged entities across versions.
    /// 3. Insert a fresh `ontology_version_snapshots` row with
    ///    the new version tag + bitemporal columns.
    /// 4. Bulk-insert the pointer set into
    ///    `ontology_version_entities`.
    ///
    /// Executes in a single transaction — either the whole
    /// commit lands or none of it does.
    async fn commit_version(
        &self,
        ontology_id: Uuid,
        ir: &ox_ontology::OntologyIR,
        version: &str,
        parent_version_id: Option<Uuid>,
        committed_by: &str,
        commit_message: &str,
    ) -> OxResult<crate::models::OntologyVersionSnapshot>;

    /// Hydrate the ontology at a given version. Joins pointer set
    /// with entity store, rehydrates each entity's `content`
    /// JSONB into the typed `XxxDef`, and assembles the full
    /// `OntologyIR`.
    ///
    /// Returns `Ok(None)` when no snapshot exists for `version_id`
    /// (e.g., the snapshot was deleted between a prior lookup and
    /// this hydrate, or the caller was handed a stale handle).
    /// Returns `Err` only when stored entities are malformed
    /// (parse / deserialization failure or missing header).
    async fn get_ontology_ir(
        &self,
        version_id: Uuid,
    ) -> OxResult<Option<ox_ontology::OntologyIR>>;

    /// Fetch a version snapshot record by id (without hydrating
    /// the full IR). Used by routes that need version metadata
    /// (committed_by, commit_message, valid_from) separate from
    /// the IR content.
    async fn get_version_snapshot(
        &self,
        version_id: Uuid,
    ) -> OxResult<Option<crate::models::OntologyVersionSnapshot>>;

    /// List the version history of an ontology, newest first.
    async fn list_versions(
        &self,
        ontology_id: Uuid,
        limit: u32,
    ) -> OxResult<Vec<crate::models::OntologyVersionSnapshot>>;

    /// "Live at" version resolver. Picks the newest version
    /// whose `valid_from <= as_of` AND (`valid_to IS NULL OR
    /// valid_to > as_of`). Used by TemporalRewriter for AS-OF
    /// queries.
    async fn resolve_version_at(
        &self,
        ontology_id: Uuid,
        as_of: chrono::DateTime<chrono::Utc>,
    ) -> OxResult<Option<crate::models::OntologyVersionSnapshot>>;

    /// The current (valid_to IS NULL) version for an ontology.
    async fn get_current_version(
        &self,
        ontology_id: Uuid,
    ) -> OxResult<Option<crate::models::OntologyVersionSnapshot>>;

    /// Diff two versions. Returns one `EntityChange` per
    /// `(kind, logical_id)` whose hash differs. Order: kind then
    /// logical_id — stable for UI rendering.
    async fn diff_versions(
        &self,
        from_version: Uuid,
        to_version: Uuid,
    ) -> OxResult<Vec<crate::models::EntityChange>>;
}

// ---------------------------------------------------------------------------
// Λ-11 — Progressive Disclosure Navigation Store.
//
// Backed by the Level 3 materialised indexes (migrations 0018-
// 0021). Every method is version-scoped — the caller picks which
// ontology version to navigate, matching the temporal-rewriter
// contract.
//
// The four core flows:
//
//   1. Entry discovery — user types "orders" → `search_entry_points`
//      returns ranked hits (fuzzy + full-text + semantic).
//   2. Expansion — from a selected entity, `expand_neighbors` yields
//      the 1-hop cross-references (Property→ValueSet, etc).
//   3. Hierarchy — `walk_hierarchy` traverses closure tables
//      (CodeSystem broader, GlossaryTerm parent, Interface
//      implements) in O(1).
//   4. Similarity — `similar_to` uses the `entity_embedding` HNSW
//      index for semantic kNN.
//
// The trait is separate from OntologyVersionStore because (a)
// navigation is a read-only surface even when versioning is in
// play, (b) a future split where the navigation store is a
// read-replica / cached view stays clean.
// ---------------------------------------------------------------------------

/// Progressive-Disclosure navigation over the Level-3 flat indexes.
/// See the patent 1-pager `Progressive Disclosure` section for the
/// 4-step contract; each method corresponds to one step so the
/// layered usage (search → expand → filter → render) is clear at the
/// trait level.
///
/// Options structs in [`crate::navigation`] keep the signatures
/// parameter-rich without adding positional bloat. `Subgraph` is the
/// shared value moved through steps 2 + 3 so a caller can chain
/// without re-allocating.
#[async_trait]
pub trait OntologyNavigationStore: Send + Sync {
    /// Step 1 — anchor search. Blended trigram + full-text + embedding
    /// scoring over the searchable document. Returned hits are sorted
    /// by `score` descending; the caller picks the top-K as anchors
    /// for `expand_neighbors`.
    async fn search_entry_points(
        &self,
        options: crate::navigation::EntryPointSearchOptions,
    ) -> OxResult<Vec<crate::navigation::EntitySearchHit>>;

    /// Step 2 — BFS from a batch of anchors, depth-limited. Returns a
    /// single [`crate::navigation::Subgraph`] aggregating every
    /// reachable node / edge. Sets `Subgraph.truncated` when
    /// `max_nodes` trimmed the frontier.
    async fn expand_neighbors(
        &self,
        options: crate::navigation::NeighborExpandOptions,
    ) -> OxResult<crate::navigation::Subgraph>;

    /// Step 3 — merge hierarchy closure into an existing subgraph and
    /// optionally filter by facet. Called on the result of step 2;
    /// returns the mutated subgraph so the caller can chain or
    /// snapshot independently of the input.
    async fn apply_hierarchy_and_facet(
        &self,
        subgraph: crate::navigation::Subgraph,
        options: crate::navigation::HierarchyFacetOptions,
    ) -> OxResult<crate::navigation::Subgraph>;

    /// Step 4 — render the subgraph as markdown suited to the LLM
    /// prompt tail. Pure function; does not touch the store beyond
    /// needing `&self` for trait-object erasure.
    fn render_subgraph_for_llm(
        &self,
        subgraph: &crate::navigation::Subgraph,
        options: &crate::navigation::LlmRenderOptions,
    ) -> String;

    /// Semantic kNN over the Level-3 embedding index. Returns empty
    /// when the target entity has no embedding yet (cold row —
    /// background populator hasn't caught up). Surfaced separately
    /// from `search_entry_points` because the caller typically wants
    /// either anchor search (blend) *or* pure semantic neighbourhood
    /// — not both at once.
    async fn similar_entities(
        &self,
        version_id: Uuid,
        entity_kind: &str,
        logical_id: &str,
        top_k: u32,
    ) -> OxResult<Vec<crate::navigation::EntitySearchHit>>;
}

#[async_trait]
pub trait PinStore: Send + Sync {
    async fn create_pin(&self, user_id: &str, item: &PinboardItem) -> OxResult<()>;

    async fn list_pins(
        &self,
        user_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<PinboardItem>>;

    async fn delete_pin(&self, user_id: &str, id: Uuid) -> OxResult<bool>;
}

#[async_trait]
pub trait ProjectStore: Send + Sync {
    async fn create_design_project(&self, project: &DesignProject) -> OxResult<()>;

    async fn get_design_project(&self, id: Uuid) -> OxResult<Option<DesignProject>>;

    async fn list_design_projects(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<DesignProjectSummary>>;

    async fn update_design_options(
        &self,
        id: Uuid,
        options: &serde_json::Value,
        expected_revision: i32,
    ) -> OxResult<()>;

    async fn update_design_result(
        &self,
        id: Uuid,
        ontology: &serde_json::Value,
        quality_report: Option<&serde_json::Value>,
        expected_revision: i32,
    ) -> OxResult<()>;

    /// Update extend result — updates ontology, source mapping, quality report,
    /// and merges source schema/profile from the extension source.
    async fn update_extend_result(
        &self,
        id: Uuid,
        result: &ExtendResult,
        expected_revision: i32,
    ) -> OxResult<()>;

    /// Replace the analysis snapshot (reanalyze). Resets status to "analyzed",
    /// clears ontology/quality_report, and updates design_options (pruned by caller).
    async fn replace_analysis_snapshot(
        &self,
        id: Uuid,
        snapshot: &AnalysisSnapshot,
        expected_revision: i32,
    ) -> OxResult<()>;

    /// Mark the project as completed and link it to a committed
    /// ontology identity. The caller performs
    /// [`OntologyVersionStore::create_ontology`] + `commit_version`
    /// separately, then hands the resulting identity UUID here so
    /// the project row's `ontology_id` column can point at it.
    ///
    /// Uses optimistic CAS on `revision` — stale submissions fail
    /// rather than clobbering a concurrent update.
    async fn complete_design_project(
        &self,
        project_id: Uuid,
        ontology_id: Uuid,
        expected_revision: i32,
    ) -> OxResult<()>;

    async fn delete_design_project(&self, id: Uuid) -> OxResult<bool>;

    /// Archive WIP projects that haven't been updated within `max_age_days`.
    /// Returns per-workspace counts so the maintenance loop can record one
    /// audit row per affected workspace.
    async fn archive_stale_projects(&self, max_age_days: i64) -> OxResult<Vec<(Uuid, u64)>>;

    /// Permanently delete projects that have been archived for longer than `max_archive_days`.
    /// Returns per-workspace counts so the maintenance loop can record one
    /// audit row per affected workspace.
    async fn delete_archived_projects(&self, max_archive_days: i64) -> OxResult<Vec<(Uuid, u64)>>;

    // --- Ontology Snapshots ---

    /// Create an ontology snapshot for a given project revision.
    /// Uses ON CONFLICT DO NOTHING for idempotency.
    async fn create_ontology_snapshot(
        &self,
        project_id: Uuid,
        revision: i32,
        ontology: &serde_json::Value,
        quality_report: Option<&serde_json::Value>,
    ) -> OxResult<()>;

    /// List ontology snapshots for a project, ordered by revision DESC.
    /// Returns lightweight summaries with node/edge counts extracted from JSONB.
    async fn list_ontology_snapshots(
        &self,
        project_id: Uuid,
    ) -> OxResult<Vec<OntologySnapshotSummary>>;

    /// Get a single ontology snapshot by project_id + revision.
    async fn get_ontology_snapshot(
        &self,
        project_id: Uuid,
        revision: i32,
    ) -> OxResult<Option<OntologySnapshot>>;
}

#[async_trait]
pub trait PerspectiveStore: Send + Sync {
    async fn upsert_perspective(&self, perspective: &WorkbenchPerspective) -> OxResult<()>;

    async fn get_perspective(
        &self,
        user_id: &str,
        lineage_id: &str,
        name: &str,
    ) -> OxResult<Option<WorkbenchPerspective>>;

    async fn get_default_perspective(
        &self,
        user_id: &str,
        lineage_id: &str,
    ) -> OxResult<Option<WorkbenchPerspective>>;

    /// 2-tier perspective lookup:
    /// 1. Exact match: lineage_id + default
    /// 2. Topology match: different lineage but same topology_signature
    /// Returns the best matching perspective, or None.
    async fn get_best_perspective(
        &self,
        user_id: &str,
        lineage_id: &str,
        topology_signature: &str,
    ) -> OxResult<Option<WorkbenchPerspective>>;

    async fn list_perspectives(
        &self,
        user_id: &str,
        lineage_id: &str,
    ) -> OxResult<Vec<WorkbenchPerspective>>;

    async fn delete_perspective(&self, user_id: &str, id: Uuid) -> OxResult<bool>;
}

#[async_trait]
pub trait ConfigStore: Send + Sync {
    async fn list_config(&self) -> OxResult<Vec<SystemConfigRow>>;

    /// Get a single config value by key.
    async fn get_config(&self, key: &str) -> OxResult<Option<String>>;

    /// Set a single config value (upserts).
    async fn update_config(&self, category: &str, key: &str, value: &str) -> OxResult<()>;

    /// Batch update config values in a single transaction.
    /// All updates succeed or none are applied.
    async fn update_config_batch(&self, updates: &[(String, String, String)]) -> OxResult<()>;
}

#[async_trait]
pub trait UserStore: Send + Sync {
    /// Insert or update a user (matched by provider + provider_sub).
    /// On conflict, updates name, picture, and last_login_at.
    async fn upsert_user(&self, user: &User) -> OxResult<User>;

    async fn get_user_by_id(&self, id: Uuid) -> OxResult<Option<User>>;

    async fn get_user_by_provider(
        &self,
        provider: &str,
        provider_sub: &str,
    ) -> OxResult<Option<User>>;

    async fn list_users(&self, pagination: &CursorParams) -> OxResult<CursorPage<User>>;

    async fn update_user_role(&self, id: Uuid, role: &str) -> OxResult<()>;

    async fn count_users(&self) -> OxResult<i64>;
}

#[async_trait]
pub trait RecipeStore: Send + Sync {
    async fn upsert_recipe(&self, recipe: &AnalysisRecipe) -> OxResult<()>;
    async fn get_recipe(&self, id: Uuid) -> OxResult<Option<AnalysisRecipe>>;
    async fn list_recipes(&self, pagination: &CursorParams)
    -> OxResult<CursorPage<AnalysisRecipe>>;
    async fn delete_recipe(&self, id: Uuid) -> OxResult<bool>;
    async fn update_recipe_status(&self, id: Uuid, status: &str) -> OxResult<()>;
    async fn create_recipe_version(&self, recipe: &AnalysisRecipe) -> OxResult<()>;
    async fn list_recipe_versions(&self, parent_id: Uuid) -> OxResult<Vec<AnalysisRecipe>>;
    /// Batch upsert multiple recipes in a single transaction.
    async fn upsert_recipes_batch(&self, recipes: &[AnalysisRecipe]) -> OxResult<()>;
}

#[async_trait]
pub trait DashboardStore: Send + Sync {
    async fn create_dashboard(&self, dashboard: &Dashboard) -> OxResult<()>;
    async fn get_dashboard(&self, id: Uuid) -> OxResult<Option<Dashboard>>;
    async fn list_dashboards(
        &self,
        user_id: &str,
        is_admin: bool,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<Dashboard>>;
    async fn update_dashboard(
        &self,
        id: Uuid,
        name: &str,
        description: Option<&str>,
        layout: &serde_json::Value,
        is_public: bool,
    ) -> OxResult<()>;
    async fn delete_dashboard(&self, id: Uuid) -> OxResult<bool>;
    /// Set or clear the share token. When `token` is `Some`, the caller
    /// must also pass `expires_at` so the token has a definite TTL.
    async fn update_dashboard_share_token(
        &self,
        id: Uuid,
        token: Option<&str>,
        expires_at: Option<DateTime<Utc>>,
    ) -> OxResult<()>;
    /// Resolve a share token to its dashboard. Returns `Ok(None)` if the
    /// token is unknown OR if the token has expired.
    async fn get_dashboard_by_share_token(&self, token: &str) -> OxResult<Option<Dashboard>>;

    async fn create_widget(&self, widget: &DashboardWidget) -> OxResult<()>;
    async fn list_widgets(&self, dashboard_id: Uuid) -> OxResult<Vec<DashboardWidget>>;
    async fn update_widget(
        &self,
        id: Uuid,
        title: Option<&str>,
        widget_type: Option<&str>,
        query: Option<&str>,
        refresh_interval_secs: Option<i32>,
        thresholds: Option<&serde_json::Value>,
    ) -> OxResult<()>;
    async fn update_widget_result(&self, id: Uuid, result: &serde_json::Value) -> OxResult<()>;
    async fn delete_widget(&self, id: Uuid) -> OxResult<bool>;
    /// Batch create multiple widgets in a single transaction.
    async fn create_widgets_batch(&self, widgets: &[DashboardWidget]) -> OxResult<()>;
}

/// Cron-based scheduled recipe execution.
#[async_trait]
pub trait ScheduledTaskStore: Send + Sync {
    async fn create_scheduled_task(&self, task: &ScheduledTask) -> OxResult<()>;
    async fn get_scheduled_task(&self, id: Uuid) -> OxResult<Option<ScheduledTask>>;
    async fn list_scheduled_tasks(&self, recipe_id: Option<Uuid>) -> OxResult<Vec<ScheduledTask>>;
    async fn list_due_tasks(&self) -> OxResult<Vec<ScheduledTask>>;
    async fn update_task_after_run(
        &self,
        id: Uuid,
        next_run_at: DateTime<Utc>,
        status: &str,
    ) -> OxResult<()>;
    async fn update_scheduled_task_enabled(&self, id: Uuid, enabled: bool) -> OxResult<()>;
    async fn delete_scheduled_task(&self, id: Uuid) -> OxResult<bool>;
}

/// Storage for analysis execution results with input-hash-based caching.
#[async_trait]
pub trait AnalysisResultStore: Send + Sync {
    async fn create_analysis_result(&self, result: &AnalysisResult) -> OxResult<()>;
    async fn get_cached_result(
        &self,
        input_hash: &str,
        recipe_id: Option<Uuid>,
    ) -> OxResult<Option<AnalysisResult>>;
    async fn list_analysis_results(
        &self,
        recipe_id: Uuid,
        limit: i64,
    ) -> OxResult<Vec<AnalysisResult>>;
    /// Delete analysis results older than `max_age_days`. Returns
    /// per-workspace counts so the maintenance loop can record one
    /// audit row per affected workspace.
    async fn cleanup_old_results(&self, max_age_days: i64) -> OxResult<Vec<(Uuid, u64)>>;
}

#[async_trait]
pub trait HealthStore: Send + Sync {
    async fn health_check(&self) -> bool;
}

// ---------------------------------------------------------------------------
// PromptTemplateStore — versioned prompt management
// ---------------------------------------------------------------------------

#[async_trait]
pub trait PromptTemplateStore: Send + Sync {
    async fn list_prompt_templates(&self, active_only: bool) -> OxResult<Vec<PromptTemplateRow>>;
    async fn get_prompt_template(&self, id: Uuid) -> OxResult<Option<PromptTemplateRow>>;
    async fn get_active_prompt(&self, name: &str) -> OxResult<Option<PromptTemplateRow>>;
    /// Exact lookup by `(name, version)` — required for the TOML seed
    /// flow's drift-detection pass. Returns the row regardless of
    /// `is_active` so seed can compare content against an operator-
    /// deactivated row.
    async fn find_prompt_template_by_name_version(
        &self,
        name: &str,
        version: &PromptVersion,
    ) -> OxResult<Option<PromptTemplateRow>>;
    /// Resolve a prompt with workspace-specific override fallback.
    /// Returns the workspace's override if one exists, otherwise the
    /// global active prompt with the same name.
    async fn get_active_prompt_for_workspace(
        &self,
        name: &str,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Option<PromptTemplateRow>>;
    async fn create_prompt_template(&self, row: &PromptTemplateRow) -> OxResult<()>;
    async fn update_prompt_template(
        &self,
        id: Uuid,
        content: &str,
        variables: &serde_json::Value,
        is_active: bool,
    ) -> OxResult<()>;
    async fn delete_prompt_template(&self, id: Uuid) -> OxResult<bool>;
    /// Deactivate all versions of a prompt with the given name except `exclude_id`.
    async fn update_prompt_template_active_only(
        &self,
        name: &str,
        exclude_id: Uuid,
    ) -> OxResult<()>;
}

// ---------------------------------------------------------------------------
// AgentSessionStore — session recording for replay and audit
// ---------------------------------------------------------------------------

#[async_trait]
pub trait AgentSessionStore: Send + Sync {
    async fn create_agent_session(&self, session: &AgentSession) -> OxResult<()>;
    async fn complete_agent_session(&self, id: Uuid, final_text: Option<&str>) -> OxResult<()>;
    async fn get_agent_session(&self, id: Uuid) -> OxResult<Option<AgentSession>>;
    async fn list_agent_sessions(
        &self,
        user_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<AgentSession>>;
    async fn create_agent_event(&self, event: &AgentEvent) -> OxResult<()>;
    async fn list_agent_events(&self, session_id: Uuid) -> OxResult<Vec<AgentEvent>>;
    async fn delete_agent_session(&self, id: Uuid) -> OxResult<bool>;
    /// Returns per-workspace counts so the maintenance loop can record one
    /// audit row per affected workspace.
    async fn cleanup_old_sessions(&self, retention_days: i64) -> OxResult<Vec<(Uuid, u64)>>;
}

// ---------------------------------------------------------------------------
// ReportStore — parameterized saved reports
// ---------------------------------------------------------------------------

/// Persistent storage for parameterized saved reports (Cypher templates with bind variables).
#[async_trait]
pub trait ReportStore: Send + Sync {
    async fn create_report(&self, report: &SavedReport) -> OxResult<()>;
    async fn get_report(&self, id: Uuid) -> OxResult<Option<SavedReport>>;
    async fn list_reports(
        &self,
        user_id: &str,
        ontology_lineage_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<SavedReport>>;
    async fn update_report(
        &self,
        id: Uuid,
        title: &str,
        description: Option<&str>,
        query_template: &str,
        parameters: &serde_json::Value,
        widget_type: Option<&str>,
        is_public: bool,
    ) -> OxResult<()>;
    async fn delete_report(&self, id: Uuid) -> OxResult<bool>;
}

// ---------------------------------------------------------------------------
// PatternStore — saved canvas-editable PatternIR (positions + zoom preserved)
// ---------------------------------------------------------------------------

/// Persistent storage for saved visual-query-builder patterns. The payload
/// is the PatternIR itself rather than a compiled QueryIR so a reopen
/// restores the user's node layout and viewport without re-layout.
#[async_trait]
pub trait PatternStore: Send + Sync {
    async fn create_pattern(&self, pattern: &SavedQueryPattern) -> OxResult<()>;
    async fn get_pattern(&self, id: Uuid) -> OxResult<Option<SavedQueryPattern>>;
    async fn list_patterns(
        &self,
        user_id: &str,
        ontology_lineage_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<SavedQueryPattern>>;
    async fn update_pattern(
        &self,
        id: Uuid,
        name: &str,
        description: Option<&str>,
        pattern_ir: &serde_json::Value,
    ) -> OxResult<bool>;
    async fn delete_pattern(&self, id: Uuid) -> OxResult<bool>;
}

// ---------------------------------------------------------------------------
// EmbeddingRetryStore — pending embedding retry queue
// ---------------------------------------------------------------------------

/// Queue for embedding operations that failed and need retry on the next periodic sweep.
#[async_trait]
pub trait EmbeddingRetryStore: Send + Sync {
    async fn create_pending_embedding(
        &self,
        content: &str,
        metadata: &serde_json::Value,
    ) -> OxResult<()>;
    async fn list_pending_embeddings(&self, limit: i64) -> OxResult<Vec<PendingEmbedding>>;
    async fn mark_embedding_failed(&self, id: Uuid, error: &str) -> OxResult<()>;
    async fn delete_pending_embedding(&self, id: Uuid) -> OxResult<bool>;
}

// ---------------------------------------------------------------------------
// VerificationStore — element-level verification tracking
// ---------------------------------------------------------------------------

#[async_trait]
pub trait VerificationStore: Send + Sync {
    async fn verify_element(&self, v: &ElementVerification) -> OxResult<Uuid>;
    async fn get_verifications(
        &self,
        ontology_lineage_id: &str,
    ) -> OxResult<Vec<ElementVerification>>;
    async fn invalidate_for_elements(
        &self,
        ontology_lineage_id: &str,
        element_ids: &[&str],
        reason: &str,
    ) -> OxResult<u64>;
    async fn delete_verification(
        &self,
        ontology_lineage_id: &str,
        element_id: &str,
        user_id: Uuid,
    ) -> OxResult<bool>;
}

// ---------------------------------------------------------------------------
// ToolApprovalStore — HITL tool review decisions
// ---------------------------------------------------------------------------

#[async_trait]
pub trait ToolApprovalStore: Send + Sync {
    async fn create_tool_approval(&self, approval: &ToolApproval) -> OxResult<()>;
    async fn get_tool_approval(
        &self,
        session_id: Uuid,
        tool_call_id: &str,
    ) -> OxResult<Option<ToolApproval>>;
}

// ---------------------------------------------------------------------------
// WorkspaceStore — workspace management (not subject to RLS)
// ---------------------------------------------------------------------------

#[async_trait]
pub trait WorkspaceStore: Send + Sync {
    async fn create_workspace(&self, workspace: &Workspace) -> OxResult<()>;
    async fn get_workspace(&self, id: Uuid) -> OxResult<Option<Workspace>>;
    async fn get_workspace_by_slug(&self, slug: &str) -> OxResult<Option<Workspace>>;
    async fn list_user_workspaces(&self, user_id: Uuid) -> OxResult<Vec<WorkspaceSummary>>;
    async fn update_workspace(
        &self,
        id: Uuid,
        name: &str,
        settings: &serde_json::Value,
    ) -> OxResult<()>;
    async fn delete_workspace(&self, id: Uuid) -> OxResult<bool>;

    /// Update the workspace's primary locale + fallback chain. `primary_locale`
    /// must be a BCP 47 tag (ox-core's `LanguageTag::parse` syntax);
    /// `locale_fallback` must be a non-empty JSONB array of the same shape.
    /// Both are enforced by DB CHECK constraints.
    async fn update_workspace_locale(
        &self,
        id: Uuid,
        primary_locale: &str,
        locale_fallback: &serde_json::Value,
    ) -> OxResult<()>;

    // Membership
    async fn add_workspace_member(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> OxResult<()>;
    async fn remove_workspace_member(&self, workspace_id: Uuid, user_id: Uuid) -> OxResult<bool>;
    async fn update_member_role(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> OxResult<()>;
    async fn get_member_role(&self, workspace_id: Uuid, user_id: Uuid) -> OxResult<Option<String>>;
    async fn list_workspace_members(&self, workspace_id: Uuid) -> OxResult<Vec<WorkspaceMember>>;

    /// Get user's default workspace (first workspace they belong to, or the "default" slug).
    async fn get_default_workspace(&self, user_id: Uuid) -> OxResult<Option<Workspace>>;

    /// Every workspace id known to the cluster. Used by
    /// system-bypass cron jobs that fan out per-tenant work — the
    /// per-workspace bodies run inside `WORKSPACE_ID.scope(id, …)`
    /// so RLS lands on the right tenant.
    async fn list_workspace_ids(&self) -> OxResult<Vec<Uuid>>;
}

// ---------------------------------------------------------------------------
// AuditStore — append-only event log for enterprise governance
// ---------------------------------------------------------------------------

#[async_trait]
pub trait AuditStore: Send + Sync {
    /// Record an audit event (append-only). The current workspace is
    /// inferred from the `WORKSPACE_ID` task-local. To attribute a
    /// system-bypass action to a specific workspace, use
    /// [`record_audit_for_workspace`].
    async fn record_audit(
        &self,
        user_id: Option<Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: serde_json::Value,
    ) -> OxResult<()>;

    /// Record an audit event whose target workspace differs from the
    /// caller's. Used by SYSTEM_BYPASS maintenance tasks so workspace
    /// admins can later see which system actions touched their data.
    ///
    /// `affected_workspace_id` is stored in `audit_log.affected_workspace_id`
    /// (added in migration 0005). When `None` it falls back to the same
    /// behaviour as [`record_audit`].
    async fn record_audit_for_workspace(
        &self,
        user_id: Option<Uuid>,
        affected_workspace_id: Option<Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: serde_json::Value,
    ) -> OxResult<()>;

    /// List audit events with cursor pagination.
    async fn list_audit_events(&self, params: CursorParams) -> OxResult<CursorPage<AuditEntry>>;
}

// ---------------------------------------------------------------------------
// MeteringStore — cost/usage tracking for billing and budgeting
// ---------------------------------------------------------------------------

#[async_trait]
pub trait MeteringStore: Send + Sync {
    /// Record a usage event (LLM call, query execution, etc.)
    async fn record_usage(
        &self,
        user_id: Option<Uuid>,
        resource_type: &str,
        provider: Option<&str>,
        model: Option<&str>,
        operation: Option<&str>,
        input_tokens: i64,
        output_tokens: i64,
        duration_ms: i64,
        cost_usd: f64,
        metadata: serde_json::Value,
    ) -> OxResult<()>;

    /// Get aggregated usage summary for a time range.
    async fn usage_summary(
        &self,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> OxResult<Vec<UsageSummary>>;
}

// ---------------------------------------------------------------------------
// LineageStore — data provenance tracking
// ---------------------------------------------------------------------------

#[async_trait]
pub trait LineageStore: Send + Sync {
    /// Record the start of a data load operation.
    async fn create_lineage_entry(&self, entry: &LineageEntry) -> OxResult<()>;

    /// Mark a lineage entry as completed (success or failure).
    async fn complete_lineage_entry(
        &self,
        id: Uuid,
        record_count: i64,
        status: &str,
        error_message: Option<&str>,
    ) -> OxResult<()>;

    /// Get lineage entries for a specific graph label.
    async fn list_lineage_for_label(&self, graph_label: &str) -> OxResult<Vec<LineageEntry>>;

    /// Get lineage entries for a project.
    async fn list_lineage_for_project(&self, project_id: Uuid) -> OxResult<Vec<LineageEntry>>;

    /// Get a summary of lineage per graph label (for overview).
    async fn lineage_summary(&self) -> OxResult<Vec<LineageSummary>>;
}

// ---------------------------------------------------------------------------
// ApprovalStore — configurable gates for schema deployment & migration
// ---------------------------------------------------------------------------

#[async_trait]
pub trait ApprovalStore: Send + Sync {
    /// Create a new approval request.
    async fn create_approval_request(
        &self,
        requester_id: Uuid,
        action_type: &str,
        resource_type: &str,
        resource_id: &str,
        payload: serde_json::Value,
    ) -> OxResult<ApprovalRequest>;

    /// Get a single approval request by ID.
    async fn get_approval_request(&self, id: Uuid) -> OxResult<Option<ApprovalRequest>>;

    /// List pending approvals for the current workspace.
    async fn list_pending_approvals(&self, workspace_id: Uuid) -> OxResult<Vec<ApprovalRequest>>;

    /// Approve or reject an approval request. A non-empty trimmed
    /// `note` is recorded as the first entry on the comment thread
    /// in the same transaction as the row update — both writes land
    /// or both roll back. Returns the created comment when one was
    /// recorded.
    async fn review_approval(
        &self,
        id: Uuid,
        reviewer_id: Uuid,
        approved: bool,
        note: Option<String>,
    ) -> OxResult<Option<ApprovalComment>>;

    /// Expire old pending approvals past their `expires_at`.
    /// Returns per-workspace counts so the maintenance loop can record
    /// one audit row per affected workspace.
    async fn expire_old_approvals(&self) -> OxResult<Vec<(Uuid, u64)>>;
}

// ---------------------------------------------------------------------------
// AuditTrailStore — workspace-wide PROV-O audit records
// ---------------------------------------------------------------------------

/// Free-form filter applied to the audit endpoint. Every field is
/// optional; an empty filter returns the full workspace stream.
#[derive(Debug, Clone, Default)]
pub struct AuditTrailFilter {
    pub ontology_id: Option<Uuid>,
    pub activity_kind: Option<String>,
    pub agent_kind: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

/// One record in the audit stream. The `provenance` payload is the
/// content-addressed PROV-O entity (`ProvenanceDef`) emitted at IR
/// commit time; the surrounding fields attribute it to the source
/// ontology so a multi-ontology workspace can render a rolled-up
/// view without an extra detail fetch.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditRecord {
    pub ontology_id: Uuid,
    pub ontology_lineage_id: String,
    pub ontology_name: String,
    pub provenance: serde_json::Value,
    pub at_time: DateTime<Utc>,
}

#[async_trait]
pub trait AuditTrailStore: Send + Sync {
    /// Stream PROV-O records across every committed ontology in the
    /// workspace, filtered + cursor-paginated. Ordering is `at_time`
    /// descending with the entity hash as the deterministic tiebreak.
    async fn list_audit_records(
        &self,
        filter: AuditTrailFilter,
        cursor: Option<&str>,
        limit: i64,
    ) -> OxResult<CursorPage<AuditRecord>>;
}

// ---------------------------------------------------------------------------
// ApprovalCommentStore — thread of comments attached to an approval request
// ---------------------------------------------------------------------------

#[async_trait]
pub trait ApprovalCommentStore: Send + Sync {
    /// List every comment attached to an approval, oldest first.
    async fn list_approval_comments(&self, approval_id: Uuid) -> OxResult<Vec<ApprovalComment>>;

    /// Append a comment to an approval thread.
    async fn create_approval_comment(
        &self,
        approval_id: Uuid,
        author_id: Uuid,
        body: &str,
    ) -> OxResult<ApprovalComment>;
}

// ---------------------------------------------------------------------------
// QualityStore — declarative data quality rules with evaluation
// ---------------------------------------------------------------------------

#[async_trait]
pub trait QualityStore: Send + Sync {
    async fn create_quality_rule(&self, rule: &QualityRule) -> OxResult<()>;
    async fn get_quality_rule(&self, id: Uuid) -> OxResult<Option<QualityRule>>;
    async fn list_quality_rules(
        &self,
        ontology_lineage_id: Option<&str>,
        target_label: Option<&str>,
    ) -> OxResult<Vec<QualityRule>>;
    async fn update_quality_rule(
        &self,
        id: Uuid,
        name: &str,
        threshold: f64,
        is_active: bool,
    ) -> OxResult<()>;
    async fn delete_quality_rule(&self, id: Uuid) -> OxResult<bool>;
    async fn record_quality_result(&self, result: &QualityResult) -> OxResult<()>;
    async fn list_latest_results(&self, rule_id: Uuid, limit: i64) -> OxResult<Vec<QualityResult>>;
    async fn list_quality_dashboard_entries(&self) -> OxResult<Vec<QualityDashboardEntry>>;
}

// ---------------------------------------------------------------------------
// AclStore — fine-grained attribute-based access control
// ---------------------------------------------------------------------------

#[async_trait]
pub trait AclStore: Send + Sync {
    /// Create an ACL policy.
    async fn create_acl_policy(&self, policy: &AclPolicy) -> OxResult<()>;

    /// Get a single ACL policy.
    async fn get_acl_policy(&self, id: Uuid) -> OxResult<Option<AclPolicy>>;

    /// List active ACL policies, optionally filtered by subject or resource.
    async fn list_acl_policies(
        &self,
        subject_type: Option<&str>,
        resource_value: Option<&str>,
    ) -> OxResult<Vec<AclPolicy>>;

    /// Update an ACL policy.
    async fn update_acl_policy(
        &self,
        id: Uuid,
        name: &str,
        action: &str,
        properties: Option<&[String]>,
        mask_pattern: Option<&str>,
        priority: i32,
        is_active: bool,
    ) -> OxResult<()>;

    /// Delete an ACL policy.
    async fn delete_acl_policy(&self, id: Uuid) -> OxResult<bool>;

    /// Get all active policies applicable to a given subject (for runtime evaluation).
    /// Returns policies ordered by priority DESC (most specific first).
    async fn get_effective_policies(
        &self,
        platform_role: &str,
        workspace_role: &str,
        user_id: Option<Uuid>,
    ) -> OxResult<Vec<AclPolicy>>;
}

// ---------------------------------------------------------------------------
// ModelConfigStore — runtime LLM model configuration
// ---------------------------------------------------------------------------

use crate::models::{
    ModelConfig, ModelConfigUpdate, ModelRoutingRule, NewModelConfig, NewRoutingRule,
    RoutingRuleUpdate,
};

#[async_trait]
pub trait ModelConfigStore: Send + Sync {
    async fn list_model_configs(&self, workspace_id: Option<Uuid>) -> OxResult<Vec<ModelConfig>>;
    async fn get_model_config(&self, id: Uuid) -> OxResult<Option<ModelConfig>>;
    async fn create_model_config(&self, config: &NewModelConfig) -> OxResult<ModelConfig>;
    async fn update_model_config(
        &self,
        id: Uuid,
        update: &ModelConfigUpdate,
    ) -> OxResult<ModelConfig>;
    async fn delete_model_config(&self, id: Uuid) -> OxResult<bool>;

    async fn list_routing_rules(
        &self,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Vec<ModelRoutingRule>>;
    async fn get_routing_rule(&self, id: Uuid) -> OxResult<Option<ModelRoutingRule>>;
    async fn create_routing_rule(&self, rule: &NewRoutingRule) -> OxResult<ModelRoutingRule>;
    async fn update_routing_rule(
        &self,
        id: Uuid,
        update: &RoutingRuleUpdate,
    ) -> OxResult<ModelRoutingRule>;
    async fn delete_routing_rule(&self, id: Uuid) -> OxResult<bool>;

    /// Single optimized query: find the best model for an operation + workspace.
    /// Checks workspace-specific rules first, then global rules, then wildcard.
    async fn find_model_for_operation(
        &self,
        operation: &str,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Option<ModelConfig>>;
}

// ---------------------------------------------------------------------------
// KnowledgeStore — failure-driven learning knowledge base
// ---------------------------------------------------------------------------

use crate::models::KnowledgeEntry;

#[async_trait]
pub trait KnowledgeStore: Send + Sync {
    async fn create_knowledge_entry(&self, entry: &KnowledgeEntry) -> OxResult<()>;
    async fn get_knowledge_entry(&self, id: Uuid) -> OxResult<Option<KnowledgeEntry>>;
    async fn update_knowledge_entry(
        &self,
        id: Uuid,
        title: &str,
        content: &str,
        structured_data: &serde_json::Value,
        affected_labels: &[String],
        affected_properties: &[String],
    ) -> OxResult<()>;
    async fn delete_knowledge_entry(&self, id: Uuid) -> OxResult<bool>;

    async fn list_knowledge_entries(
        &self,
        ontology_name: Option<&str>,
        kind: Option<&str>,
        status: Option<&str>,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<KnowledgeEntry>>;

    /// Approved entries for a given ontology name and version, ordered by confidence.
    async fn list_active_knowledge(
        &self,
        ontology_name: &str,
        ontology_version: i32,
        kinds: &[&str],
        limit: i64,
    ) -> OxResult<Vec<KnowledgeEntry>>;

    async fn update_knowledge_status(
        &self,
        id: Uuid,
        status: &str,
        reviewer_id: Option<Uuid>,
        review_notes: Option<&str>,
    ) -> OxResult<()>;

    async fn update_knowledge_confidence(&self, id: Uuid, confidence: f64) -> OxResult<()>;

    /// Bulk-mark entries as stale when affected_labels overlap with changed labels.
    async fn mark_stale_by_labels(
        &self,
        ontology_name: &str,
        changed_labels: &[String],
    ) -> OxResult<u64>;

    /// Fire-and-forget: increment use_count and update last_used_at.
    async fn record_knowledge_usage(&self, ids: &[Uuid]) -> OxResult<()>;

    /// Admin confirms knowledge is valid for a given ontology version.
    async fn verify_knowledge(&self, id: Uuid, version: i32) -> OxResult<()>;

    /// Label-based GIN lookup: affected_labels && $labels, ordered by confidence.
    async fn search_knowledge_by_labels(
        &self,
        ontology_name: &str,
        ontology_version: i32,
        labels: &[&str],
        limit: i64,
    ) -> OxResult<Vec<KnowledgeEntry>>;

    /// Counts grouped by (status, kind) — for dashboard stats without loading all rows.
    async fn count_knowledge_by_status_kind(&self) -> OxResult<Vec<(String, String, i64)>>;

    /// Delete deprecated entries older than N days + auto-deprecate confidence < 0.1.
    async fn cleanup_knowledge(&self, older_than_days: i64) -> OxResult<u64>;
}

// ---------------------------------------------------------------------------
// LoadCheckpointStore — watermark-based incremental load state
// ---------------------------------------------------------------------------

#[async_trait]
pub trait LoadCheckpointStore: Send + Sync {
    /// Get the latest checkpoint for a specific (project, source_table, graph_label) combination.
    async fn get_checkpoint(
        &self,
        project_id: Uuid,
        source_table: &str,
        graph_label: &str,
    ) -> OxResult<Option<LoadCheckpoint>>;

    /// Create or update a checkpoint (matched by project + source_table + graph_label).
    async fn upsert_checkpoint(&self, checkpoint: &LoadCheckpoint) -> OxResult<()>;

    /// List all checkpoints for a project.
    async fn list_checkpoints(&self, project_id: Uuid) -> OxResult<Vec<LoadCheckpoint>>;

    /// Delete a specific checkpoint (forces a full reload on next run).
    async fn delete_checkpoint(&self, id: Uuid) -> OxResult<bool>;
}

// ---------------------------------------------------------------------------
// ApiKeyStore — DB-backed API key management for programmatic access
// ---------------------------------------------------------------------------

#[async_trait]
pub trait ApiKeyStore: Send + Sync {
    /// Create a new API key with a server-generated plaintext. The
    /// plaintext is returned to the caller exactly once; only the
    /// SHA-256 hash is persisted.
    ///
    /// `role` must be one of `admin`, `designer`, or `viewer` — the DB
    /// CHECK constraint rejects any other value. Prefer `viewer` as the
    /// default for automation keys and escalate deliberately.
    async fn create_api_key(
        &self,
        label: &str,
        workspace_id: Option<Uuid>,
        created_by: &str,
        role: &str,
    ) -> OxResult<(ApiKey, String)>;

    /// Insert an API key whose hash is already computed by the caller.
    /// Used by first-boot bootstrap (operator supplies the plaintext via
    /// `OX_AUTH__BOOTSTRAP_KEY`) — the operator must already know the
    /// plaintext so the server cannot return a generated one.
    async fn insert_api_key(
        &self,
        label: &str,
        workspace_id: Option<Uuid>,
        created_by: &str,
        key_hash: &[u8],
        role: &str,
    ) -> OxResult<ApiKey>;

    /// Look up an API key by SHA-256 hash. Returns `None` if the key is
    /// unknown OR has been revoked.
    async fn find_api_key_by_hash(&self, hash: &[u8]) -> OxResult<Option<ApiKey>>;

    /// List all non-revoked API keys (admin view).
    async fn list_api_keys(&self) -> OxResult<Vec<ApiKey>>;

    /// Mark an API key as revoked. Returns `true` if a row was updated.
    /// Uses the `update_*` verb per the Store naming convention; the
    /// "revoked" qualifier is in the suffix to keep the verb prefix
    /// stable for `find`/`update`/`delete` greppability.
    async fn update_api_key_revoked(&self, id: Uuid) -> OxResult<bool>;
}

// ---------------------------------------------------------------------------
// NotificationStore — notification channels and delivery log
// ---------------------------------------------------------------------------

#[async_trait]
pub trait NotificationStore: Send + Sync {
    async fn create_notification_channel(&self, ch: &NotificationChannel) -> OxResult<()>;
    async fn get_notification_channel(&self, id: Uuid) -> OxResult<Option<NotificationChannel>>;
    async fn list_notification_channels(&self) -> OxResult<Vec<NotificationChannel>>;
    async fn update_notification_channel(
        &self,
        id: Uuid,
        name: Option<&str>,
        config: Option<&serde_json::Value>,
        events: Option<&[String]>,
        enabled: Option<bool>,
    ) -> OxResult<()>;
    async fn delete_notification_channel(&self, id: Uuid) -> OxResult<bool>;

    /// Find channels that subscribe to a given event type and are enabled.
    async fn list_channels_for_event(&self, event_type: &str)
    -> OxResult<Vec<NotificationChannel>>;

    async fn create_notification_log(&self, log: &NotificationLog) -> OxResult<()>;
    async fn list_notification_logs(&self, limit: i64) -> OxResult<Vec<NotificationLog>>;
}

// ---------------------------------------------------------------------------
// DataSourceStore — federation (VOL) adapter configurations
//
// One row per registered `source_id` the planner can resolve at
// query time. Workspace-scoped via RLS — every CRUD below runs
// through the workspace context set on the pool.
//
// `upsert_data_source_by_source_id` is the method the admin HTTP
// endpoint calls: register-or-replace semantics on the
// (workspace_id, source_id) natural key.
// ---------------------------------------------------------------------------

#[async_trait]
pub trait DataSourceStore: Send + Sync {
    async fn create_data_source(&self, item: &crate::models::DataSource) -> OxResult<()>;

    async fn get_data_source(&self, id: Uuid) -> OxResult<Option<crate::models::DataSource>>;

    async fn find_data_source_by_source_id(
        &self,
        source_id: &str,
    ) -> OxResult<Option<crate::models::DataSource>>;

    async fn list_data_sources(&self) -> OxResult<Vec<crate::models::DataSource>>;

    async fn upsert_data_source_by_source_id(
        &self,
        source_id: &str,
        kind: &str,
        config: &serde_json::Value,
    ) -> OxResult<crate::models::DataSource>;

    async fn delete_data_source_by_source_id(&self, source_id: &str) -> OxResult<bool>;

    /// Persist the most-recent introspection result for a source.
    /// Stores the full `AnalysisResult` (schema + profile + warnings)
    /// alongside the per-table fingerprint map so subsequent re-scans
    /// can compute a delta without describing every table again.
    /// `last_analyzed_at` is set to `now()` on the server side.
    ///
    /// Returns the updated row. Errors when the source_id is unknown
    /// in the current workspace (RLS-scoped).
    async fn update_data_source_analysis(
        &self,
        source_id: &str,
        snapshot: &serde_json::Value,
        fingerprints: &serde_json::Value,
    ) -> OxResult<crate::models::DataSource>;
}

// ---------------------------------------------------------------------------
// Store — super-trait combining all sub-traits
// ---------------------------------------------------------------------------

/// Signal log + aggregation for the "6 창" ontology-quality
/// dashboard. Sees every successful query (fire-and-forget) and
/// rolls the log into window-scoped metrics on demand.
#[async_trait]
pub trait QualitySignalStore: Send + Sync {
    /// Append a single query's signal row. Fire-and-forget —
    /// callers spawn this off the hot path and log write errors
    /// instead of propagating them.
    async fn create_query_execution_signal(
        &self,
        signal: &crate::quality_signal::QueryExecutionSignal,
    ) -> OxResult<()>;

    /// Aggregate the six dashboard metrics for the current
    /// workspace over `window`. Returns Wilson-score bands plus
    /// trend deltas against the immediately-previous window of the
    /// same length.
    async fn aggregate_quality_metrics(
        &self,
        window: crate::quality_signal::MetricWindow,
    ) -> OxResult<crate::quality_signal::QualityMetricsReport>;

    /// Grouped SHACL-failure distribution for the "실패 유형 분포"
    /// chart over `window`. Returns one row per observed
    /// `ShaclFailureKind`, zero rows when no failures recorded.
    async fn list_shacl_failure_distribution(
        &self,
        window: crate::quality_signal::MetricWindow,
    ) -> OxResult<Vec<crate::quality_signal::ShaclFailureCount>>;

    /// Upsert "last used" timestamps + rolling 7/30-day counts for
    /// every type in `type_ids`. Called from the signal write path
    /// so the stale scan doesn't have to rescan signal history.
    async fn upsert_type_last_used(
        &self,
        type_ids: &[(uuid::Uuid, &str)],
    ) -> OxResult<()>;

    /// List types whose `last_used_at` is older than
    /// `stale_after_days` for the current workspace, sorted by
    /// `last_used_at` ascending (staleest first).
    async fn list_stale_types(
        &self,
        stale_after_days: i64,
    ) -> OxResult<Vec<crate::quality_signal::StaleTypeEntry>>;
}

/// Workspace-level quality-metric baselines for adaptive alert
/// thresholds. Populated nightly by the `quality_baseline` cron
/// from `QualitySignalStore::aggregate_quality_metrics` rollups;
/// the banner consults it at render time when Phase B wires the
/// adaptive path — until then the table accumulates data so Phase
/// B has a real warm-up window to validate against.
#[async_trait]
pub trait QualityBaselineStore: Send + Sync {
    /// Upsert the current-workspace baseline row. Cron calls this
    /// once per workspace per day; upsert-in-place means consumers
    /// always read the latest snapshot without a window-picking
    /// predicate.
    async fn upsert_quality_baseline(
        &self,
        baseline: &crate::quality_signal::WorkspaceQualityBaseline,
    ) -> OxResult<()>;

    /// Fetch the current-workspace baseline, if any. `None` means
    /// the cron hasn't run yet (fresh workspace / first boot);
    /// the banner falls back to its hardcoded prior in that case.
    async fn get_quality_baseline(
        &self,
    ) -> OxResult<Option<crate::quality_signal::WorkspaceQualityBaseline>>;
}

/// Durable stale-concept deprecation proposals. Populated by the
/// `scan_stale_concepts` cron; admins decide approve / dismiss.
#[async_trait]
pub trait StaleConceptProposalStore: Send + Sync {
    /// List proposals visible to the current workspace. When
    /// `pending_only` is true, terminal (approved/dismissed) rows
    /// are excluded — the admin dashboard hot path.
    async fn list_stale_concept_proposals(
        &self,
        pending_only: bool,
    ) -> OxResult<Vec<crate::quality_signal::StaleConceptProposal>>;

    /// Get a single proposal by id. Returns `None` when not found
    /// (RLS-scoped — a cross-workspace id looks like "not found").
    async fn get_stale_concept_proposal(
        &self,
        id: uuid::Uuid,
    ) -> OxResult<Option<crate::quality_signal::StaleConceptProposal>>;

    /// Insert if not present (natural key = `(workspace_id, type_id)`).
    /// Cron calls this per stale hit; duplicates are silently no-ops.
    /// Returns the resulting row (newly inserted OR the existing one).
    async fn upsert_stale_concept_proposal(
        &self,
        proposal: crate::quality_signal::StaleConceptProposal,
    ) -> OxResult<crate::quality_signal::StaleConceptProposal>;

    /// Record an admin decision on a pending proposal. Noop when
    /// the proposal is already in a terminal state (returns the
    /// existing row).
    async fn record_stale_proposal_decision(
        &self,
        id: uuid::Uuid,
        decision: crate::quality_signal::StaleProposalDecision,
        decided_by_user_id: Option<uuid::Uuid>,
        reason: Option<String>,
    ) -> OxResult<crate::quality_signal::StaleConceptProposal>;
}

/// Change-type routing rules — one per `(workspace_id?, change_type)`.
/// Global defaults live with `workspace_id IS NULL` and are seeded
/// by the migration. Workspace overrides go through
/// [`ChangeRoutingStore::upsert_change_routing_rule`]; the resolve
/// path returns the higher-priority row (workspace override > global).
///
/// The `change_*` prefix on every method disambiguates from
/// [`ModelConfigStore`]'s `list_routing_rules` (LLM model routing —
/// a different concept routing between providers).
#[async_trait]
pub trait ChangeRoutingStore: Send + Sync {
    /// List every rule visible to the current workspace (global +
    /// overrides), ordered by `change_type` then `priority DESC`.
    /// Used by the admin UI to render the full routing table.
    async fn list_change_routing_rules(
        &self,
    ) -> OxResult<Vec<ox_ontology::change_routing::ChangeRoutingRule>>;

    /// Resolve the single active rule for `change_type`. Workspace
    /// override wins over global default via higher `priority`.
    /// Returns `None` when no rule matches — caller treats that as
    /// "require approval" by policy, not "silently auto-apply".
    async fn resolve_change_routing(
        &self,
        change_type: ox_ontology::change_routing::ChangeType,
    ) -> OxResult<Option<ox_ontology::change_routing::ChangeRoutingRule>>;

    /// Upsert a workspace override. Natural key is
    /// `(workspace_id, change_type)` so a workspace has at most one
    /// override per change type. The store fills `workspace_id` from
    /// `app.workspace_id` — callers don't pass it (a caller writing
    /// global defaults uses the SYSTEM_BYPASS path at migration time).
    async fn upsert_change_routing_rule(
        &self,
        rule: ox_ontology::change_routing::ChangeRoutingRule,
    ) -> OxResult<ox_ontology::change_routing::ChangeRoutingRule>;

    /// Drop a workspace override, reverting to the global default.
    /// Returns `true` when a row was deleted.
    async fn delete_change_routing_rule(
        &self,
        change_type: ox_ontology::change_routing::ChangeType,
    ) -> OxResult<bool>;
}

/// Closed-loop ambiguity resolver storage.
///
/// Context rows are detected during source analysis and upserted by
/// natural key `(source_id, relation, column)`. Resolutions append to
/// a history log; at most one non-revoked resolution is active per
/// context (DB-enforced by partial unique index). Superseding a
/// resolution revokes the previous active row *and* writes the new
/// row in the same transaction — the store impl is responsible for
/// the atomicity.
#[async_trait]
pub trait AmbiguityStore: Send + Sync {
    async fn list_ambiguity_contexts(
        &self,
        source_id: &ox_ontology::mapping::refs::SourceId,
    ) -> OxResult<Vec<ox_ontology::ambiguity::AmbiguityContext>>;

    /// List every context visible in the current workspace (RLS
    /// bounded). Backs the admin `/settings/ambiguity` dashboard,
    /// which can't scope by source_id because it shows all pending
    /// ambiguities across data sources at once.
    async fn list_ambiguity_contexts_in_workspace(
        &self,
    ) -> OxResult<Vec<ox_ontology::ambiguity::AmbiguityContext>>;

    async fn get_ambiguity_context(
        &self,
        id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<Option<ox_ontology::ambiguity::AmbiguityContext>>;

    async fn find_ambiguity_context_by_column(
        &self,
        source_id: &ox_ontology::mapping::refs::SourceId,
        column: &ox_ontology::mapping::refs::ColumnRef,
    ) -> OxResult<Option<ox_ontology::ambiguity::AmbiguityContext>>;

    /// Upsert by natural key. Replaces the row when
    /// `(source_id, relation, column)` already exists — the refresh
    /// path for re-running analysis against a changed schema.
    async fn upsert_ambiguity_context(
        &self,
        context: ox_ontology::ambiguity::AmbiguityContext,
    ) -> OxResult<ox_ontology::ambiguity::AmbiguityContext>;

    async fn delete_ambiguity_context(
        &self,
        id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<bool>;

    async fn list_ambiguity_resolutions(
        &self,
        context_id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<Vec<ox_ontology::ambiguity::AmbiguityResolution>>;

    async fn get_active_ambiguity_resolution(
        &self,
        source_id: &ox_ontology::mapping::refs::SourceId,
        column: &ox_ontology::mapping::refs::ColumnRef,
    ) -> OxResult<Option<ox_ontology::ambiguity::AmbiguityResolution>>;

    /// Atomically revoke the prior active resolution (if any) and
    /// record the new resolution as active. `supersedes` on the new
    /// row points at the revoked one. Returns the inserted row.
    async fn create_ambiguity_resolution(
        &self,
        resolution: ox_ontology::ambiguity::AmbiguityResolution,
    ) -> OxResult<ox_ontology::ambiguity::AmbiguityResolution>;

    /// Revoke the currently-active resolution for `context_id`, if any.
    /// Returns `true` when a row transitioned to revoked.
    async fn revoke_active_ambiguity_resolution(
        &self,
        context_id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<bool>;
}

pub trait Store:
    QueryStore
    + PinStore
    + ProjectStore
    + PerspectiveStore
    + ConfigStore
    + UserStore
    + RecipeStore
    + DashboardStore
    + AnalysisResultStore
    + ScheduledTaskStore
    + ReportStore
    + PatternStore
    + PromptTemplateStore
    + AgentSessionStore
    + EmbeddingRetryStore
    + VerificationStore
    + ToolApprovalStore
    + WorkspaceStore
    + AuditStore
    + MeteringStore
    + LineageStore
    + ApprovalStore
    + ApprovalCommentStore
    + AuditTrailStore
    + QualityStore
    + AclStore
    + ModelConfigStore
    + KnowledgeStore
    + LoadCheckpointStore
    + HealthStore
    + NotificationStore
    + ApiKeyStore
    + DataSourceStore
    + OntologyVersionStore
    + OntologyNavigationStore
    + AmbiguityStore
    + ChangeRoutingStore
    + QualitySignalStore
    + QualityBaselineStore
    + StaleConceptProposalStore
{
}

impl<T> Store for T where
    T: QueryStore
        + PinStore
        + ProjectStore
        + PerspectiveStore
        + ConfigStore
        + UserStore
        + RecipeStore
        + DashboardStore
        + AnalysisResultStore
        + ScheduledTaskStore
        + ReportStore
        + PatternStore
        + PromptTemplateStore
        + AgentSessionStore
        + EmbeddingRetryStore
        + VerificationStore
        + ToolApprovalStore
        + WorkspaceStore
        + AuditStore
        + MeteringStore
        + LineageStore
        + ApprovalStore
        + ApprovalCommentStore
        + AuditTrailStore
        + QualityStore
        + AclStore
        + ModelConfigStore
        + KnowledgeStore
        + LoadCheckpointStore
        + HealthStore
        + NotificationStore
        + ApiKeyStore
        + DataSourceStore
        + OntologyVersionStore
        + OntologyNavigationStore
        + AmbiguityStore
        + ChangeRoutingStore
        + QualitySignalStore
        + QualityBaselineStore
        + StaleConceptProposalStore
{
}
