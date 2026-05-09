//! Schema-pinned regression test for the OpenAPI surface that
//! drives FE codegen.
//!
//! The FE `web/src/types/api.generated.ts` is regenerated from
//! `web/openapi.json`, which in turn is dumped from
//! `ox_api::openapi::ApiDoc`. If any future commit reverts the
//! Phase-A wire-shape unification — e.g., re-marks the IR as
//! `value_type = Object`, drops `ToSchema` from `LocalizedText`, or
//! collapses `OntologyDetail.description` back to a plain string —
//! this test fails before the regression reaches the FE.
//!
//! Three invariants only. Snapshot tests on the entire spec
//! create noise on every legitimate change; the focused
//! assertions here only fire when the *load-bearing* shape moves.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::Value;
use utoipa::OpenApi;

fn schema(spec: &Value, name: &str) -> Value {
    spec.pointer(&format!("/components/schemas/{name}"))
        .unwrap_or_else(|| panic!("schema {name} missing from OpenAPI spec"))
        .clone()
}

fn dump_spec() -> Value {
    let api = ox_api::openapi::ApiDoc::openapi();
    serde_json::to_value(&api).expect("OpenAPI spec serialises as JSON")
}

fn assert_schema_ref(spec: &Value, pointer: &str, expected_schema: &str) {
    let schema = spec
        .pointer(pointer)
        .unwrap_or_else(|| panic!("OpenAPI schema pointer {pointer} missing"));
    let stringified = schema.to_string();
    assert!(
        stringified.contains(expected_schema),
        "OpenAPI schema at {pointer} must reference {expected_schema}. Got: {stringified}",
    );
}

fn assert_page_response_ref(spec: &Value, response_schema_pointer: &str, page: &str, item: &str) {
    assert_schema_ref(spec, response_schema_pointer, page);
    assert_schema_ref(
        spec,
        &format!("/components/schemas/{page}/properties/items/items"),
        item,
    );
}

fn ontology_command_variant<'a>(command_schema: &'a Value, op: &str) -> &'a Value {
    command_schema
        .pointer("/oneOf")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("OntologyCommand must be a oneOf union. Got: {command_schema}"))
        .iter()
        .find(|variant| {
            variant
                .pointer("/properties/op/enum")
                .and_then(Value::as_array)
                .is_some_and(|ops| ops.iter().any(|candidate| candidate == op))
        })
        .unwrap_or_else(|| panic!("OntologyCommand variant '{op}' missing. Got: {command_schema}"))
}

fn tagged_variant<'a>(schema: &'a Value, tag: &str, value: &str) -> &'a Value {
    schema
        .pointer("/oneOf")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("schema must be a oneOf union. Got: {schema}"))
        .iter()
        .find(|variant| {
            variant
                .pointer(&format!("/properties/{tag}/enum"))
                .and_then(Value::as_array)
                .is_some_and(|values| values.iter().any(|candidate| candidate == value))
        })
        .unwrap_or_else(|| panic!("variant '{value}' missing for tag '{tag}'. Got: {schema}"))
}

fn router_paths_from_source() -> Vec<String> {
    let routes = include_str!("../../src/routes/mod.rs");
    let mut paths = Vec::new();
    let mut rest = routes;
    while let Some(idx) = rest.find(".route(") {
        rest = &rest[idx + ".route(".len()..];
        let trimmed = rest.trim_start();
        let Some(after_quote) = trimmed.strip_prefix('"') else {
            continue;
        };
        let Some(end) = after_quote.find('"') else {
            continue;
        };
        let path = &after_quote[..end];
        if path != "/ws/collab" {
            paths.push(format!("/api{path}"));
        }
        rest = &after_quote[end + 1..];
    }
    paths.sort();
    paths.dedup();
    paths
}

fn collect_rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|err| {
        panic!("failed to read directory {}: {err}", dir.display());
    }) {
        let entry = entry.expect("directory entry readable");
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn ontology_ir_is_a_typed_schema_not_an_opaque_object() {
    let spec = dump_spec();
    let detail = schema(&spec, "OntologyDetail");

    let ontology_ir_field = detail
        .pointer("/properties/ontology_ir")
        .expect("OntologyDetail.ontology_ir property defined");

    // The pre-fix schema was `{"type": "object"}` — opaque, no
    // structure. The post-fix shape is `$ref` to OntologyIR (or
    // `oneOf` / `allOf` containing the ref because the field is
    // optional). Either form is correct as long as the structured
    // type is reachable.
    let stringified = ontology_ir_field.to_string();
    assert!(
        stringified.contains("OntologyIR"),
        "OntologyDetail.ontology_ir must reference the typed OntologyIR \
         schema, not be an opaque Object. Got: {stringified}",
    );
    assert!(
        spec.pointer("/components/schemas/OntologyIR").is_some(),
        "OntologyIR itself must be registered as a schema component",
    );
}

#[test]
fn node_type_def_description_field_resolves_to_localized_text() {
    let spec = dump_spec();
    let node_type = schema(&spec, "NodeTypeDef");

    let description = node_type
        .pointer("/properties/description")
        .expect("NodeTypeDef.description property defined");

    let stringified = description.to_string();
    assert!(
        stringified.contains("LocalizedText"),
        "NodeTypeDef.description must reference LocalizedText (not a \
         plain string) so FE codegen carries `{{default, translations}}` \
         through. Got: {stringified}",
    );
}

#[test]
fn localized_text_translations_is_a_string_to_string_map() {
    let spec = dump_spec();
    let lt = schema(&spec, "LocalizedText");

    let translations = lt
        .pointer("/properties/translations")
        .expect("LocalizedText.translations property defined");

    // Map<String, String> shape: `{"type": "object", "additionalProperties": {"type": "string"}}`.
    // The exact path utoipa generates is checked here so a future
    // bump that loses `additionalProperties` (and turns the map into
    // an opaque object) fails the test.
    let value_type = translations
        .pointer("/additionalProperties/type")
        .or_else(|| translations.pointer("/additionalProperties/$ref"))
        .unwrap_or_else(|| {
            panic!(
                "LocalizedText.translations must declare additionalProperties \
                 as a string map. Got: {translations}",
            )
        });

    assert!(
        value_type == "string"
            || value_type
                .as_str()
                .map(|s| s.contains("String"))
                .unwrap_or(false),
        "LocalizedText.translations values must be `string`. Got: {value_type}",
    );
}

#[test]
fn property_type_schema_matches_tagged_wire_shape() {
    let spec = dump_spec();
    let property_type = schema(&spec, "PropertyType");
    let variants = property_type
        .pointer("/oneOf")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("PropertyType must be a oneOf union. Got: {property_type}"));

    for variant in [
        "bool",
        "int",
        "float",
        "string",
        "date",
        "date_time",
        "duration",
        "bytes",
        "map",
    ] {
        let scalar = variants
            .iter()
            .find(|schema| {
                schema
                    .pointer("/properties/type/enum")
                    .and_then(Value::as_array)
                    .is_some_and(|values| values.iter().any(|value| value == variant))
            })
            .unwrap_or_else(|| panic!("PropertyType scalar {variant} missing"));
        assert_eq!(
            scalar.pointer("/type"),
            Some(&Value::String("object".to_string())),
            "PropertyType.{variant} must be a tagged object. Got: {scalar}",
        );
        assert!(
            scalar.pointer("/properties/element").is_none(),
            "PropertyType.{variant} must not allow a stray element. Got: {scalar}",
        );
    }

    let list = variants
        .iter()
        .find(|schema| {
            schema
                .pointer("/properties/type/enum")
                .and_then(Value::as_array)
                .is_some_and(|values| values.iter().any(|value| value == "list"))
        })
        .unwrap_or_else(|| panic!("PropertyType list variant missing. Got: {property_type}"));
    assert!(
        list.pointer("/required")
            .and_then(Value::as_array)
            .is_some_and(|required| required.iter().any(|field| field == "element")),
        "PropertyType.list must require element. Got: {list}",
    );
    assert!(
        list.pointer("/properties/element")
            .is_some_and(|schema| schema.to_string().contains("PropertyType")),
        "PropertyType.list.element must recursively reference PropertyType. Got: {list}",
    );

    let generated = include_str!("../../../../web/src/types/api.generated.ts");
    assert!(
        generated.contains("element: components[\"schemas\"][\"PropertyType\"];")
            && !generated.contains("element?: components[\"schemas\"][\"PropertyType\"];"),
        "Generated frontend API types must expose PropertyType as the \
         tagged object union where only list requires element.",
    );
}

