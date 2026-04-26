//! Incremental ontology extension — merge two `OntologyIR`s into one.
//!
//! Use case: a user picks five tables, generates an initial ontology
//! (`OntologyIR::A`), then later picks three more tables and generates
//! an extension ontology (`OntologyIR::B`). Calling `A.extend(B)` walks
//! every collection in `B` and:
//!
//! - **Adds** items whose id is absent in `A`. The base always grows.
//! - **Skips** items whose id is present in `A` *and* whose serialised
//!   content matches byte-for-byte. Re-discovering the same node from
//!   the same scan window must be a no-op, not a churn event.
//! - **Records a conflict** for items whose id is present in `A` but
//!   whose content differs. The base wins — incoming content is
//!   dropped — and the conflict shows up in the [`MergeReport`] so the
//!   UI can surface "the source schema for `X` changed; review and
//!   re-introspect" rather than silently flipping the IR.
//!
//! Re-introspection of an already-known table is therefore an explicit
//! gesture (drop the cache + run `analyze_subset` again + apply the
//! resulting ops through the edit pipeline) — never a side effect of
//! extension.
//!
//! Item equivalence uses serde JSON. The full Def shape participates,
//! so any field divergence (description, deprecation timestamp, source
//! column, etc.) flips the result from skip to conflict.

use serde::Serialize;

use ox_core::error::{OxError, OxResult};

use crate::ir::OntologyIR;

/// Outcome of an `OntologyIR::extend` call. Surfaced to the caller so
/// the UI can list every concrete change without re-walking the merged
/// IR. Append-only — adding a new collection later just adds new
/// entries to the existing vectors with a new `kind` string.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MergeReport {
    /// Items added because the id was new to the base.
    pub added: Vec<MergedItem>,
    /// Items skipped because the id was present and content matched.
    pub skipped: Vec<MergedItem>,
    /// Items rejected because the id was present but content differed.
    /// The base value is preserved; the incoming value is discarded.
    pub conflicts: Vec<MergeConflict>,
}

impl MergeReport {
    /// Whether the merge produced any actionable change. A no-op
    /// extension (every incoming item already present and identical)
    /// returns false — the UI can use this to suppress "extension
    /// applied" toasts that wouldn't carry information.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.conflicts.is_empty()
    }
}

/// One id + collection-name pair recorded in either the `added` or
/// `skipped` list. `kind` is the IR collection name (`"node_type"`,
/// `"edge_type"`, `"object_mapping"`, etc.) — kept as a string rather
/// than an enum so adding a new collection in the future doesn't
/// reshape the wire format.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MergedItem {
    pub kind: String,
    pub id: String,
}

/// One id + collection-name pair plus a human-readable reason. Today
/// the reason is always "content differs"; the field exists so future
/// merge rules (e.g., per-field justification, "label collides with X")
/// can attach detail without growing a separate type.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MergeConflict {
    pub kind: String,
    pub id: String,
    pub reason: String,
}

/// Compare two `Serialize` values by canonicalising to JSON and
/// checking equality. Avoids requiring `PartialEq` on every Def type
/// in the IR — the JSON value is the wire-level identity that the
/// store and federation layers already round-trip through.
fn defs_match<T: Serialize>(a: &T, b: &T) -> OxResult<bool> {
    let a = serde_json::to_value(a).map_err(OxError::Serialization)?;
    let b = serde_json::to_value(b).map_err(OxError::Serialization)?;
    Ok(a == b)
}

