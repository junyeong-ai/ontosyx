//! End-to-end regression suite for the IR-level invariants pinned
//! across the 04-23 → 04-29 sessions. Each invariant lives next to
//! the validator code that enforces it; this file is the cross-
//! invariant harness — `OntologyIR::validate` should accept a clean
//! fixture and reject every targeted misuse.
//!
//! Adding a new invariant: extend the fixture builder if the new
//! rule needs setup, then add one accept-test (the clean fixture
//! satisfies it) and one reject-test (a minimal mutation triggers
//! the diagnostic code).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use ox_core::i18n::LocalizedText;
use ox_ontology::OntologyIR;
use ox_ontology::glossary::{GlossaryTermDef, TermGovernance, TermLifecycle};
use ox_ontology::test_fixtures::test_ontology;

fn diagnostic_codes(ir: &OntologyIR) -> Vec<String> {
    ir.validate().into_iter().map(|d| d.code).collect()
}

fn glossary_term(id: &str, label: &str) -> GlossaryTermDef {
    GlossaryTermDef {
        id: id.into(),
        term: LocalizedText::new(label),
        display_name: LocalizedText::default(),
        description: LocalizedText::default(),
        examples: Vec::new(),
        category: None,
        aliases: Vec::new(),
        related_terms: Vec::new(),
        governance: TermGovernance::default(),
        valid_from: None,
        valid_to: None,
        lifecycle: TermLifecycle::default(),
    realisation: None,
    }
}

// ---------------------------------------------------------------------------
// Baseline — the platform fixture must validate cleanly.
// ---------------------------------------------------------------------------

#[test]
fn baseline_test_ontology_validates_clean() {
    let ir = test_ontology();
    let codes = diagnostic_codes(&ir);
    assert!(
        codes.is_empty(),
        "baseline fixture must have zero validation diagnostics, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Invariant: glossary_anchors must resolve in OntologyIR.glossary
// ---------------------------------------------------------------------------

#[test]
fn glossary_anchor_resolves_when_term_exists() {
    let mut ir = test_ontology();
    ir.add_glossary_term(glossary_term("gt-customer", "Customer"))
        .expect("seed glossary term");
    // Round-trip via JSON — anchor mutation needs structural access
    // and `OntologyIR` exposes a typed setter (`add_node_glossary_anchor`)
    // for production paths but not a mutable handle into `node_types`.
    // The JSON detour stays well-bounded (full IR is serializable).
    let mut value = serde_json::to_value(&ir).expect("ir to json");
    value["node_types"][0]["glossary_anchors"] =
        serde_json::json!(["gt-customer"]);
    let mutated: OntologyIR =
        serde_json::from_value(value).expect("json to ir");
    let codes = diagnostic_codes(&mutated);
    assert!(
        codes.is_empty(),
        "anchor pointing at an existing term must validate, got: {codes:?}",
    );
}

#[test]
fn glossary_anchor_pointing_at_unknown_term_is_rejected() {
    let ir = test_ontology();
    let mut value = serde_json::to_value(&ir).expect("ir to json");
    value["node_types"][0]["glossary_anchors"] =
        serde_json::json!(["gt-orphan"]);
    let mutated: OntologyIR =
        serde_json::from_value(value).expect("json to ir");
    let codes = diagnostic_codes(&mutated);
    assert!(
        codes
            .iter()
            .any(|c| c == "ontology.validate.node.unknown_glossary_anchor"),
        "expected `unknown_glossary_anchor` diagnostic, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Invariant: EdgeKind::Composition requires source-singular cardinality
// (commit 26af8d8 — UML strong ownership).
// ---------------------------------------------------------------------------

#[test]
fn composition_with_many_to_one_source_is_rejected() {
    let ir = test_ontology();
    let mut value = serde_json::to_value(&ir).expect("ir to json");
    value["edge_types"][0]["kind"] = serde_json::json!("composition");
    value["edge_types"][0]["cardinality"] = serde_json::json!("many_to_one");
    let mutated: OntologyIR =
        serde_json::from_value(value).expect("json to ir");
    let codes = diagnostic_codes(&mutated);
    assert!(
        codes
            .iter()
            .any(|c| c == "ontology.validate.edge.composition_requires_singular_source"),
        "expected composition cardinality diagnostic, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Invariant: RuleDef.sh_message routes through but doesn't itself
// affect validation — adding it must not regress the clean fixture
// (D1 / ADR 0013).
// ---------------------------------------------------------------------------

#[test]
fn rule_sh_message_does_not_alter_validation() {
    use ox_ontology::action::RuleId;
    use ox_ontology::rule::{
        ConstraintTarget, EnforcementKind, RuleActivationKind, RuleDef, RuleKind,
        RuleOrigin, Severity, ShaclConstraint,
    };

    let mut ir = test_ontology();
    let nt = ir.node_types()[0].clone();
    let nt_id = nt.id.clone();
    let prop_id = nt.properties[0].id.clone();
    let rule = RuleDef {
        id: RuleId::new("r-min-count"),
        name: LocalizedText::new("Name must be present"),
        description: LocalizedText::default(),
        rationale: LocalizedText::default(),
        kind: RuleKind::PropertyShape {
            target_node_type_id: nt_id,
            target_property_id: prop_id,
        },
        severity: Severity::Violation,
        enforcement: EnforcementKind::Write,
        activation: RuleActivationKind::Always,
        origin: RuleOrigin::Authored,
        constraints: vec![ShaclConstraint::MinCount {
            target: ConstraintTarget::Inherit,
            min: 1,
        }],
        valid_from: None,
        valid_to: None,
        sh_message: Some(LocalizedText::bilingual(
            "이름은 필수입니다.",
            "Name is required.",
        )),
    };
    ir.add_rule(rule).expect("rule should add cleanly");
    let codes = diagnostic_codes(&ir);
    assert!(
        codes.is_empty(),
        "rule with sh_message set must not trip validation, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Invariant: SourceMappingArtifact's content hash is deterministic
// (B1 / ADR 0011) — same body in, same hash out, regardless of
// allocation churn or clone identity.
// ---------------------------------------------------------------------------

#[test]
fn source_mapping_artifact_hash_is_stable_across_clones() {
    use chrono::{DateTime, Utc};
    use ox_ontology::source_mapping::{
        ArtifactProvenance, SourceMappingArtifact,
    };
    use std::collections::BTreeMap;

    let a = SourceMappingArtifact {
        id: "sma-1".into(),
        source_id: "pg-main".into(),
        schema_snapshot_hash: "deadbeef".into(),
        property_mappings: Vec::new(),
        edge_mappings: Vec::new(),
        open_questions: Vec::new(),
        provenance: ArtifactProvenance {
            prompt_id: "design.standard".into(),
            prompt_version: "0.4.2".into(),
            model_id: "anthropic:claude-sonnet-4-6".into(),
            params: BTreeMap::new(),
            prompt_render_hash: String::new(),
        },
        created_at: DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        created_by: "user-1".into(),
    };
    let b = a.clone();
    assert_eq!(a.content_hash(), b.content_hash());
}