#[test]
fn property_value_schema_matches_tagged_wire_shape() {
    let spec = dump_spec();
    let property_value = schema(&spec, "PropertyValue");
    let variants = property_value
        .pointer("/oneOf")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("PropertyValue must be a oneOf union. Got: {property_value}"));

    let find_variant = |kind: &str| {
        variants
            .iter()
            .find(|schema| {
                schema
                    .pointer("/properties/type/enum")
                    .and_then(Value::as_array)
                    .is_some_and(|values| values.iter().any(|value| value == kind))
            })
            .unwrap_or_else(|| panic!("PropertyValue variant {kind} missing"))
    };

    let null = find_variant("null");
    assert!(
        null.pointer("/properties/value").is_none(),
        "PropertyValue.null must not carry value. Got: {null}",
    );

    for kind in [
        "bool",
        "int",
        "float",
        "string",
        "date",
        "date_time",
        "duration",
        "bytes",
        "list",
        "map",
    ] {
        let variant = find_variant(kind);
        assert!(
            variant
                .pointer("/required")
                .and_then(Value::as_array)
                .is_some_and(|required| required.iter().any(|field| field == "value")),
            "PropertyValue.{kind} must require value. Got: {variant}",
        );
    }

    let list = find_variant("list");
    assert!(
        list.pointer("/properties/value/items")
            .is_some_and(|schema| schema.to_string().contains("PropertyValue")),
        "PropertyValue.list.value items must recursively reference PropertyValue. Got: {list}",
    );

    let map = find_variant("map");
    assert!(
        map.pointer("/properties/value/additionalProperties")
            .is_some_and(|schema| schema.to_string().contains("PropertyValue")),
        "PropertyValue.map.value entries must recursively reference PropertyValue. Got: {map}",
    );

    let generated = include_str!("../../../../web/src/types/api.generated.ts");
    assert!(
        !generated.contains("PropertyValue: {\n            [key: string]: unknown;"),
        "Generated frontend API types must not expose PropertyValue as a \
         free-form object.",
    );
    assert!(
        generated.contains("type: \"null\";")
            && generated.contains("type: \"list\";")
            && generated.contains("value: components[\"schemas\"][\"PropertyValue\"][];"),
        "Generated frontend API types must expose recursive tagged \
         PropertyValue variants.",
    );
}

#[test]
fn community_summary_responses_are_typed_not_opaque_objects() {
    let spec = dump_spec();
    let response = schema(&spec, "CommunitySummaryResponse");
    let list_response = schema(&spec, "ListCommunitySummariesResponse");

    let summary = response
        .pointer("/properties/summary")
        .expect("CommunitySummaryResponse.summary property defined");
    assert!(
        summary.to_string().contains("CommunitySummaryDto"),
        "CommunitySummaryResponse.summary must reference CommunitySummaryDto, \
         not be an opaque object. Got: {summary}",
    );

    let items = list_response
        .pointer("/properties/items/items")
        .expect("ListCommunitySummariesResponse.items item schema defined");
    assert!(
        items.to_string().contains("CommunitySummaryDto"),
        "ListCommunitySummariesResponse.items must reference CommunitySummaryDto, \
         not be opaque objects. Got: {items}",
    );
}

#[test]
fn ambiguity_resolver_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "AmbiguitySummary",
        "AmbiguityListResponse",
        "AmbiguityDetailResponse",
        "ResolveAmbiguityRequest",
        "RevokeAmbiguityResponse",
        "BulkRevokeAmbiguitiesRequest",
        "BulkRevokeAmbiguitiesResponse",
        "AmbiguityContext",
        "AmbiguityKind",
        "RepoHint",
        "AmbiguityResolution",
        "AmbiguityMapping",
        "ValueMapEntry",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1ambiguities/get/responses/200/content/application~1json/schema",
        "AmbiguityListResponse",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ambiguities~1{id}/get/responses/200/content/application~1json/schema",
        "AmbiguityDetailResponse",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ambiguities~1{id}~1resolve/post/requestBody/content/application~1json/schema",
        "ResolveAmbiguityRequest",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ambiguities~1{id}~1resolve/post/responses/201/content/application~1json/schema",
        "AmbiguityResolution",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ambiguities~1{id}~1revoke/post/responses/200/content/application~1json/schema",
        "RevokeAmbiguityResponse",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ambiguities~1bulk-revoke/post/requestBody/content/application~1json/schema",
        "BulkRevokeAmbiguitiesRequest",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ambiguities~1bulk-revoke/post/responses/200/content/application~1json/schema",
        "BulkRevokeAmbiguitiesResponse",
    );
}

#[test]
fn ontology_exchange_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "InputOntologyDef",
        "InputNodeTypeDef",
        "InputEdgeTypeDef",
        "InputPropertyDef",
        "InputNodeConstraint",
        "InputIndexDef",
        "NormalizeOntologyResponse",
        "NormalizeWarning",
        "OntologyIR",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1normalize/post/requestBody/content/application~1json/schema",
        "InputOntologyDef",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1normalize/post/responses/200/content/application~1json/schema",
        "NormalizeOntologyResponse",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1export/post/requestBody/content/application~1json/schema",
        "OntologyIR",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1export/post/responses/200/content/application~1json/schema",
        "InputOntologyDef",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1import~1owl/post/responses/200/content/application~1json/schema",
        "OntologyIR",
    );

    for path in [
        "/paths/~1api~1ontology~1export~1cypher/post/requestBody/content/application~1json/schema",
        "/paths/~1api~1ontology~1export~1mermaid/post/requestBody/content/application~1json/schema",
        "/paths/~1api~1ontology~1export~1graphql/post/requestBody/content/application~1json/schema",
        "/paths/~1api~1ontology~1export~1owl/post/requestBody/content/application~1json/schema",
        "/paths/~1api~1ontology~1export~1shacl/post/requestBody/content/application~1json/schema",
        "/paths/~1api~1ontology~1export~1typescript/post/requestBody/content/application~1json/schema",
        "/paths/~1api~1ontology~1export~1python/post/requestBody/content/application~1json/schema",
    ] {
        assert_schema_ref(&spec, path, "OntologyIR");
    }
}

#[test]
fn ontology_schema_ops_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "AdoptGraphRequest",
        "GraphAuditReport",
        "SyncStatus",
        "InsightHint",
        "OntologyIR",
        "OntologyCommand",
        "PropertyPatch",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1audit/post/responses/200/content/application~1json/schema",
        "GraphAuditReport",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1adopt-graph/post/requestBody/content/application~1json/schema",
        "AdoptGraphRequest",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1adopt-graph/post/responses/200/content/application~1json/schema",
        "OntologyIR",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1suggestions/post/requestBody/content/application~1json/schema",
        "OntologyIR",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1suggestions/post/responses/200/content/application~1json/schema",
        "InsightHint",
    );
}

