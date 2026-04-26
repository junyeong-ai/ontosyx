//! `ProvenanceDef` — W3C PROV-O aligned origin record (ADR 0008).
//!
//! Every fact asserted by the platform carries a `ProvenanceDef`
//! pointer that names *what activity produced it*, *which agent was
//! responsible*, and *what inputs it derived from*. The record
//! shape mirrors PROV-O's three core types (Entity / Activity /
//! Agent) with typed relations (`wasDerivedFrom`, `used`,
//! `wasAttributedTo`, `atTime`).
//!
//! Keeping provenance first-class (rather than a diagnostic sidecar)
//! lets downstream systems walk a result's lineage without a second
//! log pipeline: "what changed this address?" becomes a single
//! provenance query.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::action::ActionId;
use crate::enrichment::EnrichmentId;
use crate::function::FunctionId;
use crate::ir::{EdgeTypeId, NodeTypeId, PropertyId};
use crate::mapping::{ObjectMappingId, SourceId};
use crate::action::RuleId;

ox_core::define_id_newtype!(
    /// Stable identifier for a provenance record.
    ProvenanceId
);

/// Origin record for one fact the platform produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ProvenanceDef {
    pub id: ProvenanceId,

    /// The entity whose origin this record explains. Polymorphic —
    /// PROV-O equivalent: `prov:Entity`.
    pub subject: EntityRef,

    /// The activity that produced `subject`. PROV-O: `prov:Activity`.
    pub activity: ProvenanceActivityKind,

    /// Who ran the activity. PROV-O: `prov:wasAssociatedWith`.
    pub agent: AgentRef,

    /// Timestamp the activity occurred at. PROV-O: `prov:atTime`.
    pub at_time: DateTime<Utc>,

    /// Input entities the activity used. PROV-O: `prov:used`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub used: Vec<EntityRef>,

    /// Parent entities `subject` was derived from. PROV-O:
    /// `prov:wasDerivedFrom`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<EntityRef>,

    /// Ontology time the subject was valid under (bitemporal axis,
    /// ADR 0007). `None` means "current".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ontology_valid_at: Option<DateTime<Utc>>,

    /// Data time the subject was valid under (bitemporal axis).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_valid_at: Option<DateTime<Utc>>,
}

/// What the record points at — intentionally a closed set of
/// ontology-modelled entities plus a generic free-form form for
/// subjects the core model does not type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntityRef {
    NodeInstance {
        node_type_id: NodeTypeId,
        element_id: String,
    },
    EdgeInstance {
        edge_type_id: EdgeTypeId,
        element_id: String,
    },
    PropertyValue {
        node_type_id: NodeTypeId,
        element_id: String,
        property_id: PropertyId,
    },
    /// Free-form label for anything outside the typed set —
    /// validation reports, exported documents, cached query
    /// results, etc.
    Arbitrary { label: String },
}

/// PROV-O activity with platform-specific detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProvenanceActivityKind {
    /// Source scan via an object mapping (ADR 0003). Records which
    /// mapping fetched the data and which underlying source it
    /// came from.
    SourceScan {
        source_id: SourceId,
        mapping_id: ObjectMappingId,
    },
    /// Evaluation of a derived property / UDF.
    FunctionEval { function_id: FunctionId },
    /// Rule validation outcome.
    RuleValidate {
        rule_id: RuleId,
        outcome: ValidationOutcomeKind,
    },
    /// Action execution. Records the idempotency key so replays
    /// attribute correctly.
    ActionExecute {
        action_id: ActionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        idempotency_key: Option<String>,
    },
    /// Ontology edit — create / rename / delete of a type.
    OntologyEdit { command_summary: String },
    /// LLM-produced draft. Tracks prompt + model so a downstream
    /// audit can trace hallucinations back to their source.
    DraftProposal {
        prompt_name: String,
        prompt_version: String,
        model_id: String,
    },
    /// Graph cache refresh on a specific mapping.
    CacheRefresh { mapping_id: ObjectMappingId },
    /// External enrichment call.
    Enrichment { enrichment_id: EnrichmentId },
    Import {
        format: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_uri: Option<String>,
    },
    Export {
        format: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        destination_uri: Option<String>,
    },
}

/// Outcome of a rule evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValidationOutcomeKind {
    Pass,
    Warn,
    Fail,
}

/// Who ran the activity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentRef {
    User { user_id: String },
    Service { service_id: String },
    LlmModel { model_id: String },
    /// Scheduled task / reconciler. Used when no human principal
    /// is in the loop (cache refresh cron, batch data-quality run).
    System,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_scan_provenance_roundtrips() {
        let p = ProvenanceDef {
            id: ProvenanceId::new("pr-1"),
            subject: EntityRef::NodeInstance {
                node_type_id: NodeTypeId::new("nt-customer"),
                element_id: "cust-123".into(),
            },
            activity: ProvenanceActivityKind::SourceScan {
                source_id: SourceId::new("pg-main"),
                mapping_id: ObjectMappingId::new("om-customers"),
            },
            agent: AgentRef::System,
            at_time: Utc::now(),
            used: vec![],
            derived_from: vec![],
            ontology_valid_at: None,
            data_valid_at: None,
        };
        let j = serde_json::to_value(&p).unwrap();
        let back: ProvenanceDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn llm_draft_records_prompt_and_model() {
        let p = ProvenanceDef {
            id: ProvenanceId::new("pr-2"),
            subject: EntityRef::Arbitrary {
                label: "ontology-draft-v17".into(),
            },
            activity: ProvenanceActivityKind::DraftProposal {
                prompt_name: "design_ontology".into(),
                prompt_version: "1.2.0".into(),
                model_id: "anthropic/claude-opus-4-7".into(),
            },
            agent: AgentRef::LlmModel {
                model_id: "anthropic/claude-opus-4-7".into(),
            },
            at_time: Utc::now(),
            used: vec![EntityRef::Arbitrary {
                label: "schema-snapshot-2026-04-20".into(),
            }],
            derived_from: vec![],
            ontology_valid_at: Some(Utc::now()),
            data_valid_at: None,
        };
        let j = serde_json::to_string(&p).unwrap();
        assert!(j.contains("claude-opus-4-7"));
        assert!(j.contains("design_ontology"));
    }
}
