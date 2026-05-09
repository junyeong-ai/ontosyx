use serde::{Deserialize, Serialize};

use ox_ontology::command::OntologyCommand;
use ox_ontology::design_gate::{DesignGate, evaluate_design_gates};
use ox_ontology::ir::OntologyIR;
use ox_ontology::source_analysis::{DesignOptions, SourceAnalysisReport};
use ox_source::AnalyzeSelection;
use ox_store::OntologyDraft;

/// Wire shape for any endpoint that returns a project. Carries the
/// underlying [`OntologyDraft`] flattened (so existing fields stay
/// at the top level) plus the server-evaluated [`Vec<DesignGate>`]
/// the FE renders alongside the disabled-design-button checklist.
///
/// Computing gates server-side keeps the FE from reimplementing the
/// gate-evaluation rules and guarantees the rendering matches what
/// the design endpoint will accept.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OntologyDraftView {
    #[serde(flatten)]
    #[schema(value_type = ox_store::OntologyDraft)]
    pub project: OntologyDraft,
    pub design_gates: Vec<DesignGate>,
    /// Status of the persisted `analysis_report` blob against the
    /// current wire shape. The FE renders a soft banner when
    /// `Stale` so the operator knows gate enforcement was skipped
    /// even though design proceeded — re-running analyse refreshes
    /// the report and the gates start enforcing again.
    pub analysis_report_status: AnalysisReportStatus,
}

/// Health of the persisted analysis-report row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisReportStatus {
    /// No report — typical for `BaseOntology`-origin projects.
    Missing,
    /// Persisted report deserialises against the current wire
    /// shape. Gate enforcement is fully active.
    Current,
    /// Persisted report exists but cannot be deserialised — the row
    /// was written under an older schema. Gates are skipped; the FE
    /// surfaces a "재분석을 권장합니다" banner.
    Stale,
}

impl OntologyDraftView {
    /// Wrap a project, computing gates from the persisted
    /// `analysis_report` + `design_options`. Drafts without an
    /// analysis report (still being analysed, or sourced from an
    /// existing ontology) get an empty gate vector — there is
    /// nothing to gate yet.
    pub fn from_ontology_draft(project: OntologyDraft) -> Self {
        let (gates, analysis_report_status) = derive_gate_state(&project);
        Self {
            project,
            design_gates: gates,
            analysis_report_status,
        }
    }
}

