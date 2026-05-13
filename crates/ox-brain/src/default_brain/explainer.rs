//! `Explainer` impl for [`DefaultBrain`].

use std::collections::HashMap;

use async_trait::async_trait;
use entelix::ExecutionContext;

use ox_core::error::{OxError, OxResult};
use ox_ontology::ir::OntologyIR;

use crate::model_resolver::operation;
use crate::*;

#[async_trait]
impl Explainer for DefaultBrain {
    async fn explain(
        &self,
        user_message: &str,
        ctx: &ExecutionContext,
    ) -> OxResult<ExplanationOutput> {
        // `chat_default` is the canonical free-form chat prompt;
        // its system body is the operator-curated persona. Route
        // through `call_text_traced` so the budget gate / cost
        // observation / OTel span / evaluation capture pipeline
        // applies uniformly with the structured paths.
        let mut vars: HashMap<&str, &str> = HashMap::new();
        vars.insert("message", user_message);
        let (content, usage, provenance) = self
            .call_text_traced(
                "chat_default",
                None,
                operation::EXPLAIN,
                &vars,
                "Generating free-form explanation",
                ctx,
            )
            .await?;

        Ok(ExplanationOutput {
            content,
            model: provenance.model_id,
            usage: Some(usage),
        })
    }

    async fn suggest_insights(
        &self,
        ontology: &OntologyIR,
        graph_stats: Option<&serde_json::Value>,
        ctx: &ExecutionContext,
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

        let mut vars: HashMap<&str, &str> = HashMap::new();
        vars.insert("schema_summary", schema_summary.as_str());
        vars.insert("stats_text", stats_text.as_str());

        // Suggest-insights is best-effort: a parse failure or LLM
        // hiccup returns an empty vec rather than surfacing the
        // error — the dashboard renders an empty suggestion strip,
        // not an error toast. Cost / budget caps still fire normally
        // (those propagate via `?` from the upstream funnel).
        match self
            .call_structured::<Vec<ox_ontology::InsightHint>>(
                operation::SUGGEST_INSIGHTS,
                Some("1.0.0"),
                operation::SUGGEST_INSIGHTS,
                &vars,
                "Generating insight suggestions",
                ctx,
            )
            .await
        {
            Ok(suggestions) => Ok(suggestions),
            // Typed LLM failures (budget / auth / rate-limit / 5xx)
            // propagate so the operator sees the structured failure
            // mode. Other variants degrade to an empty suggestion
            // strip — the dashboard renders nothing rather than an
            // error toast.
            Err(err @ OxError::Llm { .. }) => Err(err),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to generate insight suggestions");
                Ok(vec![])
            }
        }
    }
}
