use ox_core::error::OxResult;
use ox_core::query_ir::{AnalyticsSource, GraphAlgorithm, PathAlgorithm, QueryOp};

use super::expr::{compile_agg_function, compile_expr, compile_order_by, compile_projection};
use super::mutate::compile_mutate_op;
use super::params::{ParamCollector, escape_identifier};
use super::pattern::{
    compile_chain_step, compile_node_ref_inline, compile_pattern, format_direction_pattern,
};

pub(super) fn compile_op(
    op: &QueryOp,
    parts: &mut Vec<String>,
    pc: &mut ParamCollector,
) -> OxResult<()> {
    match op {
        QueryOp::Match {
            patterns,
            filter,
            projections,
            optional,
            group_by: _, // Cypher infers GROUP BY from aggregation functions in RETURN
        } => {
            let keyword = if *optional { "OPTIONAL MATCH" } else { "MATCH" };
            for pattern in patterns {
                parts.push(format!("{keyword} {}", compile_pattern(pattern, pc)));
            }
            if let Some(filter) = filter {
                parts.push(format!("WHERE {}", compile_expr(filter, pc)));
            }
            if !projections.is_empty() {
                parts.push(format!(
                    "RETURN {}",
                    projections
                        .iter()
                        .map(|p| compile_projection(p, pc))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }

        QueryOp::PathFind {
            start,
            end,
            edge_types,
            direction,
            max_depth,
            algorithm,
        } => {
            let rel_types = if edge_types.is_empty() {
                String::new()
            } else {
                let escaped: Vec<String> =
                    edge_types.iter().map(|t| escape_identifier(t)).collect();
                format!(":{}", escaped.join("|"))
            };
            let depth = max_depth.map(|d| format!("*..{d}")).unwrap_or_default();
            let path_fn = match algorithm {
                PathAlgorithm::ShortestPath => "shortestPath",
                PathAlgorithm::AllShortestPaths => "allShortestPaths",
                PathAlgorithm::AllPaths => {
                    // AllPaths uses variable-length pattern, not a function
                    let start_pat = compile_node_ref_inline(
                        &start.variable,
                        &start.label,
                        &start.property_filters,
                        pc,
                    );
                    let end_pat = compile_node_ref_inline(
                        &end.variable,
                        &end.label,
                        &end.property_filters,
                        pc,
                    );
                    let rel = format_direction_pattern(&format!("[{rel_types}{depth}]"), direction);
                    parts.push(format!("MATCH p = {start_pat}{rel}{end_pat}"));
                    parts.push("RETURN p".to_string());
                    return Ok(());
                }
            };
            let start_pat =
                compile_node_ref_inline(&start.variable, &start.label, &start.property_filters, pc);
            let end_pat =
                compile_node_ref_inline(&end.variable, &end.label, &end.property_filters, pc);
            let rel = format_direction_pattern(&format!("[{rel_types}{depth}]"), direction);
            parts.push(format!("MATCH p = {path_fn}({start_pat}{rel}{end_pat})"));
            parts.push("RETURN p".to_string());
        }

        QueryOp::Aggregate {
            source,
            group_by,
            aggregations,
        } => {
            // Compile the source query without its own RETURN
            compile_op(&source.operation, parts, pc)?;

            // Remove the last RETURN if it exists (we'll add our own)
            if parts.last().is_some_and(|p| p.starts_with("RETURN")) {
                parts.pop();
            }

            let mut return_items = Vec::new();
            for g in group_by {
                let field = if let Some(ref f) = g.field {
                    let expr = format!("{}.{}", g.variable, escape_identifier(f));
                    // Add alias so ORDER BY can reference by name (e.g. ca.`name` AS name)
                    format!("{expr} AS {}", escape_identifier(f))
                } else {
                    g.variable.clone()
                };
                return_items.push(field);
            }
            for agg in aggregations {
                let field = if let Some(ref f) = agg.field.field {
                    format!("{}.{}", agg.field.variable, escape_identifier(f))
                } else {
                    agg.field.variable.clone()
                };
                let func = compile_agg_function(&agg.function, &field, agg.distinct);
                return_items.push(format!("{func} AS {}", agg.alias));
            }
            parts.push(format!("RETURN {}", return_items.join(", ")));
        }

        QueryOp::Union { queries, all } => {
            let union_keyword = if *all { "UNION ALL" } else { "UNION" };
            let compiled: Vec<String> = queries
                .iter()
                .map(|q| -> OxResult<String> {
                    let mut sub_parts = Vec::new();
                    compile_op(&q.operation, &mut sub_parts, pc)?;
                    if !q.order_by.is_empty() {
                        sub_parts.push(compile_order_by(&q.order_by, pc));
                    }
                    Ok(sub_parts.join("\n"))
                })
                .collect::<OxResult<Vec<String>>>()?;
            parts.push(compiled.join(&format!("\n{union_keyword}\n")));
        }

        QueryOp::Chain { steps } => {
            for step in steps {
                compile_chain_step(step, parts, pc)?;
            }
        }

        QueryOp::CallSubquery {
            inner,
            import_variables,
        } => {
            let mut inner_parts = Vec::new();
            if !import_variables.is_empty() {
                inner_parts.push(format!("WITH {}", import_variables.join(", ")));
            }
            compile_op(&inner.operation, &mut inner_parts, pc)?;
            if !inner.order_by.is_empty() {
                inner_parts.push(compile_order_by(&inner.order_by, pc));
            }
            if let Some(skip) = inner.skip {
                inner_parts.push(format!("SKIP {skip}"));
            }
            if let Some(limit) = inner.limit {
                inner_parts.push(format!("LIMIT {limit}"));
            }
            parts.push(format!("CALL {{\n  {}\n}}", inner_parts.join("\n  ")));
        }

        QueryOp::Mutate {
            context,
            operations,
            returning,
        } => {
            if let Some(ctx) = context {
                compile_op(ctx, parts, pc)?;
                // Remove RETURN from context (mutation follows)
                if parts.last().is_some_and(|p| p.starts_with("RETURN")) {
                    parts.pop();
                }
            }
            for op in operations {
                parts.push(compile_mutate_op(op, pc));
            }
            if !returning.is_empty() {
                parts.push(format!(
                    "RETURN {}",
                    returning
                        .iter()
                        .map(|p| compile_projection(p, pc))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }

        QueryOp::Analytics {
            algorithm,
            source,
            params,
            projections,
        } => {
            let procedure = match algorithm {
                GraphAlgorithm::PageRank => "gds.pageRank.stream",
                GraphAlgorithm::CommunityDetection => "gds.louvain.stream",
                GraphAlgorithm::BetweennessCentrality => "gds.betweenness.stream",
                GraphAlgorithm::ShortestPath => "gds.shortestPath.dijkstra.stream",
                GraphAlgorithm::NodeSimilarity => "gds.nodeSimilarity.stream",
            };

            let yield_clause = match algorithm {
                GraphAlgorithm::PageRank => "nodeId, score",
                GraphAlgorithm::CommunityDetection => "nodeId, communityId",
                GraphAlgorithm::BetweennessCentrality => "nodeId, score",
                GraphAlgorithm::ShortestPath => "index, sourceNode, targetNode, totalCost, nodeIds, costs, path",
                GraphAlgorithm::NodeSimilarity => "node1, node2, similarity",
            };

            // Build configuration map entries
            let mut config_entries = Vec::new();

            // Add nodeLabels from source
            match source {
                AnalyticsSource::Labels { labels } => {
                    let label_list = labels
                        .iter()
                        .map(|l| format!("'{l}'"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    config_entries.push(format!("nodeLabels: [{label_list}]"));
                }
                AnalyticsSource::Subgraph { filter } => {
                    // Compile the filter subgraph as a preceding MATCH
                    compile_op(filter, parts, pc)?;
                    // Remove RETURN from subgraph (GDS call follows)
                    if parts.last().is_some_and(|p| p.starts_with("RETURN")) {
                        parts.pop();
                    }
                }
                AnalyticsSource::WholeGraph => {
                    // No additional config needed — runs on the whole projected graph
                }
            }

            // Add user-supplied params
            for (key, value) in params {
                config_entries.push(format!("{key}: {}", compile_expr(value, pc)));
            }

            let config = if config_entries.is_empty() {
                String::new()
            } else {
                format!(", {{{}}}", config_entries.join(", "))
            };

            parts.push(format!(
                "CALL {procedure}($graph{config})\nYIELD {yield_clause}"
            ));

            if !projections.is_empty() {
                parts.push(format!(
                    "RETURN {}",
                    projections
                        .iter()
                        .map(|p| compile_projection(p, pc))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }

    Ok(())
}
