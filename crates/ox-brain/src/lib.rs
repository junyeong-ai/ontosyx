#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

pub mod auth;
pub mod client_pool;
pub mod design;
pub mod knowledge_rag;
pub mod knowledge_util;
pub mod model_resolver;
pub mod prompts;
pub mod provider;
pub mod schema;
pub mod schema_rag;
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_support;

pub use design::{DesignOntologyInput, DesignOntologyOutput};

use async_trait::async_trait;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tracing::info;

use ox_core::error::{OxError, OxResult};
use ox_ontology::load_plan::LoadPlan;
use ox_ontology::command::OntologyCommand;
use ox_ontology::ir::OntologyIR;
use ox_query_ir::query::QueryIR;
use ox_ontology::repo_insights::{FileContent, RepoInsights};
use ox_ontology::mapping::SourceId;
use ox_core::source_schema::SourceSchema;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use prompts::PromptRegistry;
use provider::{StreamChunk, TokenUsage, structured_completion};

// ---------------------------------------------------------------------------
// ExplanationOutput — result from non-structured LLM calls
// ---------------------------------------------------------------------------

pub struct ExplanationOutput {
    pub content: String,
    pub model: String,
    pub usage: Option<TokenUsage>,
}

// ---------------------------------------------------------------------------
// ProviderInfo — provider metadata for health checks and logging
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub model: String,
}

/// Type alias for a streaming explanation response.
pub type ExplanationStream =
    Pin<Box<dyn futures_core::Stream<Item = OxResult<StreamChunk>> + Send>>;

// ---------------------------------------------------------------------------
// EditCommandsOutput — result from ontology edit command generation
// ---------------------------------------------------------------------------

pub struct EditCommandsOutput {
    pub commands: Vec<OntologyCommand>,
    pub explanation: String,
    pub model: String,
}

// ---------------------------------------------------------------------------
// WidgetHint — lightweight LLM output for widget selection
// ---------------------------------------------------------------------------

/// Simple hint from LLM about which widget to use.
/// The frontend interprets this and renders the appropriate component.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WidgetHint {
    /// Which widget type to render
    pub widget_type: WidgetType,
    /// Optional title for the widget
    pub title: Option<String>,
    /// Brief reason for the selection (for debugging, not shown to user)
    pub reason: Option<String>,
}

/// Available visualization widget types for query results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WidgetType {
    /// Categorical comparisons with a single metric
    BarChart,
    /// Multiple metrics on the same category axis
    ComboChart,
    /// Proportional distribution with few categories
    PieChart,
    /// Time series or sequential trends
    LineChart,
    /// Single aggregate value
    StatCard,
    /// Multi-column detailed data
    Table,
    /// Node-edge graph visualization (paths, networks, relationships)
    Graph,
    /// Matrix of values with color-coded intensity (correlation, co-occurrence)
    Heatmap,
    /// Vertical event timeline (temporal sequences, audit trails)
    Timeline,
    /// Hierarchical area proportions (category breakdown, disk usage)
    Treemap,
    /// Conversion or process funnel (stages with drop-off rates)
    Funnel,
    /// Data is self-explanatory from text alone
    None,
}

// ---------------------------------------------------------------------------
// Sub-traits — focused LLM capability groups
// ---------------------------------------------------------------------------

/// Ontology design and refinement capabilities.
#[async_trait]
pub trait OntologyDesigner: Send + Sync {
    /// Analyze sample data + the workspace's existing domain context
    /// and design an ontology.
    ///
    /// The structured input shape lets the LLM see — alongside the
    /// raw sample data — every glossary term, code system,
    /// pre-detected ambiguity, and (in extension mode) existing
    /// ontology the workspace already knows about. The model prefers
    /// the existing canonical labels over inventing parallel ones,
    /// reducing the post-pass `binding_suggestions` to a fall-back
    /// rather than the primary correction step.
    ///
    /// `input.source_id` stamps every emitted `ObjectMappingDef` in
    /// the returned IR with the canonical source identity so
    /// federation plans, provenance, and plan-cache keys stay
    /// consistent with the DesignProject the caller is operating on.
    ///
    /// Returns the produced [`OntologyIR`] paired with the
    /// [`ArtifactProvenance`](ox_ontology::source_mapping::ArtifactProvenance)
    /// envelope (prompt id + version + resolved model id + replay
    /// params) that authored it. The caller threads the provenance
    /// straight into the
    /// [`SourceMappingArtifact`](ox_ontology::source_mapping::SourceMappingArtifact)
    /// it persists for this design action — atomic, no post-hoc
    /// lookup race against a concurrent model-config update.
    async fn design_ontology(
        &self,
        input: &DesignOntologyInput<'_>,
    ) -> OxResult<DesignOntologyOutput>;

    /// Design a partial ontology for a batch of tables (divide-and-conquer pipeline).
    /// Returns raw `InputOntologyDef` (not normalized) for later merging —
    /// the caller runs `normalize(merged, source_id)` once after all
    /// batches + cross-edge resolution land, so `source_id` is not
    /// threaded through this method.
    async fn design_ontology_batch(
        &self,
        batch_data: &str,
        context: &str,
        existing_nodes: &str,
        cross_fks: &str,
    ) -> OxResult<ox_ontology::input::InputOntologyDef>;

    /// Resolve the
    /// [`ArtifactProvenance`](ox_ontology::source_mapping::ArtifactProvenance)
    /// that *would* be authored against a named prompt + operation
    /// right now. The divide-and-conquer batch path captures
    /// provenance once at the top of the loop (every cluster shares
    /// one prompt + model resolution) and reuses it for the merged-
    /// IR artifact. Naming mirrors `resolve_cross_edges` — both are
    /// derive-from-current-state, no LLM call.
    async fn resolve_design_provenance(
        &self,
        prompt_name: &str,
        operation: &str,
    ) -> OxResult<ox_ontology::source_mapping::ArtifactProvenance>;

