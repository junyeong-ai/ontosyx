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
