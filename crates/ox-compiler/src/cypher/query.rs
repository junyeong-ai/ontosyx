use ox_core::error::{OxError, OxResult};
use ox_core::types::PropertyValue;
use ox_query_ir::hybrid_retrieval::FusionStrategy;
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
                let func = compile_agg_function(&agg.function, &field, agg.distinct, pc.dialect())?;
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

        QueryOp::HybridSearch { request } => {
            // Extract the constraint label early so both the
            // vector-only and RRF paths can weave it through
            // their YIELD stream / fusion CTE consistently.
            // First pattern node's label is the operator's
            // primary intent; richer constraints (property
            // filters / multi-node / edges) are still
            // deferred until the lowering surface grows.
            let constraint_label: Option<String> = request
                .graph_constraints
                .as_ref()
                .and_then(|p| p.nodes.first())
                .and_then(|n| n.label.as_ref())
                .map(|lbl| escape_identifier(lbl.as_str()));

            // Vector-index name — convention
            // `entity_embedding_index` is the platform's
            // hardcoded default that the ontology materialise
            // step writes against. A future
            // `HybridSearchRequest.vector_index_name` optional
            // field would let operators target a workspace-
            // specific index without a recompile; until then
            // the convention is the contract.
            let vec_index_param = pc.push(PropertyValue::String("entity_embedding_index".into()));
            let top_k_param = pc.push(PropertyValue::Int(request.top_k as i64));
            // Vector → List(Float) — pgvector / Neo4j vector
            // ingestion both accept Cypher list-of-doubles, so
            // the f32 source widens to f64 cleanly.
            let vector_value: Vec<PropertyValue> = request
                .vector_query
                .vector
                .iter()
                .map(|v| PropertyValue::Float(*v as f64))
                .collect();
            let vector_param = pc.push(PropertyValue::List(vector_value));

            match &request.fulltext_query {
                None => {
                    // Vector-only path — single procedure call
                    // + score-desc projection. When
                    // `graph_constraints` carries a node
                    // pattern, weave its label as a `WHERE
                    // node:Label` filter on the YIELD stream
                    // so the procedure's top-K candidates trim
                    // to the structural cohort the operator
                    // anchored.
                    parts.push(format!(
                        "CALL db.index.vector.queryNodes({vec_index_param}, {top_k_param}, {vector_param})",
                    ));
                    parts.push("YIELD node, score".to_string());
                    if let Some(label) = &constraint_label {
                        parts.push(format!("WHERE node:{label}"));
                    }
                    parts.push("RETURN node, score".to_string());
                    parts.push("ORDER BY score DESC".to_string());
                }
                Some(fulltext_query) => {
                    // Two-source fusion. RRF (default) is rank-
                    // based — each source's i-th result gets
                    // `1 / (k + rank)` and the fused score is
                    // the sum across sources. WeightedSum is
                    // score-based — each source's raw score
                    // is multiplied by the operator-supplied
                    // weight before summing. The two paths
                    // share the procedure-call shell + the
                    // graph_constraints pre-filter; they
                    // diverge only on the per-source `rrf:` /
                    // `weighted:` projection inside the list
                    // comprehension.
                    //
                    // Per-source procedure expanded to `top_k`
                    // candidates so the fusion has room to
                    // re-rank — the final `LIMIT $top_k`
                    // trims back to the operator-requested
                    // window. Without the expansion, a fusion
                    // that swaps a vector-rank-1 with a
                    // fulltext-rank-1 has no headroom to
                    // discover the true top-K under the
                    // operator's k.
                    let fulltext_index_param =
                        pc.push(PropertyValue::String("entity_doc_index".into()));
                    let fulltext_query_param =
                        pc.push(PropertyValue::String(fulltext_query.clone()));

                    parts.push(format!(
                        "CALL db.index.vector.queryNodes({vec_index_param}, {top_k_param}, {vector_param})",
                    ));
                    parts.push("YIELD node AS v_node, score AS v_score".to_string());
                    // graph_constraints label filter weaves
                    // through *both* source streams via a list
                    // comprehension — `[n IN raw WHERE n:Label
                    // | n]` pre-filters before the fusion
                    // ranking lands. Without it the structural
                    // cohort would be lost across the
                    // UNWIND + group-by rebind. Vector and
                    // fulltext filters land symmetrically so a
                    // node only contributes to the fused score
                    // when both sources see it under the
                    // anchor cohort.
                    //
                    // RRF path collects nodes only (rank from
                    // list position); WeightedSum path collects
                    // {node, score} pairs (score from yield).
                    let collect_vec = match request.fuse {
                        FusionStrategy::ReciprocalRankFusion { .. } => {
                            if let Some(label) = &constraint_label {
                                format!(
                                    "WITH [n IN collect(v_node) WHERE n:{label} | n] AS vec_nodes",
                                )
                            } else {
                                "WITH collect(v_node) AS vec_nodes".to_string()
                            }
                        }
                        FusionStrategy::WeightedSum { .. } => {
                            if let Some(label) = &constraint_label {
                                format!(
                                    "WITH [r IN collect({{node: v_node, score: v_score}}) \
                                     WHERE r.node:{label} | r] AS vec_data",
                                )
                            } else {
                                "WITH collect({node: v_node, score: v_score}) AS vec_data"
                                    .to_string()
                            }
                        }
                    };
                    parts.push(collect_vec);

                    parts.push(format!(
                        "CALL db.index.fulltext.queryNodes({fulltext_index_param}, {fulltext_query_param})",
                    ));
                    parts.push("YIELD node AS f_node, score AS f_score".to_string());

                    let collect_txt = match request.fuse {
                        FusionStrategy::ReciprocalRankFusion { .. } => {
                            if let Some(label) = &constraint_label {
                                format!(
                                    "WITH vec_nodes, [n IN collect(f_node) WHERE n:{label} | n] AS txt_nodes",
                                )
                            } else {
                                "WITH vec_nodes, collect(f_node) AS txt_nodes".to_string()
                            }
                        }
                        FusionStrategy::WeightedSum { .. } => {
                            if let Some(label) = &constraint_label {
                                format!(
                                    "WITH vec_data, [r IN collect({{node: f_node, score: f_score}}) \
                                     WHERE r.node:{label} | r] AS txt_data",
                                )
                            } else {
                                "WITH vec_data, collect({node: f_node, score: f_score}) AS txt_data"
                                    .to_string()
                            }
                        }
                    };
                    parts.push(collect_txt);

                    match request.fuse {
                        FusionStrategy::ReciprocalRankFusion { k } => {
                            let rrf_k_param = pc.push(PropertyValue::Int(k as i64));
                            parts.push(format!(
                                "WITH [i IN range(0, size(vec_nodes) - 1) | \
                                 {{node: vec_nodes[i], rrf: 1.0 / ({rrf_k_param} + i + 1)}}] AS vec_rrf, \
                                 [j IN range(0, size(txt_nodes) - 1) | \
                                 {{node: txt_nodes[j], rrf: 1.0 / ({rrf_k_param} + j + 1)}}] AS txt_rrf",
                            ));
                            parts.push("UNWIND vec_rrf + txt_rrf AS r".to_string());
                            parts.push("WITH r.node AS node, sum(r.rrf) AS score".to_string());
                        }
                        FusionStrategy::WeightedSum {
                            vector_weight,
                            fulltext_weight,
                        } => {
                            let v_weight_param =
                                pc.push(PropertyValue::Float(vector_weight as f64));
                            let f_weight_param =
                                pc.push(PropertyValue::Float(fulltext_weight as f64));
                            parts.push(format!(
                                "WITH [v IN vec_data | \
                                 {{node: v.node, weighted: v.score * {v_weight_param}}}] AS vec_w, \
                                 [f IN txt_data | \
                                 {{node: f.node, weighted: f.score * {f_weight_param}}}] AS txt_w",
                            ));
                            parts.push("UNWIND vec_w + txt_w AS r".to_string());
                            parts.push("WITH r.node AS node, sum(r.weighted) AS score".to_string());
                        }
                    }

                    parts.push("RETURN node, score".to_string());
                    parts.push("ORDER BY score DESC".to_string());
                    parts.push(format!("LIMIT {top_k_param}"));
                }
            }
        }
    }

    Ok(())
}