#[test]
fn ontology_command_surface_is_typed_not_free_form() {
    let spec = dump_spec();
    let command = schema(&spec, "OntologyCommand");
    let property_owner = schema(&spec, "PropertyOwner");

    for op in ["node", "edge"] {
        let variant = property_owner
            .pointer("/oneOf")
            .and_then(Value::as_array)
            .and_then(|variants| {
                variants.iter().find(|variant| {
                    variant
                        .pointer("/properties/kind/enum")
                        .and_then(Value::as_array)
                        .is_some_and(|kinds| kinds.iter().any(|kind| kind == op))
                })
            })
            .unwrap_or_else(|| {
                panic!("PropertyOwner '{op}' variant missing. Got: {property_owner}")
            });
        assert!(
            variant.pointer("/properties/type_id").is_some(),
            "PropertyOwner.{op} must carry type_id explicitly, not encode \
             the owner id as a newtype/allOf intersection. Got: {variant}",
        );
    }

    let add_node = ontology_command_variant(&command, "add_node");
    assert!(
        add_node.pointer("/properties/source_table").is_none(),
        "OntologyCommand.add_node must not expose legacy source_table. \
         Rich source lineage belongs on create_node_type.node.source_lineage. \
         Got: {add_node}",
    );

    for (op, field, expected_schema) in [
        ("create_node_type", "node", "NodeTypeDef"),
        ("create_edge_type", "edge", "EdgeTypeDef"),
        ("update_property", "patch", "PropertyPatch"),
        ("add_property", "owner", "PropertyOwner"),
        ("create_object_mapping", "mapping", "ObjectMappingDef"),
        ("create_link_mapping", "mapping", "LinkMappingDef"),
    ] {
        let variant = ontology_command_variant(&command, op);
        let field_schema = variant
            .pointer(&format!("/properties/{field}"))
            .unwrap_or_else(|| {
                panic!("OntologyCommand.{op}.{field} must be present. Got: {variant}")
            });
        assert!(
            field_schema.to_string().contains(expected_schema),
            "OntologyCommand.{op}.{field} must reference {expected_schema}, \
             not be an opaque object. Got: {field_schema}",
        );
    }

    let batch = ontology_command_variant(&command, "batch");
    let nested = batch
        .pointer("/properties/commands/items")
        .expect("OntologyCommand.batch.commands items schema defined");
    assert!(
        nested.to_string().contains("OntologyCommand"),
        "OntologyCommand.batch.commands must reference OntologyCommand \
         without expanding into an unbounded recursive object. Got: {nested}",
    );

    assert_schema_ref(
        &spec,
        "/components/schemas/ApplyOntologyCommandsRequest/properties/commands/items",
        "OntologyCommand",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EditOntologyDraftResponse/properties/commands/items",
        "OntologyCommand",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology-drafts~1{id}~1ontology/patch/requestBody/content/application~1json/schema",
        "ApplyOntologyCommandsRequest",
    );
    assert!(
        schema(&spec, "ServerMessage")
            .to_string()
            .contains("OntologyCommand"),
        "ServerMessage.entity_updated.commands must reference \
         OntologyCommand so remote operation previews are generated \
         from a typed contract",
    );
}

#[test]
fn ontology_complete_map_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "MapSummaryResponse",
        "AxisCounts",
        "AxisEntry",
        "DanglerEntry",
        "AxisItem",
        "Axis",
        "CrossRefEdge",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1map-summary/get/responses/200/content/application~1json/schema",
        "MapSummaryResponse",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1axis-items/get/responses/200/content/application~1json/schema",
        "AxisItem",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1cross-refs/get/responses/200/content/application~1json/schema",
        "CrossRefEdge",
    );
}

#[test]
fn ontology_admin_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "SchemaDependencyGraph",
        "DependencyBucket",
        "DependencyEdge",
        "ElementVerification",
        "EditOntologyRequest",
        "OntologyEditReceipt",
        "OntologyEditPreCheck",
        "EditOntologyResponse",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1dependencies/get/responses/200/content/application~1json/schema",
        "SchemaDependencyGraph",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1verifications/get/responses/200/content/application~1json/schema",
        "ElementVerification",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1edits/post/requestBody/content/application~1json/schema",
        "EditOntologyRequest",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1edits/post/responses/200/content/application~1json/schema",
        "EditOntologyResponse",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology~1edits/post/responses/201/content/application~1json/schema",
        "EditOntologyResponse",
    );
}

#[test]
fn ontology_inference_and_binding_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "SourceSchema",
        "SourceProfile",
        "LocalizedText",
        "OntologyEditOp",
        "CodeSystemDef",
        "ValueSetDef",
        "NotationPatternDef",
        "ProposeValueSetsRequest",
        "ProposalBody",
        "ProposeNotationPatternsRequest",
        "NotationProposalBody",
        "SuggestBindingsRequest",
        "ConceptCandidate",
        "CreateOntologyRequest",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    for pointer in [
        "/components/schemas/ProposeValueSetsRequest/properties/schema",
        "/components/schemas/ProposeNotationPatternsRequest/properties/schema",
    ] {
        assert_schema_ref(&spec, pointer, "SourceSchema");
    }
    for pointer in [
        "/components/schemas/ProposeValueSetsRequest/properties/profile",
        "/components/schemas/ProposeNotationPatternsRequest/properties/profile",
    ] {
        assert_schema_ref(&spec, pointer, "SourceProfile");
    }
    assert_schema_ref(
        &spec,
        "/components/schemas/ProposalBody/properties/code_system_json",
        "CodeSystemDef",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/ProposalBody/properties/value_set_json",
        "ValueSetDef",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/NotationProposalBody/properties/pattern_json",
        "NotationPatternDef",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/SuggestBindingsRequest/properties/term",
        "LocalizedText",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/SuggestBindingsRequest/properties/aliases/items",
        "LocalizedText",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/ConceptCandidate/properties/term",
        "LocalizedText",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/CreateOntologyRequest/properties/initial_operations/items",
        "OntologyEditOp",
    );
}

