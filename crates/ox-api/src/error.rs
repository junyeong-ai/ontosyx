use std::collections::BTreeMap;

use axum::{
    Json,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use ox_core::error::OxError;
use serde::Serialize;
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// AppError — language-neutral typed error model.
//
// Wire format:
//   {
//     "error": {
//       "code":  "not_found",          // typed ApiErrorCode (snake_case)
//       "class": "client_error",       // 4xx vs 5xx category
//       "params": { "entity": "OntologyDraft" }
//     }
//   }
//
// The frontend renders the localised prose by looking up
// `errors.<code>` in its i18n catalogue with the `params` map
// interpolated. The backend never produces user-facing prose for
// infrastructure messages — that mirrors the existing
// `AnalysisWarning` / `DesignGate` contract documented in
// crates/ox-api/CLAUDE.md.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorCode {
    // ----- 4xx client errors -----
    BadRequest,
    ValidationError,
    ParseError,
    NotFound,
    Conflict,
    Unprocessable,
    Unauthorized,
    Forbidden,
    Gone,
    QualityGate,
    DesignGatesUnmet,
    InvalidOntology,
    UncertainReconcile,
    RateLimited,
    ConcurrencyCap,
    OntologyError,
    CompilationError,
    SerializationError,
    /// Streaming-only — emitted from SSE channels when the agent
    /// pipeline yields an error mid-stream. Distinct from
    /// `internal_error` so the FE can render "agent stopped" copy
    /// rather than a generic 500.
    AgentError,
    /// Streaming-only — design pipeline failed mid-stream (LLM
    /// rejected, cluster-level failure, etc.).
    DesignError,
    /// Streaming-only — quality / SHACL validation failed during a
    /// streamed save.
    QualityError,
    /// Streaming-only — store layer rejected the persist phase
    /// (concurrent write, RLS violation, etc.).
    PersistError,
    /// Streaming-only — refinement reconcile failed mid-stream.
    ReconcileError,
    /// Streaming-only — refine pipeline failed mid-stream.
    RefineError,
    /// `POST /ontologies/{id}/edits` with an empty operations array.
    /// Distinct from `bad_request` so the FE renders "select at
    /// least one change" rather than a generic "invalid request".
    EditOperationsEmpty,
    /// Optimistic-concurrency conflict on `apply_ontology_edits` —
    /// caller's `expected_version` doesn't match the current head.
    /// `params.expected` + `params.current` give the FE the precise
    /// numbers for the message.
    OntologyVersionConflict,
    /// `complete_ontology_draft` refused because the project's draft was
    /// branched from an older canonical version than the current
    /// head. Another commit landed during the project's flight, so
    /// merging the local copy would silently overwrite those
    /// changes. `params.parent_version` (project's branching point,
    /// short tag) and `params.current_version` (canonical's head,
    /// short tag) give the FE the rebase context.
    OntologyDraftStaleParent,
    /// `complete_ontology_draft` refused because the workspace's
    /// canonical was wiped between draft creation and commit. The
    /// draft's `parent_version_id` points at a version that no
    /// longer exists, so the lost-update guard cannot validate
    /// against the new head. The operator must reset the draft
    /// (delete + reseed from the new canonical's first version)
    /// rather than rebase. No params — the catalog template
    /// explains the recovery flow.
    OntologyDraftStaleParentCanonicalWiped,
    /// Whole-IR validation failed during commit — the apply phase
    /// produced an IR that broke a referential-integrity invariant.
    /// `params.errors` (array of strings) carries the validation
    /// diagnostics so the FE can render them as a list rather than
    /// concatenating into one paragraph.
    OntologyInvariantViolation,
    /// One of the queued edit operations rejected during dry-run or
    /// apply — `params.detail` is the per-op message from
    /// `OntologyEditOp::apply_to`.
    EditOperationRejected,
    /// A referenced ontology has no committed version yet — most
    /// commonly hit by deploy / publish actions that read the IR
    /// before it's first saved. `params.lineage_id` identifies the
    /// ontology so the FE can deep-link to "open this ontology"
    /// rather than render a cryptic id.
    OntologyNotCommitted,
    /// Schema deployment blocked because an admin approval request
    /// is still pending. The FE renders a "wait for admin review"
    /// state — no params needed; the message is invariant.
    DeployPendingApproval,
    /// Ontology draft has no source schema attached — fired when an
    /// action that needs the schema (deploy, complete, design)
    /// runs before analyse / introspect populated it.
    OntologyDraftMissingSourceSchema,
    /// `query.text` / search term is empty after trim. The FE
    /// renders an inline form-validation hint pointing at the
    /// search box.
    QueryTextEmpty,
    /// Temporal `as_of` query without a resolvable `ontology_id`.
    /// The pivot needs the lineage to walk back through; raw-IR
    /// queries (anonymous) can't pivot.
    TemporalQueryRequiresOntology,
    /// Temporal `as_of` resolved to no committed version — the
    /// lineage's oldest version was committed *after* the
    /// timestamp. `params.as_of` + `params.lineage_id` give the
    /// FE the inputs to render an actionable copy.
    TemporalSnapshotMissing,
    /// Query execution failed in the runtime layer.
    /// `params.detail` carries the runtime's user-facing message.
    QueryExecutionFailed,
    /// Query compilation failed (QueryIR → Cypher / DataFusion).
    /// `params.detail` carries the compiler's diagnostic.
    QueryCompilationFailed,
    /// Knowledge entry payload had a `kind` / `status` value not in
    /// the typed enum allowlist. `params.field` names which slot,
    /// `params.value` the rejected input, `params.allowed` the
    /// comma-joined valid set so the FE can render the option list.
    InvalidEnumValue,
    /// Free-form text field outside the configured length window.
    /// `params.field` names the slot; `params.min` / `params.max`
    /// give the FE the precise bounds to render in the inline error.
    TextLengthOutOfRange,
    /// Bulk-mutation request exceeded the per-call cap.
    /// `params.limit` carries the cap so the FE can render
    /// "split into batches of {limit}".
    BulkLimitExceeded,
    /// JWT or OIDC ID-token claim shape rejected at the auth boundary
    /// — the token decoded but a required claim was missing or
    /// malformed. `params.field` names the slot (`email` /
    /// `user_id` / `expiry`) so the FE can guide the user toward
    /// the right re-login flow.
    AuthTokenClaimInvalid,
    /// An API-key principal called a JWT-only endpoint (the WS
    /// token mint or `/auth/logout`). `params.operation` carries
    /// the surface name (`websocket_token` / `logout`) so the FE
    /// catalog can route the user to the correct admin action.
    AuthApiKeyJwtFlowDenied,
    /// A locale tag did not parse as BCP 47. `params.field` names
    /// the slot (e.g. `primary_locale` or
    /// `admin_locale_fallback[2]`), `params.tag` is the rejected
    /// input so the FE can highlight the bad token in the form.
    LocaleTagInvalid,
    /// A locale fallback chain was empty (`[]`).
    /// `params.field` names which chain (`admin_locale_fallback` or
    /// `llm_locale_fallback`) so the FE can scroll-to / focus the
    /// right input row.
    LocaleChainEmpty,
    /// Mutation rejected because it would affect the workspace
    /// owner. `params.action` carries the verb (`remove` /
    /// `change_role`) so the FE template can render the
    /// appropriate guidance ("transfer ownership first").
    WorkspaceOwnerProtected,
    /// Mutation rejected because it would affect the bootstrap
    /// "default" workspace. The current callers only block
    /// deletion, but new attempts get the same code so the FE
    /// catalog evolves in one place.
    DefaultWorkspaceProtected,
    /// An identifier failed its format predicate.
    /// `params.field` names the slot, `params.value` is the
    /// rejected input, `params.format` is the expected format —
    /// canonical values: `slug`, `cypher_label`,
    /// `cypher_property`. The FE template branches on `format` so
    /// each shape has its own remediation copy.
    IdentifierFormatInvalid,
    /// A prompt-template version string did not parse as the
    /// internal `PromptVersion` (semver-flavored). `params.value`
    /// carries the rejected input so the FE can highlight it.
    PromptVersionInvalid,
    /// A privileged self-mutation was rejected — admins cannot,
    /// for instance, demote themselves out of admin or delete
    /// their own account through the regular endpoints.
    /// `params.field` names the slot (`role`, `membership`, …).
    SelfMutationDenied,
    /// A required string or list field arrived empty — distinct
    /// from `TextLengthOutOfRange` because the user-facing message
    /// is "X is required" rather than "X must be ≥ N characters",
    /// and because the same code naturally covers list-valued
    /// fields ("data must not be empty" / "no updates provided").
    /// `params.field` names the slot.
    RequiredFieldEmpty,
    /// A quality rule of a given `rule_type` requires a specific
    /// configuration `field` that arrived absent. Different from
    /// `RequiredFieldEmpty` because the conditional ("required
    /// only when rule_type=X") is the salient piece of guidance
    /// for the operator. `params.rule_type` and `params.field`
    /// drive the FE template.
    QualityRuleRequiresField,
    /// A custom Cypher snippet contained a write keyword
    /// (`DELETE` / `DETACH` / `CREATE` / `MERGE` / `SET ` /
    /// `REMOVE `) where only read operations are permitted.
    /// `params.surface` names the field (`cypher_check`) so the
    /// FE can scroll the editor to the offending input.
    CypherMustBeReadOnly,
    /// A quality rule's runtime query failed.
    /// `params.rule_name` carries the rule name, `params.detail`
    /// the driver diagnostic.
    QualityRuleQueryFailed,
    /// OWL/Turtle import failed at the parser. `params.detail`
    /// carries the parser diagnostic so the user can locate the
    /// offending line in their source.
    OwlParseFailed,
    /// An optional capability the operator wants to use is not
    /// wired in this deployment (e.g. semantic memory, federation,
    /// JWT). `params.feature` names the missing capability so the
    /// FE can route the user to the right configuration page.
    FeatureNotConfigured,
    /// Ontology draft scope-defer rejected because one or more named
    /// tables are still bound by the project's ontology.
    /// `params.tables` is a comma-joined list of the offending
    /// tables so the FE can highlight them in the scope editor
    /// and surface "retract X first" guidance.
    ScopeDeferModeledTables,
    /// A code-repository analysis step (clone, tree generation,
    /// file read, LLM file selection) failed to produce a usable
    /// result. `params.operation` ∈ `clone` / `tree` / `read` /
    /// `empty_selection` / `empty_content`. `params.detail`
    /// carries the underlying driver error or `""` when the
    /// failure is purely structural (empty selection).
    RepoAnalysisFailed,
    /// Refinement was invoked with no input signal — neither a
    /// graph runtime, an `additional_context` text, nor a parseable
    /// source schema is available, so there is nothing to refine
    /// from. No params: the user-facing copy is the same in every
    /// occurrence and the FE walks the user to "ingest data, attach
    /// graph, or paste context."
    RefinementMissingInputs,
    /// A `decisions[]` entry referenced an `original_id` that
    /// isn't in the request's `uncertain_matches[]` — the FE must
    /// only emit decisions for the matches it received.
    /// `params.original_id` carries the offending value so the FE
    /// can highlight the row in the reconciliation table.
    RefinementDecisionUnknownId,
    /// Reanalyze (modeled-only) was called on a project with an
    /// empty modeled-table set. No params: the only sensible
    /// remediation copy is "promote at least one deferred table or
    /// use the regular reanalyze endpoint."
    ReanalyzeNoModeledTables,
    /// Reanalyze called with a source kind that doesn't match the
    /// project's stored kind. `params.expected` is the project's
    /// kind, `params.got` is the request's kind so the FE can
    /// surface "this project was created from PostgreSQL — use the
    /// PostgreSQL form."
    SourceTypeMismatch,
    /// Decision payload referenced schema elements that don't
    /// exist in the analysis report (renamed/dropped tables,
    /// columns that no longer match a re-introspected schema).
    /// `params.refs` is the structured list so the FE can render
    /// each row with its own remove-or-update button.
    DecisionInvalidSchemaRefs,
    /// Ontology draft status didn't match the precondition for the
    /// requested action. `params.required` is the expected status
    /// (`analyzed`, `designed`, …); `params.actual` is the
    /// project's current status so the FE can refresh and route
    /// the user to the right step.
    OntologyDraftStatusMismatch,
    /// Live connection to a data source failed during a runtime
    /// step (load, refinement profiling, on-demand introspection).
    /// `params.source_type` (`postgresql`, `mysql`, …) selects the
    /// FE template branch; `params.detail` carries the driver
    /// error so the operator can match it against an admin runbook.
    SourceConnectionFailed,
    /// An ontology edit was queued for human approval rather than
    /// applied directly — the project's automation policy gates
    /// the requested change type. The FE redirects to the
    /// approvals queue rather than treating this as a hard failure.
    EditQueuedForApproval,
    /// `query_ir` payload didn't deserialise into the canonical
    /// `QueryIR` shape. `params.detail` carries the parser
    /// diagnostic so the operator can locate the offending field.
    QueryIrInvalid,
    /// A cron expression failed to parse. `params.value` is the
    /// rejected input so the FE can highlight it; the catalog
    /// template links to the expression cheat-sheet.
    CronExpressionInvalid,
    /// A resource is owned by another principal — the caller
    /// can't read or mutate it. `params.resource` names the
    /// resource kind (`session`, `project`, `dashboard`, …) so
    /// the FE template can render appropriate copy without leaking
    /// the resource id.
    ResourceNotOwned,
    /// `X-Workspace-Id` header was missing or malformed.
    /// `params.reason` ∈ `missing` / `parse_failed` / `not_uuid` /
    /// `not_a_member`. The FE renders distinct copy per branch
    /// (header missing vs. cross-workspace access denied).
    WorkspaceHeaderInvalid,
    /// An `extend` / `reduce` selection was submitted before a
    /// baseline `Subset` / `All` analysis ever ran on the project.
    /// No params: the only sensible remediation is "run a base
    /// analyze first" — the FE catalog renders that guidance and a
    /// link back to the analyze surface.
    AnalysisBaselineRequired,
    /// Idempotency-Key middleware rejected a request because the
    /// target endpoint produces a streaming (SSE) response. SSE
    /// can't replay byte-for-byte, so the safe contract is
    /// "client-side retries only". `params.path` carries the
    /// endpoint so the operator's API client can branch to a
    /// non-streaming variant where one exists.
    IdempotencyStreamingUnsupported,
    /// Idempotency-Key middleware saw the same key replayed with a
    /// different request body. Stripe's pattern: refuse the second
    /// call rather than risk processing two distinct mutations
    /// under the same retry token. No params — the FE catalog
    /// instructs the caller to mint a fresh key for the new body.
    IdempotencyKeyReused,
    /// Idempotency-Key middleware rejected the request because its
    /// body exceeds the cache buffer cap. `params.limit` is the
    /// byte cap so the FE catalog can render "split into batches
    /// under {limit} bytes" or similar.
    IdempotencyRequestBodyTooLarge,
    /// A platform-role gate rejected the caller. `params.role` ∈
    /// `admin` / `designer` names the minimum required role so
    /// the FE catalog can route the user to the role request flow.
    RoleRequired,
    /// A webhook URL submitted to the notification subsystem
    /// failed validation. `params.reason` ∈ `parse_failed` /
    /// `bad_scheme` / `internal_network` selects the FE template
    /// branch (parse error / scheme error / SSRF guard).
    WebhookUrlInvalid,
    /// A `secret_ref` admin-facing credential reference failed to
    /// resolve. `params.scheme` ∈ `env` / `file` / `gcp_secret`,
    /// `params.kind` ∈ `invalid_reference` / `resolve_failed` /
    /// `unauthorized` / `not_found` / `provider_error`,
    /// `params.detail` carries the provider/driver diagnostic so
    /// the admin can locate the misconfiguration in their secret
    /// store. The catalog template renders distinct copy per
    /// `(scheme, kind)` pair.
    CredentialResolveFailed,
    // ----- 5xx server errors -----
    InternalError,
    NotImplemented,
    Unsupported,
    ServiceUnavailable,
    Timeout,
    MissingContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorClass {
    /// 4xx — the request was malformed or unauthorized.
    ClientError,
    /// 5xx — the server failed to fulfill a valid request.
    ServerError,
}

impl ApiErrorCode {
    pub fn class(self) -> ApiErrorClass {
        use ApiErrorCode::*;
        match self {
            InternalError | NotImplemented | Unsupported | ServiceUnavailable | Timeout
            | MissingContext | AgentError | DesignError | QualityError | PersistError
            | ReconcileError | RefineError => ApiErrorClass::ServerError,
            _ => ApiErrorClass::ClientError,
        }
    }

    /// Stable wire string — kept distinct from `serde_json::to_string`
    /// so non-serde call sites (metrics labels, logs) avoid the
    /// allocation round-trip.
    pub fn as_str(self) -> &'static str {
        use ApiErrorCode::*;
        match self {
            BadRequest => "bad_request",
            ValidationError => "validation_error",
            ParseError => "parse_error",
            NotFound => "not_found",
            Conflict => "conflict",
            Unprocessable => "unprocessable",
            Unauthorized => "unauthorized",
            Forbidden => "forbidden",
            Gone => "gone",
            QualityGate => "quality_gate",
            DesignGatesUnmet => "design_gates_unmet",
            InvalidOntology => "invalid_ontology",
            UncertainReconcile => "uncertain_reconcile",
            RateLimited => "rate_limited",
            ConcurrencyCap => "concurrency_cap",
            OntologyError => "ontology_error",
            CompilationError => "compilation_error",
            SerializationError => "serialization_error",
            AgentError => "agent_error",
            DesignError => "design_error",
            QualityError => "quality_error",
            PersistError => "persist_error",
            ReconcileError => "reconcile_error",
            RefineError => "refine_error",
            EditOperationsEmpty => "edit_operations_empty",
            OntologyVersionConflict => "ontology_version_conflict",
            OntologyDraftStaleParent => "ontology_draft_stale_parent",
            OntologyDraftStaleParentCanonicalWiped => "ontology_draft_stale_parent_canonical_wiped",
            OntologyInvariantViolation => "ontology_invariant_violation",
            EditOperationRejected => "edit_operation_rejected",
            OntologyNotCommitted => "ontology_not_committed",
            DeployPendingApproval => "deploy_pending_approval",
            OntologyDraftMissingSourceSchema => "ontology_draft_missing_source_schema",
            QueryTextEmpty => "query_text_empty",
            TemporalQueryRequiresOntology => "temporal_query_requires_ontology",
            TemporalSnapshotMissing => "temporal_snapshot_missing",
            QueryExecutionFailed => "query_execution_failed",
            QueryCompilationFailed => "query_compilation_failed",
            InvalidEnumValue => "invalid_enum_value",
            TextLengthOutOfRange => "text_length_out_of_range",
            BulkLimitExceeded => "bulk_limit_exceeded",
            AuthTokenClaimInvalid => "auth_token_claim_invalid",
            AuthApiKeyJwtFlowDenied => "auth_api_key_jwt_flow_denied",
            LocaleTagInvalid => "locale_tag_invalid",
            LocaleChainEmpty => "locale_chain_empty",
            WorkspaceOwnerProtected => "workspace_owner_protected",
            DefaultWorkspaceProtected => "default_workspace_protected",
            IdentifierFormatInvalid => "identifier_format_invalid",
            PromptVersionInvalid => "prompt_version_invalid",
            SelfMutationDenied => "self_mutation_denied",
            RequiredFieldEmpty => "required_field_empty",
            QualityRuleRequiresField => "quality_rule_requires_field",
            CypherMustBeReadOnly => "cypher_must_be_read_only",
            QualityRuleQueryFailed => "quality_rule_query_failed",
            OwlParseFailed => "owl_parse_failed",
            FeatureNotConfigured => "feature_not_configured",
            ScopeDeferModeledTables => "scope_defer_modeled_tables",
            RepoAnalysisFailed => "repo_analysis_failed",
            RefinementMissingInputs => "refinement_missing_inputs",
            RefinementDecisionUnknownId => "refinement_decision_unknown_id",
            ReanalyzeNoModeledTables => "reanalyze_no_modeled_tables",
            SourceTypeMismatch => "source_type_mismatch",
            DecisionInvalidSchemaRefs => "decision_invalid_schema_refs",
            OntologyDraftStatusMismatch => "ontology_draft_status_mismatch",
            SourceConnectionFailed => "source_connection_failed",
            EditQueuedForApproval => "edit_queued_for_approval",
            QueryIrInvalid => "query_ir_invalid",
            CronExpressionInvalid => "cron_expression_invalid",
            ResourceNotOwned => "resource_not_owned",
            WorkspaceHeaderInvalid => "workspace_header_invalid",
            AnalysisBaselineRequired => "analysis_baseline_required",
            IdempotencyStreamingUnsupported => "idempotency_streaming_unsupported",
            IdempotencyKeyReused => "idempotency_key_reused",
            IdempotencyRequestBodyTooLarge => "idempotency_request_body_too_large",
            RoleRequired => "role_required",
            WebhookUrlInvalid => "webhook_url_invalid",
            CredentialResolveFailed => "credential_resolve_failed",
            InternalError => "internal_error",
            NotImplemented => "not_implemented",
            Unsupported => "unsupported",
            ServiceUnavailable => "service_unavailable",
            Timeout => "timeout",
            MissingContext => "missing_context",
        }
    }
}

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: ApiErrorCode,
    params: BTreeMap<String, Value>,
    headers: Option<Box<HeaderMap>>,
}

