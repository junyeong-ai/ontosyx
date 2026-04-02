pub mod auth;
pub mod client_pool;
pub mod model_resolver;
pub mod prompts;
pub mod provider;
pub mod schema;
pub mod knowledge_rag;
pub mod knowledge_util;
pub mod schema_rag;

use async_trait::async_trait;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tracing::{info, warn};

use ox_core::error::{OxError, OxResult};
use ox_core::load_plan::LoadPlan;
use ox_core::ontology_command::OntologyCommand;
use ox_core::ontology_ir::OntologyIR;
use ox_core::query_ir::QueryIR;
use ox_core::repo_insights::{FileContent, RepoInsights};
use ox_core::source_mapping::SourceMapping;
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
    /// Analyze sample data and design an ontology.
    /// Returns both the ontology and the source mapping extracted from LLM output.
    async fn design_ontology(
        &self,
        sample_data: &str,
        context: &str,
    ) -> OxResult<(OntologyIR, SourceMapping)>;

    /// Design a partial ontology for a batch of tables (divide-and-conquer pipeline).
    /// Returns raw `OntologyInputIR` (not normalized) for later merging.
    async fn design_ontology_batch(
        &self,
        batch_data: &str,
        context: &str,
        existing_nodes: &str,
        cross_fks: &str,
    ) -> OxResult<ox_core::ontology_input::OntologyInputIR>;

    /// Generate missing cross-domain edges for uncovered FK relationships.
    /// Returns edge definitions to be appended to the merged InputIR.
    async fn resolve_cross_edges(
        &self,
        node_labels: &str,
        existing_edges: &str,
        uncovered_fks: &str,
    ) -> OxResult<Vec<ox_core::InputEdgeTypeDef>>;

    /// Refine an ontology's metadata using graph profile statistics and/or additional context.
    /// `refinement_context` is pre-formatted and may contain graph profile data,
    /// domain gap resolutions, or both combined.
    /// Returns both the refined ontology and the source mapping extracted from LLM output.
    async fn refine_ontology(
        &self,
        ontology: &OntologyIR,
        refinement_context: &str,
    ) -> OxResult<(OntologyIR, SourceMapping)>;
}

/// Query translation and widget selection capabilities.
#[async_trait]
pub trait QueryTranslator: Send + Sync {
    /// Translate natural language question into a QueryIR
    async fn translate_query(&self, question: &str, ontology: &OntologyIR) -> OxResult<QueryIR>;

    /// Generate a LoadPlan from an ontology and source data description
    async fn plan_load(
        &self,
        ontology: &OntologyIR,
        source_description: &str,
    ) -> OxResult<LoadPlan>;