#[test]
fn ontology_draft_design_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "OntologyDraftView",
        "OntologyDraftOrigin",
        "OntologyDraftDataSourceSpec",
        "RepoSource",
        "AnalyzeSelection",
        "DesignOptions",
        "ConfirmedRelationship",
        "ColumnClarification",
        "SourceConfig",
        "SourceHistoryEntry",
        "SourceTypeKind",
        "SourceAnalysisReport",
        "SchemaStats",
        "AnalysisWarning",
        "WarningScope",
        "AnalysisScope",
        "DeferredTable",
        "SourceSchema",
        "SourceProfile",
        "ReconcileReport",
        "MatchDecision",
        "UncertainMatch",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology-drafts~1{id}~1decisions/patch/responses/200/content/application~1json/schema",
        "OntologyDraftView",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/CreateOntologyDraftRequest",
        "OntologyDraftOrigin",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/PreviewSourceRequest",
        "OntologyDraftDataSourceSpec",
    );
    let draft_source_spec = schema(&spec, "OntologyDraftDataSourceSpec");
    let draft_source_variants = draft_source_spec
        .pointer("/oneOf")
        .and_then(Value::as_array)
        .unwrap_or_else(|| {
            panic!("OntologyDraftDataSourceSpec must be a tagged union. Got: {draft_source_spec}")
        });
    for source_type in [
        "text",
        "csv",
        "json",
        "postgresql",
        "mysql",
        "mongodb",
        "snowflake",
        "bigquery",
        "duckdb",
        "code_repository",
    ] {
        assert!(
            draft_source_variants.iter().any(|variant| {
                variant
                    .pointer("/properties/type/enum")
                    .and_then(Value::as_array)
                    .is_some_and(|values| values.iter().any(|value| value == source_type))
            }),
            "OntologyDraftDataSourceSpec must include source type {source_type}. Got: {draft_source_spec}",
        );
    }
    let load_plan_source_spec = schema(&spec, "DataSourceSpec");
    assert!(
        load_plan_source_spec.to_string().contains("\"format\""),
        "Load-plan DataSourceSpec must remain separate from ontology draft source credentials",
    );
    let source_type_kind = schema(&spec, "SourceTypeKind");
    let source_type_values = source_type_kind
        .pointer("/enum")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("SourceTypeKind must be a string enum. Got: {source_type_kind}"));
    assert!(
        source_type_values.iter().any(|value| value == "duckdb")
            && !source_type_values.iter().any(|value| value == "duck_db"),
        "SourceTypeKind must use the runtime wire value `duckdb`, not `duck_db`. Got: {source_type_kind}",
    );
    let warning_class = schema(&spec, "WarningClass");
    let warning_values = warning_class
        .pointer("/enum")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("WarningClass must be a string enum. Got: {warning_class}"));
    for class in [
        "big_query_partition_filter_required",
        "big_query_clustering_filter_required",
        "big_query_jobs_create_denied",
    ] {
        assert!(
            warning_values.iter().any(|value| value == class),
            "WarningClass must include {class}. Got: {warning_class}",
        );
    }
    assert_schema_ref(
        &spec,
        "/components/schemas/UpdateOntologyDraftDecisionsRequest/properties/design_options",
        "DesignOptions",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/ReanalyzeOntologyDraftRequest/properties/selection",
        "AnalyzeSelection",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/ReconcileOntologyDraftRequest/properties/reconciled_ontology",
        "OntologyIR",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/ReconcileOntologyDraftRequest/properties/decisions/items",
        "MatchDecision",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/ReconcileOntologyDraftRequest/properties/uncertain_matches/items",
        "UncertainMatch",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/RefineOntologyDraftResponse/properties/reconcile_report",
        "ReconcileReport",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EditOntologyDraftResponse/properties/project",
        "OntologyDraftView",
    );
    assert_schema_ref(&spec, "/components/schemas/OntologyDraft", "SourceConfig");
    assert_schema_ref(
        &spec,
        "/components/schemas/OntologyDraft/properties/source_schema",
        "SourceSchema",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/OntologyDraft/properties/source_profile",
        "SourceProfile",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/OntologyDraft/properties/analysis_report",
        "SourceAnalysisReport",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/OntologyDraft/properties/design_options",
        "DesignOptions",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/OntologyDraft/properties/analysis_scope",
        "AnalysisScope",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/OntologyDraft/properties/source_history",
        "SourceHistoryEntry",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/OntologyDraftSummary/properties/source_config",
        "SourceConfig",
    );
}

#[test]
fn ontology_draft_lifecycle_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "OntologyDraftView",
        "OntologyDraftSummary",
        "OntologyDraftSummaryPage",
        "LoadPlan",
        "LoadResultResponse",
        "LoadErrorResponse",
        "GenerateOntologyDraftLoadPlanResponse",
        "CompileOntologyDraftLoadPlanRequest",
        "ExecuteOntologyDraftLoadRequest",
        "ExecuteOntologyDraftLoadResponse",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology-drafts/post/responses/201/content/application~1json/schema",
        "OntologyDraftView",
    );
    assert_page_response_ref(
        &spec,
        "/paths/~1api~1ontology-drafts/get/responses/200/content/application~1json/schema",
        "OntologyDraftSummaryPage",
        "OntologyDraftSummary",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology-drafts~1{id}/get/responses/200/content/application~1json/schema",
        "OntologyDraftView",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology-drafts~1{id}~1complete/post/responses/200/content/application~1json/schema",
        "OntologyDraftView",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/GenerateOntologyDraftLoadPlanResponse/properties/plan",
        "LoadPlan",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/CompileOntologyDraftLoadPlanRequest/properties/plan",
        "LoadPlan",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/ExecuteOntologyDraftLoadRequest/properties/plan",
        "LoadPlan",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/ExecuteOntologyDraftLoadResponse/properties/result",
        "LoadResultResponse",
    );
}

#[test]
fn ontology_draft_revision_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "OntologySnapshot",
        "OntologySnapshotSummary",
        "OntologyDiff",
        "NodeDiff",
        "NodeChange",
        "EdgeDiff",
        "EdgeChange",
        "RebaseAnalysis",
        "RebaseConflict",
        "RestoreOntologyDraftRevisionResponse",
        "RebasePreviewResponse",
        "OntologyQualityReport",
        "QualityConfidence",
        "QualityGap",
        "QualityGapRef",
        "QualityGapSeverity",
        "QualityGapCategory",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology-drafts~1{id}~1revisions/get/responses/200/content/application~1json/schema",
        "OntologySnapshotSummary",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology-drafts~1{id}~1revisions~1{rev}/get/responses/200/content/application~1json/schema",
        "OntologySnapshot",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/RestoreOntologyDraftRevisionResponse/properties/project",
        "OntologyDraftView",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology-drafts~1{id}~1revisions~1{rev1}~1diff~1{rev2}/get/responses/200/content/application~1json/schema",
        "OntologyDiff",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology-drafts~1{id}~1diff~1current/get/responses/200/content/application~1json/schema",
        "OntologyDiff",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1ontology-drafts~1{id}~1diff~1canonical/get/responses/200/content/application~1json/schema",
        "OntologyDiff",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/RebasePreviewResponse/properties/analysis",
        "RebaseAnalysis",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/OntologyDraftView",
        "OntologyDraft",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/OntologyDraft/properties/ontology",
        "OntologyIR",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/OntologyDraft/properties/quality_report",
        "OntologyQualityReport",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/OntologySnapshot/properties/ontology",
        "OntologyIR",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/OntologySnapshot/properties/quality_report",
        "OntologyQualityReport",
    );
}

