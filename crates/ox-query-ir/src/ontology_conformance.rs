//! Cross-crate helpers that bridge `OntologyIR` (in `ox-ontology`) and
//! `QueryIR` (in this crate). Living here rather than on
//! `OntologyIR::<method>` avoids the circular dependency that would
//! arise if the ontology crate had to know about query shapes.

use ox_ontology::OntologyIR;

use crate::eval;
use crate::query::QueryIR;

/// Labels referenced by `query` that `ontology` does not declare.
///
/// Returns both unknown node labels and unknown relationship types,
/// each prefixed with `"Node "` / `"Edge "` for readability in
/// diagnostic messages. Empty `Vec` means the query's labels are all
/// known — the caller can accept the query.
///
/// This is a *pre-flight* check used by `ox-brain` to catch LLM label
/// hallucinations before invoking compile + runtime. The runtime's
/// `OntologyValidator` remains the final authority over the AST-level
/// surface (inline property keys, etc.); this helper only looks at
/// the QueryIR's extracted label set.
pub fn unknown_labels_in_query(ontology: &OntologyIR, query: &QueryIR) -> Vec<String> {
    let node_labels = eval::extract_node_labels(query);
    let edge_labels = eval::extract_edge_labels(query);
    let mut unknown = Vec::new();
    for label in &node_labels {
        if ontology.node_by_label(label).is_none() {
            unknown.push(format!("Node '{label}'"));
        }
    }
    let known_edge_labels: std::collections::HashSet<&str> = ontology
        .edge_types()
        .iter()
        .map(|e| e.label.as_str())
        .collect();
    for label in &edge_labels {
        if !known_edge_labels.contains(label.as_str()) {
            unknown.push(format!("Edge '{label}'"));
        }
    }
    unknown
}