    /// Generate a LoadPlan from ontology + source mapping + source schema.
    /// Uses the source_mapping and source_schema for higher-quality plans.
    async fn generate_load_plan(
        &self,
        ontology: &OntologyIR,
        source_mapping: &SourceMapping,
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
    ) -> OxResult<Vec<ox_core::InsightSuggestion>>;
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
    ontology_id: Option<String>,
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
            ontology_id: None,
            knowledge_store: None,
        }
    }

    /// Set memory store for schema RAG in query translation.
    pub fn with_memory(
        mut self,
        memory: Arc<ox_memory::MemoryStore>,
        ontology_id: Option<String>,
    ) -> Self {
        self.memory = Some(memory);
        self.ontology_id = ontology_id;
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
    /// Uses `get_by_provider` to look up the already-authenticated client
    /// from the pool — no credentials needed since the client was pre-warmed
    /// during server startup.
    async fn resolve_for_operation(
        &self,
        operation: &str,
    ) -> OxResult<(Arc<branchforge::Client>, model_resolver::ResolvedModel)> {
        let resolved = self.model_resolver.resolve(operation).await?;
        let client = self
            .client_pool
            .get_by_provider(&resolved.provider)
            .ok_or_else(|| OxError::Runtime {
                message: format!(
                    "No LLM client available for provider '{}'. \
                     Ensure it was registered during server startup.",
                    resolved.provider
                ),
            })?;
        Ok((client, resolved))
    }

    /// Core LLM call: resolve model via operation name, load prompt template,
    /// render variables, call structured_completion with prompt caching.
    async fn call_structured<T: serde::de::DeserializeOwned + schemars::JsonSchema>(
        &self,
        prompt_name: &str,
        min_version: Option<&str>,
        operation: &str,
        vars: &HashMap<&str, &str>,
        log_message: &str,
    ) -> OxResult<T> {
        let tmpl = match min_version {
            Some(v) => self.prompts.get_checked(prompt_name, v)?,
            None => self.prompts.get(prompt_name)?,
        };
        let user_prompt = tmpl.render_user(vars);

        let (client, resolved) = self.resolve_for_operation(operation).await?;
        info!(
            model = %resolved.model_id,
            operation,
            prompt_version = %tmpl.version,
            "{log_message}"
        );

        structured_completion(
            &client,
            &resolved.model_id,
            &tmpl.system,
            &user_prompt,
            resolved.max_tokens.unwrap_or(tmpl.max_tokens),
            resolved.temperature.or(tmpl.temperature),
        )
        .await
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
        sample_data: &str,
        context: &str,
    ) -> OxResult<(OntologyIR, SourceMapping)> {
        let mut vars = HashMap::new();
        vars.insert("sample_data", sample_data);
        vars.insert("context", context);

        let input: ox_core::ontology_input::OntologyInputIR = self
            .call_structured(
                "design_ontology",
                Some("2.0.0"),
                "design_ontology",
                &vars,
                "Designing ontology from sample data",
            )
            .await?;

        let norm_result =
            ox_core::ontology_input::normalize(input).map_err(|errors| OxError::Ontology {
                message: format!(
                    "LLM-generated ontology normalization failed: {}",
                    errors.join("; ")
                ),
            })?;
        let ontology = norm_result.ontology;
        let source_mapping = norm_result.source_mapping;

        // validate() is already called inside normalize(), but keep explicit validation
        // as a safety net
        let errors = ontology.validate();
        if !errors.is_empty() {
            return Err(OxError::Ontology {
                message: format!(
                    "LLM-generated ontology has validation errors: {}",
                    errors.join("; ")
                ),
            });
        }

        Ok((ontology, source_mapping))
    }

    async fn design_ontology_batch(
        &self,
        batch_data: &str,
        context: &str,
        existing_nodes: &str,
        cross_fks: &str,
    ) -> OxResult<ox_core::ontology_input::OntologyInputIR> {
        let base_prompt = self.prompts.get("design_ontology")?;
        let batch_tmpl = self.prompts.get_checked("design_ontology_batch", "1.0.0")?;

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

        structured_completion(
            &client,
            &resolved.model_id,
            &system,
            &user_prompt,
            resolved.max_tokens.unwrap_or(batch_tmpl.max_tokens),
            resolved.temperature.or(batch_tmpl.temperature),
        )
        .await
    }

    async fn resolve_cross_edges(
        &self,
        node_labels: &str,
        existing_edges: &str,
        uncovered_fks: &str,
    ) -> OxResult<Vec<ox_core::InputEdgeTypeDef>> {
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
    ) -> OxResult<(OntologyIR, SourceMapping)> {
        let ontology_json = serialize_pretty(ontology, "ontology")?;

        let mut vars = HashMap::new();
        vars.insert("ontology", ontology_json.as_str());
        vars.insert("refinement_context", refinement_context);

        let input: ox_core::ontology_input::OntologyInputIR = self
            .call_structured(
                "refine_ontology",
                Some("1.1.0"),
                "refine_ontology",
                &vars,
                "Refining ontology metadata",
            )
            .await?;

        let norm_result =
            ox_core::ontology_input::normalize(input).map_err(|errors| OxError::Ontology {
                message: format!(
                    "Refined ontology normalization failed: {}",
                    errors.join("; ")
                ),
            })?;
        let refined = norm_result.ontology;
        let source_mapping = norm_result.source_mapping;

        let errors = refined.validate();
        if !errors.is_empty() {
            return Err(OxError::Ontology {
                message: format!(
                    "Refined ontology has validation errors: {}",
                    errors.join("; ")
                ),
            });
        }

        Ok((refined, source_mapping))
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
        let ontology_json = serialize_pretty(ontology, "ontology")?;

        let mut vars = HashMap::new();
        vars.insert("ontology", ontology_json.as_str());
        vars.insert("user_request", user_request);

        let response: EditCommandsResponse = self
            .call_structured(
                "edit_ontology",
                Some("1.0.0"),
                "edit_ontology",
                &vars,
                "Generating ontology edit commands",
            )
            .await?;

        let resolved = self.model_resolver.resolve("edit_ontology").await?;
        Ok(EditCommandsOutput {
            commands: response.commands,
            explanation: response.explanation,
            model: resolved.model_id,
        })
    }
}

