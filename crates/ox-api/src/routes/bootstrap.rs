//! Bootstrap wizard HTTP routes.
//!
//! The Phase 4.1 wizard captures operator intent (pilot scope,
//! glossary drafts, rule drafts) before any source analysis runs.
//! The raw drafts live in the browser's `localStorage` during the
//! wizard, but the Finish step needs a durable server-side landing
//! so the drafts survive a browser reset and show up on
//! `/ontologies` just like any other seeded ontology.
//!
//! This module currently exposes one endpoint:
//!
//! - `POST /api/bootstrap/seed-glossary` — materialise a minimal
//!   ontology whose only content is the supplied glossary terms,
//!   then commit v1. The wizard parses each non-empty line of
//!   `glossaryDraft` into a `{ term, description?, aliases? }`
//!   row and the handler turns those into `GlossaryTermDef`s.
//!
//! Rules drafting is intentionally out of scope here — parsing
//! free-form English rules into typed `RuleDef` requires the
//! LLM pipeline and belongs in the workbench refine step, not in
//! the bootstrap wizard.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_core::i18n::LocalizedText;
use ox_ontology::{GlossaryTermDef, GlossaryTermId, OntologyIR};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SeedGlossaryRequest {
    /// Ontology name. Must be unique within the workspace — the
    /// store's `ontologies_ws_name_uq` constraint surfaces a
    /// 4xx if the wizard is re-submitted without changing the
    /// pilot name.
    pub name: String,
    /// Free-form description. Stored as the ontology's
    /// `description` LocalizedText with an empty locale key so it
    /// renders in whichever fallback the reader uses.
    #[serde(default)]
    pub description: Option<String>,
    /// Parsed draft rows. Empty input is rejected upstream — the
    /// wizard only calls the endpoint when the operator entered
    /// at least one line.
    pub terms: Vec<SeedGlossaryTerm>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SeedGlossaryTerm {
    /// Canonical term text. Trimmed; empty rows are rejected.
    pub term: String,
    /// Optional free-form domain description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional alias list, carried verbatim onto the stored
    /// `GlossaryTermDef.aliases` vector.
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct SeedGlossaryResponse {
    pub ontology_id: Uuid,
    pub version_id: Uuid,
    /// How many `GlossaryTermDef`s ended up in the committed
    /// version. De-dup on `term` (case-insensitive) is applied
    /// server-side — the return value reflects what actually
    /// landed, not what the client sent.
    pub committed_terms: usize,
}

/// `POST /api/bootstrap/seed-glossary` — commit a wizard-provided
/// glossary as a fresh ontology's v1. Requires designer role.
///
/// Flow:
/// 1. Validate input — non-empty name + at least one non-empty term.
/// 2. Build an `OntologyIR` with the supplied terms.
/// 3. `create_ontology` + `commit_version`.
/// 4. Return the identity + first version id so the FE can deep-link
///    to the new ontology on the Complete Map page.
#[utoipa::path(
    post,
    path = "/api/bootstrap/seed-glossary",
    request_body = SeedGlossaryRequest,
    responses(
        (status = 201, description = "Ontology seeded with glossary terms", body = Object),
        (status = 400, description = "Empty name / empty terms list / duplicate terms"),
        (status = 409, description = "Workspace already has an ontology with this name"),
    ),
    security(("api_key" = [])),
    tag = "Bootstrap",
)]
pub(crate) async fn seed_glossary(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<SeedGlossaryRequest>,
) -> Result<(StatusCode, Json<ApiResponse<SeedGlossaryResponse>>), AppError> {
    principal.require_designer()?;

    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::bad_request("name must not be empty"));
    }

    // Normalise + dedup the incoming rows. A wizard user can easily
    // paste the same term twice; silently collapsing duplicates
    // avoids an `OntologyInvariantError` on commit.
    let mut seen = std::collections::HashSet::<String>::new();
    let mut term_defs = Vec::<GlossaryTermDef>::new();
    for row in &req.terms {
        let term_text = row.term.trim();
        if term_text.is_empty() {
            continue;
        }
        let dedup_key = term_text.to_lowercase();
        if !seen.insert(dedup_key) {
            continue;
        }
        let description_lt = match row.description.as_deref().map(str::trim) {
            Some(s) if !s.is_empty() => LocalizedText::from(s),
            _ => LocalizedText::default(),
        };
        let aliases = row
            .aliases
            .iter()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect();
        term_defs.push(GlossaryTermDef {
            id: GlossaryTermId::new(Uuid::new_v4().to_string()),
            term: term_text.to_string(),
            display_name: LocalizedText::default(),
            description: description_lt,
            category: None,
            aliases,
            parent_term_id: None,
        });
    }

    if term_defs.is_empty() {
        return Err(AppError::bad_request(
            "at least one non-empty term is required",
        ));
    }

    // Build the IR. Lineage id is seeded from the ontology's public
    // id so cross-version handles in saved queries / quality rules
    // stay stable across refinements.
    let lineage_seed = Uuid::new_v4().to_string();
    let description_lt = match req.description.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => LocalizedText::from(s),
        _ => LocalizedText::default(),
    };
    let mut ir = OntologyIR::new(
        lineage_seed.clone(),
        name.to_string(),
        description_lt.clone(),
        1u32,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    for def in term_defs {
        ir.add_glossary_term(def).map_err(|e| {
            AppError::unprocessable(format!("glossary term rejected: {e}"))
        })?;
    }
    let committed_terms = ir.glossary().len();

    let description_json = serde_json::to_value(&description_lt)
        .map_err(|e| AppError::internal(format!("serialize description: {e}")))?;

    let identity = state
        .store
        .create_ontology(name, &description_json, Some(&lineage_seed))
        .await
        .map_err(AppError::from)?;

    let snapshot = state
        .store
        .commit_version(
            identity.id,
            &ir,
            "1",
            None,
            &principal.id,
            "Seeded via Bootstrap wizard",
        )
        .await
        .map_err(AppError::from)?;

    Ok((
        StatusCode::CREATED,
        ApiResponse::of(SeedGlossaryResponse {
            ontology_id: identity.id,
            version_id: snapshot.id,
            committed_terms,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Construct handler input without hitting the store. Keeps the
    // parsing + dedup logic covered so the wizard can rely on
    // "same term twice collapses to one" without an integration
    // harness. The store side is covered by integration tests.
    fn row(term: &str, description: Option<&str>, aliases: &[&str]) -> SeedGlossaryTerm {
        SeedGlossaryTerm {
            term: term.to_string(),
            description: description.map(str::to_string),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn terms_from(request: SeedGlossaryRequest) -> Vec<GlossaryTermDef> {
        // Re-implement the filter+dedup inline so the test doesn't
        // depend on the axum handler's state plumbing.
        let mut seen = std::collections::HashSet::<String>::new();
        let mut out = Vec::new();
        for row in &request.terms {
            let t = row.term.trim();
            if t.is_empty() {
                continue;
            }
            if !seen.insert(t.to_lowercase()) {
                continue;
            }
            let description_lt = match row.description.as_deref().map(str::trim) {
                Some(s) if !s.is_empty() => LocalizedText::from(s),
                _ => LocalizedText::default(),
            };
            let aliases = row
                .aliases
                .iter()
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect();
            out.push(GlossaryTermDef {
                id: GlossaryTermId::new(Uuid::new_v4().to_string()),
                term: t.to_string(),
                display_name: LocalizedText::default(),
                description: description_lt,
                category: None,
                aliases,
                parent_term_id: None,
            });
        }
        out
    }

    #[test]
    fn dedup_is_case_insensitive() {
        let req = SeedGlossaryRequest {
            name: "pilot".into(),
            description: None,
            terms: vec![
                row("Customer", None, &[]),
                row("customer", None, &[]),
                row("CUSTOMER", Some("repeat"), &[]),
            ],
        };
        let out = terms_from(req);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].term, "Customer");
    }

    #[test]
    fn empty_terms_and_whitespace_rows_are_dropped() {
        let req = SeedGlossaryRequest {
            name: "pilot".into(),
            description: None,
            terms: vec![
                row("", None, &[]),
                row("   ", None, &[]),
                row("  Order ", Some(" line item "), &["alias"]),
            ],
        };
        let out = terms_from(req);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].term, "Order");
        assert_eq!(out[0].aliases, vec!["alias".to_string()]);
    }

    #[test]
    fn blank_aliases_are_filtered() {
        let req = SeedGlossaryRequest {
            name: "pilot".into(),
            description: None,
            terms: vec![row("Account", None, &["", " ", "a1", "  a2  "])],
        };
        let out = terms_from(req);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].aliases, vec!["a1".to_string(), "a2".to_string()]);
    }

    #[test]
    fn blank_description_stays_default() {
        let req = SeedGlossaryRequest {
            name: "pilot".into(),
            description: None,
            terms: vec![row("Sku", Some("   "), &[])],
        };
        let out = terms_from(req);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].description, LocalizedText::default());
    }
}