fn derive_gate_state(project: &OntologyDraft) -> (Vec<DesignGate>, AnalysisReportStatus) {
    let Some(value) = project.analysis_report.as_ref() else {
        return (Vec::new(), AnalysisReportStatus::Missing);
    };
    match serde_json::from_value::<SourceAnalysisReport>(value.clone()) {
        Ok(report) => {
            let options: DesignOptions =
                serde_json::from_value(project.design_options.clone()).unwrap_or_default();
            (
                evaluate_design_gates(&report, &options),
                AnalysisReportStatus::Current,
            )
        }
        Err(_) => (Vec::new(), AnalysisReportStatus::Stale),
    }
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateOntologyDraftRequest {
    pub title: Option<String>,
    /// Ontology draft origin: source analysis or base ontology.
    #[serde(flatten)]
    pub origin: OntologyDraftOrigin,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(tag = "origin_type", rename_all = "snake_case")]
// One-shot request DTO; the variant-size disparity is irrelevant
// against a single deserialise per HTTP call.
#[allow(clippy::large_enum_variant)]
pub enum OntologyDraftOrigin {
    Source {
        source: DataSourceSpec,
        #[serde(default)]
        repo_source: Option<ox_ontology::repo_insights::RepoSource>,
        /// Which tables of the source to introspect. The caller picks
        /// `{"kind": "all"}` deliberately or names a `subset` /
        /// `extend` list — there is no implicit full-warehouse sweep.
        selection: AnalyzeSelection,
    },
    /// Seed the project from the workspace's canonical ontology.
    /// Workspace × ontology is 1:1 — no id needed. The server
    /// resolves the current version and hydrates its IR into the
    /// new project.
    BaseOntology,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[schema(as = OntologyDraftDataSourceSpec)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DataSourceSpec {
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
    Snowflake {
        /// Snowflake account identifier (e.g., `xy12345.us-east-1`)
        account: String,
        /// Login username
        user: String,
        /// Login password
        password: String,
        /// Compute warehouse name
        #[serde(default)]
        warehouse: String,
        /// Target database
        database: String,
        /// Target schema within the database
        #[serde(default = "default_snowflake_schema")]
        schema: String,
    },
    Bigquery {
        /// Data project — the GCP project that owns the dataset. Used
        /// to fully-qualify identifiers in `INFORMATION_SCHEMA` queries.
        project_id: String,
        /// BigQuery dataset name.
        dataset: String,
        /// Project that runs and is billed for the BigQuery jobs.
        /// Defaults to `project_id`. Required when the caller has
        /// `bigquery.tables.list` on the data project but lacks
        /// `bigquery.jobs.create` there (typical for shared data
        /// projects), or when a VPC Service Controls perimeter forces
        /// jobs to run from a specific project.
        #[serde(default)]
        billing_project_id: Option<String>,
        /// Optional path to a credentials file. Accepts either a
        /// service-account JSON key or an authorized-user secret
        /// (the file `gcloud auth application-default login` writes).
        /// Falls back to ADC chain (`GOOGLE_APPLICATION_CREDENTIALS`,
        /// gcloud default file, workload-identity metadata server)
        /// when omitted.
        #[serde(default)]
        credentials_path: Option<String>,
    },
    /// DuckDB in-process file analysis (Parquet, CSV, JSON).
    /// The `file_path` must be an absolute path to a local file.
    Duckdb {
        file_path: String,
    },
    CodeRepository {
        url: String,
    },
}

fn default_pg_schema() -> String {
    "public".to_string()
}

fn default_snowflake_schema() -> String {
    "PUBLIC".to_string()
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateOntologyDraftDecisionsRequest {
    /// User design decisions.
    pub design_options: DesignOptions,
    pub revision: i32,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct DesignOntologyDraftRequest {
    pub revision: i32,
    /// Domain hints for the LLM.
    #[serde(default)]
    pub context: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct DesignOntologyDraftResponse {
    pub project: OntologyDraftView,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReanalyzeOntologyDraftRequest {
    /// Data source to re-analyze (must match original source type).
    pub source: DataSourceSpec,
    pub revision: i32,
    /// Optional repository source for enrichment.
    #[serde(default)]
    pub repo_source: Option<ox_ontology::repo_insights::RepoSource>,
    /// Which tables of the source to introspect on this re-analysis.
    /// Required and explicit — `kind: "all"` for a full sweep,
    /// `kind: "subset"` to narrow.
    pub selection: AnalyzeSelection,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReanalyzeOntologyDraftResponse {
    pub project: OntologyDraftView,
    /// Design decisions that were invalidated by the schema change.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub invalidated_decisions: Vec<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RefineOntologyDraftRequest {
    pub revision: i32,
    /// Additional context for the LLM refinement.
    pub additional_context: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RefineOntologyDraftResponse {
    pub project: OntologyDraftView,
    /// Summary of graph profiling results.
    pub profile_summary: String,
    /// Report on ID reconciliation between original and refined ontology.
    pub reconcile_report: ox_ontology::ReconcileReport,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReconcileOntologyDraftRequest {
    pub revision: i32,
    /// Reconciled ontology with user decisions applied.
    pub reconciled_ontology: OntologyIR,
    /// User accept/reject decisions for uncertain matches.
    pub decisions: Vec<ox_ontology::MatchDecision>,
    /// The uncertain matches being decided upon.
    pub uncertain_matches: Vec<ox_ontology::UncertainMatch>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ExtendOntologyDraftRequest {
    pub revision: i32,
    /// New data source to merge into the project.
    pub source: DataSourceSpec,
    /// Which tables of the new source to introspect. Required —
    /// `kind: "all"` to take everything advertised, `kind: "subset"`
    /// for a curated list, `kind: "extend"` to grow the existing
    /// baseline with the named tables.
    pub selection: AnalyzeSelection,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ExtendOntologyDraftResponse {
    pub project: OntologyDraftView,
    /// Report on ID reconciliation between existing and new ontology entities.
    pub reconcile_report: ox_ontology::ReconcileReport,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CompleteOntologyDraftRequest {
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
pub struct EditOntologyDraftRequest {
    pub revision: i32,
    /// Natural language description of the desired ontology change.
    pub user_request: String,
    /// If true, returns generated commands without applying them.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct EditOntologyDraftResponse {
    /// Updated project (null in dry_run mode).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<OntologyDraftView>,
    /// Generated ontology mutation commands.
    pub commands: Vec<OntologyCommand>,
    /// LLM explanation of what was changed and why.
    pub explanation: String,
}