    /// Generate missing cross-domain edges for uncovered FK relationships.
    /// Returns edge definitions to be appended to the merged InputIR.
    async fn resolve_cross_edges(
        &self,
        node_labels: &str,
        existing_edges: &str,
        uncovered_fks: &str,
    ) -> OxResult<Vec<ox_ontology::InputEdgeTypeDef>>;

    /// Refine an ontology's metadata using graph profile statistics and/or additional context.
    /// `refinement_context` is pre-formatted and may contain graph profile data,
    /// domain gap resolutions, or both combined.
    async fn refine_ontology(
        &self,
        ontology: &OntologyIR,
        refinement_context: &str,
        source_id: &SourceId,
    ) -> OxResult<OntologyIR>;
}

/// Query translation and widget selection capabilities.
#[async_trait]
pub trait QueryTranslator: Send + Sync {
    /// Translate natural language question into a QueryIR
    async fn translate_query(
        &self,
        question: &str,
        ontology: &OntologyIR,
        ctx: &branchforge::ExecutionContext,
    ) -> OxResult<QueryIR>;

    /// Generate a LoadPlan from an ontology and source data description
    async fn plan_load(
        &self,
        ontology: &OntologyIR,
        source_description: &str,
    ) -> OxResult<LoadPlan>;

    /// Generate a LoadPlan from ontology + source schema.
    ///
    /// Reads mapping information directly from `ontology.object_mappings()`
    /// — the IR is the single source of truth for where nodes and
    /// properties live in the source, so no external mapping
    /// parameter is needed. `source_schema` carries the physical
    /// schema the load plan must reconcile with.
    async fn generate_load_plan(
        &self,
        ontology: &OntologyIR,
        source_schema: &SourceSchema,
    ) -> OxResult<LoadPlan>;

    /// Select the best widget type for displaying query results
    async fn select_widget(&self, query: &QueryIR, result_sample: &str) -> OxResult<WidgetHint>;
}

/// Text explanation capabilities (structured and streaming).
#[async_trait]
pub trait Explainer: Send + Sync {
    /// Generate a text explanation of query results.
    async fn explain(&self, user_message: &str) -> OxResult<ExplanationOutput>;

    /// Stream a text explanation of query results as an async stream of text chunks.
    async fn explain_stream(&self, user_message: String) -> OxResult<ExplanationStream>;

    /// Generate proactive insight suggestions from ontology structure.
    async fn suggest_insights(
        &self,
        ontology: &OntologyIR,
        graph_stats: Option<&serde_json::Value>,
    ) -> OxResult<Vec<ox_ontology::InsightHint>>;
}

/// Repository analysis capabilities.
#[async_trait]
pub trait RepoAnalyzer: Send + Sync {
    /// Phase 1 repo analysis: given a file tree string, select up to 30 relevant files for analysis.
    /// Returns relative paths the LLM considers most useful for ontology design.
    async fn navigate_repo(&self, file_tree: &str) -> OxResult<Vec<String>>;

    /// Phase 2 repo analysis: given file contents, extract structured domain insights.
    /// Returns enum definitions, ORM relationships, field hints, and domain notes.
    async fn analyze_repo_files(&self, files: &[FileContent]) -> OxResult<RepoInsights>;
}

/// Surgical ontology editing via atomic commands.
#[async_trait]
pub trait OntologyEditor: Send + Sync {
    /// Generate a list of atomic OntologyCommand operations to fulfill the user's edit request.
    /// Returns surgical commands instead of a full ontology replacement.
    async fn generate_edit_commands(
        &self,
        ontology: &OntologyIR,
        user_request: &str,
    ) -> OxResult<EditCommandsOutput>;
}

/// LLM provider metadata for health checks and observability.
pub trait LlmMetadata: Send + Sync {
    /// Default model info for logging/audit purposes.
    fn default_model_info(&self) -> ProviderInfo;

    /// List all loaded prompt templates with their versions.
    fn list_prompts(&self) -> Vec<(String, String)>;

    /// Template-level SHA-256 hex of the named prompt — system body
    /// only, pre-render. The fingerprint a content-addressed cache
    /// (such as ADR-0027's draft cluster checkpoint store) folds in
    /// alongside its other inputs. Distinct from
    /// `ArtifactProvenance::prompt_render_hash` (which captures the
    /// post-render content of one call): caches that key on a
    /// *unit-of-work* shape, not a single call, must avoid letting
    /// per-call variables enter the cache key. An admin who edits
    /// the prompt body without bumping `prompt_version` shifts this
    /// hash, so every cache entry authored under the prior body
    /// invalidates on the next read — the correct contract.
    fn prompt_template_hash(&self, prompt_name: &str) -> OxResult<String>;
}

// ---------------------------------------------------------------------------
// Brain trait — composite supertrait aggregating all LLM capabilities
// ---------------------------------------------------------------------------

/// Convenience supertrait that aggregates all LLM capabilities.
/// Use specific sub-traits (`OntologyDesigner`, `QueryTranslator`, etc.) when
/// a component only needs a subset of capabilities.
pub trait Brain:
    OntologyDesigner + OntologyEditor + QueryTranslator + Explainer + RepoAnalyzer + LlmMetadata
{
}

/// Blanket impl: anything implementing all sub-traits is automatically a Brain.
impl<T> Brain for T where
    T: OntologyDesigner + OntologyEditor + QueryTranslator + Explainer + RepoAnalyzer + LlmMetadata
{
}