#[test]
fn evaluation_core_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "EvaluationRunStatus",
        "EvaluationRun",
        "EvaluationCase",
        "EvaluationCaseInput",
        "EvaluationExpected",
        "EvaluationActual",
        "EvaluationRetrievedAnchor",
        "EvaluationCaseMetadata",
        "EvaluationMetricMetadata",
        "EvaluationJudgeSource",
        "EvaluationCaptureAxis",
        "EvaluationFingerprint",
        "EvaluationFingerprintInput",
        "ModelCall",
        "ModelPrices",
        "ConfigHash",
        "ModelId",
        "PromptTemplateId",
        "RetrievalProfileId",
        "PipelineStage",
        "StageOutcome",
        "ErrorClassification",
        "SessionOutcome",
        "AttemptOutcome",
        "InferenceSession",
        "InferenceAttempt",
        "RetrievalProfile",
        "TraversalStrategy",
        "RetrievalLimits",
        "CommunityDetectionPolicy",
        "CommunityDetectionPolicyId",
        "VerifiedQueryDef",
        "VerifiedQueryId",
        "ComplexityClass",
        "VerifiedQueryStatus",
        "PromoteVerifiedQueryRequest",
        "TransitionVerifiedQueryStatusRequest",
        "VerifiedQueryListResponse",
        "EvaluationMetric",
        "EvaluationDataset",
        "EvaluationDatasetItem",
        "EvaluationDatasetSummary",
        "RunSummary",
        "RunComparisonReport",
        "EvaluationRunResponse",
        "EvaluationCaseResponse",
        "EvaluationMetricResponse",
        "EvaluationRunPage",
        "EvaluationDatasetSummaryPage",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/components/schemas/EvaluationRunResponse/properties/run",
        "EvaluationRun",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EvaluationCaseResponse/properties/case",
        "EvaluationCase",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EvaluationMetricResponse/properties/metric",
        "EvaluationMetric",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1evaluation~1runs/get/responses/200/content/application~1json/schema",
        "EvaluationRunPage",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EvaluationRunPage/properties/items/items",
        "EvaluationRun",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1evaluation~1runs~1{run_id}~1cases/get/responses/200/content/application~1json/schema/items",
        "EvaluationCase",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1evaluation~1cases~1{case_id}~1metrics/get/responses/200/content/application~1json/schema/items",
        "EvaluationMetric",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EvaluationDatasetResponse/properties/dataset",
        "EvaluationDataset",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/CreateRunFromDatasetResponse/properties/run",
        "EvaluationRun",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/PromoteCaseToDatasetResponse/properties/item",
        "EvaluationDatasetItem",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1evaluation~1datasets/get/responses/200/content/application~1json/schema",
        "EvaluationDatasetSummaryPage",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EvaluationDatasetSummaryPage/properties/items/items",
        "EvaluationDatasetSummary",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1evaluation~1datasets~1{id}~1items/get/responses/200/content/application~1json/schema/items",
        "EvaluationDatasetItem",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1evaluation~1runs~1{run_id}~1summary/get/responses/200/content/application~1json/schema",
        "RunSummary",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1evaluation~1runs~1diff/get/responses/200/content/application~1json/schema",
        "RunComparisonReport",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EvaluationCaseInput/oneOf/0/properties/expected_query_ir",
        "QueryIR",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EvaluationCase/properties/input",
        "EvaluationCaseInput",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EvaluationCase/properties/expected",
        "EvaluationExpected",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EvaluationCase/properties/actual",
        "EvaluationActual",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EvaluationCase/properties/metadata",
        "EvaluationCaseMetadata",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EvaluationMetric/properties/metadata",
        "EvaluationMetricMetadata",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/RecordEvaluationMetricRequest/properties/metadata",
        "EvaluationMetricMetadata",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/EvaluationDatasetItem/properties/input",
        "EvaluationCaseInput",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/UpsertEvaluationCaseRequest/properties/input",
        "EvaluationCaseInput",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/BulkUpsertEvaluationCaseEntry/properties/input",
        "EvaluationCaseInput",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/UpsertEvaluationDatasetItemEntry/properties/input",
        "EvaluationCaseInput",
    );
}

#[test]
fn query_history_and_saved_pattern_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "QueryExecution",
        "QueryExecutionSummary",
        "QueryIR",
        "QueryOp",
        "PatternIR",
        "WidgetHint",
        "WidgetType",
        "ResolvedQueryBindings",
        "SavedPatternResponse",
        "QueryFeedbackValue",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_page_response_ref(
        &spec,
        "/paths/~1api~1query~1history/get/responses/200/content/application~1json/schema",
        "QueryExecutionSummaryPage",
        "QueryExecutionSummary",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1query~1history~1{id}/get/responses/200/content/application~1json/schema",
        "QueryExecution",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/QueryExecution/properties/query_ir",
        "QueryIR",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/QueryExecution/properties/ontology_snapshot",
        "OntologyIR",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/QueryExecution/properties/query_bindings",
        "ResolvedQueryBindings",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/QueryExecution/properties/results",
        "QueryResult",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/QueryExecution/properties/widget",
        "WidgetHint",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/SubmitQueryFeedbackRequest/properties/feedback",
        "QueryFeedbackValue",
    );
    let feedback_schema = schema(&spec, "QueryFeedbackValue");
    let feedback_values = feedback_schema
        .pointer("/enum")
        .and_then(Value::as_array)
        .expect("QueryFeedbackValue must expose feedback enum values");
    assert!(
        feedback_values.iter().any(|value| value == "positive")
            && feedback_values.iter().any(|value| value == "negative"),
        "QueryFeedbackValue must expose stable feedback wire values. Got: {feedback_schema}",
    );
    assert_page_response_ref(
        &spec,
        "/paths/~1api~1query~1pattern~1saved/get/responses/200/content/application~1json/schema",
        "SavedPatternResponsePage",
        "SavedPatternResponse",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/ExecuteFromIrRequest/properties/query_ir",
        "QueryIR",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/CompilePatternRequest/properties/pattern_ir",
        "PatternIR",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/CompilePatternResponse/properties/query_ir",
        "QueryIR",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/ExecuteFromIrResponse/properties/widget_hint",
        "WidgetHint",
    );
}

#[test]
fn quality_platform_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "QualityRule",
        "QualityResult",
        "QualityDashboardEntry",
        "QualityMetricsReport",
        "MetricValue",
        "MetricWindow",
        "ShaclFailureKind",
        "ShaclFailureCount",
        "WorkspaceQualityBaseline",
        "StaleTypeEntry",
        "StaleProposalDecision",
        "StaleConceptProposal",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1quality~1rules/post/responses/201/content/application~1json/schema",
        "QualityRule",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1quality~1rules/get/responses/200/content/application~1json/schema/items",
        "QualityRule",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1quality~1dashboard/get/responses/200/content/application~1json/schema/items",
        "QualityDashboardEntry",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1quality~1rules~1{id}~1results/get/responses/200/content/application~1json/schema/items",
        "QualityResult",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1quality~1rules~1{id}~1execute/post/responses/200/content/application~1json/schema",
        "QualityResult",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1quality~1metrics/get/responses/200/content/application~1json/schema",
        "QualityMetricsReport",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1quality~1shacl-failures/get/responses/200/content/application~1json/schema/items",
        "ShaclFailureCount",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1quality~1baseline/get/responses/200/content/application~1json/schema",
        "WorkspaceQualityBaseline",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1quality~1stale-types/get/responses/200/content/application~1json/schema/items",
        "StaleTypeEntry",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1quality~1stale-proposals/get/responses/200/content/application~1json/schema/items",
        "StaleConceptProposal",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1quality~1stale-proposals~1{id}/patch/responses/200/content/application~1json/schema",
        "StaleConceptProposal",
    );

    let shacl_constraint = schema(&spec, "ShaclConstraint");
    for op in ["or", "and", "xone"] {
        let variant = tagged_variant(&shacl_constraint, "kind", op);
        assert_eq!(
            variant
                .pointer("/properties/branches/items/$ref")
                .and_then(Value::as_str),
            Some("#/components/schemas/ShaclConstraint"),
            "ShaclConstraint {op}.branches must remain a typed recursive edge. Got: {variant}",
        );
    }

    let not = tagged_variant(&shacl_constraint, "kind", "not");
    assert_eq!(
        not.pointer("/properties/inner/$ref")
            .and_then(Value::as_str),
        Some("#/components/schemas/ShaclConstraint"),
        "ShaclConstraint not.inner must remain a typed recursive edge. Got: {not}",
    );

    let qualified = tagged_variant(&shacl_constraint, "kind", "qualified_value_shape");
    assert_eq!(
        qualified
            .pointer("/properties/shape/$ref")
            .and_then(Value::as_str),
        Some("#/components/schemas/ShaclConstraint"),
        "ShaclConstraint qualified_value_shape.shape must remain a typed recursive edge. Got: {qualified}",
    );
}

