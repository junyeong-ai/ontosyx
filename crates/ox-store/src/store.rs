use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use chrono::{DateTime, Utc};
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
    pub ontology: serde_json::Value,
    pub source_mapping: serde_json::Value,
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
        user_id: &str,
        id: Uuid,
        feedback: Option<&str>,
    ) -> OxResult<bool>;
}

#[async_trait]
pub trait OntologyStore: Send + Sync {
    async fn get_saved_ontology(&self, id: Uuid) -> OxResult<Option<SavedOntology>>;

    async fn list_saved_ontologies(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<SavedOntology>>;

    async fn get_latest_ontology(&self, name: &str) -> OxResult<Option<SavedOntology>>;

    /// Save a standalone ontology (not tied to a design project).
    /// Used by Graph Adopt flow to persist adopted ontologies.
    async fn create_standalone_ontology(
        &self,
        name: &str,
        ontology_ir: &serde_json::Value,
    ) -> OxResult<Uuid>;
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
        source_mapping: Option<&serde_json::Value>,
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

    /// Atomically save the ontology and mark the project as completed.
    /// Uses a single transaction to prevent orphan ontologies.
    async fn complete_design_project(
        &self,
        project_id: Uuid,
        ontology: &SavedOntology,
        expected_revision: i32,
    ) -> OxResult<()>;

    async fn delete_design_project(&self, id: Uuid) -> OxResult<bool>;

    /// Archive WIP projects that haven't been updated within `max_age_days`.
    /// Returns the number of projects archived.
    async fn archive_stale_projects(&self, max_age_days: i64) -> OxResult<u64>;

    /// Permanently delete projects that have been archived for longer than `max_archive_days`.
    /// Returns the number of projects deleted.
    async fn delete_archived_projects(&self, max_archive_days: i64) -> OxResult<u64>;

    // --- Ontology Snapshots ---

    /// Create an ontology snapshot for a given project revision.
    /// Uses ON CONFLICT DO NOTHING for idempotency.
    async fn create_ontology_snapshot(
        &self,
        project_id: Uuid,
        revision: i32,
        ontology: &serde_json::Value,
        source_mapping: Option<&serde_json::Value>,
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
    async fn get_all_config(&self) -> OxResult<Vec<SystemConfigRow>>;

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

    async fn get_user_count(&self) -> OxResult<i64>;
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
        layout: &serde_json::Value,
        is_public: bool,
    ) -> OxResult<()>;
    async fn delete_dashboard(&self, id: Uuid) -> OxResult<bool>;

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
    /// Delete analysis results older than `max_age_days`. Returns count deleted.
    async fn cleanup_old_results(&self, max_age_days: i64) -> OxResult<u64>;
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
    async fn create_prompt_template(&self, row: &PromptTemplateRow) -> OxResult<()>;
    async fn update_prompt_template(
        &self,
        id: Uuid,
        content: &str,
        variables: &serde_json::Value,
        is_active: bool,
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
    async fn cleanup_old_sessions(&self, retention_days: i64) -> OxResult<u64>;
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
        ontology_id: &str,
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
// EmbeddingRetryStore — pending embedding retry queue
// ---------------------------------------------------------------------------

/// Queue for embedding operations that failed and need retry on the next periodic sweep.
#[async_trait]
pub trait EmbeddingRetryStore: Send + Sync {
    async fn enqueue_pending_embedding(
        &self,
        content: &str,
        metadata: &serde_json::Value,
    ) -> OxResult<()>;
    async fn list_pending_embeddings(&self, limit: i64) -> OxResult<Vec<PendingEmbedding>>;
    async fn mark_embedding_failed(&self, id: Uuid, error: &str) -> OxResult<()>;
    async fn delete_pending_embedding(&self, id: Uuid) -> OxResult<()>;
}

// ---------------------------------------------------------------------------
// VerificationStore — element-level verification tracking
// ---------------------------------------------------------------------------

#[async_trait]
pub trait VerificationStore: Send + Sync {
    async fn verify_element(&self, v: &ElementVerification) -> OxResult<Uuid>;
    async fn get_verifications(&self, ontology_id: &str) -> OxResult<Vec<ElementVerification>>;
    async fn invalidate_for_elements(
        &self,
        ontology_id: &str,
        element_ids: &[&str],
        reason: &str,
    ) -> OxResult<u64>;
    async fn delete_verification(
        &self,
        ontology_id: &str,
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
}

// ---------------------------------------------------------------------------
// AuditStore — append-only event log for enterprise governance
// ---------------------------------------------------------------------------

#[async_trait]
pub trait AuditStore: Send + Sync {
    /// Record an audit event (append-only).
    async fn record_audit(
        &self,
        user_id: Option<Uuid>,
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
    async fn get_lineage_for_label(&self, graph_label: &str) -> OxResult<Vec<LineageEntry>>;

    /// Get lineage entries for a project.
    async fn get_lineage_for_project(&self, project_id: Uuid) -> OxResult<Vec<LineageEntry>>;

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

    /// Approve or reject an approval request.
    async fn review_approval(
        &self,
        id: Uuid,
        reviewer_id: Uuid,
        approved: bool,
        notes: Option<&str>,
    ) -> OxResult<()>;

    /// Expire old pending approvals past their `expires_at`. Returns count expired.
    async fn expire_old_approvals(&self) -> OxResult<u64>;
}

// ---------------------------------------------------------------------------
// QualityStore — declarative data quality rules with evaluation
// ---------------------------------------------------------------------------

#[async_trait]
pub trait QualityStore: Send + Sync {
    async fn create_quality_rule(&self, rule: &QualityRule) -> OxResult<()>;
    async fn get_quality_rule(&self, id: Uuid) -> OxResult<Option<QualityRule>>;
    async fn list_quality_rules(&self, target_label: Option<&str>) -> OxResult<Vec<QualityRule>>;
    async fn update_quality_rule(
        &self,
        id: Uuid,
        name: &str,
        threshold: f64,
        is_active: bool,
    ) -> OxResult<()>;
    async fn delete_quality_rule(&self, id: Uuid) -> OxResult<bool>;
    async fn record_quality_result(&self, result: &QualityResult) -> OxResult<()>;
    async fn get_latest_results(&self, rule_id: Uuid, limit: i64) -> OxResult<Vec<QualityResult>>;
    async fn get_quality_dashboard(&self) -> OxResult<Vec<QualityDashboardEntry>>;
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
    async fn delete_model_config(&self, id: Uuid) -> OxResult<()>;

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
    async fn delete_routing_rule(&self, id: Uuid) -> OxResult<()>;

    /// Single optimized query: find the best model for an operation + workspace.
    /// Checks workspace-specific rules first, then global rules, then wildcard.
    async fn find_model_for_operation(
        &self,
        operation: &str,
        workspace_id: Option<Uuid>,
    ) -> OxResult<Option<ModelConfig>>;
}

// ---------------------------------------------------------------------------
// Store — super-trait combining all sub-traits
// ---------------------------------------------------------------------------

pub trait Store:
    QueryStore
    + OntologyStore
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
    + QualityStore
    + AclStore
    + ModelConfigStore
    + HealthStore
{
}

impl<T> Store for T where
    T: QueryStore
        + OntologyStore
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
        + QualityStore
        + AclStore
        + ModelConfigStore
        + HealthStore
{
}