// ---------------------------------------------------------------------------
// ModelHint — per-method cost optimization
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// DefaultBrain — uses ClientPool + ModelResolver + PromptRegistry
// ---------------------------------------------------------------------------

pub struct DefaultBrain {
    client_pool: Arc<client_pool::ClientPool>,
    model_resolver: Arc<dyn model_resolver::ModelResolver>,
    prompts: PromptRegistry,
    /// Cached default model info for sync access (logging, audit).
    default_model: ProviderInfo,
    /// Optional memory store for schema RAG. When available, `translate_query`
    /// uses vector search to discover relevant sub-schema instead of injecting
    /// the entire ontology JSON (~120K tokens → ~2K tokens).
    memory: Option<Arc<ox_memory::MemoryStore>>,
    /// Ontology ID for scoping schema RAG searches.
    ontology_lineage_id: Option<String>,
    /// Optional knowledge store for failure-driven learning.
    /// When available, `translate_query` injects learned corrections.
    knowledge_store: Option<Arc<dyn ox_store::KnowledgeStore>>,
}

impl DefaultBrain {
    pub fn new(
        client_pool: Arc<client_pool::ClientPool>,
        model_resolver: Arc<dyn model_resolver::ModelResolver>,
        prompts: PromptRegistry,
        default_model: ProviderInfo,
    ) -> Self {
        Self {
            client_pool,
            model_resolver,
            prompts,
            default_model,
            memory: None,
            ontology_lineage_id: None,
            knowledge_store: None,
        }
    }

    /// Set memory store for schema RAG in query translation.
    pub fn with_memory(
        mut self,
        memory: Arc<ox_memory::MemoryStore>,
        ontology_lineage_id: Option<String>,
    ) -> Self {
        self.memory = Some(memory);
        self.ontology_lineage_id = ontology_lineage_id;
        self
    }

    /// Set knowledge store for failure-driven learning in query translation.
    pub fn with_knowledge(mut self, store: Arc<dyn ox_store::KnowledgeStore>) -> Self {
        self.knowledge_store = Some(store);
        self
    }

    /// Access the knowledge store (for extraction triggers in ox-agent).
    pub fn knowledge_store(&self) -> Option<&Arc<dyn ox_store::KnowledgeStore>> {
        self.knowledge_store.as_ref()
    }

    /// Resolve model and client for a given operation.
    ///
    /// Uses `by_provider` to look up the already-authenticated client
    /// from the pool — no credentials needed since the client was pre-warmed
    /// during server startup.
    async fn resolve_for_operation(
        &self,
        operation: &str,
    ) -> OxResult<(Arc<dyn branchforge::LlmCall>, model_resolver::ResolvedModel)> {
        let resolved = self.model_resolver.resolve(operation).await?;
        let client = self
            .client_pool
            .by_provider(&resolved.provider)
            .ok_or_else(|| OxError::Runtime {
                message: format!(
                    "No LLM client available for provider '{}'. \
                     Ensure it was registered during server startup.",
                    resolved.provider
                ),
            })?;
        Ok((client, resolved))
    }

    /// Core LLM call: resolve model via operation name, load prompt
    /// template, render variables, call `structured_completion` with
    /// prompt caching.
    async fn call_structured<T: serde::de::DeserializeOwned + schemars::JsonSchema>(
        &self,
        prompt_name: &str,
        min_version: Option<&str>,
        operation: &str,
        vars: &HashMap<&str, &str>,
        log_message: &str,
    ) -> OxResult<T> {
        let (parsed, _) = self
            .call_structured_traced(prompt_name, min_version, operation, vars, log_message)
            .await?;
        Ok(parsed)
    }

    /// Same as [`Self::call_structured`] but additionally returns a
    /// [`CallProvenance`] capturing the exact prompt + model the
    /// call ran against. Callers that need provenance for downstream
    /// artifact authoring (e.g., `design_ontology` → `ArtifactProvenance`)
    /// use this method directly — single registry / resolver round-trip,
    /// no post-hoc lookup that could drift behind a concurrent
    /// model-config update.
    async fn call_structured_traced<
        T: serde::de::DeserializeOwned + schemars::JsonSchema,
    >(
        &self,
        prompt_name: &str,
        min_version: Option<&str>,
        operation: &str,
        vars: &HashMap<&str, &str>,
        log_message: &str,
    ) -> OxResult<(T, CallProvenance)> {
        let tmpl = match min_version {
            Some(v) => self.prompts.checked_for(prompt_name, v)?,
            None => self.prompts.get(prompt_name)?,
        };
        let user_prompt = tmpl.render_user(vars);

        let (client, resolved) = self.resolve_for_operation(operation).await?;
        let effective_max_tokens = resolved.max_tokens.unwrap_or(tmpl.max_tokens);
        let effective_temperature = resolved.temperature.or(tmpl.temperature);

        info!(
            model = %resolved.model_id,
            operation,
            prompt_version = %tmpl.version,
            "{log_message}"
        );

        let parsed = structured_completion(
            client.as_ref(),
            &resolved.model_id,
            &tmpl.system,
            &user_prompt,
            effective_max_tokens,
            effective_temperature,
        )
        .await?;

        // Render hash captures system + user post-interpolation.
        // An admin who edits the DB-backing prompt without bumping
        // `prompt_version` shifts this value, and `ContentBody`
        // re-hashes against the new provenance — the prior artifact
        // id no longer matches and the operator sees the divergence
        // rather than a silent cache hit.
        let prompt_render_hash =
            ox_ontology::source_mapping::ArtifactProvenance::compute_prompt_render_hash(
                &format!("{}\n\n{}", tmpl.system, user_prompt),
            );

        Ok((
            parsed,
            CallProvenance {
                prompt_id: prompt_name.to_string(),
                prompt_version: tmpl.version.clone(),
                model_id: resolved.model_id,
                max_tokens: effective_max_tokens,
                temperature: effective_temperature,
                prompt_render_hash,
            },
        ))
    }
}

