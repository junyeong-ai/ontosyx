//! Persisted insight artefact — a saved multi-hop discovery the
//! team wants to keep, share, and re-run as the underlying
//! schema/data evolves.
//!
//! Sits in `ox-query-ir` because the load-bearing field is the
//! [`QueryIR`] re-run anchor; pulling that into `ox-ontology` would
//! invert the workspace dependency arrow. The transient
//! `InsightHint` (proactive hints generated from ontology
//! structure) stays in `ox-ontology::insight` — different audience,
//! different lifecycle, different layering home.
//!
//! Industry reference: Palantir Foundry "Insights", Looker Looks,
//! Snowflake `SAVED_QUERIES`. The shape favours **logical**
//! re-running over snapshot replay — saved cell values rot, the
//! IR + ontology version pair doesn't.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_core::define_id_newtype;
use ox_core::i18n::LocalizedText;

use crate::query::{QueryIR, QueryProvenance};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_insight(provenance: Option<serde_json::Value>) -> InsightDef {
        InsightDef {
            id: InsightId::new("ins-test"),
            question: LocalizedText::default(),
            description: LocalizedText::default(),
            tags: Vec::new(),
            concept_anchors: Vec::new(),
            query_ir: serde_json::Value::Null,
            original_provenance: provenance,
            author_id: Uuid::nil(),
            expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn compatibility_returns_compatible_when_ontology_and_version_match() {
        let ins = make_insight(Some(json!({
            "ontology_id": "ont-x", "ontology_version": "3"
        })));
        assert_eq!(
            ins.compatibility_with("ont-x", "3"),
            InsightCompatibility::Compatible
        );
    }

    #[test]
    fn compatibility_returns_version_drift_for_same_ontology_different_version() {
        let ins = make_insight(Some(json!({
            "ontology_id": "ont-x", "ontology_version": "2"
        })));
        match ins.compatibility_with("ont-x", "3") {
            InsightCompatibility::VersionDrift { saved_version } => {
                assert_eq!(saved_version, "2");
            }
            other => panic!("expected VersionDrift, got {other:?}"),
        }
    }

    #[test]
    fn compatibility_returns_ontology_mismatch_for_different_lineage() {
        let ins = make_insight(Some(json!({
            "ontology_id": "ont-old", "ontology_version": "3"
        })));
        match ins.compatibility_with("ont-new", "3") {
            InsightCompatibility::OntologyMismatch { saved_ontology_id } => {
                assert_eq!(saved_ontology_id, "ont-old");
            }
            other => panic!("expected OntologyMismatch, got {other:?}"),
        }
    }

    #[test]
    fn compatibility_returns_provenance_missing_when_provenance_absent() {
        let ins = make_insight(None);
        assert_eq!(
            ins.compatibility_with("ont-x", "3"),
            InsightCompatibility::ProvenanceMissing
        );
    }
}

define_id_newtype!(
    /// Stable identifier for an [`InsightDef`].
    InsightId
);

/// Persisted insight definition.
///
/// `query_ir` and `original_provenance` are typed `JsonValue` on the
/// wire so the OpenAPI spec stays import-friendly (the rich
/// `QueryIR` / `QueryProvenance` schemas live in the generated
/// `ox-query-ir` types and don't need to be re-emitted by every
/// endpoint that just round-trips an opaque blob). The Rust API
/// keeps the typed handles via [`InsightDef::query`] /
/// [`InsightDef::provenance`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct InsightDef {
    pub id: InsightId,
    /// Human-readable question — surfaces as the card title in the
    /// dashboard / inspector. Localised so a multi-region tenant
    /// can serve the same insight in the viewer's language.
    pub question: LocalizedText,
    /// Optional richer description / context — what the insight
    /// reveals, why it matters. Localised.
    #[serde(default)]
    pub description: LocalizedText,
    /// Open-ended tag set the admin UI uses for filtering. Common
    /// values: `"trend"`, `"distribution"`, `"anomaly"`,
    /// `"relationship"`, `"summary"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Glossary terms the insight realises. The wire payload is a
    /// `Vec<String>` of `GlossaryTermId`s — typed at the consumer
    /// boundary so adding the field doesn't drag the full ontology
    /// crate into the OpenAPI schema. The semantic axis: `tags` is
    /// freeform admin shorthand; `concept_anchors` ties the insight
    /// to the workspace's vocabulary (the 1-pager's
    /// "용어 사전이 다리" surface), so cross-team filtering by
    /// concept stays consistent even as tags drift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concept_anchors: Vec<String>,
    /// The query the insight runs (logical IR — survives backend /
    /// dialect changes). Wire shape = canonical `QueryIR` JSON.
    #[schema(value_type = Object)]
    pub query_ir: serde_json::Value,
    /// Provenance the insight was originally computed against —
    /// ontology id + version + registry hashes. Wire shape =
    /// canonical `QueryProvenance` JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub original_provenance: Option<serde_json::Value>,
    /// Author user id — owner of the insight; gates edit/delete in
    /// the admin UI.
    pub author_id: Uuid,
    /// When the insight stops being trustworthy. `None` = no
    /// expiry. Useful for rolling KPIs that only matter for a
    /// quarter; the dashboard hides expired insights from the
    /// default surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl InsightDef {
    /// Decode the stored `query_ir` blob back into a typed `QueryIR`.
    /// Returns `Err` only when the row was hand-edited to a shape
    /// the current schema rejects.
    pub fn query(&self) -> Result<QueryIR, serde_json::Error> {
        serde_json::from_value(self.query_ir.clone())
    }

    /// Decode the optional original provenance back into the typed
    /// shape; `Ok(None)` when no provenance was captured at save
    /// time.
    pub fn provenance(&self) -> Result<Option<QueryProvenance>, serde_json::Error> {
        match &self.original_provenance {
            Some(v) => serde_json::from_value(v.clone()).map(Some),
            None => Ok(None),
        }
    }

    /// Lightweight typed accessors over `original_provenance` so
    /// consumers can ask "what ontology / version was this against?"
    /// without round-tripping the full `QueryProvenance`. Returns
    /// `None` when the provenance is absent or the field wasn't
    /// captured at save time.
    pub fn ontology_id(&self) -> Option<String> {
        self.original_provenance
            .as_ref()
            .and_then(|v| v.get("ontology_id"))
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    pub fn ontology_version(&self) -> Option<String> {
        self.original_provenance
            .as_ref()
            .and_then(|v| v.get("ontology_version"))
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    /// Compatibility classification against the current ontology
    /// snapshot. Drives the FE's `InsightListPanel` badges so an
    /// operator can tell at a glance whether a saved discovery is
    /// runnable as-is, was authored on a different ontology
    /// (cannot migrate without a re-bind), or just lags the current
    /// version (re-running may produce different bindings but the
    /// schema is the same lineage).
    pub fn compatibility_with(
        &self,
        current_ontology_id: &str,
        current_version: &str,
    ) -> InsightCompatibility {
        match self.ontology_id() {
            None => InsightCompatibility::ProvenanceMissing,
            Some(saved) if saved != current_ontology_id => {
                InsightCompatibility::OntologyMismatch {
                    saved_ontology_id: saved,
                }
            }
            Some(_) => match self.ontology_version() {
                Some(v) if v == current_version => InsightCompatibility::Compatible,
                Some(saved) => InsightCompatibility::VersionDrift {
                    saved_version: saved,
                },
                None => InsightCompatibility::ProvenanceMissing,
            },
        }
    }
}

/// Result of [`InsightDef::compatibility_with`]. The FE renders one
/// badge per variant:
/// - `Compatible` — green, "runnable as-is"
/// - `VersionDrift` — amber, "same ontology, newer version; bindings
///   may need re-resolution"
/// - `OntologyMismatch` — red, "different ontology lineage; rebind
///   manually before running"
/// - `ProvenanceMissing` — grey, "saved without provenance; cannot
///   classify"
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum InsightCompatibility {
    Compatible,
    VersionDrift { saved_version: String },
    OntologyMismatch { saved_ontology_id: String },
    ProvenanceMissing,
}
