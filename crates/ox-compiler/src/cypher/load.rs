use ox_core::error::OxResult;
use ox_core::load_plan::{ConflictStrategy, LoadOp};

use super::params::escape_identifier;

// ---------------------------------------------------------------------------
// LoadPlan compilation → Cypher per-record MERGE
//
// Each record is executed individually by the runtime (with concurrency).
// Parameters use `$row_<source_column>` naming so the runtime can bind
// each JSON field directly — no UNWIND, no JSON parsing, no APOC needed.
// ---------------------------------------------------------------------------

/// Build a parameter name from a source column: `$row_<source_column>`.
fn load_param(source_column: &str) -> String {
    format!("$row_{source_column}")
}

pub(super) fn compile_load_op(op: &LoadOp) -> OxResult<String> {
    match op {
        LoadOp::UpsertNode {
            target_label,
            match_fields,
            set_fields,
            on_conflict,
        } => {
            let match_props = match_fields
                .iter()
                .map(|m| {
                    format!(
                        "{}: {}",
                        escape_identifier(&m.graph_property),
                        load_param(&m.source_column)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");

            let set_props = set_fields
                .iter()
                .map(|m| {
                    format!(
                        "n.{} = {}",
                        escape_identifier(&m.graph_property),
                        load_param(&m.source_column)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");

            let on_clause = match on_conflict {
                ConflictStrategy::Update => {
                    format!("\n  ON CREATE SET {set_props}\n  ON MATCH SET {set_props}")
                }
                ConflictStrategy::Skip => format!("\n  ON CREATE SET {set_props}"),
                ConflictStrategy::Error => format!("\n  ON CREATE SET {set_props}"),
                ConflictStrategy::MergeNonNull => {
                    let coalesce_props = set_fields
                        .iter()
                        .map(|m| {
                            let escaped_prop = escape_identifier(&m.graph_property);
                            format!(
                                "n.{} = COALESCE({}, n.{})",
                                escaped_prop,
                                load_param(&m.source_column),
                                escaped_prop
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("\n  ON CREATE SET {set_props}\n  ON MATCH SET {coalesce_props}")
                }
            };

            let escaped_target_label = escape_identifier(target_label);
            Ok(format!(
                "MERGE (n:{escaped_target_label} {{{match_props}}}){on_clause}"
            ))
        }

        LoadOp::UpsertEdge {
            target_label,
            source_match,
            target_match,
            set_fields,
            on_conflict,
        } => {
            let set_props = if set_fields.is_empty() {
                String::new()
            } else {
                let props = set_fields
                    .iter()
                    .map(|m| {
                        format!(
                            "r.{} = {}",
                            escape_identifier(&m.graph_property),
                            load_param(&m.source_column)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                match on_conflict {
                    ConflictStrategy::Update => {
                        format!("\n  ON CREATE SET {props}\n  ON MATCH SET {props}")
                    }
                    ConflictStrategy::Skip => format!("\n  ON CREATE SET {props}"),
                    _ => format!("\n  ON CREATE SET {props}"),
                }
            };

            let escaped_target_label = escape_identifier(target_label);
            Ok(format!(
                "MATCH (a:{} {{{}: {}}})\n\
                 MATCH (b:{} {{{}: {}}})\n\
                 MERGE (a)-[r:{escaped_target_label}]->(b){set_props}",
                escape_identifier(&source_match.label),
                escape_identifier(&source_match.match_property),
                load_param(&source_match.source_field),
                escape_identifier(&target_match.label),
                escape_identifier(&target_match.match_property),
                load_param(&target_match.source_field),
            ))
        }
    }
}
