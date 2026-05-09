// ---------------------------------------------------------------------------
// Design project types — project lifecycle, analysis, source introspection
// ---------------------------------------------------------------------------

import type { components } from "./api.generated";
import type { OntologyCommand, OntologyIR } from "./ontology";
import type { ClientPage } from "./pagination";

// --- Ontology Drafts ---

export type DesignSource = components["schemas"]["OntologyDraftDataSourceSpec"];

// --- Ontology Draft action wire shapes ---

export type GateId = components["schemas"]["GateId"];
export type GateStatus = components["schemas"]["GateStatus"];
export type DesignGate = components["schemas"]["DesignGate"];
export type OntologyDraftStatus = "analyzed" | "designed" | "completed";
export type SourceTypeKind = components["schemas"]["SourceTypeKind"];
export type SourceConfig = components["schemas"]["SourceConfig"];
export type SourceHistoryEntry = components["schemas"]["SourceHistoryEntry"];
export type OntologyDraft = Omit<
  components["schemas"]["OntologyDraftView"],
  "analysis_report" | "ontology" | "status"
> & {
  analysis_report?: SourceAnalysisReport | null;
  ontology?: OntologyIR | null;
  status: OntologyDraftStatus;
};
export type AnalysisReportStatus = components["schemas"]["AnalysisReportStatus"];
export type OntologyDraftSummary = Omit<
  components["schemas"]["OntologyDraftSummary"],
  "status"
> & { status: OntologyDraftStatus };
export type OntologyDraftSummaryPage = ClientPage<
  Omit<components["schemas"]["OntologyDraftSummaryPage"], "items"> & {
  items: OntologyDraftSummary[];
  }
>;
export type DataSourceSpec = components["schemas"]["OntologyDraftDataSourceSpec"];
export type AnalyzeSelection = components["schemas"]["AnalyzeSelection"];
export type AnalysisScope = components["schemas"]["AnalysisScope"];
export type DeferredTable = components["schemas"]["DeferredTable"];
export type RepoSource = components["schemas"]["RepoSource"];
export type CreateOntologyDraftRequest = components["schemas"]["CreateOntologyDraftRequest"];
export type UpdateOntologyDraftDecisionsRequest =
  components["schemas"]["UpdateOntologyDraftDecisionsRequest"];
export type DesignOntologyDraftRequest =
  components["schemas"]["DesignOntologyDraftRequest"];
export type DesignOntologyDraftResponse = Omit<
  components["schemas"]["DesignOntologyDraftResponse"],
  "project"
> & { project: OntologyDraft };
export type ReanalyzeOntologyDraftRequest =
  components["schemas"]["ReanalyzeOntologyDraftRequest"];
export type ReanalyzeOntologyDraftResponse = Omit<
  components["schemas"]["ReanalyzeOntologyDraftResponse"],
  "project"
> & { project: OntologyDraft };
export type ReanalyzeModeledOntologyDraftRequest =
  components["schemas"]["ReanalyzeModeledOntologyDraftRequest"];
export type RefineOntologyDraftRequest =
  components["schemas"]["RefineOntologyDraftRequest"];
export type RefineOntologyDraftResponse = Omit<
  components["schemas"]["RefineOntologyDraftResponse"],
  "project"
> & { project: OntologyDraft };
export type EditOntologyDraftRequest =
  components["schemas"]["EditOntologyDraftRequest"];
export type EditOntologyDraftResponse = Omit<
  components["schemas"]["EditOntologyDraftResponse"],
  "commands" | "project"
> & {
  project: OntologyDraft | null;
  commands: OntologyCommand[];
};
export type ExtendOntologyDraftRequest =
  components["schemas"]["ExtendOntologyDraftRequest"];
export type ExtendOntologyDraftResponse = Omit<
  components["schemas"]["ExtendOntologyDraftResponse"],
  "project"
> & { project: OntologyDraft };

// --- Source preview (cheap table listing) ---

export type PreviewSourceRequest = components["schemas"]["PreviewSourceRequest"];
export type PreviewTableSummary = components["schemas"]["PreviewTableSummary"];
export type PreviewSourceResponse = components["schemas"]["PreviewSourceResponse"];
export type CompleteOntologyDraftRequest =
  components["schemas"]["CompleteOntologyDraftRequest"];