/// Concrete record of one `structured_completion` invocation —
/// everything a downstream consumer needs to attribute the call,
/// replay it, or diff it against another. Folds into
/// [`ox_ontology::source_mapping::ArtifactProvenance`] via
/// [`CallProvenance::into_artifact_provenance`].
#[derive(Debug, Clone)]
pub struct CallProvenance {
    pub prompt_id: String,
    pub prompt_version: String,
    pub model_id: String,
    pub max_tokens: u32,
    pub temperature: Option<f32>,
    /// SHA-256 hex of the rendered prompt (system + user with every
    /// variable interpolated) — ADR-0029. Stays empty for paths
    /// that have not been wired through the render-hashing helper
    /// yet; a populated value bumps `ContentBody`'s hash so an
    /// admin who edited the DB-backing prompt without bumping
    /// `prompt_version` cannot accidentally re-use the prior
    /// artifact id.
    pub prompt_render_hash: String,
}

impl CallProvenance {
    /// Project into the artifact-side envelope. Numeric knobs land
    /// in `params` so a future replay can reconstruct the call
    /// shape without a parallel mapping table.
    pub fn into_artifact_provenance(self) -> ox_ontology::source_mapping::ArtifactProvenance {
        let mut params: std::collections::BTreeMap<String, serde_json::Value> =
            std::collections::BTreeMap::new();
        params.insert("max_tokens".into(), serde_json::Value::from(self.max_tokens));
        if let Some(t) = self.temperature {
            params.insert("temperature".into(), serde_json::Value::from(t));
        }
        ox_ontology::source_mapping::ArtifactProvenance {
            prompt_id: self.prompt_id,
            prompt_version: self.prompt_version,
            model_id: self.model_id,
            params,
            prompt_render_hash: self.prompt_render_hash,
        }
    }
}

fn serialize_pretty(value: &impl serde::Serialize, label: &str) -> OxResult<String> {
    serde_json::to_string_pretty(value).map_err(|e| OxError::Runtime {
        message: format!("Failed to serialize {label}: {e}"),
    })
}

#[async_trait]
impl OntologyDesigner for DefaultBrain {
    async fn design_ontology(
        &self,
        input: &DesignOntologyInput<'_>,
    ) -> OxResult<DesignOntologyOutput> {
        // Render every domain-context slice into a compact prompt
        // section. Empty slices produce empty strings so the prompt
        // collapses without conditional template syntax — no
        // leftover headers in the rendered prompt.
        let glossary_section = design::render_glossary_section(input.glossary_terms);
        let code_systems_section =
            design::render_code_systems_section(input.code_systems);
        let ambiguity_section = design::render_ambiguity_section(input.ambiguity_hints);
        let existing_ontology_section =
            design::render_existing_ontology_section(input.existing_ontology);

        let mut vars: HashMap<&str, &str> = HashMap::new();
        vars.insert("sample_data", input.sample_data);
        vars.insert("context", input.context);
        vars.insert("glossary_section", glossary_section.as_str());
        vars.insert("code_systems_section", code_systems_section.as_str());
        vars.insert("ambiguity_section", ambiguity_section.as_str());
        vars.insert("existing_ontology_section", existing_ontology_section.as_str());

        info!(
            has_domain_context = input.has_domain_context(),
            glossary_terms = input.glossary_terms.len(),
            code_systems = input.code_systems.len(),
            ambiguity_hints = input.ambiguity_hints.len(),
            extending = input.existing_ontology.is_some(),
            "Designing ontology from sample data + domain context",
        );

        // `call_structured_traced` returns the parsed output plus
        // the call's `CallProvenance` — single registry + resolver
        // round-trip, no post-hoc lookup that could drift behind a
        // concurrent admin update. The provenance folds straight
        // into the artifact-side envelope.
        let (llm_output, call): (design::LlmDesignOutput, _) = self
            .call_structured_traced(
                "design_ontology",
                Some("1.0.0"),
                "design_ontology",
                &vars,
                "Designing ontology from sample data",
            )
            .await?;
        let provenance = call.into_artifact_provenance();

        let raw_input = design::into_input_ontology(llm_output);

        let norm_result =
            ox_ontology::input::normalize(raw_input, input.source_id).map_err(|errors| {
                OxError::Ontology {
                    message: format!(
                        "LLM-generated ontology normalization failed: {}",
                        ox_core::join_messages(&errors, "; ")
                    ),
                }
            })?;
        let ontology = norm_result.ontology;

        // validate() is already called inside normalize(), but keep explicit validation
        // as a safety net
        let errors = ontology.validate();
        if !errors.is_empty() {
            return Err(OxError::Ontology {
                message: format!(
                    "LLM-generated ontology has validation errors: {}",
                    ox_core::join_messages(&errors, "; ")
                ),
            });
        }

        Ok(DesignOntologyOutput { ontology, provenance })
    }

