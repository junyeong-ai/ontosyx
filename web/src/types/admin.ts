// ---------------------------------------------------------------------------
// Admin/system types — config, users, prompts, sessions, recipes, reports
// ---------------------------------------------------------------------------

import type { components } from "./api.generated";
import type { ClientPage } from "./pagination";

// --- System configuration (runtime-tunable from DB) ---

export type UiConfig = components["schemas"]["UiConfig"];
export type ConfigEntry = components["schemas"]["ConfigEntry"];

/** GET /api/config response: config entries grouped by category */
export type ConfigResponse = components["schemas"]["ConfigResponse"];

export type ConfigUpdateItem = components["schemas"]["ConfigUpdate"];
export type ConfigUpdateRequest = components["schemas"]["UpdateConfigRequest"];

// --- User Management ---

export type UserInfo = components["schemas"]["UserInfo"];
export type UserInfoPage = ClientPage<components["schemas"]["UserInfoPage"]>;

// --- Prompt Templates (Admin) ---

export type PromptTemplate = components["schemas"]["PromptTemplateRow"];

// --- Agent Sessions (Audit) ---

export type AgentSession = components["schemas"]["AgentSession"];
export type AgentEvent = components["schemas"]["AgentEvent"];
export type AgentSessionPage = ClientPage<components["schemas"]["AgentSessionPage"]>;

export type RecipeStatus = components["schemas"]["RecipeStatus"];

export type AnalysisRecipe = components["schemas"]["AnalysisRecipe"];
export type AnalysisRecipePage =
  ClientPage<components["schemas"]["AnalysisRecipePage"]>;

// --- Saved Reports ---

export type ReportParameter = components["schemas"]["SavedReportParameter"];
export type SavedReport = components["schemas"]["SavedReport"];
export type SavedReportPage = ClientPage<components["schemas"]["SavedReportPage"]>;
export type ReportCreateRequest = components["schemas"]["CreateReportRequest"];
export type ReportUpdateRequest = components["schemas"]["UpdateReportRequest"];

// --- Scheduled Tasks ---

export type ScheduledTaskStatus = NonNullable<components["schemas"]["ScheduledTask"]["last_status"]>;
export type ScheduledTask = components["schemas"]["ScheduledTask"];
export type ScheduleCreateRequest = components["schemas"]["CreateScheduleRequest"];
export type ScheduleUpdateRequest = components["schemas"]["UpdateScheduleRequest"];

// --- Knowledge Base ---

export type KnowledgeKind = components["schemas"]["KnowledgeKind"];
export type KnowledgeStatus = components["schemas"]["KnowledgeStatus"];
export type KnowledgeEntry = components["schemas"]["KnowledgeEntry"];
export type KnowledgeEntryPage = ClientPage<components["schemas"]["KnowledgeEntryPage"]>;
export type KnowledgeCreateRequest = components["schemas"]["CreateKnowledgeEntryRequest"];
export type KnowledgeUpdateRequest = components["schemas"]["UpdateKnowledgeEntryRequest"];
export type KnowledgeStats = components["schemas"]["KnowledgeStats"];
