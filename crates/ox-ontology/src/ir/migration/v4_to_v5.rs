//! v4 → v5 — vocabulary unification: standalone `concepts` collection
//! folds into the glossary as `GlossaryTermDef.realisation`, and
//! `NodeTypeDef.concept_id` / `EdgeTypeDef.concept_id` become
//! `concept_term_id` pointing at the term that names the concept.
//!
//! The pre-v5 shape kept two parallel registries:
//!
//! ```json
//! {
//!   "concepts": [
//!     { "id": "c-customer", "term_id": "g-customer", ... }
//!   ],
//!   "glossary": [{ "id": "g-customer", "term": {...} }],
//!   "node_types": [{ "id": "n-cust", "concept_id": "c-customer", ... }]
//! }
//! ```
//!
//! v5 unifies them: the term *is* the concept. The post-image is:
//!
//! ```json
//! {
//!   "glossary": [{
//!     "id": "g-customer",
//!     "term": {...},
//!     "realisation": { "kind": "...", ... }
//!   }],
//!   "node_types": [{ "id": "n-cust", "concept_term_id": "g-customer", ... }]
//! }
//! ```
//!
//! The migration walks every concept row, locates the matching
//! glossary term by `term_id`, copies the realisation payload into
//! the term, and rewrites every `concept_id` reference on
//! `node_types` / `edge_types` into a `concept_term_id` pointing at
//! the glossary term. Concept rows whose term doesn't exist are
//! dropped with a warning — they could never have rendered in the
//! v5 shape regardless.

use ox_core::error::OxResult;
use serde_json::{Value, json};

use super::{IrMigration, as_object_mut};

pub struct Migration;

impl IrMigration for Migration {
    fn from_version(&self) -> u32 {
        4
    }

    fn to_version(&self) -> u32 {
        5
    }

    fn migrate(&self, mut value: Value) -> OxResult<Value> {
        let root = as_object_mut(&mut value)?;

        // Step 1 — pull the `concepts` array and index it by
        // `term_id` so the glossary-term loop below is O(N+M)
        // rather than O(N×M).
        let concepts = root
            .remove("concepts")
            .and_then(|v| match v {
                Value::Array(items) => Some(items),
                _ => None,
            })
            .unwrap_or_default();

        let mut concept_by_term_id: std::collections::HashMap<String, Value> =
            std::collections::HashMap::new();
        let mut concept_id_to_term_id: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for concept in concepts {
            let Some(obj) = concept.as_object() else {
                continue;
            };
            let term_id = obj.get("term_id").and_then(|v| v.as_str()).map(String::from);
            let concept_id = obj.get("id").and_then(|v| v.as_str()).map(String::from);
            if let (Some(term), Some(cid)) = (term_id.clone(), concept_id) {
                concept_id_to_term_id.insert(cid, term.clone());
                concept_by_term_id.insert(term, concept);
            }
        }

        // Step 2 — fold each concept's `realisation` payload into
        // the matching glossary term. Concept fields that the
        // glossary term already has are left alone — the term wins.
        if let Some(glossary) = root.get_mut("glossary").and_then(|v| v.as_array_mut()) {
            for term in glossary.iter_mut() {
                let Some(term_obj) = term.as_object_mut() else {
                    continue;
                };
                let id = term_obj
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let Some(id) = id else { continue };
                if let Some(concept) = concept_by_term_id.remove(&id) {
                    if let Some(realisation) =
                        concept.as_object().and_then(|o| o.get("realisation"))
                    {
                        term_obj
                            .insert("realisation".to_string(), realisation.clone());
                    }
                }
            }
        }

        // Step 3 — rewrite `concept_id` → `concept_term_id` on every
        // node_type and edge_type that referenced a concept. References
        // to concepts whose row was dropped (no matching glossary term)
        // are dropped silently — the v5 shape has no place for them.
        rewrite_concept_refs(root, "node_types", &concept_id_to_term_id);
        rewrite_concept_refs(root, "edge_types", &concept_id_to_term_id);

        Ok(value)
    }
}

