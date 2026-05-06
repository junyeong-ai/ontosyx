use ox_core::error::{OxError, OxResult};
use ox_query_ir::query::{AnalyticsSource, GraphAlgorithm, PathAlgorithm, QueryOp};

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
                parts.push(format!("{keyword} {}", compile_pattern(pattern, pc)?));
            }
            if let Some(filter) = filter {
                parts.push(format!("WHERE {}", compile_expr(filter, pc)?));
            }
            if !projections.is_empty() {
                let projs = projections
                    .iter()
                    .map(|p| compile_projection(p, pc))
                    .collect::<OxResult<Vec<_>>>()?;
                parts.push(format!("RETURN {}", projs.join(", ")));
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
                        start.label.as_deref(),
                        &start.property_filters,
                        pc,
                    )?;
                    let end_pat = compile_node_ref_inline(
                        &end.variable,
                        end.label.as_deref(),
                        &end.property_filters,
                        pc,
                    )?;
                    let rel = format_direction_pattern(&format!("[{rel_types}{depth}]"), direction);
                    parts.push(format!("MATCH p = {start_pat}{rel}{end_pat}"));
                    parts.push("RETURN p".to_string());
                    return Ok(());
                }
            };
            let start_pat = compile_node_ref_inline(
                &start.variable,
                start.label.as_deref(),
                &start.property_filters,
                pc,
            )?;
            let end_pat = compile_node_ref_inline(
                &end.variable,
                end.label.as_deref(),
                &end.property_filters,
                pc,
            )?;
            let rel = format_direction_pattern(&format!("[{rel_types}{depth}]"), direction);
            parts.push(format!("MATCH p = {path_fn}({start_pat}{rel}{end_pat})"));
            parts.push("RETURN p".to_string());
        }

        QueryOp::Aggregate {
            source,
            group_by,
            aggregations,
            having,
        } => {
            // Compile the source query without its own RETURN
            compile_op(&source.operation, parts, pc)?;

            // Remove the last RETURN if it exists (we'll add our own)
            if parts.last().is_some_and(|p| p.starts_with("RETURN")) {
                parts.pop();
            }

            let mut projections = Vec::new();
            let mut projected_names = Vec::new();
            for g in group_by {
                let field = if let Some(ref f) = g.field {
                    let expr = format!("{}.{}", g.variable, escape_identifier(f));
                    let alias = escape_identifier(f);
                    projected_names.push(alias.clone());
                    format!("{expr} AS {alias}")
                } else {
                    projected_names.push(g.variable.to_string());
                    g.variable.to_string()
                };
                projections.push(field);
            }
            for agg in aggregations {
                let field = if let Some(ref f) = agg.field.field {
                    format!("{}.{}", agg.field.variable, escape_identifier(f))
                } else {
                    agg.field.variable.to_string()
                };
                let func =
                    compile_agg_function(&agg.function, &field, agg.distinct, pc.dialect())?;
                projections.push(format!("{func} AS {}", agg.alias));
                projected_names.push(agg.alias.clone());
            }

            // HAVING compiles as an extra `WITH ... WHERE <expr>` step so
            // the filter runs *after* aggregation. Cypher has no HAVING
            // keyword; the idiom is `WITH alias, ... WHERE alias > 10`.
            // Without a HAVING we emit the RETURN directly, same as before.
            if let Some(having_expr) = having {
                parts.push(format!("WITH {}", projections.join(", ")));
                parts.push(format!("WHERE {}", compile_expr(having_expr, pc)?));
                parts.push(format!("RETURN {}", projected_names.join(", ")));
            } else {
                parts.push(format!("RETURN {}", projections.join(", ")));
            }
        }

        QueryOp::Union { queries, all } => {
            let union_keyword = if *all { "UNION ALL" } else { "UNION" };
            let compiled: Vec<String> = queries
                .iter()
                .map(|q| -> OxResult<String> {
                    let mut sub_parts = Vec::new();
                    compile_op(&q.operation, &mut sub_parts, pc)?;
                    if !q.order_by.is_empty() {
                        sub_parts.push(compile_order_by(&q.order_by, pc)?);
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
                inner_parts.push(compile_order_by(&inner.order_by, pc)?);
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
                parts.push(compile_mutate_op(op, pc)?);
            }
            if !returning.is_empty() {
                let projs = returning
                    .iter()
                    .map(|p| compile_projection(p, pc))
                    .collect::<OxResult<Vec<_>>>()?;
                parts.push(format!("RETURN {}", projs.join(", ")));
            }
        }

        QueryOp::Analytics {
            algorithm,
            source,
            params,
            projections,
        } => {
            // GDS procedures (`gds.*`) are Neo4j Enterprise surface
            // area. Memgraph ships its own analytics (MAGE module:
            // `mg.pagerank.get`, etc.) with different return shapes;
            // emitting `gds.pageRank.stream` against a Memgraph
            // driver fails opaquely at execution. Refuse at compile
            // time and name the alternative path.
            if pc.dialect() == super::CypherDialect::Memgraph {
                return Err(OxError::Compilation {
                    message: format!(
                        "Graph analytics ({algorithm:?}) lower to Neo4j GDS \
                         procedures (`gds.*`) which Memgraph does not \
                         expose. Run this query against a Neo4j backend, \
                         or rewrite as an explicit MATCH for the small \
                         subset Memgraph's MAGE module covers."
                    ),
                });
            }
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
                GraphAlgorithm::ShortestPath => {
                    "index, sourceNode, targetNode, totalCost, nodeIds, costs, path"
                }
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
                config_entries.push(format!("{key}: {}", compile_expr(value, pc)?));
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
                let projs = projections
                    .iter()
                    .map(|p| compile_projection(p, pc))
                    .collect::<OxResult<Vec<_>>>()?;
                parts.push(format!("RETURN {}", projs.join(", ")));
            }
        }

        QueryOp::HybridSearch { .. } => {
            // Hybrid retrieval lowers to engine-specific
            // index-procedure calls (`db.index.vector.queryNodes`
            // + `db.index.fulltext.queryNodes` on Neo4j;
            // `vector_search.search` + `text_search.search` on
            // Memgraph) plus a Reciprocal Rank Fusion CTE. The
            // emission path is non-trivial (different procedures
            // per dialect, fusion CTE shape varies, top_k must
            // round-trip without parameter substitution into the
            // procedure call) and ships in a follow-up. The
            // variant is reachable through the type system today
            // so consumers can construct it; emission lands when
            // the per-dialect index-procedure conventions are
            // wired through.
            return Err(OxError::UnsupportedOperation {
                target: "graph:cypher".into(),
                operation: "QueryOp::HybridSearch — hybrid \
                    retrieval emission lands in a follow-up; \
                    the agent's `try_retrieve_subgraph_md` \
                    helper bypasses QueryIR for now"
                    .into(),
            });
        }
    }

    Ok(())
}