export type ConfirmedRelationship = components["schemas"]["ConfirmedRelationship"];
export type PiiKind = components["schemas"]["PiiKind"];
export type PiiAnnotation = components["schemas"]["PiiAnnotation"];
export type ExcludedColumn = components["schemas"]["ExcludedColumn"];
export type ColumnClarification = components["schemas"]["ColumnClarification"];
export type DesignOptions = components["schemas"]["DesignOptions"];

export type RepoColumnSuggestion = components["schemas"]["RepoColumnSuggestion"];
export type SchemaStats = components["schemas"]["SchemaStats"];
export type AnalysisCompleteness = components["schemas"]["AnalysisCompleteness"];
export type AnalysisPhase = components["schemas"]["AnalysisPhase"];
export type WarningClass = components["schemas"]["WarningClass"];
export type WarningLevel = components["schemas"]["WarningLevel"];
export type WarningScope = components["schemas"]["WarningScope"];
export type AnalysisWarning = components["schemas"]["AnalysisWarning"];
export type ImpliedFkPattern = components["schemas"]["ImpliedFkPattern"];
export type ImpliedRelationship = components["schemas"]["ImpliedRelationship"];
export type PiiSuggestion = components["schemas"]["PiiSuggestion"];
export type AmbiguityKind = components["schemas"]["AmbiguityKind"];
export type RepoHint = components["schemas"]["RepoHint"];
export type AmbiguityColumnRef = components["schemas"]["ColumnRef"];
export type AmbiguityContext = components["schemas"]["AmbiguityContext"];
export type TableExclusionReason = components["schemas"]["TableExclusionReason"];
export type TableExclusionSuggestion = components["schemas"]["TableExclusionSuggestion"];
export type LargeSchemaWarning = components["schemas"]["LargeSchemaWarning"];
export type RepoAnalysisStatus = components["schemas"]["RepoAnalysisStatus"];
export type RepoFailureKind = components["schemas"]["RepoFailureKind"];
export type FieldHint = components["schemas"]["FieldHint"];
export type RepoAnalysisSummary = components["schemas"]["RepoAnalysisSummary"];
export type SourceAnalysisReport = components["schemas"]["SourceAnalysisReport"];

// --- Source introspection (returned only for DB sources) ---

export type ColumnDef = components["schemas"]["SourceColumnDef"];
export type ForeignKeyDef = components["schemas"]["ForeignKeyDef"];
export type SourceTableDef = components["schemas"]["SourceTableDef"];
export type SourceSchema = components["schemas"]["SourceSchema"];
export type ColumnStats = components["schemas"]["ColumnStats"];
export type PiiSuspectKind = components["schemas"]["PiiSuspectKind"];
export type TableProfile = components["schemas"]["TableProfile"];
export type SourceProfile = components["schemas"]["SourceProfile"];

// --- Schema deploy / migration / load plan ---

export type DeployOntologyDraftSchemaRequest =
  components["schemas"]["DeployOntologyDraftSchemaRequest"];
export type DeployOntologyDraftSchemaResponse =
  components["schemas"]["DeployOntologyDraftSchemaResponse"];
export type MigrateOntologyDraftSchemaRequest =
  components["schemas"]["MigrateOntologyDraftSchemaRequest"];
export type MigrateOntologyDraftSchemaResponse =
  components["schemas"]["MigrateOntologyDraftSchemaResponse"];
export type GenerateOntologyDraftLoadPlanResponse =
  components["schemas"]["GenerateOntologyDraftLoadPlanResponse"];
export type LoadPlan = components["schemas"]["LoadPlan"];
export type LoadStep = components["schemas"]["LoadStep"];
export type CompileOntologyDraftLoadPlanRequest =
  components["schemas"]["CompileOntologyDraftLoadPlanRequest"];
export type CompileOntologyDraftLoadPlanResponse =
  components["schemas"]["CompileOntologyDraftLoadPlanResponse"];
export type ExecuteOntologyDraftLoadRequest =
  components["schemas"]["ExecuteOntologyDraftLoadRequest"];
export type ExecuteOntologyDraftLoadResponse =
  components["schemas"]["ExecuteOntologyDraftLoadResponse"];
