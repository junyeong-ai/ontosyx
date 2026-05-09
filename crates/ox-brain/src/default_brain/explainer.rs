//! `Explainer` impl for [`DefaultBrain`].

use async_trait::async_trait;
use tracing::info;

use ox_core::error::{OxError, OxResult};
use ox_ontology::ir::OntologyIR;

use crate::model_resolver::operation;
use crate::provider::{StreamChunk, TokenUsage, structured_completion};
use crate::*;

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

        let (client, resolved) = self.resolve_for_operation(operation::EXPLAIN).await?;

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

        let (client, resolved) = self.resolve_for_operation(operation::EXPLAIN).await?;

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
        let (client, resolved) = self
            .resolve_for_operation(operation::SUGGEST_INSIGHTS)
            .await?;

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
            Ok((suggestions, _usage)) => Ok(suggestions),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to generate insight suggestions");
                Ok(vec![])
            }
        }
    }
}