#[async_trait]
impl QueryTranslator for DefaultBrain {
    async fn translate_query(&self, question: &str, ontology: &OntologyIR) -> OxResult<QueryIR> {
        // Schema RAG: discover relevant sub-schema and which labels are relevant
        let (ontology_json, discovered_labels) = if let Some(memory) = &self.memory {
            let oid = self.ontology_id.as_deref().unwrap_or(&ontology.id);
            schema_rag::discover_schema(memory, ontology, question, oid).await
        } else if ontology.node_types.len() <= 20 {
            // Small ontology: use all labels as both seed and expanded
            let all_node_labels: Vec<&str> = ontology
                .node_types
                .iter()
                .map(|n| n.label.as_str())
                .collect();
            let all_label_strings: Vec<String> = ontology.node_types.iter().map(|n| n.label.clone())
                .chain(ontology.edge_types.iter().map(|e| e.label.clone()))
                .collect();
            // Use progressive schema even for small ontologies (consistent format)
            let schema = schema_rag::build_progressive_schema(
                ontology, &all_node_labels, &all_node_labels,
            );
            (schema, all_label_strings)
        } else {
            let all_labels: Vec<String> = ontology.node_types.iter().map(|n| n.label.clone())
                .chain(ontology.edge_types.iter().map(|e| e.label.clone()))
                .collect();
            (serialize_pretty(ontology, "ontology")?, all_labels)
        };

        // Knowledge RAG: label-based lookup using question-relevant labels only
        let label_refs: Vec<&str> = discovered_labels.iter().map(|s| s.as_str()).collect();
        let knowledge_context = if let Some(kb) = &self.knowledge_store {
            knowledge_rag::discover_knowledge(
                kb.as_ref(),
                &label_refs,
                self.ontology_id.as_deref().unwrap_or(&ontology.name),
                ontology.version as i32,
                8,
            )
            .await
        } else {
            String::new()
        };

        let mut vars = HashMap::new();
        vars.insert("question", question);
        vars.insert("ontology", ontology_json.as_str());
        vars.insert("knowledge", knowledge_context.as_str());

        // Attempt 1: standard structured completion
        let result: OxResult<QueryIR> = self
            .call_structured(
                "translate_query",
                Some("2.0.0"),
                "translate_query",
                &vars,
                "Translating natural language to QueryIR",
            )
            .await;

        let query_ir = match result {
            Ok(qir) => qir,
            Err(first_err) => {
                // Attempt 2: retry once on deserialization failure
                info!(
                    error = %first_err,
                    "Query translation failed, retrying once"
                );
                let retry_result = self.call_structured::<QueryIR>(
                    "translate_query",
                    Some("2.0.0"),
                    "translate_query",
                    &vars,
                    "Retrying query translation",
                )
                .await;

                retry_result.map_err(|retry_err| {
                    info!(retry_error = %retry_err, "Query translation retry also failed");
                    first_err
                })?
            }
        };

        // Post-translation validation: reject queries with non-existent labels.
        let warnings = validate_query_labels(&query_ir, ontology);
        if !warnings.is_empty() {
            let available: Vec<String> = ontology
                .node_types
                .iter()
                .map(|n| n.label.clone())
                .collect();
            let msg = format!(
                "Query references unknown labels: {}. Available node types: {}",
                warnings.join("; "),
                available.join(", "),
            );
            warn!(%msg, "Rejecting query with invalid labels");
            return Err(OxError::Validation {
                field: "query_ir".to_string(),
                message: msg,
            });
        }

        Ok(query_ir)
    }

    async fn plan_load(
        &self,
        ontology: &OntologyIR,
        source_description: &str,
    ) -> OxResult<LoadPlan> {
        let ontology_json = serialize_pretty(ontology, "ontology")?;

        let mut vars = HashMap::new();
        vars.insert("source_description", source_description);
        vars.insert("ontology", ontology_json.as_str());

        self.call_structured("plan_load", None, "plan_load", &vars, "Planning data load")
            .await
    }