    async fn resolve_design_provenance(
        &self,
        prompt_name: &str,
        operation: &str,
    ) -> OxResult<ox_ontology::source_mapping::ArtifactProvenance> {
        let tmpl = self.prompts.get(prompt_name)?;
        let resolved = self.model_resolver.resolve(operation).await?;
        let max_tokens = resolved.max_tokens.unwrap_or(tmpl.max_tokens);
        let temperature = resolved.temperature.or(tmpl.temperature);
        // No render hash on this path — the caller resolves
        // provenance ahead of (or independent of) the actual LLM
        // call, so there is no rendered prompt body to hash. The
        // batch path that *does* emit prompts records its render
        // hash through `call_structured` like the other LLM-driven
        // operations.
        Ok(CallProvenance {
            prompt_id: prompt_name.to_string(),
            prompt_version: tmpl.version.clone(),
            model_id: resolved.model_id,
            max_tokens,
            temperature,
            prompt_render_hash: String::new(),
        }
        .into_artifact_provenance())
    }

    async fn design_ontology_batch(
        &self,
        batch_data: &str,
        context: &str,
        existing_nodes: &str,
        cross_fks: &str,
    ) -> OxResult<ox_ontology::input::InputOntologyDef> {
        let base_prompt = self.prompts.get("design_ontology")?;
        let batch_tmpl = self.prompts.checked_for("design_ontology_batch", "1.0.0")?;

        // Inject full base instructions — token budget is safe after profile compression
        let system = batch_tmpl
            .system
            .replace("{{base_instructions}}", &base_prompt.system);

        let mut vars = HashMap::new();
        vars.insert("existing_nodes", existing_nodes);
        vars.insert("cross_fks", cross_fks);
        vars.insert("sample_data", batch_data);
        vars.insert("context", context);
        let user_prompt = batch_tmpl.render_user(&vars);

        let (client, resolved) = self.resolve_for_operation("design_ontology").await?;
        info!(
            model = %resolved.model_id,
            prompt_version = %batch_tmpl.version,
            "Designing ontology batch (divide-and-conquer)"
        );

        let llm_output: design::LlmDesignOutput = structured_completion(
            client.as_ref(),
            &resolved.model_id,
            &system,
            &user_prompt,
            resolved.max_tokens.unwrap_or(batch_tmpl.max_tokens),
            resolved.temperature.or(batch_tmpl.temperature),
        )
        .await?;
        Ok(design::into_input_ontology(llm_output))
    }

    async fn resolve_cross_edges(
        &self,
        node_labels: &str,
        existing_edges: &str,
        uncovered_fks: &str,
    ) -> OxResult<Vec<ox_ontology::InputEdgeTypeDef>> {
        let mut vars = HashMap::new();
        vars.insert("node_labels", node_labels);
        vars.insert("existing_edges", existing_edges);
        vars.insert("uncovered_fks", uncovered_fks);

        self.call_structured(
            "resolve_cross_edges",
            Some("1.0.0"),
            "resolve_cross_edges",
            &vars,
            "Resolving cross-domain edges",
        )
        .await
    }

    async fn refine_ontology(
        &self,
        ontology: &OntologyIR,
        refinement_context: &str,
        source_id: &SourceId,
    ) -> OxResult<OntologyIR> {
        let ontology_json = serialize_pretty(
            &ontology.to_agent_view(ox_core::llm_locale_fallback_default_tags()),
            "ontology",
        )?;

        let mut vars = HashMap::new();
        vars.insert("ontology", ontology_json.as_str());
        vars.insert("refinement_context", refinement_context);

        let llm_output: design::LlmDesignOutput = self
            .call_structured(
                "refine_ontology",
                Some("1.0.0"),
                "refine_ontology",
                &vars,
                "Refining ontology metadata",
            )
            .await?;
        let input = design::into_input_ontology(llm_output);

        let norm_result = ox_ontology::input::normalize(input, source_id).map_err(|errors| {
            OxError::Ontology {
                message: format!(
                    "Refined ontology normalization failed: {}",
                    ox_core::join_messages(&errors, "; ")
                ),
            }
        })?;
        let refined = norm_result.ontology;

        let errors = refined.validate();
        if !errors.is_empty() {
            return Err(OxError::Ontology {
                message: format!(
                    "Refined ontology has validation errors: {}",
                    ox_core::join_messages(&errors, "; ")
                ),
            });
        }

        Ok(refined)
    }
}

// ---------------------------------------------------------------------------
// EditCommandsResponse — internal struct for LLM structured output
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
struct EditCommandsResponse {
    commands: Vec<OntologyCommand>,
    explanation: String,
}

#[async_trait]
impl OntologyEditor for DefaultBrain {
    async fn generate_edit_commands(
        &self,
        ontology: &OntologyIR,
        user_request: &str,
    ) -> OxResult<EditCommandsOutput> {
        let ontology_json = serialize_pretty(
            &ontology.to_agent_view(ox_core::llm_locale_fallback_default_tags()),
            "ontology",
        )?;

        let mut vars = HashMap::new();
        vars.insert("ontology", ontology_json.as_str());
        vars.insert("user_request", user_request);

        // `call_structured_traced` already captures the resolved
        // model id alongside the parsed output — no second
        // `model_resolver.resolve` round-trip that could drift behind
        // a concurrent admin update.
        let (response, call): (EditCommandsResponse, _) = self
            .call_structured_traced(
                "edit_ontology",
                Some("1.0.0"),
                "edit_ontology",
                &vars,
                "Generating ontology edit commands",
            )
            .await?;
        Ok(EditCommandsOutput {
            commands: response.commands,
            explanation: response.explanation,
            model: call.model_id,
        })
    }
}