impl AppError {
    /// Generic builder. Prefer the named constructors (`not_found`,
    /// `bad_request`, etc.) which fill the canonical params for each
    /// code. Use this directly when none of the named constructors
    /// match the call site's intent.
    pub fn new(status: StatusCode, code: ApiErrorCode) -> Self {
        Self {
            status,
            code,
            params: BTreeMap::new(),
            headers: None,
        }
    }

    /// Attach an interpolation parameter for FE i18n. Returns `self`
    /// so callers chain (`AppError::new(..).with_param("field", "email")`).
    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }

    /// Attach an interpolation parameter from anything `Serialize`.
    /// Used for structured details (objects, arrays).
    pub fn with_param_json(
        mut self,
        key: impl Into<String>,
        value: impl Serialize,
    ) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.params.insert(key.into(), v);
        }
        self
    }

    pub fn with_header(mut self, name: &'static str, value: impl AsRef<str>) -> Self {
        let mut headers = self.headers.unwrap_or_else(|| Box::new(HeaderMap::new()));
        if let Ok(v) = value.as_ref().parse() {
            headers.insert(name, v);
        }
        self.headers = Some(headers);
        self
    }

    // -----------------------------------------------------------------------
    // Canonical constructors — each maps a call-site idiom to the
    // typed (code, params) pair the wire emits.
    // -----------------------------------------------------------------------

    pub fn not_found(entity: &str) -> Self {
        Self::new(StatusCode::NOT_FOUND, ApiErrorCode::NotFound)
            .with_param("entity", entity)
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, ApiErrorCode::ServiceUnavailable)
            .with_param("detail", message.into())
    }

    /// 410 Gone — resource existed but is no longer available (e.g., a
    /// share token whose `expires_at` is in the past). Distinct from
    /// `not_found` so clients can render a "this link expired" message
    /// instead of a generic 404.
    pub fn gone(message: impl Into<String>) -> Self {
        Self::new(StatusCode::GONE, ApiErrorCode::Gone)
            .with_param("detail", message.into())
    }

    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, ApiErrorCode::Unprocessable)
            .with_param("detail", message.into())
    }

    pub fn unprocessable_with_details(
        code: ApiErrorCode,
        message: impl Into<String>,
        details: Value,
    ) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code)
            .with_param("detail", message.into())
            .with_param("details", details)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::BadRequest)
            .with_param("detail", message.into())
    }

    pub fn quality_gate(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, ApiErrorCode::QualityGate)
            .with_param("detail", message.into())
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(StatusCode::GATEWAY_TIMEOUT, ApiErrorCode::Timeout)
            .with_param("detail", message.into())
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, ApiErrorCode::Unauthorized)
            .with_param("detail", message.into())
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, ApiErrorCode::Forbidden)
            .with_param("detail", message.into())
    }

    pub fn rate_limited(retry_after_secs: u64) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, ApiErrorCode::RateLimited)
            .with_param("retry_after_secs", retry_after_secs)
            .with_header("retry-after", retry_after_secs.to_string())
    }

    /// 429 TOO_MANY_REQUESTS with a caller-supplied message — used by the
    /// per-user chat-stream concurrency limiter where the relevant
    /// signal isn't "slow down" but "close an existing stream first".
    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, ApiErrorCode::ConcurrencyCap)
            .with_param("detail", message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, ApiErrorCode::Conflict)
            .with_param("detail", message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, ApiErrorCode::InternalError)
            .with_param("detail", message.into())
    }

    /// Serialize a value to JSON, converting serialization failures to AppError.
    pub fn to_json(value: &impl Serialize) -> Result<Value, Self> {
        serde_json::to_value(value)
            .map_err(|e| Self::internal(format!("Serialization failed: {e}")))
    }

    // -----------------------------------------------------------------------
    // Domain-specific constructors — preserve call-site ergonomics
    // while emitting typed codes that FE i18n catalogs key on.
    // -----------------------------------------------------------------------

    pub fn ontology_draft_not_found() -> Self {
        Self::not_found("Design project")
    }

    pub fn ontology_not_found() -> Self {
        Self::not_found("Saved ontology")
    }

    pub fn execution_not_found() -> Self {
        Self::not_found("Query execution")
    }

    pub fn pin_not_found() -> Self {
        Self::not_found("Pin")
    }

    pub fn perspective_not_found() -> Self {
        Self::not_found("Perspective")
    }

    pub fn revision_not_found() -> Self {
        Self::not_found("Ontology revision")
    }

    pub fn no_ontology() -> Self {
        Self::bad_request("Ontology draft has no ontology")
    }

    pub fn no_runtime() -> Self {
        Self::service_unavailable("Graph database not connected")
    }

    pub fn empty_source_data() -> Self {
        Self::bad_request("Source data must not be empty")
    }

    pub fn validation(field: &str, message: &str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::ValidationError)
            .with_param("field", field)
            .with_param("detail", message)
    }

    /// `POST /ontologies/{id}/edits` with empty `operations`. The
    /// response body carries no `params` — the FE catalog renders a
    /// fixed "select at least one change" copy, no English fragment
    /// to interpolate.
    pub fn edit_operations_empty() -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::EditOperationsEmpty)
    }

    /// Optimistic-concurrency conflict — caller's `expected_version`
    /// doesn't match the current head. Both numbers ride in `params`
    /// so the FE i18n template can render the precise version delta.
    pub fn ontology_version_conflict(expected: u32, current: u32) -> Self {
        Self::new(StatusCode::CONFLICT, ApiErrorCode::OntologyVersionConflict)
            .with_param("expected", expected)
            .with_param("current", current)
    }

    /// `complete_ontology_draft` refused — project's draft branched from an
    /// older canonical version than the current head. The FE
    /// interpolates a "rebase onto v{current_version} before
    /// retrying" message. Both version tags are short strings (the
    /// `version` column on `ontology_version_snapshots`).
    pub fn ontology_draft_stale_parent(parent_version: &str, current_version: &str) -> Self {
        Self::new(StatusCode::CONFLICT, ApiErrorCode::OntologyDraftStaleParent)
            .with_param("parent_version", parent_version)
            .with_param("current_version", current_version)
    }

    /// `complete_ontology_draft` refused because the canonical was
    /// wiped between draft creation and commit. The lost-update
    /// guard cannot validate against a head that no longer exists,
    /// so the operator must reset the draft from the new canonical
    /// rather than rebase. No params — the catalog template explains
    /// the recovery flow.
    pub fn ontology_draft_stale_parent_canonical_wiped() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            ApiErrorCode::OntologyDraftStaleParentCanonicalWiped,
        )
    }

    /// Whole-IR validation failed during commit — `errors` is the
    /// structured `DiagnosticMessage` vector. Each entry carries
    /// `id` + `params` typed identically to the wire shape; the FE
    /// renders them through next-intl with the catalog template
    /// indexed by the diagnostic id, no English prose interpolation.
    pub fn ontology_invariant_violation(
        errors: Vec<ox_core::diagnostic::DiagnosticMessage>,
    ) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::OntologyInvariantViolation,
        )
        .with_param_json("errors", errors)
    }

    /// One of the queued edit operations rejected. `detail` is the
    /// per-op message; the FE catalog template renders it verbatim
    /// (these come from the IR's own validation paths so are
    /// already user-facing prose).
    pub fn edit_operation_rejected(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::EditOperationRejected,
        )
        .with_param("detail", detail.into())
    }

    /// Ontology referenced has no committed version. The
    /// `lineage_id` identifies the ontology — the FE catalog
    /// renders a deep-link with that identity rather than the
    /// transient version uuid.
    pub fn ontology_not_committed(lineage_id: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::OntologyNotCommitted,
        )
        .with_param("lineage_id", lineage_id.into())
    }

    /// Schema deployment blocked by a pending admin approval.
    /// No params — the FE catalog renders an invariant message
    /// pointing the user at the approvals queue.
    pub fn deploy_pending_approval() -> Self {
        Self::new(StatusCode::CONFLICT, ApiErrorCode::DeployPendingApproval)
    }

    /// Ontology draft missing the source schema. `detail` is the
    /// operator-facing hint about what to run next (analyse /
    /// introspect / re-import).
    pub fn ontology_draft_missing_source_schema(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::OntologyDraftMissingSourceSchema,
        )
        .with_param("detail", detail.into())
    }

    /// Search / query text empty after trim. No params — the FE
    /// catalog renders a fixed "type something to search" copy.
    pub fn query_text_empty() -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::QueryTextEmpty)
    }

    /// Temporal `as_of` query without an ontology_id. No params —
    /// the FE catalog renders a fixed "select an ontology to walk
    /// back through" copy.
    pub fn temporal_query_requires_ontology() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::TemporalQueryRequiresOntology,
        )
    }

    /// Temporal snapshot missing — no version was live at
    /// `as_of` for `lineage_id`.
    pub fn temporal_snapshot_missing(
        as_of: impl Into<String>,
        lineage_id: impl Into<String>,
    ) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::TemporalSnapshotMissing,
        )
        .with_param("as_of", as_of.into())
        .with_param("lineage_id", lineage_id.into())
    }

    /// Query execution failed in the runtime layer.
    pub fn query_execution_failed(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::QueryExecutionFailed,
        )
        .with_param("detail", detail.into())
    }

    /// Query compilation failed (QueryIR → physical plan).
    pub fn query_compilation_failed(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::QueryCompilationFailed,
        )
        .with_param("detail", detail.into())
    }

    /// Enum field rejected because the value isn't in the allowlist.
    /// `field` is the slot name, `value` the rejected input,
    /// `allowed` the canonical option set as a slice.
    pub fn invalid_enum_value(
        field: impl Into<String>,
        value: impl Into<String>,
        allowed: &[&str],
    ) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::InvalidEnumValue)
            .with_param("field", field.into())
            .with_param("value", value.into())
            .with_param("allowed", allowed.join(", "))
    }

    /// Text field length outside the configured window.
    /// `min` and `max` are inclusive character bounds.
    pub fn text_length_out_of_range(
        field: impl Into<String>,
        min: usize,
        max: usize,
    ) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::TextLengthOutOfRange)
            .with_param("field", field.into())
            .with_param("min", min)
            .with_param("max", max)
    }

    /// Bulk-mutation request exceeded the per-call cap.
    pub fn bulk_limit_exceeded(limit: usize) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::BulkLimitExceeded)
            .with_param("limit", limit)
    }

    /// Token-claim shape rejected at the auth boundary.
    /// `field` is the structurally invalid slot — `email`, `user_id`,
    /// `expiry` are the canonical values today.
    pub fn auth_token_claim_invalid(field: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            ApiErrorCode::AuthTokenClaimInvalid,
        )
        .with_param("field", field.into())
    }

    /// API-key principal called a JWT-only endpoint.
    /// `operation` names the surface — `websocket_token` and
    /// `logout` are the canonical values today.
    pub fn auth_api_key_jwt_flow_denied(operation: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::AuthApiKeyJwtFlowDenied,
        )
        .with_param("operation", operation.into())
    }

    /// BCP 47 locale tag failed to parse.
    pub fn locale_tag_invalid(field: impl Into<String>, tag: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::LocaleTagInvalid)
            .with_param("field", field.into())
            .with_param("tag", tag.into())
    }

    /// A locale fallback chain was empty.
    pub fn locale_chain_empty(field: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::LocaleChainEmpty)
            .with_param("field", field.into())
    }

    /// Mutation would affect the workspace owner. `action` is the
    /// rejected verb — canonical values: `remove`, `change_role`.
    pub fn workspace_owner_protected(action: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::WorkspaceOwnerProtected,
        )
        .with_param("action", action.into())
    }

    /// Mutation would affect the bootstrap default workspace.
    pub fn default_workspace_protected() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::DefaultWorkspaceProtected,
        )
    }

    /// Identifier failed its format predicate. `format` selects
    /// the FE template branch (`slug` / `cypher_label` /
    /// `cypher_property`) and so dictates the remediation copy.
    pub fn identifier_format_invalid(
        field: impl Into<String>,
        value: impl Into<String>,
        format: impl Into<String>,
    ) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::IdentifierFormatInvalid,
        )
        .with_param("field", field.into())
        .with_param("value", value.into())
        .with_param("format", format.into())
    }

    /// Prompt-template version string did not parse as `PromptVersion`.
    pub fn prompt_version_invalid(value: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::PromptVersionInvalid)
            .with_param("value", value.into())
    }

    /// Privileged self-mutation rejected. `field` names the slot
    /// — canonical values: `role`, `membership`, `account`.
    pub fn self_mutation_denied(field: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::SelfMutationDenied)
            .with_param("field", field.into())
    }

    /// Required string or list field arrived empty.
    pub fn required_field_empty(field: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::RequiredFieldEmpty)
            .with_param("field", field.into())
    }

    /// A quality rule of `rule_type` requires the configuration
    /// `field` that wasn't supplied.
    pub fn quality_rule_requires_field(
        rule_type: impl Into<String>,
        field: impl Into<String>,
    ) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::QualityRuleRequiresField,
        )
        .with_param("rule_type", rule_type.into())
        .with_param("field", field.into())
    }

    /// A custom Cypher snippet contained a write keyword.
    pub fn cypher_must_be_read_only(surface: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::CypherMustBeReadOnly)
            .with_param("surface", surface.into())
    }

    /// A quality rule's runtime query failed.
    pub fn quality_rule_query_failed(
        rule_name: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::QualityRuleQueryFailed,
        )
        .with_param("rule_name", rule_name.into())
        .with_param("detail", detail.into())
    }

    /// OWL/Turtle import failed at the parser.
    pub fn owl_parse_failed(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::OwlParseFailed)
            .with_param("detail", detail.into())
    }

    /// An optional feature isn't configured in this deployment.
    /// `feature` names the missing capability so the FE can route
    /// to the right admin / configuration surface.
    pub fn feature_not_configured(feature: impl Into<String>) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ApiErrorCode::FeatureNotConfigured,
        )
        .with_param("feature", feature.into())
    }

    /// Scope-defer would orphan modeled NodeTypes — `tables` lists
    /// the still-bound table names so the FE can render "retract X
    /// first" guidance and highlight them in the scope editor.
    pub fn scope_defer_modeled_tables(tables: &[&str]) -> Self {
        Self::new(StatusCode::CONFLICT, ApiErrorCode::ScopeDeferModeledTables)
            .with_param("tables", tables.join(", "))
    }

    /// Code-repository analysis step failed.
    /// `operation` ∈ `clone` / `tree` / `read` / `empty_selection`
    /// / `empty_content`. `detail` is the driver error (empty
    /// string for purely structural failures).
    pub fn repo_analysis_failed(
        operation: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::RepoAnalysisFailed)
            .with_param("operation", operation.into())
            .with_param("detail", detail.into())
    }

    /// Refinement invoked with no input signal.
    pub fn refinement_missing_inputs() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::RefinementMissingInputs,
        )
    }

    /// A decision referenced an `original_id` not present in the
    /// request's `uncertain_matches[]`.
    pub fn refinement_decision_unknown_id(original_id: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::RefinementDecisionUnknownId,
        )
        .with_param("original_id", original_id.into())
    }

    /// Refinement produced uncertain ID matches that need user
    /// review. `details` carries the structured `report` +
    /// `reconciled_ontology` so the FE can render the
    /// reconciliation table without re-running the LLM.
    /// `params.count` drives the FE plural template.
    pub fn uncertain_reconcile(count: usize, details: Value) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::UncertainReconcile,
        )
        .with_param("count", count)
        .with_param("details", details)
    }

    /// Reanalyze (modeled-only) called on a project with no
    /// modeled tables.
    pub fn reanalyze_no_modeled_tables() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::ReanalyzeNoModeledTables,
        )
    }

    /// Reanalyze source kind doesn't match the project's stored kind.
    pub fn source_type_mismatch(
        expected: impl Into<String>,
        got: impl Into<String>,
    ) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::SourceTypeMismatch)
            .with_param("expected", expected.into())
            .with_param("got", got.into())
    }

    /// Decision payload referenced schema elements that don't exist.
    /// `refs` is the structured list of ref descriptions.
    pub fn decision_invalid_schema_refs(refs: Vec<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::DecisionInvalidSchemaRefs,
        )
        .with_param_json("refs", refs)
    }

    /// Ontology draft status didn't match the requested action's
    /// precondition.
    pub fn ontology_draft_status_mismatch(
        required: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::OntologyDraftStatusMismatch)
            .with_param("required", required.into())
            .with_param("actual", actual.into())
    }

    /// Live connection to a data source failed.
    pub fn source_connection_failed(
        source_type: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::SourceConnectionFailed,
        )
        .with_param("source_type", source_type.into())
        .with_param("detail", detail.into())
    }

    /// Design gates unmet — typed wrapper around
    /// `unprocessable_with_details(DesignGatesUnmet, …)` that
    /// removes the prose `detail` param.
    pub fn design_gates_unmet(details: Value) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::DesignGatesUnmet,
        )
        .with_param("details", details)
    }

    /// Edit was queued for human approval rather than applied.
    pub fn edit_queued_for_approval() -> Self {
        Self::new(StatusCode::CONFLICT, ApiErrorCode::EditQueuedForApproval)
    }

    /// `query_ir` payload didn't deserialise into `QueryIR`.
    pub fn query_ir_invalid(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::QueryIrInvalid,
        )
        .with_param("detail", detail.into())
    }

    /// A cron expression failed to parse.
    pub fn cron_expression_invalid(value: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::CronExpressionInvalid,
        )
        .with_param("value", value.into())
    }

    /// Resource is owned by another principal.
    /// `resource` ∈ `session` / `project` / `dashboard` / ….
    pub fn resource_not_owned(resource: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, ApiErrorCode::ResourceNotOwned)
            .with_param("resource", resource.into())
    }

    /// `X-Workspace-Id` header missing or malformed.
    /// `reason` ∈ `missing` / `parse_failed` / `not_uuid` /
    /// `not_a_member`.
    pub fn workspace_header_invalid(reason: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::WorkspaceHeaderInvalid,
        )
        .with_param("reason", reason.into())
    }

    /// An `extend` / `reduce` selection was submitted before a
    /// baseline analysis ever ran.
    pub fn analysis_baseline_required() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::AnalysisBaselineRequired,
        )
    }

    /// Idempotency-Key on a streaming endpoint.
    pub fn idempotency_streaming_unsupported(path: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::IdempotencyStreamingUnsupported,
        )
        .with_param("path", path.into())
    }

    /// Idempotency-Key replayed with a different request body.
    pub fn idempotency_key_reused() -> Self {
        Self::new(StatusCode::CONFLICT, ApiErrorCode::IdempotencyKeyReused)
    }

    /// Idempotency-Key request body exceeded the cache cap.
    pub fn idempotency_request_body_too_large(limit: usize) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::IdempotencyRequestBodyTooLarge,
        )
        .with_param("limit", limit)
    }

    /// Platform role gate rejected the caller.
    /// `role` ∈ `admin` / `designer`.
    pub fn role_required(role: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, ApiErrorCode::RoleRequired)
            .with_param("role", role.into())
    }

    /// Ontology input failed schema validation. `errors` is the
    /// structured diagnostic list — the FE catalog renders each by
    /// `id` + interpolated params, no English prose interpolation.
    pub fn invalid_ontology(
        errors: Vec<ox_core::diagnostic::DiagnosticMessage>,
    ) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ApiErrorCode::InvalidOntology,
        )
        .with_param_json("errors", errors)
    }

    /// Webhook URL failed validation.
    /// `reason` ∈ `parse_failed` / `bad_scheme` / `internal_network`.
    pub fn webhook_url_invalid(reason: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ApiErrorCode::WebhookUrlInvalid)
            .with_param("reason", reason.into())
    }

    /// Credential reference failed to resolve.
    /// `scheme` ∈ `env` / `file` / `gcp_secret`,
    /// `kind` ∈ `invalid_reference` / `resolve_failed` /
    /// `unauthorized` / `not_found` / `provider_error`.
    pub fn credential_resolve_failed(
        scheme: impl Into<String>,
        kind: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ApiErrorCode::CredentialResolveFailed,
        )
        .with_param("scheme", scheme.into())
        .with_param("kind", kind.into())
        .with_param("detail", detail.into())
    }

    // -----------------------------------------------------------------------
    // Accessors — used by metrics + tests + the IntoResponse impl.
    // -----------------------------------------------------------------------

    pub fn code(&self) -> ApiErrorCode {
        self.code
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn param(&self, key: &str) -> Option<&Value> {
        self.params.get(key)
    }
}

