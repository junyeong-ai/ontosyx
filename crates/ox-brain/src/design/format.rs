//! Prompt-section formatters for [`super::DesignOntologyInput`].
//!
//! Each function renders one slice into the markdown block the
//! design_ontology prompt template expects. **Empty slice → empty
//! string** so the prompt template collapses without conditional
//! template syntax — the rendered prompt has no leftover headers
//! that would confuse the model.
//!
//! Section formats are intentionally compact: the LLM operates under
//! a token budget, so each entry is one line of "id — short
//! description" rather than a full record dump. Callers that need
//! the full record can fetch it through the IR accessors.

use ox_ontology::OntologyIR;
use ox_ontology::ambiguity::AmbiguityContext;
use ox_ontology::code_system::CodeSystemDef;
use ox_ontology::glossary::GlossaryTermDef;

/// Render the workspace's glossary as a domain-vocabulary section
/// the LLM should prefer over inventing parallel labels.
///
/// Multi-locale aware: every translation of a term's preferred label
/// and aliases is included so a Korean alias (`고객`) surfaces
/// alongside its English canonical (`Customer`). The LLM uses the
/// alias surface to recognise user phrasing in any locale and route
/// it to the correct term identity. Empty glossary → empty string.
///
/// Deprecated terms render with a `[deprecated]` marker plus an
/// arrow to the successor's preferred label, so the LLM avoids
/// proposing the old label in fresh designs while still recognising
/// it on input.
pub fn render_glossary_section(glossary: &[GlossaryTermDef]) -> String {
    if glossary.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "## Available Domain Terms\n\
         The workspace already defines these business terms. Prefer their canonical \
         labels (or the closest match) over inventing new node / edge / property \
         names for the same concept. Aliases below are recognised inputs for the \
         same term — match against them when the operator phrases their query \
         differently.\n\n",
    );
    for term in glossary {
        let aliases = collect_alias_surface(term);
        let alias_part = if aliases.is_empty() {
            String::new()
        } else {
            format!(" (aliases: {})", aliases.join(", "))
        };
        let desc = term.description.default.clone();
        let desc_part = if desc.is_empty() {
            String::new()
        } else {
            format!(" — {desc}")
        };
        let lifecycle_part = match &term.lifecycle {
            ox_ontology::glossary::TermLifecycle::Active => String::new(),
            ox_ontology::glossary::TermLifecycle::Deprecated { replaced_by, .. } => {
                let successor = replaced_by
                    .as_ref()
                    .and_then(|id| glossary.iter().find(|t| &t.id == id))
                    .map(|t| t.term.default.as_str())
                    .unwrap_or("");
                if successor.is_empty() {
                    " [deprecated]".to_string()
                } else {
                    format!(" [deprecated → `{successor}`]")
                }
            }
            ox_ontology::glossary::TermLifecycle::Retired { .. } => " [retired]".to_string(),
        };
        out.push_str(&format!(
            "- `{}`{}{}{}\n",
            term.term.default, alias_part, lifecycle_part, desc_part,
        ));
    }
    out
}

/// Collect every locale variant of `term.term`, `display_name`, and
/// `aliases` into a deduplicated, stable-ordered list — minus the
/// canonical `term.default` itself (which is rendered separately).
/// The locale variants drive cross-language matching during
/// NL-to-Cypher routing.
fn collect_alias_surface(term: &GlossaryTermDef) -> Vec<String> {
    let canonical = term.term.default.trim().to_string();
    let mut surface: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed == canonical {
            return;
        }
        if !surface.iter().any(|existing| existing == trimmed) {
            surface.push(trimmed.to_string());
        }
    };
    for variant in std::iter::once(&term.term.default)
        .chain(term.term.translations.values())
    {
        push(variant);
    }
    for variant in std::iter::once(&term.display_name.default)
        .chain(term.display_name.translations.values())
    {
        push(variant);
    }
    for alias in &term.aliases {
        for variant in std::iter::once(&alias.default).chain(alias.translations.values()) {
            push(variant);
        }
    }
    surface
}

/// Render the workspace's code-system registry as a reference list.
/// LLMs should refer to these by id in property descriptions instead
/// of redefining codes inline. Empty registry → empty string.
pub fn render_code_systems_section(code_systems: &[CodeSystemDef]) -> String {
    if code_systems.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "## Existing Code Systems\n\
         These code systems are already registered. When a property carries values \
         drawn from one of them, mention the system id in the property description \
         (e.g., \"ISO-3166 country codes — see code_system `iso-3166`\") rather \
         than enumerating values inline.\n\n",
    );
    for cs in code_systems {
        let kind = format!("{:?}", cs.kind).to_lowercase();
        out.push_str(&format!(
            "- `{}` ({}) — {} codes\n",
            cs.id,
            kind,
            cs.codes.len(),
        ));
    }
    out
}

/// Render pre-detected ambiguities the planner has not yet resolved.
/// Each entry tells the LLM "this column has multiple plausible
/// readings — surface the choice in the property description".
pub fn render_ambiguity_section(ambiguities: &[AmbiguityContext]) -> String {
    if ambiguities.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "## Pre-detected Ambiguities\n\
         The introspection pipeline flagged these columns as ambiguous. When \
         designing properties for them, either pick a canonical interpretation \
         (and note it in the description) or recommend a code-system / glossary \
         binding in the description so the operator can resolve later.\n\n",
    );
    for amb in ambiguities {
        let kind = format!("{:?}", amb.kind);
        let sample = if amb.sample_values.is_empty() {
            String::new()
        } else {
            format!(
                " (sample: {})",
                amb.sample_values
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        };
        out.push_str(&format!(
            "- {}.{} — {}{}\n",
            amb.column.relation, amb.column.column, kind, sample,
        ));
    }
    out
}

