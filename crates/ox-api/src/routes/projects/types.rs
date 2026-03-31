use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_core::ontology_command::OntologyCommand;
use ox_core::ontology_ir::OntologyIR;
use ox_core::source_analysis::DesignOptions;
use ox_store::DesignProject;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateProjectRequest {
    pub title: Option<String>,
    /// Project origin: source analysis or base ontology.
    #[serde(flatten)]
    #[schema(value_type = Object)]
    pub origin: ProjectOrigin,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(tag = "origin_type", rename_all = "snake_case")]
pub enum ProjectOrigin {
    Source {
        source: ProjectSource,
        #[serde(default)]
        #[schema(value_type = Option<Object>)]
        repo_source: Option<ox_core::repo_insights::RepoSource>,
    },
    BaseOntology {
        base_ontology_id: Uuid,
    },
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProjectSource {
    Text {
        data: String,
    },
    Csv {
        data: String,
    },
    Json {
        data: String,
    },
    Postgresql {
        connection_string: String,
        #[serde(default = "default_pg_schema")]
        schema: String,
    },
    Mysql {
        connection_string: String,
        /// MySQL "schema" is the database name
        schema: String,
    },
    Mongodb {
        connection_string: String,
        /// MongoDB database name
        database: String,
    },
    CodeRepository {
        url: String,
    },
}

fn default_pg_schema() -> String {
    "public".to_string()
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateDecisionsRequest {
    /// User design decisions.
    #[schema(value_type = Object)]
    pub design_options: DesignOptions,
    pub revision: i32,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ProjectDesignRequest {
    pub revision: i32,
    /// Domain hints for the LLM.
    #[serde(default)]
    pub context: String,
    /// Must be set to true if the schema has >100 tables and the user wants to proceed anyway.
    #[serde(default)]
    pub acknowledge_large_schema: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProjectDesignResponse {
    #[schema(value_type = Object)]
    pub project: DesignProject,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ProjectReanalyzeRequest {
    /// Data source to re-analyze (must match original source type).
    pub source: ProjectSource,
    pub revision: i32,
    /// Optional repository source for enrichment.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub repo_source: Option<ox_core::repo_insights::RepoSource>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProjectReanalyzeResponse {
    #[schema(value_type = Object)]
    pub project: DesignProject,
    /// Design decisions that were invalidated by the schema change.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub invalidated_decisions: Vec<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ProjectRefineRequest {
    pub revision: i32,
    /// Additional context for the LLM refinement.
    pub additional_context: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProjectRefineResponse {
    #[schema(value_type = Object)]
    pub project: DesignProject,
    /// Summary of graph profiling results.
    pub profile_summary: String,
    /// Report on ID reconciliation between original and refined ontology.
    #[schema(value_type = Object)]
    pub reconcile_report: ox_core::ReconcileReport,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ProjectReconcileRequest {
    pub revision: i32,
    /// Reconciled ontology with user decisions applied.
    #[schema(value_type = Object)]
    pub reconciled_ontology: OntologyIR,
    /// User accept/reject decisions for uncertain matches.
    #[schema(value_type = Vec<Object>)]
    pub decisions: Vec<ox_core::MatchDecision>,
    /// The uncertain matches being decided upon.
    #[schema(value_type = Vec<Object>)]
    pub uncertain_matches: Vec<ox_core::UncertainMatch>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ProjectExtendRequest {
    pub revision: i32,
    /// New data source to merge into the project.
    pub source: ProjectSource,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProjectExtendResponse {
    #[schema(value_type = Object)]
    pub project: DesignProject,
    /// Report on ID reconciliation between existing and new ontology entities.
    #[schema(value_type = Object)]
    pub reconcile_report: ox_core::ReconcileReport,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ProjectCompleteRequest {
    pub revision: i32,
    /// Name for the saved ontology.
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Must be set to true if the quality report has low confidence or high-severity gaps.
    /// Prevents accidental promotion of low-quality ontologies.
    #[serde(default)]
    pub acknowledge_quality_risks: bool,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ProjectEditRequest {
    pub revision: i32,
    /// Natural language description of the desired ontology change.
    pub user_request: String,
    /// If true, returns generated commands without applying them.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProjectEditResponse {
    /// Updated project (null in dry_run mode).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub project: Option<DesignProject>,
    /// Generated ontology mutation commands.
    #[schema(value_type = Vec<Object>)]
    pub commands: Vec<OntologyCommand>,
    /// LLM explanation of what was changed and why.
    pub explanation: String,
}