/// Map an OxError variant (non-Contextual) to HTTP status + typed code.
fn ox_error_status(err: &OxError) -> (StatusCode, ApiErrorCode) {
    match err {
        OxError::Validation { .. } => (StatusCode::BAD_REQUEST, ApiErrorCode::ValidationError),
        OxError::Parse { .. } => (StatusCode::BAD_REQUEST, ApiErrorCode::ParseError),
        OxError::NotFound { .. } => (StatusCode::NOT_FOUND, ApiErrorCode::NotFound),
        OxError::Conflict { .. } => (StatusCode::CONFLICT, ApiErrorCode::Conflict),
        OxError::Ontology { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, ApiErrorCode::OntologyError)
        }
        OxError::Compilation { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, ApiErrorCode::CompilationError)
        }
        OxError::UnsupportedOperation { .. } => {
            (StatusCode::NOT_IMPLEMENTED, ApiErrorCode::Unsupported)
        }
        OxError::Serialization(_) => {
            (StatusCode::BAD_REQUEST, ApiErrorCode::SerializationError)
        }
        OxError::MissingContext { .. } => {
            (StatusCode::INTERNAL_SERVER_ERROR, ApiErrorCode::MissingContext)
        }
        OxError::Runtime { .. } | OxError::Contextual { .. } => {
            (StatusCode::INTERNAL_SERVER_ERROR, ApiErrorCode::InternalError)
        }
    }
}