#[async_trait]
impl QueryTranslator for DefaultBrain {
    async fn translate_query(
        &self,
        question: &str,
        ontology: &OntologyIR,
        ctx: &branchforge::ExecutionContext,
    ) -> OxResult<QueryIR> {
        // Phase 1: Schema discovery
        ctx.progress("schema_discovery").started();
        let t_schema = std::time::Instant::now();

        // RAG is only used when (a) the ontology is large enough to need
        // schema selection and (b) a vector memory backend is available.
        // Binding `memory` here removes the redundant `.unwrap()`.
        let rag_memory = self
            .memory
            .as_ref()
            .filter(|_| ontology.node_types().len() > schema_rag::FULL_SCHEMA_NODE_THRESHOLD);

        let (ontology_json, discovered_labels) = if let Some(memory) = rag_memory {
            let oid = self.ontology_lineage_id.as_deref().unwrap_or(&ontology.id);
            schema_rag::discover_schema(memory, ontology, question, oid).await
        } else {
            let all_node_labels: Vec<&str> = ontology
                .node_types()
                .iter()
                .map(|n| n.label.as_str())
                .collect();
            let all_label_strings: Vec<String> = ontology
                .node_types()
                .iter()
                .map(|n| n.label.to_string())
                .chain(ontology.edge_types().iter().map(|e| e.label.to_string()))
                .collect();
            let schema = schema_rag::build_progressive_schema(ontology, &all_node_labels);
            (schema, all_label_strings)
        };

        ctx.progress("schema_discovery")
            .completed(t_schema.elapsed().as_millis() as u64);

        // Phase 2: Knowledge RAG
        ctx.progress("knowledge_lookup").started();
        let t_knowledge = std::time::Instant::now();
        let label_refs: Vec<&str> = discovered_labels.iter().map(|s| s.as_str()).collect();
        let knowledge_context = if let Some(kb) = &self.knowledge_store {
            knowledge_rag::discover_knowledge(
                kb.as_ref(),
                &label_refs,
                self.ontology_lineage_id
                    .as_deref()
                    .unwrap_or(&ontology.name),
                ontology.version.number as i32,
                8,
            )
            .await
        } else {
            String::new()
        };

        ctx.progress("knowledge_lookup")
            .completed(t_knowledge.elapsed().as_millis() as u64);

        let mut vars = HashMap::new();
        vars.insert("question", question);
        vars.insert("ontology", ontology_json.as_str());
        vars.insert("knowledge", knowledge_context.as_str());
        // Empty placeholder by default; the label-retry path below
        // overrides it with the offending labels. Keeps `{{correction}}`
        // from leaking into the LLM prompt as a literal token on the
        // happy path (PromptTemplate::render_user does plain string
        // replacement — unmatched placeholders survive).
        vars.insert("correction", "");

        // Phase 3: Primary LLM call (StructuredMatchQuery structured output)
        ctx.progress("llm_primary").started();
        let t_llm = std::time::Instant::now();
        let query_ir = match self
            .call_structured::<ox_query_ir::StructuredMatchQuery>(
                "translate_match_query",
                Some("1.3.0"),
                "translate_match_query",
                &vars,
                "Translating to StructuredMatchQuery (structured output)",
            )
            .await
            .and_then(|match_ir| match_ir.into_query_ir())
        {
            Ok(qir) => {
                ctx.progress("llm_primary")
                    .completed(t_llm.elapsed().as_millis() as u64);
                info!("StructuredMatchQuery structured output succeeded");
                qir
            }
            Err(match_err) => {
                ctx.progress("llm_primary").failed_with(t_llm.elapsed().as_millis() as u64,
                    serde_json::json!({ "error": format!("{match_err}").chars().take(200).collect::<String>() }));

                // Phase 4: Fallback LLM call (full QueryIR JSON mode)
                ctx.progress("llm_fallback").started();
                let t_fallback = std::time::Instant::now();
                info!(
                    error = %match_err,
                    "StructuredMatchQuery path failed, falling back to full QueryIR"
                );
                let result: OxResult<QueryIR> = self
                    .call_structured(
                        "translate_query",
                        Some("1.2.0"),
                        "translate_query",
                        &vars,
                        "Translating to QueryIR (JSON mode fallback)",
                    )
                    .await;

                match result {
                    Ok(qir) => {
                        ctx.progress("llm_fallback")
                            .completed(t_fallback.elapsed().as_millis() as u64);
                        qir
                    }
                    Err(first_err) => {
                        ctx.progress("llm_fallback")
                            .failed(t_fallback.elapsed().as_millis() as u64);
                        // Final retry
                        ctx.progress("llm_retry").started();
                        let t_retry = std::time::Instant::now();
                        info!(
                            error = %first_err,
                            "QueryIR translation failed, retrying once"
                        );
                        let retry_result = self
                            .call_structured::<QueryIR>(
                                "translate_query",
                                Some("1.2.0"),
                                "translate_query",
                                &vars,
                                "Retrying query translation",
                            )
                            .await
                            .map_err(|retry_err| {
                                ctx.progress("llm_retry")
                                    .failed(t_retry.elapsed().as_millis() as u64);
                                info!(
                                    first_error = %first_err,
                                    retry_error = %retry_err,
                                    "Query translation retry also failed"
                                );
                                retry_err
                            });
                        if retry_result.is_ok() {
                            ctx.progress("llm_retry")
                                .completed(t_retry.elapsed().as_millis() as u64);
                        }
                        retry_result?
                    }
                }
            }
        };

        // Pre-flight label validation. The runtime's OntologyValidator is
        // the final authority (operates on the AST, catches inline
        // property keys too), but a cheap QueryIR-level check here
        // short-circuits the agent-level retry when the LLM hallucinates
        // a label. If we spot unknown labels, retry once with the
        // offending labels listed in the prompt context. If the retry
        // still produces unknowns, surface them to the runtime — it will
        // reject consistently, so the agent can still learn via the
        // tool-error path.
        let query_ir = match ox_query_ir::unknown_labels_in_query(ontology, &query_ir) {
            unknown if unknown.is_empty() => query_ir,
            unknown => {
                ctx.progress("llm_label_retry").started();
                let t_label_retry = std::time::Instant::now();
                info!(
                    unknown_labels = ?unknown,
                    "LLM returned unknown labels; retrying translate with explicit schema context",
                );
                // Enrich the prompt variables with the specific unknown
                // labels the last attempt produced — the LLM tends to
                // correct itself when given the exact violation.
                let correction = format!(
                    "Previous attempt referenced labels that do not exist in the ontology: \
                     {}. Use only labels listed in the schema above.",
                    unknown.join(", "),
                );
                let mut retry_vars = vars.clone();
                retry_vars.insert("correction", correction.as_str());
                let retry: OxResult<QueryIR> = self
                    .call_structured(
                        "translate_query",
                        Some("1.2.0"),
                        "translate_query",
                        &retry_vars,
                        "Retrying query translation with label correction",
                    )
                    .await;
                match retry {
                    Ok(qir) => {
                        ctx.progress("llm_label_retry")
                            .completed(t_label_retry.elapsed().as_millis() as u64);
                        qir
                    }
                    Err(_) => {
                        ctx.progress("llm_label_retry")
                            .failed(t_label_retry.elapsed().as_millis() as u64);
                        // Retry failed — fall through to the downstream
                        // OntologyValidator so the agent sees a
                        // deterministic rejection.
                        query_ir
                    }
                }
            }
        };

        Ok(query_ir)
    }