#[test]
fn workbench_saved_state_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "PinboardItem",
        "WorkbenchPerspective",
        "CanvasPosition",
        "CanvasViewport",
        "UpsertPerspectiveRequest",
        "DeletePerspectiveResponse",
        "LoadCheckpoint",
        "LoadPlan",
        "LoadExecutionResult",
        "LoadExecutionError",
        "GenerateLoadPlanResponse",
        "ExecuteLoadResponse",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1pins/post/responses/201/content/application~1json/schema",
        "PinboardItem",
    );
    assert_page_response_ref(
        &spec,
        "/paths/~1api~1pins/get/responses/200/content/application~1json/schema",
        "PinboardItemPage",
        "PinboardItem",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1perspectives/put/responses/200/content/application~1json/schema",
        "WorkbenchPerspective",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/WorkbenchPerspective/properties/positions/additionalProperties",
        "CanvasPosition",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/WorkbenchPerspective/properties/viewport",
        "CanvasViewport",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/UpsertPerspectiveRequest/properties/positions/additionalProperties",
        "CanvasPosition",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1perspectives~1by-lineage~1{lineage_id}/get/responses/200/content/application~1json/schema/items",
        "WorkbenchPerspective",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1perspectives~1by-lineage~1{lineage_id}~1default/get/responses/200/content/application~1json/schema",
        "WorkbenchPerspective",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1perspectives~1by-lineage~1{lineage_id}~1best/get/responses/200/content/application~1json/schema",
        "WorkbenchPerspective",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1perspectives~1{id}/delete/responses/200/content/application~1json/schema",
        "DeletePerspectiveResponse",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/GenerateLoadPlanResponse/properties/plan",
        "LoadPlan",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/ExecuteLoadResponse/properties/result",
        "LoadExecutionResult",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1load~1checkpoints/get/responses/200/content/application~1json/schema/items",
        "LoadCheckpoint",
    );
}

#[test]
fn knowledge_reports_and_recipes_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "KnowledgeEntry",
        "KnowledgeKind",
        "KnowledgeStatus",
        "SavedReport",
        "SavedReportParameter",
        "SavedReportParameterType",
        "ExecuteReportRequest",
        "QueryResult",
        "QueryMetadata",
        "QueryProvenance",
        "ColumnLineage",
        "AnalysisRecipe",
        "RecipeStatus",
        "RecipeExecutionResult",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1knowledge/post/responses/200/content/application~1json/schema",
        "KnowledgeEntry",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/CreateKnowledgeEntryRequest/properties/kind",
        "KnowledgeKind",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/KnowledgeEntry/properties/kind",
        "KnowledgeKind",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/KnowledgeEntry/properties/status",
        "KnowledgeStatus",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/UpdateKnowledgeStatusRequest/properties/status",
        "KnowledgeStatus",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/BulkReviewApprovalsRequest/properties/status",
        "KnowledgeStatus",
    );
    assert_page_response_ref(
        &spec,
        "/paths/~1api~1knowledge/get/responses/200/content/application~1json/schema",
        "KnowledgeEntryPage",
        "KnowledgeEntry",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1knowledge~1{id}/get/responses/200/content/application~1json/schema",
        "KnowledgeEntry",
    );
    assert_page_response_ref(
        &spec,
        "/paths/~1api~1knowledge~1stale/get/responses/200/content/application~1json/schema",
        "KnowledgeEntryPage",
        "KnowledgeEntry",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1reports/post/responses/200/content/application~1json/schema",
        "SavedReport",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/SavedReportParameter/properties/type",
        "SavedReportParameterType",
    );
    assert_page_response_ref(
        &spec,
        "/paths/~1api~1reports/get/responses/200/content/application~1json/schema",
        "SavedReportPage",
        "SavedReport",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1reports~1{id}/get/responses/200/content/application~1json/schema",
        "SavedReport",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1reports~1{id}~1execute/post/responses/200/content/application~1json/schema",
        "QueryResult",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1reports~1{id}~1execute/post/requestBody/content/application~1json/schema",
        "ExecuteReportRequest",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1recipes/post/responses/200/content/application~1json/schema",
        "AnalysisRecipe",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/AnalysisRecipe/properties/status",
        "RecipeStatus",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/RecipeStatusUpdateRequest/properties/status",
        "RecipeStatus",
    );
    assert_page_response_ref(
        &spec,
        "/paths/~1api~1recipes/get/responses/200/content/application~1json/schema",
        "AnalysisRecipePage",
        "AnalysisRecipe",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1recipes~1{id}~1results/get/responses/200/content/application~1json/schema/items",
        "RecipeExecutionResult",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1recipes~1{id}~1versions/get/responses/200/content/application~1json/schema/items",
        "AnalysisRecipe",
    );
}

#[test]
fn admin_operations_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "AclPolicy",
        "AdapterAnalysisResponse",
        "AdapterAnalysisDriftEntry",
        "AnalysisResult",
        "PromptTemplateRow",
        "CreatePromptRequest",
        "UpdatePromptRequest",
        "NotificationChannel",
        "NotificationChannelType",
        "WebhookNotificationConfig",
        "NotificationLog",
        "ScheduledTask",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1acl~1policies/post/responses/201/content/application~1json/schema",
        "AclPolicy",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1acl~1policies/get/responses/200/content/application~1json/schema/items",
        "AclPolicy",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1acl~1policies~1{id}/get/responses/200/content/application~1json/schema",
        "AclPolicy",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1acl~1effective/get/responses/200/content/application~1json/schema/items",
        "AclPolicy",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1notifications~1channels/post/responses/200/content/application~1json/schema",
        "NotificationChannel",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1notifications~1channels/get/responses/200/content/application~1json/schema/items",
        "NotificationChannel",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/NotificationChannel/properties/channel_type",
        "NotificationChannelType",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/NotificationChannel/properties/config",
        "WebhookNotificationConfig",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/CreateChannelRequest/properties/channel_type",
        "NotificationChannelType",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/CreateChannelRequest/properties/config",
        "WebhookNotificationConfig",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/UpdateChannelRequest/properties/config/oneOf/1",
        "WebhookNotificationConfig",
    );
    assert_eq!(
        spec.pointer("/components/schemas/WebhookNotificationConfig/properties/headers/additionalProperties/type"),
        Some(&Value::from("string")),
        "webhook headers must be a string-to-string map",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1notifications~1log/get/responses/200/content/application~1json/schema/items",
        "NotificationLog",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1recipes~1{id}~1schedule/post/responses/201/content/application~1json/schema",
        "ScheduledTask",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1scheduled-tasks/get/responses/200/content/application~1json/schema/items",
        "ScheduledTask",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1scheduled-tasks~1{id}/get/responses/200/content/application~1json/schema",
        "ScheduledTask",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1admin~1prompts/get/responses/200/content/application~1json/schema/items",
        "PromptTemplateRow",
    );
    for pointer in [
        "/components/schemas/PromptTemplateRow/properties/variables",
        "/components/schemas/CreatePromptRequest/properties/variables",
        "/components/schemas/UpdatePromptRequest/properties/variables",
    ] {
        let variables = spec
            .pointer(pointer)
            .unwrap_or_else(|| panic!("OpenAPI schema pointer {pointer} missing"));
        let stringified = variables.to_string();
        assert!(
            stringified.contains("\"array\"") && stringified.contains("\"type\":\"string\""),
            "{pointer} must be a string array. Got: {stringified}",
        );
    }
    let create_prompt_metadata = spec
        .pointer("/components/schemas/CreatePromptRequest/properties/metadata")
        .expect("CreatePromptRequest.metadata property defined");
    let stringified = create_prompt_metadata.to_string();
    assert!(
        stringified.contains("additionalProperties"),
        "CreatePromptRequest.metadata must be an object map. Got: {stringified}",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1admin~1federation~1adapters~1{source_id}~1analysis/get/responses/200/content/application~1json/schema",
        "AdapterAnalysisResponse",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/AdapterAnalysisResponse/properties/snapshot",
        "AnalysisResult",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/AnalysisResult/properties/schema",
        "SourceSchema",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/AnalysisResult/properties/profile",
        "SourceProfile",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/AnalysisResult/properties/warnings/items",
        "AnalysisWarning",
    );
}

