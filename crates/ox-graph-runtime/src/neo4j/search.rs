use std::collections::HashMap;

use ox_core::error::OxResult;
use ox_core::types::PropertyValue;
use ox_ontology::graph_exploration::{
    ExpandNeighbor, GraphSchemaOverview, LabelStat, NodeExpansion, PropertySchema,
    RelationshipPattern, SearchResultNode,
};

use crate::GraphRuntime;

use super::runtime::Neo4jRuntime;

impl Neo4jRuntime {
    pub(super) async fn do_search_nodes(
        &self,
        search_query: &str,
        limit: usize,
        labels: Option<&[String]>,
    ) -> OxResult<Vec<SearchResultNode>> {
        // Build label filter using Cypher pattern matching
        let label_match = match labels {
            Some(lbls) if !lbls.is_empty() => {
                let safe: Vec<&str> = lbls
                    .iter()
                    .filter(|l| l.chars().all(|c| c.is_alphanumeric() || c == '_'))
                    .map(|l| l.as_str())
                    .collect();
                if safe.is_empty() {
                    "MATCH (n)".to_string()
                } else {
                    let clauses: Vec<String> = safe.iter().map(|l| format!("n:`{l}`")).collect();
                    format!("MATCH (n) WHERE ({})", clauses.join(" OR "))
                }
            }
            _ => "MATCH (n)".to_string(),
        };

        // Wildcard = match all; otherwise text search across properties
        let has_where = label_match.contains("WHERE");
        let property_clause = if search_query == "*" {
            String::new()
        } else if has_where {
            " AND any(key IN keys(n) WHERE toString(n[key]) CONTAINS $search)".to_string()
        } else {
            " WHERE any(key IN keys(n) WHERE toString(n[key]) CONTAINS $search)".to_string()
        };

        let cypher = format!(
            "{label_match}{property_clause} \
             RETURN labels(n) AS labels, properties(n) AS props, elementId(n) AS element_id \
             LIMIT $limit"
        );

        let mut params = HashMap::new();
        if search_query != "*" {
            params.insert(
                "search".to_string(),
                PropertyValue::String(search_query.to_string()),
            );
        }
        params.insert("limit".to_string(), PropertyValue::Int(limit as i64));

        let result = self.execute_query(&cypher, &params).await?;

        let col_idx: HashMap<&str, usize> = result
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.as_str(), i))
            .collect();

        let nodes = result
            .rows
            .into_iter()
            .filter_map(|row| {
                let get = |name: &str| col_idx.get(name).and_then(|&i| row.get(i));

                let element_id = match get("element_id")? {
                    PropertyValue::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let labels = match get("labels") {
                    Some(PropertyValue::List(arr)) => arr
                        .iter()
                        .filter_map(|v| match v {
                            PropertyValue::String(s) => Some(s.clone()),
                            _ => None,
                        })
                        .collect(),
                    _ => vec![],
                };
                let props: HashMap<String, serde_json::Value> = match get("props") {
                    Some(PropertyValue::Map(m)) => m
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or_default()))
                        .collect(),
                    _ => HashMap::new(),
                };

                Some(SearchResultNode {
                    element_id,
                    labels,
                    props,
                })
            })
            .collect();

        Ok(nodes)
    }

    pub(super) async fn do_expand_node(
        &self,
        element_id: &str,
        limit: usize,
    ) -> OxResult<NodeExpansion> {
        let cypher = "\
            MATCH (n)-[r]-(m) WHERE elementId(n) = $id \
            RETURN type(r) AS rel_type, \
                   labels(m) AS labels, \
                   properties(m) AS props, \
                   elementId(m) AS element_id, \
                   CASE WHEN startNode(r) = n THEN 'outgoing' ELSE 'incoming' END AS direction \
            LIMIT $limit";

        let mut params = HashMap::new();
        params.insert(
            "id".to_string(),
            PropertyValue::String(element_id.to_string()),
        );
        params.insert("limit".to_string(), PropertyValue::Int(limit as i64));

        let result = self.execute_query(cypher, &params).await?;

        let col_idx: HashMap<&str, usize> = result
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.as_str(), i))
            .collect();

        let neighbors = result
            .rows
            .into_iter()
            .filter_map(|row| {
                let get = |name: &str| col_idx.get(name).and_then(|&i| row.get(i));

                let eid = match get("element_id")? {
                    PropertyValue::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let labels = match get("labels") {
                    Some(PropertyValue::List(arr)) => arr
                        .iter()
                        .filter_map(|v| match v {
                            PropertyValue::String(s) => Some(s.clone()),
                            _ => None,
                        })
                        .collect(),
                    _ => vec![],
                };
                let props: HashMap<String, serde_json::Value> = match get("props") {
                    Some(PropertyValue::Map(m)) => m
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or_default()))
                        .collect(),
                    _ => HashMap::new(),
                };
                let relationship_type = match get("rel_type")? {
                    PropertyValue::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let direction = match get("direction") {
                    Some(PropertyValue::String(s)) => s.clone(),
                    _ => "outgoing".to_string(),
                };

                Some(ExpandNeighbor {
                    element_id: eid,
                    labels,
                    props,
                    relationship_type,
                    direction,
                })
            })
            .collect();

        Ok(NodeExpansion {
            source_id: element_id.to_string(),
            neighbors,
        })
    }

    pub(super) async fn do_graph_overview(&self) -> OxResult<GraphSchemaOverview> {
        let empty_params = HashMap::new();

        // Label statistics
        let label_cypher = "\
            CALL db.labels() YIELD label \
            CALL { WITH label MATCH (n) WHERE label IN labels(n) RETURN count(n) AS cnt } \
            RETURN label, cnt ORDER BY cnt DESC";

        let label_result = self.execute_query(label_cypher, &empty_params).await?;
        let label_col_idx: HashMap<&str, usize> = label_result
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.as_str(), i))
            .collect();

        let mut labels = Vec::new();
        let mut total_nodes: i64 = 0;
        for row in &label_result.rows {
            let get = |name: &str| label_col_idx.get(name).and_then(|&i| row.get(i));
            let label = match get("label") {
                Some(PropertyValue::String(s)) => s.clone(),
                _ => continue,
            };
            let count = match get("cnt") {
                Some(PropertyValue::Int(n)) => *n,
                _ => 0,
            };
            total_nodes += count;
            labels.push(LabelStat { label, count });
        }

        // Relationship patterns
        let rel_cypher = "\
            MATCH (a)-[r]->(b) \
            WITH labels(a)[0] AS from_label, type(r) AS rel_type, labels(b)[0] AS to_label, count(*) AS cnt \
            RETURN from_label, rel_type, to_label, cnt \
            ORDER BY cnt DESC LIMIT 50";

        let rel_result = self.execute_query(rel_cypher, &empty_params).await?;
        let rel_col_idx: HashMap<&str, usize> = rel_result
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.as_str(), i))
            .collect();

        let mut relationships = Vec::new();
        let mut total_relationships: i64 = 0;
        for row in &rel_result.rows {
            let get = |name: &str| rel_col_idx.get(name).and_then(|&i| row.get(i));
            let from_label = match get("from_label") {
                Some(PropertyValue::String(s)) => s.clone(),
                _ => continue,
            };
            let rel_type = match get("rel_type") {
                Some(PropertyValue::String(s)) => s.clone(),
                _ => continue,
            };
            let to_label = match get("to_label") {
                Some(PropertyValue::String(s)) => s.clone(),
                _ => continue,
            };
            let count = match get("cnt") {
                Some(PropertyValue::Int(n)) => *n,
                _ => 0,
            };
            total_relationships += count;
            relationships.push(RelationshipPattern {
                from_label,
                rel_type,
                to_label,
                count,
            });
        }

        // --- Property introspection ---
        // Neo4j 2026 uses SHOW NODE/RELATIONSHIP TYPE PROPERTIES (GQL standard).
        // Falls back to deprecated db.schema.* procedures for Neo4j 5.x compat.
        let mut node_properties = Vec::new();
        let mut rel_properties = Vec::new();

        let node_prop_result = self.execute_query(
            "SHOW NODE TYPE PROPERTIES YIELD nodeType, propertyName, propertyTypes, mandatory RETURN nodeType, propertyName, propertyTypes, mandatory",
            &empty_params,
        ).await;
        let node_prop_result = match node_prop_result {
            Ok(r) => Ok(r),
            Err(_) => self.execute_query(
                "CALL db.schema.nodeTypeProperties() YIELD nodeType, propertyName, propertyTypes, mandatory RETURN nodeType, propertyName, propertyTypes, mandatory",
                &empty_params,
            ).await,
        };
        if let Ok(prop_result) = node_prop_result {
            let col_idx: HashMap<&str, usize> = prop_result
                .columns
                .iter()
                .enumerate()
                .map(|(i, c)| (c.as_str(), i))
                .collect();
            for row in &prop_result.rows {
                let get = |name: &str| col_idx.get(name).and_then(|&i| row.get(i));
                let entity_type = match get("nodeType") {
                    Some(PropertyValue::String(s)) => {
                        s.trim_start_matches(':').trim_matches('`').to_string()
                    }
                    _ => continue,
                };
                let property_name = match get("propertyName") {
                    Some(PropertyValue::String(s)) => s.clone(),
                    _ => continue,
                };
                let property_types = match get("propertyTypes") {
                    Some(PropertyValue::List(list)) => list
                        .iter()
                        .filter_map(|v| {
                            if let PropertyValue::String(s) = v {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .collect(),
                    _ => vec!["STRING".to_string()],
                };
                let mandatory = matches!(get("mandatory"), Some(PropertyValue::Bool(true)));
                node_properties.push(PropertySchema {
                    entity_type,
                    property_name,
                    property_types,
                    mandatory,
                });
            }
        } else if let Err(e) = node_prop_result {
            tracing::debug!(error = %e, "Property introspection not available — skipping node properties");
        }

        let rel_prop_result = self.execute_query(
            "SHOW RELATIONSHIP TYPE PROPERTIES YIELD relType, propertyName, propertyTypes, mandatory RETURN relType, propertyName, propertyTypes, mandatory",
            &empty_params,
        ).await;
        let rel_prop_result = match rel_prop_result {
            Ok(r) => Ok(r),
            Err(_) => self.execute_query(
                "CALL db.schema.relTypeProperties() YIELD relType, propertyName, propertyTypes, mandatory RETURN relType, propertyName, propertyTypes, mandatory",
                &empty_params,
            ).await,
        };
        if let Ok(prop_result) = rel_prop_result {
            let col_idx: HashMap<&str, usize> = prop_result
                .columns
                .iter()
                .enumerate()
                .map(|(i, c)| (c.as_str(), i))
                .collect();
            for row in &prop_result.rows {
                let get = |name: &str| col_idx.get(name).and_then(|&i| row.get(i));
                let entity_type = match get("relType") {
                    Some(PropertyValue::String(s)) => {
                        s.trim_start_matches(':').trim_matches('`').to_string()
                    }
                    _ => continue,
                };
                let property_name = match get("propertyName") {
                    Some(PropertyValue::String(s)) => s.clone(),
                    _ => continue,
                };
                let property_types = match get("propertyTypes") {
                    Some(PropertyValue::List(list)) => list
                        .iter()
                        .filter_map(|v| {
                            if let PropertyValue::String(s) = v {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .collect(),
                    _ => vec!["STRING".to_string()],
                };
                let mandatory = matches!(get("mandatory"), Some(PropertyValue::Bool(true)));
                rel_properties.push(PropertySchema {
                    entity_type,
                    property_name,
                    property_types,
                    mandatory,
                });
            }
        } else if let Err(e) = rel_prop_result {
            tracing::debug!(error = %e, "Property introspection not available — skipping relationship properties");
        }

        Ok(GraphSchemaOverview {
            labels,
            relationships,
            total_nodes,
            total_relationships,
            node_properties,
            rel_properties,
        })
    }
}