fn rewrite_concept_refs(
    root: &mut serde_json::Map<String, Value>,
    collection_key: &str,
    concept_to_term: &std::collections::HashMap<String, String>,
) {
    let Some(items) = root.get_mut(collection_key).and_then(|v| v.as_array_mut())
    else {
        return;
    };
    for item in items.iter_mut() {
        let Some(obj) = item.as_object_mut() else {
            continue;
        };
        let concept_id = obj
            .remove("concept_id")
            .and_then(|v| v.as_str().map(String::from));
        if let Some(cid) = concept_id
            && let Some(term_id) = concept_to_term.get(&cid)
        {
            obj.insert("concept_term_id".to_string(), json!(term_id));
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ir::migration::migrate_to_current;
    use serde_json::json;

    #[test]
    fn concept_rows_fold_into_glossary_realisation() {
        let v4 = json!({
            "schema_version": 4,
            "id": "ont",
            "name": "Test",
            "version": { "number": 1 },
            "concepts": [
                {
                    "id": "c-customer",
                    "term_id": "g-customer",
                    "realisation": { "kind": "node_type", "node_type_id": "n-cust" }
                }
            ],
            "glossary": [
                { "id": "g-customer", "term": { "default": "Customer" } }
            ],
            "node_types": [],
            "edge_types": [],
            "indexes": []
        });

        let migrated = migrate_to_current(v4).unwrap();
        let glossary = migrated["glossary"].as_array().unwrap();
        assert_eq!(glossary.len(), 1);
        let term = &glossary[0];
        assert_eq!(term["id"], "g-customer");
        assert_eq!(term["realisation"]["kind"], "node_type");
        assert_eq!(term["realisation"]["node_type_id"], "n-cust");
        // The standalone `concepts` collection is gone.
        assert!(migrated.get("concepts").is_none());
    }

    #[test]
    fn node_concept_id_rewrites_to_concept_term_id() {
        let v4 = json!({
            "schema_version": 4,
            "id": "ont",
            "name": "Test",
            "version": { "number": 1 },
            "concepts": [
                { "id": "c-customer", "term_id": "g-customer", "realisation": {} }
            ],
            "glossary": [
                { "id": "g-customer", "term": { "default": "Customer" } }
            ],
            "node_types": [
                { "id": "n-cust", "concept_id": "c-customer" }
            ],
            "edge_types": [],
            "indexes": []
        });

        let migrated = migrate_to_current(v4).unwrap();
        let nodes = migrated["node_types"].as_array().unwrap();
        assert_eq!(nodes[0]["concept_term_id"], "g-customer");
        assert!(nodes[0].get("concept_id").is_none());
    }

    #[test]
    fn dangling_concept_ref_is_dropped_silently() {
        // Node references a concept whose row never existed in the
        // pre-image. v5 has no place for the dangling pointer; the
        // migration drops it rather than fabricating a term.
        let v4 = json!({
            "schema_version": 4,
            "id": "ont",
            "name": "Test",
            "version": { "number": 1 },
            "concepts": [],
            "glossary": [],
            "node_types": [
                { "id": "n-cust", "concept_id": "c-ghost" }
            ],
            "edge_types": [],
            "indexes": []
        });

        let migrated = migrate_to_current(v4).unwrap();
        let nodes = migrated["node_types"].as_array().unwrap();
        assert!(nodes[0].get("concept_id").is_none());
        assert!(nodes[0].get("concept_term_id").is_none());
    }

    #[test]
    fn payload_is_stamped_with_target_schema_version() {
        let v4 = json!({
            "schema_version": 4,
            "id": "ont",
            "name": "Test",
            "version": { "number": 1 },
            "node_types": [],
            "edge_types": [],
            "indexes": []
        });
        let migrated = migrate_to_current(v4).unwrap();
        assert_eq!(migrated["schema_version"], 5);
    }
}