    async fn plan_load(
        &self,
        ontology: &OntologyIR,
        source_description: &str,
    ) -> OxResult<LoadPlan> {
        let ontology_json = serialize_pretty(
            &ontology.to_agent_view(ox_core::llm_locale_fallback_default_tags()),
            "ontology",
        )?;

        let mut vars = HashMap::new();
        vars.insert("source_description", source_description);
        vars.insert("ontology", ontology_json.as_str());

        self.call_structured("plan_load", None, "plan_load", &vars, "Planning data load")
            .await
    }

    async fn generate_load_plan(
        &self,
        ontology: &OntologyIR,
        source_schema: &SourceSchema,
    ) -> OxResult<LoadPlan> {
        // The agent view carries the logical schema the load plan
        // targets; the ObjectMappingDef slice is serialised
        // separately so the prompt still sees the canonical wire
        // shape the planner will consume at runtime.
        let ontology_json = serialize_pretty(
            &ontology.to_agent_view(ox_core::llm_locale_fallback_default_tags()),
            "ontology",
        )?;
        let mapping_json =
            serialize_pretty(&ontology.object_mappings(), "object_mappings")?;
        let schema_json = serialize_pretty(source_schema, "source_schema")?;
        let source_description =
            format!("Object Mappings:\n{mapping_json}\n\nSource Schema:\n{schema_json}");

        let mut vars = HashMap::new();
        vars.insert("source_description", source_description.as_str());
        vars.insert("ontology", ontology_json.as_str());

        self.call_structured(
            "plan_load",
            None,
            "plan_load",
            &vars,
            "Generating load plan from project data",
        )
        .await
    }

    async fn select_widget(&self, query: &QueryIR, result_sample: &str) -> OxResult<WidgetHint> {
        let query_json = serialize_pretty(query, "query")?;

        let mut vars = HashMap::new();
        vars.insert("query", query_json.as_str());
        vars.insert("result_sample", result_sample);

        self.call_structured(
            "select_widget",
            None,
            "select_widget",
            &vars,
            "Selecting widget for query results",
        )
        .await
    }
}