#[test]
fn observability_and_lineage_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "AuditEntry",
        "UsageSummary",
        "LineageSummary",
        "LineageEntry",
        "ProvenanceCapture",
        "ProvenanceDef",
        "ProvenancePlan",
        "EntityRef",
        "ProvenanceActivityKind",
        "AgentRef",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_page_response_ref(
        &spec,
        "/paths/~1api~1audit/get/responses/200/content/application~1json/schema",
        "AuditEntryPage",
        "AuditEntry",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/AuditRecord/properties/provenance",
        "ProvenanceDef",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/AuditRecordPage/properties/items/items",
        "AuditRecord",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1usage/get/responses/200/content/application~1json/schema/items",
        "UsageSummary",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1lineage/get/responses/200/content/application~1json/schema/items",
        "LineageSummary",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1lineage~1label~1{label}/get/responses/200/content/application~1json/schema/items",
        "LineageEntry",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1lineage~1project~1{id}/get/responses/200/content/application~1json/schema/items",
        "LineageEntry",
    );
}

#[test]
fn dashboard_config_and_model_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "Dashboard",
        "DashboardLayoutItem",
        "DashboardWidget",
        "DashboardWidgetPosition",
        "DashboardWidgetThresholds",
        "ThresholdDirection",
        "CreateWidgetRequest",
        "WidgetUpdateRequest",
        "ConfigResponse",
        "ConfigUpdateResponse",
        "ModelConfig",
        "NewModelConfig",
        "ModelConfigUpdate",
        "ModelRoutingRule",
        "NewRoutingRule",
        "RoutingRuleUpdate",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1dashboards/post/responses/200/content/application~1json/schema",
        "Dashboard",
    );
    assert_page_response_ref(
        &spec,
        "/paths/~1api~1dashboards/get/responses/200/content/application~1json/schema",
        "DashboardPage",
        "Dashboard",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1dashboards~1{id}/get/responses/200/content/application~1json/schema",
        "Dashboard",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1dashboards~1{id}~1widgets/post/responses/200/content/application~1json/schema",
        "DashboardWidget",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/Dashboard/properties/layout/items",
        "DashboardLayoutItem",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/DashboardWidget/properties/position",
        "DashboardWidgetPosition",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/DashboardWidget/properties/thresholds",
        "DashboardWidgetThresholds",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/DashboardWidgetThresholds/properties/direction",
        "ThresholdDirection",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/CreateWidgetRequest/properties/position",
        "DashboardWidgetPosition",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/WidgetUpdateRequest/properties/thresholds",
        "DashboardWidgetThresholds",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1dashboards~1{id}~1widgets/get/responses/200/content/application~1json/schema/items",
        "DashboardWidget",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1config/get/responses/200/content/application~1json/schema",
        "ConfigResponse",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1config/patch/responses/200/content/application~1json/schema",
        "ConfigUpdateResponse",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1models~1configs/get/responses/200/content/application~1json/schema/items",
        "ModelConfig",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1models~1configs/post/requestBody/content/application~1json/schema",
        "NewModelConfig",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1models~1configs/post/responses/201/content/application~1json/schema",
        "ModelConfig",
    );
    assert_eq!(
        spec.pointer(
            "/components/schemas/ModelConfig/properties/provider_meta/additionalProperties"
        ),
        Some(&Value::from(true)),
        "ModelConfig.provider_meta must remain an explicit provider extension object",
    );
    assert_eq!(
        spec.pointer("/components/schemas/NewModelConfig/properties/enabled/type"),
        Some(&Value::Array(vec![
            Value::from("boolean"),
            Value::from("null")
        ])),
        "NewModelConfig.enabled must be accepted so create honors the UI state",
    );
    assert_eq!(
        spec.pointer(
            "/components/schemas/NewModelConfig/properties/provider_meta/additionalProperties"
        ),
        Some(&Value::from(true)),
        "NewModelConfig.provider_meta must be accepted as an extension object",
    );
    let model_config_update_required = spec
        .pointer("/components/schemas/ModelConfigUpdate/required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(
        !model_config_update_required
            .iter()
            .any(|value| value.as_str() == Some("provider_meta")),
        "ModelConfigUpdate.provider_meta must stay optional for sparse PATCH"
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1models~1routing-rules/get/responses/200/content/application~1json/schema/items",
        "ModelRoutingRule",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1models~1routing-rules/post/requestBody/content/application~1json/schema",
        "NewRoutingRule",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1models~1routing-rules~1{id}/patch/requestBody/content/application~1json/schema",
        "RoutingRuleUpdate",
    );
}

