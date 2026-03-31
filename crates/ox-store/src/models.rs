use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Domain models for app state persistence
// ---------------------------------------------------------------------------

/// A single query execution record: NL question → QueryIR → compiled → results.
///
/// Ontology reproducibility: when `saved_ontology_id` is set, the ontology is
/// resolved via JOIN to `saved_ontologies` (no inline snapshot duplication).
/// Draft/unsaved ontology executions store `ontology_snapshot` inline.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QueryExecution {
    pub id: Uuid,
    pub user_id: String,
    pub question: String,
    /// Ontology identifier at execution time
    pub ontology_id: String,
    pub ontology_version: i32,
    /// FK to saved_ontologies — when set, ontology_snapshot is NULL (resolved via JOIN)
    pub saved_ontology_id: Option<Uuid>,
    /// Full OntologyIR snapshot (NULL when saved_ontology_id is set)
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
    pub ontology_id: String,
    pub ontology_version: i32,
    pub compiled_target: String,
    pub model: String,
    pub execution_time_ms: i64,
    pub row_count: i64,
    pub has_widget: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SavedOntology {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub version: i32,
    pub ontology_ir: serde_json::Value,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

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
    /// SourceMapping (node→table, property→column links)
    pub source_mapping: Option<serde_json::Value>,
    /// Generated OntologyIR
    pub ontology: Option<serde_json::Value>,
    /// OntologyQualityReport
    pub quality_report: Option<serde_json::Value>,
    /// Links to saved_ontologies after completion
    pub saved_ontology_id: Option<Uuid>,
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
    pub saved_ontology_id: Option<Uuid>,
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
    pub ontology: serde_json::Value,
    pub source_mapping: Option<serde_json::Value>,
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
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub layout: serde_json::Value,
    pub is_public: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// AnalysisRecipe — reusable data analysis algorithm
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AnalysisRecipe {
    pub id: Uuid,
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
    pub ontology_id: Option<String>,
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
    pub ontology_id: Option<String>,
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
    pub ontology_id: String,
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
    pub ontology_id: Option<String>,
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
    pub version: String,
    pub content: String,
    pub variables: serde_json::Value,
    pub metadata: serde_json::Value,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub is_active: bool,
}

// ---------------------------------------------------------------------------
// Element verifications — per-element verification tracking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ElementVerification {
    pub id: Uuid,
    pub ontology_id: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub workspace_id: Uuid,
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

// ---------------------------------------------------------------------------
// Data Quality — declarative quality rules with evaluation results
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QualityRule {
    pub id: Uuid,
    pub workspace_id: Uuid,
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