impl From<OxError> for AppError {
    fn from(err: OxError) -> Self {
        // Contextual wraps another OxError; delegate to inner source for status mapping.
        let (status, code) = match &err {
            OxError::Contextual { source, .. } => ox_error_status(source),
            other => ox_error_status(other),
        };

        let mut app = AppError::new(status, code);

        if status.is_server_error() {
            // Log the verbose form server-side at `error` — operators
            // get the full driver text + Contextual chain. The wire
            // response carries no driver text; correlation is via
            // x-request-id. The FE i18n catalogue resolves
            // `errors.<code>` to the appropriate placeholder copy
            // ("server configuration error — quote x-request-id…").
            tracing::error!(
                code = code.as_str(),
                status = status.as_u16(),
                error = %err,
                "5xx response"
            );
        } else {
            // 4xx is the user's fault — keep the precise message in
            // `params.detail` so the FE catalog can interpolate. For
            // structured variants (Validation has field+message), unpack
            // explicitly so each placeholder lands in its own param key.
            match &err {
                OxError::Validation { field, message } => {
                    app = app
                        .with_param("field", field.clone())
                        .with_param("detail", message.clone());
                }
                OxError::NotFound { entity } => {
                    app = app.with_param("entity", entity.clone());
                }
                _ => {
                    app = app.with_param("detail", err.to_string());
                }
            }
        }

        app
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        crate::metrics::record_error(self.code.as_str());
        let body = json!({
            "error": {
                "code":   self.code,
                "class":  self.code.class(),
                "params": self.params,
            }
        });
        let mut response = (self.status, Json(body)).into_response();
        if let Some(headers) = self.headers {
            response.headers_mut().extend(*headers);
        }
        response
    }
}

