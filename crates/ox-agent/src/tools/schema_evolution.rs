use std::sync::Arc;

use async_trait::async_trait;
use branchforge::tools::ExecutionContext;
use branchforge::{SchemaTool, ToolResult};
use ox_core::source_schema::SourceSchema;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::DomainContext;

// ---------------------------------------------------------------------------
// SchemaEvolutionTool — detect drift between source DB and ontology
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SchemaEvolutionInput {
    /// Action: "detect_drift" to compare source vs ontology, "suggest_updates" for recommendations
    pub action: EvolutionAction,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionAction {
    /// Compare current source schema against ontology and list differences
    DetectDrift,
    /// Generate ontology update suggestions based on detected drift
    SuggestUpdates,
}

#[derive(Debug, Serialize)]
struct DriftReport {
    /// Tables in source but not mapped to any ontology node
    unmapped_tables: Vec<String>,
    /// Ontology nodes whose source table no longer exists
    orphaned_nodes: Vec<String>,
    /// Source columns not mapped to any ontology property
    unmapped_columns: Vec<UnmappedColumn>,
    /// Ontology properties whose source column no longer exists
    orphaned_properties: Vec<OrphanedProperty>,
    /// Summary statistics
    summary: DriftSummary,
}

#[derive(Debug, Serialize)]
struct UnmappedColumn {
    table: String,
    column: String,
    data_type: String,
}

#[derive(Debug, Serialize)]
struct OrphanedProperty {
    node_label: String,
    property_name: String,
}

#[derive(Debug, Serialize)]
struct DriftSummary {
    total_source_tables: usize,
    total_ontology_nodes: usize,
    unmapped_table_count: usize,
    orphaned_node_count: usize,
    unmapped_column_count: usize,
    orphaned_property_count: usize,
    drift_detected: bool,
}

pub struct SchemaEvolutionTool {
    pub domain: Arc<DomainContext>,
}

#[async_trait]
impl SchemaTool for SchemaEvolutionTool {
    type Input = SchemaEvolutionInput;
    const NAME: &'static str = super::SCHEMA_EVOLUTION;
    const DESCRIPTION: &'static str = "Detect schema drift between source database and current ontology. \
         Use 'detect_drift' to compare source tables/columns against ontology nodes/properties. \
         Use 'suggest_updates' to get recommended ontology changes based on detected drift. \
         Call this when the user mentions schema changes, new tables, or data model evolution.";

    async fn handle(&self, input: Self::Input, _ctx: &ExecutionContext) -> ToolResult {
        let schema = match &self.domain.source_schema {
            Some(s) => s,
            None => {
                return ToolResult::error(
                    "No source schema available. Load a project with a data source first.",
                );
            }
        };
        let ontology = match &self.domain.ontology {
            Some(o) => o,
            None => return ToolResult::error("No ontology loaded. Design an ontology first."),
        };

        match input.action {
            EvolutionAction::DetectDrift => {
                let report = detect_drift(schema, ontology);
                ToolResult::success(serde_json::to_string_pretty(&report).unwrap_or_default())
            }
            EvolutionAction::SuggestUpdates => {
                let report = detect_drift(schema, ontology);
                if !report.summary.drift_detected {
                    return ToolResult::success(
                        serde_json::json!({
                            "status": "no_drift",
                            "message": "Source schema and ontology are in sync. No updates needed."
                        })
                        .to_string(),
                    );
                }

                let mut suggestions: Vec<String> = Vec::new();

                for table in &report.unmapped_tables {
                    suggestions.push(format!(
                        "ADD NODE: Create '{}' node type from unmapped source table '{}'",
                        to_pascal_case(table),
                        table
                    ));
                }

                for node in &report.orphaned_nodes {
                    suggestions.push(format!(
                        "REVIEW NODE: '{}' node has no matching source table. \
                         Consider removing or marking as deprecated.",
                        node
                    ));
                }

                for col in &report.unmapped_columns {
                    suggestions.push(format!(
                        "ADD PROPERTY: Add '{}' ({}) property to node mapped from table '{}'",
                        col.column, col.data_type, col.table
                    ));
                }

                for prop in &report.orphaned_properties {
                    suggestions.push(format!(
                        "REVIEW PROPERTY: '{}' on node '{}' has no matching source column. \
                         Consider removing.",
                        prop.property_name, prop.node_label
                    ));
                }

                let output = serde_json::json!({
                    "drift_summary": report.summary,
                    "suggestions": suggestions,
                    "suggestion_count": suggestions.len(),
                });
                ToolResult::success(serde_json::to_string_pretty(&output).unwrap_or_default())
            }
        }
    }
}

fn detect_drift(schema: &SourceSchema, ontology: &ox_core::ontology_ir::OntologyIR) -> DriftReport {
    // Extract source table names
    let source_tables: std::collections::HashSet<String> =
        schema.tables.iter().map(|t| t.name.clone()).collect();

    // Extract ontology node labels and their source_table mappings
    let ontology_nodes: std::collections::HashMap<String, Option<String>> = ontology
        .node_types
        .iter()
        .map(|n| (n.label.clone(), n.source_table.clone()))
        .collect();

    // Find unmapped tables (in source but not in ontology)
    let mapped_tables: std::collections::HashSet<String> = ontology_nodes
        .values()
        .filter_map(|st| st.clone())
        .collect();
    let unmapped_tables: Vec<String> = source_tables.difference(&mapped_tables).cloned().collect();

    // Find orphaned nodes (in ontology but source table doesn't exist)
    let orphaned_nodes: Vec<String> = ontology_nodes
        .iter()
        .filter(|(_, source_table)| {
            source_table
                .as_ref()
                .map(|st| !source_tables.contains(st))
                .unwrap_or(false)
        })
        .map(|(label, _)| label.clone())
        .collect();

    // Find unmapped columns and orphaned properties
    let mut unmapped_columns = Vec::new();
    let mut orphaned_properties = Vec::new();

    for table in &schema.tables {
        // Find the ontology node mapped to this table
        let mapped_node = ontology
            .node_types
            .iter()
            .find(|n| n.source_table.as_deref() == Some(&table.name));

        if let Some(node) = mapped_node {
            let node_prop_names: std::collections::HashSet<&str> =
                node.properties.iter().map(|p| p.name.as_str()).collect();

            let source_col_names: std::collections::HashSet<&str> =
                table.columns.iter().map(|c| c.name.as_str()).collect();

            // Unmapped columns
            for col in &table.columns {
                if !node_prop_names.contains(col.name.as_str()) {
                    unmapped_columns.push(UnmappedColumn {
                        table: table.name.clone(),
                        column: col.name.clone(),
                        data_type: col.data_type.clone(),
                    });
                }
            }

            // Orphaned properties
            for prop in &node.properties {
                if !source_col_names.contains(prop.name.as_str()) {
                    orphaned_properties.push(OrphanedProperty {
                        node_label: node.label.clone(),
                        property_name: prop.name.clone(),
                    });
                }
            }
        }
    }

    let drift_detected = !unmapped_tables.is_empty()
        || !orphaned_nodes.is_empty()
        || !unmapped_columns.is_empty()
        || !orphaned_properties.is_empty();

    DriftReport {
        summary: DriftSummary {
            total_source_tables: source_tables.len(),
            total_ontology_nodes: ontology_nodes.len(),
            unmapped_table_count: unmapped_tables.len(),
            orphaned_node_count: orphaned_nodes.len(),
            unmapped_column_count: unmapped_columns.len(),
            orphaned_property_count: orphaned_properties.len(),
            drift_detected,
        },
        unmapped_tables,
        orphaned_nodes,
        unmapped_columns,
        orphaned_properties,
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}