#[test]
fn session_and_insight_surface_is_typed_not_opaque_objects() {
    let spec = dump_spec();

    for component in [
        "AgentSession",
        "AgentSessionModelConfig",
        "AgentExecutionMode",
        "AgentEvent",
        "AgentEventPayload",
        "SessionMessagesResponse",
        "SessionChatMessage",
        "SessionMessageRole",
        "SessionToolCall",
        "SessionToolCallStatus",
        "SessionUsage",
        "InsightResponse",
        "InsightDef",
        "CreateInsightRequest",
        "UpdateInsightRequest",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    assert_page_response_ref(
        &spec,
        "/paths/~1api~1sessions/get/responses/200/content/application~1json/schema",
        "AgentSessionPage",
        "AgentSession",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1sessions~1{id}/get/responses/200/content/application~1json/schema",
        "AgentSession",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/AgentSession/properties/model_config",
        "AgentSessionModelConfig",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1sessions~1{id}~1events/get/responses/200/content/application~1json/schema/items",
        "AgentEvent",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/AgentEvent/properties/payload",
        "AgentEventPayload",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1sessions~1{id}~1messages/get/responses/200/content/application~1json/schema",
        "SessionMessagesResponse",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/SessionChatMessage/properties/role",
        "SessionMessageRole",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/SessionToolCall/properties/status",
        "SessionToolCallStatus",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1insights/get/responses/200/content/application~1json/schema",
        "CursorPage_InsightDef",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1insights~1{id}/get/responses/200/content/application~1json/schema",
        "InsightResponse",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/InsightDef/properties/query_ir",
        "QueryIR",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/InsightDef/properties/original_provenance",
        "QueryProvenance",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/CreateInsightRequest/properties/query_ir",
        "QueryIR",
    );
    assert_schema_ref(
        &spec,
        "/components/schemas/UpdateInsightRequest/properties/query_ir",
        "QueryIR",
    );
}

#[test]
fn workspace_mutation_responses_are_typed_not_opaque_values() {
    let spec = dump_spec();

    for component in [
        "WorkspaceResponse",
        "WorkspaceMeResponse",
        "UpdateWorkspaceLocaleRequest",
        "DeleteWorkspaceResponse",
        "RemoveMemberResponse",
    ] {
        assert!(
            spec.pointer(&format!("/components/schemas/{component}"))
                .is_some(),
            "{component} must be registered as an OpenAPI schema component",
        );
    }

    for pointer in [
        "/components/schemas/WorkspaceResponse/properties/admin_locale_fallback",
        "/components/schemas/WorkspaceResponse/properties/llm_locale_fallback",
        "/components/schemas/WorkspaceMeResponse/properties/admin_locale_fallback",
        "/components/schemas/WorkspaceMeResponse/properties/llm_locale_fallback",
        "/components/schemas/UpdateWorkspaceLocaleRequest/properties/admin_locale_fallback",
        "/components/schemas/UpdateWorkspaceLocaleRequest/properties/llm_locale_fallback",
    ] {
        assert_eq!(
            spec.pointer(&format!("{pointer}/type")),
            Some(&Value::from("array")),
            "{pointer} must be a string array",
        );
        assert_eq!(
            spec.pointer(&format!("{pointer}/items/type")),
            Some(&Value::from("string")),
            "{pointer} items must be strings",
        );
    }

    assert_schema_ref(
        &spec,
        "/paths/~1api~1workspaces~1{id}/delete/responses/200/content/application~1json/schema",
        "DeleteWorkspaceResponse",
    );
    assert_schema_ref(
        &spec,
        "/paths/~1api~1workspaces~1{id}~1members~1{uid}/delete/responses/200/content/application~1json/schema",
        "RemoveMemberResponse",
    );
}

#[test]
fn community_summary_schema_carries_store_constraints() {
    let spec = dump_spec();
    let summary = schema(&spec, "CommunitySummaryDto");
    let request = schema(&spec, "UpsertCommunitySummaryRequest");

    for schema in [&summary, &request] {
        assert_eq!(
            schema.pointer("/properties/level/maximum"),
            Some(&Value::from(32767)),
            "community summary level must expose the Postgres SMALLINT ceiling",
        );
        assert_eq!(
            schema.pointer("/properties/member_entity_kinds/maxItems"),
            Some(&Value::from(10000)),
            "member_entity_kinds must expose the store member limit",
        );
        assert_eq!(
            schema.pointer("/properties/member_logical_ids/maxItems"),
            Some(&Value::from(10000)),
            "member_logical_ids must expose the store member limit",
        );
        assert_eq!(
            schema.pointer("/properties/community_id/minLength"),
            Some(&Value::from(1)),
            "community_id must expose the non-empty contract",
        );
    }
}

#[test]
fn ontology_transform_paths_match_runtime_singleton_routes() {
    let spec = dump_spec();
    let paths = spec
        .pointer("/paths")
        .and_then(Value::as_object)
        .expect("OpenAPI paths object");

    for path in [
        "/api/ontology/normalize",
        "/api/ontology/export",
        "/api/ontology/export/cypher",
        "/api/ontology/export/mermaid",
        "/api/ontology/export/graphql",
        "/api/ontology/export/owl",
        "/api/ontology/export/shacl",
        "/api/ontology/export/typescript",
        "/api/ontology/export/python",
        "/api/ontology/import/owl",
        "/api/ontology/suggestions",
        "/api/ontology/type-candidates",
        "/api/ontology/versions",
        "/api/ontology/edits",
        "/api/ontology/map-summary",
        "/api/ontology/axis-items",
        "/api/ontology/cross-refs",
        "/api/ontology/reindex",
        "/api/ontology/audit",
        "/api/ontology/adopt-graph",
        "/api/ontology/enrich",
        "/api/ambiguities",
        "/api/ambiguities/bulk-revoke",
        "/api/ambiguities/{id}",
        "/api/ambiguities/{id}/resolve",
        "/api/ambiguities/{id}/revoke",
        "/api/evaluation/runs",
        "/api/evaluation/runs/{id}",
        "/api/evaluation/runs/{id}/complete",
        "/api/evaluation/runs/{run_id}/cases",
        "/api/evaluation/runs/{run_id}/cases/bulk",
        "/api/evaluation/runs/{run_id}/cases/{case_key}/execute",
        "/api/evaluation/cases/{case_id}/metrics",
        "/api/evaluation/cases/{case_id}/judge",
        "/api/evaluation/cases/{case_id}/judge_safety",
        "/api/evaluation/cases/{case_id}/promote-to-dataset",
        "/api/evaluation/datasets",
        "/api/evaluation/datasets/{id}",
        "/api/evaluation/datasets/{id}/items",
        "/api/evaluation/runs/from-dataset",
        "/api/evaluation/runs/{run_id}/summary",
        "/api/evaluation/runs/diff",
        "/api/search",
        "/api/search/expand",
        "/api/graph/overview",
        "/api/query/history/{id}/feedback",
        "/api/load/checkpoints",
        "/api/load/checkpoints/{id}",
        "/api/sources/test-connection",
    ] {
        assert!(
            paths.contains_key(path),
            "OpenAPI must document the runtime singleton ontology route {path}",
        );
    }

    let legacy_paths: Vec<_> = paths
        .keys()
        .filter(|path| path.starts_with("/api/ontologies"))
        .collect();
    assert!(
        legacy_paths.is_empty(),
        "OpenAPI must not expose legacy plural ontology routes: {legacy_paths:?}",
    );
}

#[test]
fn openapi_paths_match_axum_router_paths() {
    let spec = dump_spec();
    let openapi_paths = spec
        .pointer("/paths")
        .and_then(Value::as_object)
        .expect("OpenAPI paths object");
    let runtime_paths = router_paths_from_source();

    let mut missing = Vec::new();
    for path in &runtime_paths {
        if !openapi_paths.contains_key(path) {
            missing.push(path.clone());
        }
    }

    let runtime_set: std::collections::BTreeSet<_> = runtime_paths.iter().cloned().collect();
    let extra: Vec<_> = openapi_paths
        .keys()
        .filter(|path| !runtime_set.contains(*path))
        .cloned()
        .collect();

    assert!(
        missing.is_empty(),
        "OpenAPI is missing runtime Axum routes: {missing:?}",
    );
    assert!(
        extra.is_empty(),
        "OpenAPI exposes paths not present in the Axum router: {extra:?}",
    );
}

#[test]
fn generated_types_do_not_expose_empty_record_contracts() {
    let generated = include_str!("../../../../web/src/types/api.generated.ts");
    let offenders: Vec<_> = generated
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            if !line.contains("Record<string, never>") {
                return None;
            }
            let allowed_empty_section = line
                .contains("export type webhooks = Record<string, never>")
                || line.contains("export type $defs = Record<string, never>");
            (!allowed_empty_section).then_some(format!("{}: {}", idx + 1, line.trim()))
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "Generated frontend API types must not expose API fields as \
         Record<string, never>. Use a concrete schema for domain DTOs, \
         or an explicit free-form object/map for dynamic payloads. \
         Offenders: {offenders:#?}",
    );
}

#[test]
fn api_sources_do_not_document_domain_surfaces_as_opaque_objects() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rs_files(&manifest_dir.join("src/routes"), &mut files);
    files.push(manifest_dir.join("src/openapi.rs"));
    files.push(manifest_dir.join("src/collaboration.rs"));

    let forbidden = [
        "body = Object",
        "body = Vec<Object>",
        "request_body = Object",
        "inline(serde_json::Value)",
        "ApiResponse<serde_json::Value>",
        "Json<serde_json::Value>",
    ];
    let mut offenders = Vec::new();

    for file in files {
        let content = std::fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
        for (idx, line) in content.lines().enumerate() {
            if forbidden.iter().any(|pattern| line.contains(pattern)) {
                let rel = file.strip_prefix(manifest_dir).unwrap_or(&file);
                offenders.push(format!("{}:{}: {}", rel.display(), idx + 1, line.trim()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "API source must not expose domain routes as anonymous Object \
         schemas or raw serde_json::Value responses. Use a concrete DTO, \
         or an explicit free-form map on the exact dynamic field. \
         Offenders: {offenders:#?}",
    );
}