#[async_trait]
impl Explainer for DefaultBrain {
    async fn explain(&self, user_message: &str) -> OxResult<ExplanationOutput> {
        let system = self
            .prompts
            .get("chat_default")
            .map(|t| t.system.clone())
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "chat_default prompt missing — using minimal fallback");
                "You are Ontosyx, a knowledge graph assistant.".to_string()
            });

        let (client, resolved) = self.resolve_for_operation("explain").await?;

        let request = branchforge::ModelRequest::new(
            &resolved.model_id,
            vec![branchforge::Message::user(user_message)],
        )
        .with_max_tokens(2048)
        .with_system(branchforge::SystemPrompt::Blocks(vec![
            branchforge::SystemBlock::cached_with_ttl(&system, "1h"),
        ]))
        .with_temperature(0.3);

        let resp = client.send(&request).await.map_err(|e| OxError::Runtime {
            message: format!("Explanation failed: {e}"),
        })?;

        Ok(ExplanationOutput {
            content: resp.text(),
            model: resolved.model_id,
            usage: Some(TokenUsage {
                input_tokens: resp.usage.input_tokens,
                output_tokens: resp.usage.output_tokens,
            }),
        })
    }

    async fn explain_stream(&self, user_message: String) -> OxResult<ExplanationStream> {
        let system = self
            .prompts
            .get("chat_default")
            .map(|t| t.system.clone())
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "chat_default prompt missing — using minimal fallback");
                "You are Ontosyx, a knowledge graph assistant.".to_string()
            });

        let (client, resolved) = self.resolve_for_operation("explain").await?;

        let request = branchforge::ModelRequest::new(
            &resolved.model_id,
            vec![branchforge::Message::user(&user_message)],
        )
        .with_max_tokens(2048)
        .with_system(branchforge::SystemPrompt::Blocks(vec![
            branchforge::SystemBlock::cached_with_ttl(&system, "1h"),
        ]))
        .with_temperature(0.3);

        let cancel = branchforge::CancellationToken::new();
        let stream = client
            .send_stream(&request, cancel)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Explanation stream failed: {e}"),
            })?;

        // Convert branchforge ModelStreamChunk stream to ox-brain StreamChunk stream
        let chunk_stream = async_stream::stream! {
            let mut stream = std::pin::pin!(stream);
            while let Some(item) = tokio_stream::StreamExt::next(&mut stream).await {
                match item {
                    Ok(branchforge::ModelStreamChunk::TextDelta { text, .. }) => {
                        yield Ok(StreamChunk {
                            delta: text,
                            is_final: false,
                            usage: None,
                        });
                    }
                    Ok(_) => {
                        // Ignore non-text stream chunks (Reasoning, ToolCall, Usage, etc.)
                    }
                    Err(e) => {
                        yield Err(OxError::Runtime {
                            message: format!("Stream error: {e}"),
                        });
                        return;
                    }
                }
            }
            // Emit final chunk
            yield Ok(StreamChunk {
                delta: String::new(),
                is_final: true,
                usage: None,
            });
        };

        Ok(Box::pin(chunk_stream))
    }

    async fn suggest_insights(
        &self,
        ontology: &OntologyIR,
        graph_stats: Option<&serde_json::Value>,
    ) -> OxResult<Vec<ox_ontology::InsightHint>> {
        let nodes: Vec<String> = ontology
            .node_types()
            .iter()
            .map(|n| {
                format!(
                    "{}({})",
                    n.label,
                    n.properties
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect();
        let edges: Vec<String> = ontology
            .edge_types()
            .iter()
            .map(|e| {
                let src = ontology
                    .node_types()
                    .iter()
                    .find(|n| n.id == e.source_node_id)
                    .map(|n| n.label.as_str())
                    .unwrap_or("?");
                let tgt = ontology
                    .node_types()
                    .iter()
                    .find(|n| n.id == e.target_node_id)
                    .map(|n| n.label.as_str())
                    .unwrap_or("?");
                format!("({})-[:{}]->({})", src, e.label, tgt)
            })
            .collect();

        let schema_summary = format!(
            "Nodes:\n{}\n\nEdges:\n{}",
            nodes.join("\n"),
            edges.join("\n")
        );
        let stats_text = graph_stats
            .map(|s| {
                format!(
                    "\n\nGraph statistics:\n{}",
                    serde_json::to_string_pretty(s).unwrap_or_default()
                )
            })
            .unwrap_or_default();

        let user_prompt = format!(
            "Given this knowledge graph schema:\n{schema_summary}{stats_text}\n\n\
            Generate exactly 5 insightful questions a data analyst would ask about this data.\n\
            For each, specify:\n\
            - question: the natural language question\n\
            - category: one of \"trend\", \"distribution\", \"anomaly\", \"relationship\", \"summary\"\n\
            - suggested_tool: \"query_graph\" for data retrieval, \"execute_analysis\" for statistical analysis\n\n\
            Return as a JSON array of objects."
        );

        let system = "You are a data analyst assistant. Generate insightful questions about knowledge graphs. Return only valid JSON.";
        let (client, resolved) = self.resolve_for_operation("suggest_insights").await?;

        info!(
            model = %resolved.model_id,
            "Generating insight suggestions"
        );

        match structured_completion::<Vec<ox_ontology::InsightHint>>(
            client.as_ref(),
            &resolved.model_id,
            system,
            &user_prompt,
            2048,
            Some(0.7),
        )
        .await
        {
            Ok(suggestions) => Ok(suggestions),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to generate insight suggestions");
                Ok(vec![])
            }
        }
    }
}

#[async_trait]
impl RepoAnalyzer for DefaultBrain {
    async fn navigate_repo(&self, file_tree: &str) -> OxResult<Vec<String>> {
        let mut vars = HashMap::new();
        vars.insert("file_tree", file_tree);

        let selection: ox_ontology::repo_insights::FileSelection = self
            .call_structured(
                "repo_navigate",
                None,
                "repo_navigate",
                &vars,
                "Navigating repo file tree",
            )
            .await?;

        Ok(selection.files)
    }

    async fn analyze_repo_files(&self, files: &[FileContent]) -> OxResult<RepoInsights> {
        // Serialize files as a structured block for the LLM
        let files_text = files
            .iter()
            .map(|f| format!("=== {} ===\n{}", f.relative_path, f.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        let mut vars = HashMap::new();
        vars.insert("files", files_text.as_str());

        self.call_structured(
            "repo_analyze",
            None,
            "repo_analyze",
            &vars,
            "Analyzing repo files for domain insights",
        )
        .await
    }
}

#[async_trait]
impl LlmMetadata for DefaultBrain {
    fn default_model_info(&self) -> ProviderInfo {
        self.default_model.clone()
    }

    fn list_prompts(&self) -> Vec<(String, String)> {
        self.prompts
            .list()
            .into_iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect()
    }

    fn prompt_template_hash(&self, prompt_name: &str) -> OxResult<String> {
        // Hash the template's system body only — the user-side
        // template renders per-call with caller-supplied variables
        // (sample data, context strings, …) and a cache that wants
        // to invalidate on prompt edits but NOT on per-call shape
        // must keep those out of the fingerprint. System-only
        // hashing folds in the prompt's semantic instructions while
        // staying call-shape-agnostic.
        let tmpl = self.prompts.get(prompt_name)?;
        Ok(
            ox_ontology::source_mapping::ArtifactProvenance::compute_prompt_render_hash(
                &tmpl.system,
            ),
        )
    }
}
