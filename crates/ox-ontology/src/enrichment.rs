//! `EnrichmentDef` — externally-sourced augmentation of ontology
//! values.
//!
//! Where `ObjectMappingDef` binds a node type to a source *inside*
//! the workspace's registered data sources, an enrichment joins a
//! node type with an external service — IP-to-geo, a credit-score
//! endpoint, an embedding provider, or another ontology itself.
//!
//! Every enrichment declares:
//!
//! - which node type it augments,
//! - which node property is its join key,
//! - which target property it writes (a derived property on the
//!   node that lives outside the ontology's physical source),
//! - how often to refresh,
//! - a pointer to the external source (source id + endpoint /
//!   dataset name).
//!
//! Enrichment results are cache-first: the planner reads the cached
//! value if fresh, calls the external service on miss, and writes
//! the value back. The refresh policy determines freshness.

use chrono::Duration;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;

use crate::ir::{NodeTypeId, PropertyId};

ox_core::define_id_newtype!(
    /// Stable identifier for an `EnrichmentDef`.
    EnrichmentId
);

/// External augmentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct EnrichmentDef {
    pub id: EnrichmentId,
    pub name: String,

    #[serde(default)]
    pub description: LocalizedText,

    pub target_node_type_id: NodeTypeId,

    /// Property on the target node used as the lookup key — e.g.
    /// the IP address, the customer id, the postal code.
    pub join_key_property_id: PropertyId,

    /// Property on the target node that receives the enriched value.
    pub target_property_id: PropertyId,

    pub external_source: ExternalSourceRef,

    #[serde(default)]
    pub refresh: RefreshPolicy,
}

/// Pointer to the external service.
///
/// `kind` is a small, closed set so the planner can refuse a config
/// it does not know how to dispatch. Adding a new external-source
/// shape (gRPC, S3 object) is a variant addition — a major bump of
/// the ontology schema version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExternalSourceRef {
    /// REST / HTTPS endpoint. The platform substitutes the join-key
    /// value into `endpoint_template` via `{key}` placeholders.
    Http {
        endpoint_template: String,
        /// Authentication reference (a secret id registered in the
        /// workspace secret store).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auth_secret_id: Option<String>,
    },
    /// Another Ontosyx workspace / ontology — joins are evaluated
    /// through the same federation engine.
    SiblingOntology {
        workspace_slug: String,
        node_type_id: NodeTypeId,
    },
}

/// How often the enrichment value is refreshed.
///
/// `OnAccess` evaluates the enrichment every query; `Cached` reads
/// from cache when fresh (TTL-gated); `Scheduled` re-evaluates on a
/// cron cadence regardless of query activity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RefreshPolicy {
    #[default]
    OnAccess,
    Cached {
        ttl_seconds: u64,
    },
    Scheduled {
        cron_expression: String,
    },
}

impl RefreshPolicy {
    /// Build a `Cached` policy from a `chrono::Duration`. Negative
    /// inputs clamp to 0, which becomes "never expires" by
    /// convention (useful for reference data that virtually never
    /// changes — postal-code lookups, for example).
    pub fn cached(window: Duration) -> Self {
        RefreshPolicy::Cached {
            ttl_seconds: window.num_seconds().max(0) as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_refresh_is_on_access() {
        assert!(matches!(RefreshPolicy::default(), RefreshPolicy::OnAccess));
    }

    #[test]
    fn cached_helper_clamps_negatives_to_zero() {
        let r = RefreshPolicy::cached(Duration::seconds(-5));
        assert!(matches!(r, RefreshPolicy::Cached { ttl_seconds: 0 }));
    }

    #[test]
    fn enrichment_roundtrips_through_json() {
        let e = EnrichmentDef {
            id: EnrichmentId::new("e-ip-geo"),
            name: "ip_geo".into(),
            description: LocalizedText::default(),
            target_node_type_id: NodeTypeId::new("nt-visitor"),
            join_key_property_id: PropertyId::new("prop-ip"),
            target_property_id: PropertyId::new("prop-country"),
            external_source: ExternalSourceRef::Http {
                endpoint_template: "https://geoip.example/{key}".into(),
                auth_secret_id: Some("geoip_key".into()),
            },
            refresh: RefreshPolicy::cached(Duration::hours(24)),
        };
        let j = serde_json::to_value(&e).unwrap();
        let back: EnrichmentDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, e);
    }
}
