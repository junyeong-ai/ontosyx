use std::sync::Arc;

use async_trait::async_trait;
use branchforge::tools::ExecutionContext;
use branchforge::{SchemaTool, ToolResult};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::DomainContext;

// ---------------------------------------------------------------------------
// ExplainOntologyTool — explain ontology concepts with data context
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExplainInput {
    /// What to explain — a concept, relationship, or pattern question.
    pub question: String,
}

/// Provides explanations about the ontology structure, node/edge relationships,
/// data patterns, and quality insights. Includes actual graph statistics when
/// a graph runtime is available.
pub struct ExplainOntologyTool {
    pub domain: Arc<DomainContext>,
    pub brain: Arc<dyn ox_brain::Brain>,
}

#[async_trait]
impl SchemaTool for ExplainOntologyTool {
    type Input = ExplainInput;
    const NAME: &'static str = super::EXPLAIN_ONTOLOGY;
    const DESCRIPTION: &'static str = "Explain ontology concepts, relationships, and data patterns. \
         Use for 'what is', 'explain', 'describe' questions — live graph stats when a runtime is available.";
    const READ_ONLY: bool = true;

    async fn handle(&self, input: Self::Input, _ctx: &ExecutionContext) -> ToolResult {
        let mut context = String::new();

        if let Some(ontology) = self.domain.current_ontology() {
            // Only user-meaningful signal goes to the LLM: labels and shape.
            // Internal UUIDs, property counts, and the monotonic version
            // number add tokens without helping the model reason.
            let node_labels = ontology
                .node_types()
                .iter()
                .map(|n| n.label.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let edge_labels = ontology
                .edge_types()
                .iter()
                .map(|e| {
                    let src = ontology.node_label(&e.source_node_id).unwrap_or("?");
                    let tgt = ontology.node_label(&e.target_node_id).unwrap_or("?");
                    format!("{} ({src}→{tgt})", e.label)
                })
                .collect::<Vec<_>>()
                .join(", ");
            context.push_str(&format!(
                "Ontology: {}\nNode types: {node_labels}\nEdge types: {edge_labels}\n\n",
                ontology.name,
            ));

            // Fetch live graph statistics if runtime is available (timeout: 15s)
            if let Some(runtime) = &self.domain.runtime {
                let empty_params = std::collections::HashMap::new();
                match tokio::time::timeout(
                    std::time::Duration::from_secs(15),
                    runtime.execute_query(
                        "CALL db.labels() YIELD label \
                         CALL { WITH label MATCH (n) WHERE label IN labels(n) RETURN count(n) AS cnt } \
                         RETURN label, cnt ORDER BY cnt DESC",
                        &empty_params,
                    ),
                )
                .await
                {
                    Ok(Ok(stats)) => {
                        context.push_str("Live graph statistics:\n");
                        for row in &stats.rows {
                            if row.len() >= 2 {
                                context.push_str(&format!("  {:?}: {:?} nodes\n", row[0], row[1]));
                            }
                        }
                        context.push('\n');
                    }
                    Ok(Err(_)) | Err(_) => {
                        context.push_str("(Graph statistics unavailable)\n\n");
                    }
                }
            }
        }

        context.push_str(&format!(
            "User question: {}\n\nProvide a clear, helpful explanation.",
            input.question
        ));

        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(120),
            self.brain.explain(&context),
        )
        .await
        {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return ToolResult::error(format!("Explanation failed: {e}")),
            Err(_) => return ToolResult::error("Explanation timed out after 120 seconds"),
        };

        ToolResult::success(output.content)
    }
}
