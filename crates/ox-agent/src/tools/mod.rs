//! Domain-specific tools for the Ontosyx agent.
//!
//! Each tool implements branchforge's `SchemaTool` trait with strongly-typed
//! input/output schemas. The agent selects and invokes tools autonomously
//! based on user intent.

mod apply_ontology;
mod edit_ontology;
mod execute_analysis;
mod explain;
pub mod introspect_source;
mod query_graph;
mod recall_memory;
mod schema_evolution;
mod search_recipes;
mod visualize;

pub use apply_ontology::ApplyOntologyTool;
pub use edit_ontology::EditOntologyTool;
pub use execute_analysis::{ExecuteAnalysisTool, SandboxResult, run_analysis_sandbox};
pub use explain::ExplainOntologyTool;
pub use introspect_source::IntrospectSourceTool;
pub use query_graph::QueryGraphTool;
pub use recall_memory::RecallMemoryTool;
pub use schema_evolution::SchemaEvolutionTool;
pub use search_recipes::SearchRecipesTool;
pub use visualize::VisualizeTool;

/// Tool name constants for ToolSurface configuration.
pub const QUERY_GRAPH: &str = "query_graph";
pub const EDIT_ONTOLOGY: &str = "edit_ontology";
pub const APPLY_ONTOLOGY: &str = "apply_ontology";
pub const EXECUTE_ANALYSIS: &str = "execute_analysis";
pub const EXPLAIN_ONTOLOGY: &str = "explain_ontology";
pub const VISUALIZE: &str = "visualize";
pub const RECALL_MEMORY: &str = "recall_memory";
pub const SEARCH_RECIPES: &str = "search_recipes";
pub const INTROSPECT_SOURCE: &str = "introspect_source";
pub const SCHEMA_EVOLUTION: &str = "schema_evolution";
