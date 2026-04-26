//! `GET /api/ontologies/{id}/map-summary` — aggregated counts per
//! v3 six-axis section + a `danglers` list (Phase 1 integrity check
//! surface). Feeds the Phase 4.2 Complete Map dashboard without
//! forcing the FE to re-walk a full OntologyIR payload client-side.

use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use uuid::Uuid;

use ox_ontology::integrity::RegistryReferenceCheck;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct MapSummaryResponse {
    pub ontology_id: Uuid,
    pub version: Option<String>,
    /// Topology axis — node / edge / index counts.
    pub topology: AxisCounts,
    /// Vocabulary axis — glossary / interface counts.
    pub vocabulary: AxisCounts,
    /// Registry axis — code system / value set / notation pattern /
    /// concept map counts.
    pub registry: AxisCounts,
    /// Strategy axis — rule / segment / action / function / metric
    /// counts.
    pub strategy: AxisCounts,
    /// VOL axis — object mapping / link mapping / property mapping
    /// counts.
    pub vol: AxisCounts,
    /// Governance axis — provenance / data quality / enrichment
    /// counts.
    pub governance: AxisCounts,
    /// Dangling registry references — empty when every pointer
    /// resolves.
    pub danglers: Vec<DanglerEntry>,
}

#[derive(Debug, Serialize)]
pub struct AxisCounts {
    pub entries: Vec<AxisEntry>,
}

#[derive(Debug, Serialize)]
pub struct AxisEntry {
    pub kind: &'static str,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct DanglerEntry {
    pub kind: String,
    pub source_path: String,
    pub missing_id: String,
}

#[utoipa::path(
    get,
    path = "/api/ontologies/{id}/map-summary",
    params(("id" = Uuid, Path, description = "Ontology identity id")),
    responses(
        (status = 200, description = "Six-axis summary + dangling references", body = Object),
        (status = 404, description = "Ontology not found or has no committed version"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn map_summary(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<MapSummaryResponse>>, AppError> {
    let identity = state
        .store
        .get_ontology(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology"))?;
    let current = state
        .store
        .get_current_version(identity.id)
        .await
        .map_err(AppError::from)?;
    let Some(version) = current else {
        return Err(AppError::not_found("ontology has no committed version"));
    };
    let ir = state
        .store
        .get_ontology_ir(version.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology version"))?;

    let topology = AxisCounts {
        entries: vec![
            AxisEntry {
                kind: "node_types",
                count: ir.node_types().len(),
            },
            AxisEntry {
                kind: "edge_types",
                count: ir.edge_types().len(),
            },
            AxisEntry {
                kind: "indexes",
                count: ir.indexes().len(),
            },
        ],
    };
    let vocabulary = AxisCounts {
        entries: vec![
            AxisEntry {
                kind: "glossary_terms",
                count: ir.glossary().len(),
            },
            AxisEntry {
                kind: "interfaces",
                count: ir.interfaces().len(),
            },
        ],
    };
    let registry = AxisCounts {
        entries: vec![
            AxisEntry {
                kind: "code_systems",
                count: ir.code_systems().len(),
            },
            AxisEntry {
                kind: "value_sets",
                count: ir.value_sets().len(),
            },
            AxisEntry {
                kind: "notation_patterns",
                count: ir.notation_patterns().len(),
            },
            AxisEntry {
                kind: "concept_maps",
                count: ir.concept_maps().len(),
            },
            AxisEntry {
                kind: "value_range_sets",
                count: ir.value_range_sets().len(),
            },
        ],
    };
    let strategy = AxisCounts {
        entries: vec![
            AxisEntry {
                kind: "rules",
                count: ir.rules().len(),
            },
            AxisEntry {
                kind: "actions",
                count: ir.actions().len(),
            },
            AxisEntry {
                kind: "functions",
                count: ir.functions().len(),
            },
            AxisEntry {
                kind: "metrics",
                count: ir.metrics().len(),
            },
        ],
    };
    let vol = AxisCounts {
        entries: vec![
            AxisEntry {
                kind: "object_mappings",
                count: ir.object_mappings().len(),
            },
            AxisEntry {
                kind: "link_mappings",
                count: ir.link_mappings().len(),
            },
        ],
    };
    let governance = AxisCounts {
        entries: vec![
            AxisEntry {
                kind: "provenances",
                count: ir.provenance().len(),
            },
            AxisEntry {
                kind: "data_qualities",
                count: ir.data_quality().len(),
            },
            AxisEntry {
                kind: "enrichments",
                count: ir.enrichments().len(),
            },
        ],
    };

    // Phase 1.7 integrity check — every `Option<XxxId>` pointer on
    // Property/Rule/ValueSet/ConceptMap gets walked and unresolved
    // ids are surfaced for the dashboard's red-marker layer.
    let danglers = ir
        .dangling_references()
        .into_iter()
        .map(|d| DanglerEntry {
            kind: format!("{:?}", d.kind),
            source_path: format!("{:?}", d.source),
            missing_id: d.missing_id,
        })
        .collect();

    Ok(ApiResponse::of(MapSummaryResponse {
        ontology_id: id,
        version: Some(version.version),
        topology,
        vocabulary,
        registry,
        strategy,
        vol,
        governance,
        danglers,
    }))
}