impl OntologyIR {
    /// Grow `self` by absorbing every collection of `other` into the
    /// matching slot. See the module docs for the full add / skip /
    /// conflict semantics.
    ///
    /// `other` is consumed because the merge takes ownership of every
    /// added item. Conflict items are dropped on the floor — the base
    /// retains its existing value.
    ///
    /// Returns `Err` only when JSON canonicalisation of an item fails
    /// (effectively impossible for IR types, all of which are
    /// `Serialize`). Add-time invariant violations on the underlying
    /// `add_*` methods (e.g., a new node whose label collides with a
    /// distinct existing node) propagate as `Err` too — the partial
    /// merge state at that point is intentional, since the base
    /// remains a valid IR with the items already merged in.
    pub fn extend(&mut self, other: OntologyIR) -> OxResult<MergeReport> {
        let mut report = MergeReport::default();

        merge_node_types(self, other.node_types, &mut report)?;
        merge_edge_types(self, other.edge_types, &mut report)?;
        merge_indexes(self, other.indexes, &mut report)?;
        merge_object_mappings(self, other.object_mappings, &mut report)?;
        merge_link_mappings(self, other.link_mappings, &mut report)?;
        merge_interfaces(self, other.interfaces, &mut report)?;
        merge_glossary_terms(self, other.glossary, &mut report)?;
        merge_code_systems(self, other.code_systems, &mut report)?;
        merge_value_sets(self, other.value_sets, &mut report)?;
        merge_concept_maps(self, other.concept_maps, &mut report)?;
        merge_notation_patterns(self, other.notation_patterns, &mut report)?;
        merge_value_range_sets(self, other.value_range_sets, &mut report)?;
        merge_rules(self, other.rules, &mut report)?;
        merge_actions(self, other.actions, &mut report)?;
        merge_functions(self, other.functions, &mut report)?;
        merge_metrics(self, other.metrics, &mut report)?;
        merge_enrichments(self, other.enrichments, &mut report)?;
        merge_data_qualities(self, other.data_quality, &mut report)?;

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Per-collection merge helpers.
//
// Each follows the same shape so adding a new IR collection is a
// copy-paste of one of these blocks with the right kind string,
// `add_X` method, and id accessor — no shared abstraction is needed
// (each Def has a slightly different add signature) and the explicit
// blocks keep the per-kind handling visible for review.
// ---------------------------------------------------------------------------

fn merge_node_types(
    base: &mut OntologyIR,
    incoming: Vec<crate::ir::NodeTypeDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.node_types().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "node_type".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "node_type".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_node_type(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "node_type".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_edge_types(
    base: &mut OntologyIR,
    incoming: Vec<crate::ir::EdgeTypeDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.edge_types().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "edge_type".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "edge_type".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_edge_type(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "edge_type".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_indexes(
    base: &mut OntologyIR,
    incoming: Vec<crate::ir::IndexDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    // IndexDef is an enum; identity is "the same variant on the same
    // (node_label, property_keys) tuple". We compare by JSON value
    // which captures the variant + payload uniformly.
    for item in incoming {
        let id_repr = serde_json::to_string(&item).unwrap_or_else(|_| "<unserialisable>".into());
        if let Some(existing) = base.indexes().iter().find(|i| {
            serde_json::to_value(i).ok() == serde_json::to_value(&item).ok()
        }) {
            // Exact match — already present.
            let _ = existing;
            report.skipped.push(MergedItem {
                kind: "index".into(),
                id: id_repr,
            });
        } else {
            base.add_index(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "index".into(),
                id: id_repr,
            });
        }
    }
    Ok(())
}

fn merge_object_mappings(
    base: &mut OntologyIR,
    incoming: Vec<crate::mapping::ObjectMappingDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.object_mappings().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "object_mapping".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "object_mapping".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_object_mapping(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "object_mapping".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_link_mappings(
    base: &mut OntologyIR,
    incoming: Vec<crate::mapping::LinkMappingDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.link_mappings().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "link_mapping".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "link_mapping".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_link_mapping(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "link_mapping".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_interfaces(
    base: &mut OntologyIR,
    incoming: Vec<crate::interface::InterfaceDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.interfaces().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "interface".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "interface".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_interface(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "interface".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_glossary_terms(
    base: &mut OntologyIR,
    incoming: Vec<crate::glossary::GlossaryTermDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.glossary().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "glossary_term".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "glossary_term".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_glossary_term(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "glossary_term".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_code_systems(
    base: &mut OntologyIR,
    incoming: Vec<crate::code_system::CodeSystemDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.code_systems().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "code_system".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "code_system".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_code_system(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "code_system".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_value_sets(
    base: &mut OntologyIR,
    incoming: Vec<crate::value_set::ValueSetDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.value_sets().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "value_set".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "value_set".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_value_set(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "value_set".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_concept_maps(
    base: &mut OntologyIR,
    incoming: Vec<crate::concept_map::ConceptMapDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.concept_maps().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "concept_map".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "concept_map".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_concept_map(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "concept_map".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_notation_patterns(
    base: &mut OntologyIR,
    incoming: Vec<crate::notation_pattern::NotationPatternDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.notation_patterns().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "notation_pattern".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "notation_pattern".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_notation_pattern(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "notation_pattern".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_value_range_sets(
    base: &mut OntologyIR,
    incoming: Vec<crate::value_range::ValueRangeSetDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.value_range_sets().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "value_range_set".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "value_range_set".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_value_range_set(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "value_range_set".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_rules(
    base: &mut OntologyIR,
    incoming: Vec<crate::rule::RuleDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.rules().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "rule".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "rule".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_rule(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "rule".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_actions(
    base: &mut OntologyIR,
    incoming: Vec<crate::action::ActionDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.actions().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "action".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "action".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_action(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "action".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_functions(
    base: &mut OntologyIR,
    incoming: Vec<crate::function::FunctionDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.functions().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "function".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "function".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_function(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "function".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_metrics(
    base: &mut OntologyIR,
    incoming: Vec<crate::metric::MetricDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.metrics().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "metric".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "metric".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_metric(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "metric".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_enrichments(
    base: &mut OntologyIR,
    incoming: Vec<crate::enrichment::EnrichmentDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.enrichments().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "enrichment".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "enrichment".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_enrichment(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "enrichment".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

fn merge_data_qualities(
    base: &mut OntologyIR,
    incoming: Vec<crate::data_quality::DataQualityDef>,
    report: &mut MergeReport,
) -> OxResult<()> {
    for item in incoming {
        let id_str = item.id.to_string();
        if let Some(existing) = base.data_quality().iter().find(|n| n.id == item.id) {
            if defs_match(existing, &item)? {
                report.skipped.push(MergedItem {
                    kind: "data_quality".into(),
                    id: id_str,
                });
            } else {
                report.conflicts.push(MergeConflict {
                    kind: "data_quality".into(),
                    id: id_str,
                    reason: "content differs from existing entry".into(),
                });
            }
        } else {
            base.add_data_quality(item)
                .map_err(|e| OxError::Ontology { message: e.to_string() })?;
            report.added.push(MergedItem {
                kind: "data_quality".into(),
                id: id_str,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use ox_core::graph_label::GraphLabel;
    use ox_core::i18n::LocalizedText;

    use crate::ir::{EdgeTypeDef, NodeTypeDef, OntologyIR};

    fn label(s: &'static str) -> GraphLabel {
        GraphLabel::new(s).expect("test label")
    }

    fn node(id: &str, l: &'static str) -> NodeTypeDef {
        NodeTypeDef {
            id: id.into(),
            label: label(l),
            description: LocalizedText::default(),
            properties: vec![],
            constraints: vec![],
            ..Default::default()
        }
    }

    fn empty_ir(name: &str) -> OntologyIR {
        OntologyIR::new(
            name.to_string(),
            name.to_string(),
            LocalizedText::default(),
            1,
            vec![],
            vec![],
            vec![],
        )
    }

    #[test]
    fn extend_adds_new_node_type() {
        let mut base = empty_ir("base");
        let mut other = empty_ir("other");
        other.add_node_type(node("n-user", "User")).expect("seed");

        let report = base.extend(other).expect("extend");
        assert_eq!(report.added.len(), 1);
        assert_eq!(report.added[0].kind, "node_type");
        assert_eq!(report.added[0].id, "n-user");
        assert_eq!(report.skipped.len(), 0);
        assert_eq!(report.conflicts.len(), 0);
        assert_eq!(base.node_types().len(), 1);
    }

    #[test]
    fn extend_skips_byte_for_byte_identical_node() {
        let mut base = empty_ir("base");
        base.add_node_type(node("n-user", "User")).expect("seed");
        let mut other = empty_ir("other");
        other.add_node_type(node("n-user", "User")).expect("seed");

        let report = base.extend(other).expect("extend");
        assert_eq!(report.added.len(), 0);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.conflicts.len(), 0);
        assert!(report.is_empty(), "no actionable change");
    }

    #[test]
    fn extend_records_conflict_when_content_differs_for_same_id() {
        let mut base = empty_ir("base");
        let mut original = node("n-user", "User");
        original.description = LocalizedText::new("original description");
        base.add_node_type(original).expect("seed");

        let mut other = empty_ir("other");
        let mut diverging = node("n-user", "User");
        diverging.description = LocalizedText::new("changed description");
        other.add_node_type(diverging).expect("seed");

        let report = base.extend(other).expect("extend");
        assert_eq!(report.added.len(), 0);
        assert_eq!(report.skipped.len(), 0);
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].kind, "node_type");
        assert_eq!(report.conflicts[0].id, "n-user");
        // Base content is preserved — conflict drops the incoming item.
        let target_id: crate::ir::NodeTypeId = "n-user".into();
        let kept = base.node_types().iter().find(|n| n.id == target_id);
        assert!(kept.is_some());
        let canonical = kept.expect("kept");
        assert_eq!(canonical.description.default, "original description");
    }

    #[test]
    fn extend_grows_multiple_collections_in_one_call() {
        let mut base = empty_ir("base");
        base.add_node_type(node("n-user", "User")).expect("seed user");

        let mut other = empty_ir("other");
        other.add_node_type(node("n-order", "Order")).expect("seed order");
        // Edge between two new nodes — caller's responsibility to
        // ensure source/target exist post-merge.
        other.add_node_type(node("n-product", "Product")).expect("seed product");
        other
            .add_edge_type(EdgeTypeDef {
                id: "e-purchased".into(),
                label: label("PURCHASED"),
                description: LocalizedText::default(),
                source_node_id: "n-user".into(),
                target_node_id: "n-order".into(),
                ..Default::default()
            })
            .expect("seed edge");

        let report = base.extend(other).expect("extend");
        let added_kinds: Vec<&str> = report.added.iter().map(|i| i.kind.as_str()).collect();
        assert_eq!(added_kinds.iter().filter(|k| **k == "node_type").count(), 2);
        assert_eq!(added_kinds.iter().filter(|k| **k == "edge_type").count(), 1);
        assert_eq!(base.node_types().len(), 3);
        assert_eq!(base.edge_types().len(), 1);
    }
}