/// Render an extension-mode summary of the existing ontology so the
/// LLM emits only the new node / edge types not already covered.
/// Compact — node + edge label lists only. The full IR is too heavy
/// to inline on every design call.
pub fn render_existing_ontology_section(existing: Option<&OntologyIR>) -> String {
    let Some(ir) = existing else {
        return String::new();
    };
    let mut out = String::from(
        "## Existing Ontology (extension mode)\n\
         The workspace already has these node / edge types. Do not redefine \
         them — emit only the *new* types and edges your batch covers, plus \
         any edges that connect new nodes to the existing graph.\n\n",
    );
    let nodes: Vec<String> = ir
        .node_types()
        .iter()
        .map(|n| format!("`{}`", n.label))
        .collect();
    if !nodes.is_empty() {
        out.push_str(&format!("Existing node labels: {}\n", nodes.join(", ")));
    }
    let edges: Vec<String> = ir
        .edge_types()
        .iter()
        .map(|e| format!("`{}`", e.label))
        .collect();
    if !edges.is_empty() {
        out.push_str(&format!("Existing edge labels: {}\n", edges.join(", ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::i18n::LocalizedText;
    use ox_ontology::glossary::GlossaryTermDef;

    fn term(name: &str, aliases: &[&str], desc: &str) -> GlossaryTermDef {
        GlossaryTermDef {
            id: format!("gt-{name}").into(),
            term: LocalizedText::new(name),
            display_name: LocalizedText::default(),
            examples: Vec::new(),
            aliases: aliases
                .iter()
                .map(|s| LocalizedText::new(s.to_uppercase()))
                .collect(),
            description: LocalizedText::new(desc),
            category: None,
            related_terms: Vec::new(),
            governance: ox_ontology::glossary::TermGovernance::default(),
            valid_from: None,
            valid_to: None,
            lifecycle: ox_ontology::glossary::TermLifecycle::default(),
        concept_id: None,
        }
    }

    #[test]
    fn empty_glossary_renders_empty_string() {
        assert!(render_glossary_section(&[]).is_empty());
    }

    #[test]
    fn glossary_section_lists_each_term_with_aliases_and_description() {
        let g = vec![
            term("customer", &["client"], "End user of the platform"),
            term("order", &[], "Purchase event"),
        ];
        let s = render_glossary_section(&g);
        assert!(s.contains("Available Domain Terms"));
        assert!(s.contains("`customer`"));
        assert!(s.contains("CLIENT"));
        assert!(s.contains("End user of the platform"));
        assert!(s.contains("`order`"));
        assert!(s.contains("Purchase event"));
    }

    #[test]
    fn glossary_section_surfaces_locale_translations_as_aliases() {
        use ox_core::i18n::LanguageTag;

        let mut t = term("customer", &["buyer"], "End user");
        // Add Korean translations so the LLM sees them in the alias surface.
        t.term = LocalizedText::new("customer")
            .with_translation(LanguageTag::ko(), "고객");
        t.aliases = vec![
            LocalizedText::new("BUYER").with_translation(LanguageTag::ko(), "구매자"),
        ];
        let s = render_glossary_section(std::slice::from_ref(&t));
        assert!(s.contains("`customer`"));
        assert!(s.contains("고객"));
        assert!(s.contains("BUYER"));
        assert!(s.contains("구매자"));
    }

    #[test]
    fn glossary_section_marks_deprecated_term_with_successor() {
        use chrono::Utc;
        use ox_ontology::glossary::TermLifecycle;

        let mut old = term("client", &[], "Legacy synonym");
        old.lifecycle = TermLifecycle::Deprecated {
            replaced_by: Some(ox_ontology::glossary::GlossaryTermId::new("gt-customer")),
            deprecated_at: Utc::now(),
        };
        let mut successor = term("customer", &[], "Active term");
        successor.id = ox_ontology::glossary::GlossaryTermId::new("gt-customer");
        let s = render_glossary_section(&[successor, old]);
        assert!(
            s.contains("[deprecated → `customer`]"),
            "expected deprecation marker pointing at successor: {s}"
        );
    }

    #[test]
    fn glossary_section_omits_canonical_from_alias_surface() {
        // The canonical term's `default` value renders separately as
        // the row header — listing it again under "aliases" would be
        // visually redundant and waste tokens.
        let t = term("customer", &[], "");
        let s = render_glossary_section(std::slice::from_ref(&t));
        assert!(s.contains("`customer`"));
        assert!(!s.contains("(aliases:"));
    }

    #[test]
    fn empty_code_systems_renders_empty_string() {
        assert!(render_code_systems_section(&[]).is_empty());
    }

    #[test]
    fn empty_ambiguity_renders_empty_string() {
        assert!(render_ambiguity_section(&[]).is_empty());
    }

    #[test]
    fn no_existing_ontology_renders_empty_string() {
        assert!(render_existing_ontology_section(None).is_empty());
    }
}