    async fn generate_load_plan(
        &self,
        ontology: &OntologyIR,
        source_mapping: &SourceMapping,
        source_schema: &SourceSchema,
    ) -> OxResult<LoadPlan> {
        let ontology_json = serialize_pretty(ontology, "ontology")?;
        let mapping_json = serialize_pretty(source_mapping, "source_mapping")?;
        let schema_json = serialize_pretty(source_schema, "source_schema")?;
        let source_description =
            format!("Source Mapping:\n{mapping_json}\n\nSource Schema:\n{schema_json}");

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

        let cached_system = branchforge::types::SystemPrompt::Blocks(vec![
            branchforge::types::SystemBlock::cached_with_ttl(
                &system,
                branchforge::types::CacheTtl::OneHour,
            ),
        ]);

        let request = branchforge::client::CreateMessageRequest::new(
            &resolved.model_id,
            vec![branchforge::types::Message::user(user_message)],
        )
        .max_tokens(2048)
        .system(cached_system)
        .temperature(0.3);

        let resp = client.send(request).await.map_err(|e| OxError::Runtime {
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

        let cached_system = branchforge::types::SystemPrompt::Blocks(vec![
            branchforge::types::SystemBlock::cached_with_ttl(
                &system,
                branchforge::types::CacheTtl::OneHour,
            ),
        ]);

        let request = branchforge::client::CreateMessageRequest::new(
            &resolved.model_id,
            vec![branchforge::types::Message::user(&user_message)],
        )
        .max_tokens(2048)
        .system(cached_system)
        .temperature(0.3);

        let stream = client
            .stream_request(request)
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Explanation stream failed: {e}"),
            })?;

        // Convert branchforge text stream to ox-brain StreamChunk stream
        let chunk_stream = async_stream::stream! {
            let mut stream = std::pin::pin!(stream);
            while let Some(item) = tokio_stream::StreamExt::next(&mut stream).await {
                match item {
                    Ok(branchforge::client::StreamItem::Text(text)) => {
                        yield Ok(StreamChunk {
                            delta: text,
                            is_final: false,
                            usage: None,
                        });
                    }
                    Ok(_) => {
                        // Ignore non-text stream items (Thinking, Citation, etc.)
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
    ) -> OxResult<Vec<ox_core::InsightSuggestion>> {
        let nodes: Vec<String> = ontology
            .node_types
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
            .edge_types
            .iter()
            .map(|e| {
                let src = ontology
                    .node_types
                    .iter()
                    .find(|n| n.id == e.source_node_id)
                    .map(|n| n.label.as_str())
                    .unwrap_or("?");
                let tgt = ontology
                    .node_types
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

        match structured_completion::<Vec<ox_core::InsightSuggestion>>(
            &client,
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

        let selection: ox_core::repo_insights::FileSelection = self
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
}

// ---------------------------------------------------------------------------
// Query validation — post-translation label checks
// ---------------------------------------------------------------------------

/// Validate that all node/edge labels in a QueryIR exist in the ontology.
/// Returns a list of warnings (not errors) for labels that don't match.
fn validate_query_labels(query: &QueryIR, ontology: &OntologyIR) -> Vec<String> {
    let node_labels = ox_core::eval::extract_node_labels(query);
    let edge_labels = ox_core::eval::extract_edge_labels(query);

    let valid_node_labels: std::collections::HashSet<&str> = ontology
        .node_types
        .iter()
        .map(|n| n.label.as_str())
        .collect();
    let valid_edge_labels: std::collections::HashSet<&str> = ontology
        .edge_types
        .iter()
        .map(|e| e.label.as_str())
        .collect();

    let mut warnings = Vec::new();

    for label in &node_labels {
        if !valid_node_labels.contains(label.as_str()) {
            warnings.push(format!("Node label '{label}' not in ontology"));
        }
    }
    for label in &edge_labels {
        if !valid_edge_labels.contains(label.as_str()) {
            warnings.push(format!("Edge label '{label}' not in ontology"));
        }
    }

    warnings
}
