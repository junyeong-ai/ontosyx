//! [`OntologyNavigationStore`] — Level-3 read-only navigation
//! (entry point search, neighbors, hierarchy).
//!
//! Every query reads from the flat index tables that
//! [`super::ontology_materialize`] populates at commit time. That
//! asymmetry (commit writes, navigation reads) is why the two
//! stores share helpers but the read side declares no imports from
//! the write helpers — it only reads pre-materialised rows.

use super::*;

#[async_trait]
impl crate::store::OntologyNavigationStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn search_entry_points(
        &self,
        options: crate::navigation::EntryPointSearchOptions,
    ) -> OxResult<Vec<crate::navigation::EntitySearchHit>> {
        // Trigram + full-text blend. Embedding weight is folded into
        // the `similar_entities` path — this query is the cheap
        // text-first pass so the agent hits it first for prefix / alias
        // recall; embedding kNN is the slower semantic fallback.
        //
        // The kind filter becomes `entity_kind = ANY($kinds)` when
        // supplied. Passing `NULL` (via `Option::None`) disables the
        // clause. NULL-safety via `COALESCE($kinds IS NULL, false)`
        // keeps the filter branch-free on the SQL side.
        let kind_filter: Option<Vec<String>> = options.kinds.clone();
        let trigram_w = options.blend.trigram;
        let full_text_w = options.blend.full_text;
        // Label-match boost — a query that's a prefix or contained
        // substring of the canonical `label` should outrank a
        // description-only match. Industry retrieval (Algolia,
        // Typesense) all weight title-match higher than body-match
        // for the same reason: an operator typing "customer" wants
        // the `Customer` node, not a glossary term whose
        // *description* mentions customers. Without this, the doc-
        // trigram tie can let description-heavy rows outrank
        // structural ones (the gold gate's
        // `node_type_label_match` axis pinned this regression).
        //
        // The boost adds `1.0 * label_trigram` on top of the base
        // blend so an exact label match (`similarity('customer',
        // 'Customer') ≈ 0.6+`) can pull the row above
        // doc-only matches with similar trigram scores. `COALESCE`
        // protects rows from prior commits where `label` is NULL.
        sqlx::query_as::<_, crate::navigation::EntitySearchHit>(
            "SELECT entity_kind::text AS entity_kind, \
                    logical_id, \
                    doc, \
                    ( \
                        GREATEST( \
                            similarity(doc, $2)::real * $4, \
                            COALESCE(ts_rank(tsv, plainto_tsquery('simple', $2)), 0) * $5 \
                        ) \
                        + COALESCE(similarity(label, $2), 0)::real \
                    )::real AS score \
             FROM ontology_entity_search_vector \
             WHERE version_id = $1 \
               AND (doc ILIKE '%' || $2 || '%' \
                    OR similarity(doc, $2) > 0.1 \
                    OR tsv @@ plainto_tsquery('simple', $2) \
                    OR (label IS NOT NULL AND similarity(label, $2) > 0.1)) \
               AND ($6::text[] IS NULL OR entity_kind::text = ANY($6)) \
             ORDER BY score DESC \
             LIMIT $3",
        )
        .bind(options.version_id)
        .bind(&options.query)
        .bind(options.limit as i64)
        .bind(trigram_w)
        .bind(full_text_w)
        .bind(kind_filter.as_deref())
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn expand_neighbors(
        &self,
        options: crate::navigation::NeighborExpandOptions,
    ) -> OxResult<crate::navigation::Subgraph> {
        use crate::navigation::{
            EntityRef, NeighborDirection, Subgraph, SubgraphEdge, SubgraphNode,
        };
        use std::collections::HashMap;

        // Anchors seed the subgraph at depth 0. A shared HashMap keyed
        // by `(kind, logical_id)` dedups across iterations — the BFS
        // iterates until `depth` hops or `max_nodes` exceeded,
        // whichever comes first. `visited` protects against cycles in
        // the neighbor graph.
        let mut nodes: HashMap<(String, String), SubgraphNode> = HashMap::new();
        let mut edges: Vec<SubgraphEdge> = Vec::new();
        let mut truncated = false;

        for a in &options.anchors {
            nodes.insert(
                (a.kind.clone(), a.logical_id.clone()),
                SubgraphNode {
                    kind: a.kind.clone(),
                    logical_id: a.logical_id.clone(),
                    label: None,
                    doc: None,
                    depth: 0,
                },
            );
        }

        let mut frontier: Vec<EntityRef> = options.anchors.clone();
        let include_kinds = options.include_kinds.clone();
        let max_nodes = if options.max_nodes == 0 {
            u32::MAX
        } else {
            options.max_nodes
        };

        for hop in 1..=options.depth {
            if frontier.is_empty() {
                break;
            }
            let kinds: Vec<String> = frontier.iter().map(|r| r.kind.clone()).collect();
            let ids: Vec<String> = frontier.iter().map(|r| r.logical_id.clone()).collect();

            // `UNNEST` over the two anchor arrays produces the pair set
            // without needing tuple-IN support. Casting the column to
            // text keeps the join condition comparable against the bound
            // `text[]`s — Postgres enum equality against a text array
            // requires the explicit `::text` flip.
            let direction_where = match options.direction {
                NeighborDirection::Outgoing => {
                    "JOIN UNNEST($2::text[], $3::text[]) AS a(kind, id) \
                      ON n.from_kind::text = a.kind AND n.from_logical_id = a.id"
                }
                NeighborDirection::Incoming => {
                    "JOIN UNNEST($2::text[], $3::text[]) AS a(kind, id) \
                      ON n.to_kind::text = a.kind AND n.to_logical_id = a.id"
                }
                NeighborDirection::Both => {
                    "JOIN UNNEST($2::text[], $3::text[]) AS a(kind, id) \
                      ON (n.from_kind::text = a.kind AND n.from_logical_id = a.id) \
                      OR (n.to_kind::text = a.kind   AND n.to_logical_id = a.id)"
                }
            };
            let sql = format!(
                "SELECT n.from_kind::text AS from_kind, n.from_logical_id, \
                        n.to_kind::text AS to_kind, n.to_logical_id, n.relation_kind \
                 FROM ontology_entity_neighbors n \
                 {direction_where} \
                 WHERE n.version_id = $1",
            );

            #[derive(sqlx::FromRow)]
            struct NeighborRow {
                from_kind: String,
                from_logical_id: String,
                to_kind: String,
                to_logical_id: String,
                relation_kind: String,
            }

            let rows: Vec<NeighborRow> = sqlx::query_as::<_, NeighborRow>(&sql)
                .bind(options.version_id)
                .bind(&kinds)
                .bind(&ids)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?;

            let mut next_frontier: Vec<EntityRef> = Vec::new();
            for r in rows {
                let from = EntityRef::new(&r.from_kind, &r.from_logical_id);
                let to = EntityRef::new(&r.to_kind, &r.to_logical_id);
                edges.push(SubgraphEdge {
                    from: from.clone(),
                    to: to.clone(),
                    relation_kind: r.relation_kind,
                });

                for side in [from, to] {
                    let key = (side.kind.clone(), side.logical_id.clone());
                    let include_this = include_kinds
                        .as_ref()
                        .is_none_or(|ks| ks.iter().any(|k| k == &side.kind));
                    if !include_this {
                        continue;
                    }
                    if nodes.contains_key(&key) {
                        continue;
                    }
                    if (nodes.len() as u32) >= max_nodes {
                        truncated = true;
                        continue;
                    }
                    nodes.insert(
                        key,
                        SubgraphNode {
                            kind: side.kind.clone(),
                            logical_id: side.logical_id.clone(),
                            label: None,
                            doc: None,
                            depth: hop,
                        },
                    );
                    next_frontier.push(side);
                }
            }
            frontier = next_frontier;
        }

        let nodes: Vec<SubgraphNode> = nodes.into_values().collect();
        Ok(Subgraph {
            nodes,
            edges,
            truncated,
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn apply_hierarchy_and_facet(
        &self,
        subgraph: crate::navigation::Subgraph,
        options: crate::navigation::HierarchyFacetOptions,
    ) -> OxResult<crate::navigation::Subgraph> {
        super::require_workspace_context()?;
        use crate::navigation::{
            EntityRef, FacetFilter, HierarchyExpand, Subgraph, SubgraphEdge, SubgraphNode,
        };
        use std::collections::HashMap;

        let mut nodes: HashMap<(String, String), SubgraphNode> = subgraph
            .nodes
            .into_iter()
            .map(|n| ((n.kind.clone(), n.logical_id.clone()), n))
            .collect();
        let mut edges = subgraph.edges;
        let mut truncated = subgraph.truncated;

        // Hierarchy closure — walks `ontology_entity_hierarchy` for the
        // relation + anchor. Descendants can clamp on `max_depth`;
        // ancestors are always short so no clamp is exposed.
        if let Some(expand) = options.hierarchy_expand {
            #[derive(sqlx::FromRow)]
            struct HierarchyRow {
                relation_kind: String,
                ancestor_kind: String,
                ancestor_logical_id: String,
                descendant_kind: String,
                descendant_logical_id: String,
                depth: i32,
            }

            let rows: Vec<HierarchyRow> = match expand {
                HierarchyExpand::Descendants {
                    relation_kind,
                    anchor,
                    max_depth,
                } => sqlx::query_as::<_, HierarchyRow>(
                    "SELECT relation_kind, \
                            ancestor_kind::text AS ancestor_kind, ancestor_logical_id, \
                            descendant_kind::text AS descendant_kind, descendant_logical_id, \
                            depth \
                     FROM ontology_entity_hierarchy \
                     WHERE version_id = $1 \
                       AND relation_kind = $2 \
                       AND ancestor_kind = $3::ontology_entity_kind \
                       AND ancestor_logical_id = $4 \
                       AND depth <= $5 \
                     ORDER BY depth, descendant_logical_id",
                )
                .bind(options.version_id)
                .bind(&relation_kind)
                .bind(&anchor.kind)
                .bind(&anchor.logical_id)
                .bind(max_depth as i32)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?,
                HierarchyExpand::Ancestors {
                    relation_kind,
                    anchor,
                } => sqlx::query_as::<_, HierarchyRow>(
                    "SELECT relation_kind, \
                            ancestor_kind::text AS ancestor_kind, ancestor_logical_id, \
                            descendant_kind::text AS descendant_kind, descendant_logical_id, \
                            depth \
                     FROM ontology_entity_hierarchy \
                     WHERE version_id = $1 \
                       AND relation_kind = $2 \
                       AND descendant_kind = $3::ontology_entity_kind \
                       AND descendant_logical_id = $4 \
                     ORDER BY depth, ancestor_logical_id",
                )
                .bind(options.version_id)
                .bind(&relation_kind)
                .bind(&anchor.kind)
                .bind(&anchor.logical_id)
                .fetch_all(&self.pool)
                .await
                .map_err(to_ox_error)?,
            };

            // CodeSystem child cap — if a CodeSystem accumulates too
            // many codes via hierarchy expansion, trim to
            // `max_codes_per_code_system` descendants ordered by
            // closest depth first. Keeps the LLM-render budget
            // predictable on deep taxonomies.
            let mut codes_per_system: HashMap<(String, String), u32> = HashMap::new();

            for r in rows {
                let ancestor = EntityRef::new(&r.ancestor_kind, &r.ancestor_logical_id);
                let descendant =
                    EntityRef::new(&r.descendant_kind, &r.descendant_logical_id);

                if r.depth == 0 {
                    // Self-row — already present as the anchor.
                    continue;
                }

                if r.ancestor_kind == "CodeSystem" {
                    let entry = codes_per_system
                        .entry((r.ancestor_kind.clone(), r.ancestor_logical_id.clone()))
                        .or_insert(0);
                    if *entry >= options.max_codes_per_code_system {
                        truncated = true;
                        continue;
                    }
                    *entry += 1;
                }

                edges.push(SubgraphEdge {
                    from: ancestor,
                    to: descendant.clone(),
                    relation_kind: r.relation_kind.clone(),
                });

                nodes
                    .entry((descendant.kind.clone(), descendant.logical_id.clone()))
                    .or_insert(SubgraphNode {
                        kind: descendant.kind,
                        logical_id: descendant.logical_id,
                        label: None,
                        doc: None,
                        depth: r.depth.max(0) as u8,
                    });
            }
        }

        // Facet filter — applied LAST so hierarchy enrichment can
        // still carry nodes that the final kind-filter keeps.
        if let Some(FacetFilter { kinds: Some(ks) }) = options.facet_filter {
            nodes.retain(|(k, _), _| ks.iter().any(|pat| pat == k));
            edges.retain(|e| {
                ks.iter().any(|k| k == &e.from.kind)
                    && ks.iter().any(|k| k == &e.to.kind)
            });
        }

        Ok(Subgraph {
            nodes: nodes.into_values().collect(),
            edges,
            truncated,
        })
    }

    fn render_subgraph_for_llm(
        &self,
        subgraph: &crate::navigation::Subgraph,
        options: &crate::navigation::LlmRenderOptions,
    ) -> String {
        // Pure formatter — delegated so unit tests can cover the
        // markdown shape without standing up a pool.
        crate::navigation::render_subgraph_as_llm_markdown(subgraph, options)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn similar_entities(
        &self,
        version_id: Uuid,
        entity_kind: &str,
        logical_id: &str,
        top_k: u32,
    ) -> OxResult<Vec<crate::navigation::EntitySearchHit>> {
        // Find the query vector; if absent (embedding not yet
        // populated), return empty rather than fall back to
        // something less precise. Callers can chain with
        // `search_entry_points` for the fallback behaviour.
        sqlx::query_as::<_, crate::navigation::EntitySearchHit>(
            "WITH q AS ( \
                SELECT embedding \
                FROM ontology_entity_embedding \
                WHERE version_id = $1 \
                  AND entity_kind = $2::ontology_entity_kind \
                  AND logical_id = $3 \
                  AND embedding IS NOT NULL \
                LIMIT 1 \
             ) \
             SELECT sv.entity_kind::text AS entity_kind, \
                    sv.logical_id, \
                    sv.doc, \
                    (1.0 - (e.embedding <=> (SELECT embedding FROM q)))::real AS score \
             FROM ontology_entity_embedding e \
             JOIN ontology_entity_search_vector sv \
               ON sv.version_id = e.version_id \
              AND sv.entity_kind = e.entity_kind \
              AND sv.logical_id = e.logical_id \
             WHERE e.version_id = $1 \
               AND e.embedding IS NOT NULL \
               AND (SELECT embedding FROM q) IS NOT NULL \
               AND NOT (e.entity_kind = $2::ontology_entity_kind AND e.logical_id = $3) \
             ORDER BY e.embedding <=> (SELECT embedding FROM q) \
             LIMIT $4",
        )
        .bind(version_id)
        .bind(entity_kind)
        .bind(logical_id)
        .bind(top_k as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}