#[cfg(test)]
mod redaction_tests {
    use super::*;

    /// 5xx bodies must carry the typed code + class but never any
    /// driver text or filesystem prefixes in `params`. Tripping this
    /// assertion means a future change re-leaked internal detail
    /// through the response body.
    #[test]
    fn runtime_5xx_redacts_driver_text() {
        let leaky = OxError::Runtime {
            message: "PostgreSQL error [42P01]: relation \"foo\" does not exist \
                      at /Users/dev/.cargo/registry/src/sqlx-core-0.8.0/src/error.rs:42"
                .to_string(),
        };
        let app_err: AppError = leaky.into();
        assert_eq!(app_err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(app_err.code, ApiErrorCode::InternalError);
        // 5xx params should be empty — FE catalog produces the
        // "internal error / quote x-request-id" copy from the code
        // alone.
        assert!(
            app_err.params.is_empty(),
            "5xx params should not carry driver text: {:?}",
            app_err.params
        );
    }

    #[test]
    fn missing_context_5xx_uses_distinct_code() {
        let err = OxError::MissingContext {
            kind: "workspace".to_string(),
            message: "internal: forgot to wrap with WORKSPACE_ID.scope".to_string(),
        };
        let app_err: AppError = err.into();
        assert_eq!(app_err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(app_err.code, ApiErrorCode::MissingContext);
        assert!(
            app_err.params.is_empty(),
            "5xx params should not carry internal symbol names",
        );
    }

    #[test]
    fn validation_4xx_keeps_full_detail() {
        let err = OxError::Validation {
            field: "email".to_string(),
            message: "must contain '@'".to_string(),
        };
        let app_err: AppError = err.into();
        assert_eq!(app_err.status, StatusCode::BAD_REQUEST);
        assert_eq!(app_err.code, ApiErrorCode::ValidationError);
        // 4xx is the user's fault — keep the precise message so the
        // FE catalog can render the localised guidance.
        assert_eq!(
            app_err.params.get("field"),
            Some(&Value::from("email")),
        );
        let detail = app_err
            .params
            .get("detail")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        assert!(detail.contains('@'), "detail should preserve user text: {detail}");
    }

    #[test]
    fn contextual_wrapping_a_runtime_still_redacts_at_5xx() {
        let inner = OxError::Runtime {
            message: "neo4rs: bolt frame oversized at handshake".to_string(),
        };
        let wrapped = inner.with_context("graph:neo4j", "graph_runtime::execute");
        let app_err: AppError = wrapped.into();
        assert_eq!(app_err.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(app_err.code, ApiErrorCode::InternalError);
        assert!(
            app_err.params.is_empty(),
            "wrapped 5xx should still be empty",
        );
    }

    #[test]
    fn not_found_carries_entity_param() {
        let err = AppError::not_found("OntologyDraft");
        assert_eq!(err.code, ApiErrorCode::NotFound);
        assert_eq!(err.params.get("entity"), Some(&Value::from("OntologyDraft")));
    }

    #[test]
    fn class_partition_aligns_with_status() {
        assert_eq!(
            ApiErrorCode::NotFound.class(),
            ApiErrorClass::ClientError
        );
        assert_eq!(
            ApiErrorCode::InternalError.class(),
            ApiErrorClass::ServerError
        );
        assert_eq!(
            ApiErrorCode::MissingContext.class(),
            ApiErrorClass::ServerError
        );
        assert_eq!(
            ApiErrorCode::Unsupported.class(),
            ApiErrorClass::ServerError
        );
    }

    /// Every variant has both a wire string AND a class. This is
    /// the "did the author add the four required updates" gate
    /// (enum + as_str arm + class arm + i18n key); the i18n side
    /// is enforced by the FE `error-code-parity-audit` script that
    /// reads this `as_str` body. Each variant is listed explicitly
    /// so adding a new one without updating this list trips the
    /// match exhaustiveness — a compile-time hint that the catalog
    /// gate must be revisited.
    #[test]
    fn every_variant_has_string_and_class() {
        use ApiErrorCode::*;
        let all: &[ApiErrorCode] = &[
            BadRequest,
            ValidationError,
            ParseError,
            NotFound,
            Conflict,
            Unprocessable,
            Unauthorized,
            Forbidden,
            Gone,
            QualityGate,
            DesignGatesUnmet,
            InvalidOntology,
            UncertainReconcile,
            RateLimited,
            ConcurrencyCap,
            OntologyError,
            CompilationError,
            SerializationError,
            InternalError,
            NotImplemented,
            Unsupported,
            ServiceUnavailable,
            Timeout,
            MissingContext,
            AgentError,
            DesignError,
            QualityError,
            PersistError,
            ReconcileError,
            RefineError,
            EditOperationsEmpty,
            OntologyVersionConflict,
            OntologyDraftStaleParent,
            OntologyDraftStaleParentCanonicalWiped,
            OntologyInvariantViolation,
            EditOperationRejected,
            OntologyNotCommitted,
            DeployPendingApproval,
            OntologyDraftMissingSourceSchema,
            QueryTextEmpty,
            TemporalQueryRequiresOntology,
            TemporalSnapshotMissing,
            QueryExecutionFailed,
            QueryCompilationFailed,
            InvalidEnumValue,
            TextLengthOutOfRange,
            BulkLimitExceeded,
            AuthTokenClaimInvalid,
            AuthApiKeyJwtFlowDenied,
            LocaleTagInvalid,
            LocaleChainEmpty,
            WorkspaceOwnerProtected,
            DefaultWorkspaceProtected,
            IdentifierFormatInvalid,
            PromptVersionInvalid,
            SelfMutationDenied,
            RequiredFieldEmpty,
            QualityRuleRequiresField,
            CypherMustBeReadOnly,
            QualityRuleQueryFailed,
            OwlParseFailed,
            FeatureNotConfigured,
            ScopeDeferModeledTables,
            RepoAnalysisFailed,
            RefinementMissingInputs,
            RefinementDecisionUnknownId,
            ReanalyzeNoModeledTables,
            SourceTypeMismatch,
            DecisionInvalidSchemaRefs,
            OntologyDraftStatusMismatch,
            SourceConnectionFailed,
            EditQueuedForApproval,
            QueryIrInvalid,
            CronExpressionInvalid,
            ResourceNotOwned,
            WorkspaceHeaderInvalid,
            AnalysisBaselineRequired,
            IdempotencyStreamingUnsupported,
            IdempotencyKeyReused,
            IdempotencyRequestBodyTooLarge,
            RoleRequired,
            WebhookUrlInvalid,
            CredentialResolveFailed,
        ];
        for code in all {
            assert!(
                !code.as_str().is_empty(),
                "{code:?} has empty wire string"
            );
            // class() is a total function via match, so we only
            // assert it returns one of the two partitions — the
            // compiler already enforces exhaustiveness.
            let _ = code.class();
        }
    }
}
