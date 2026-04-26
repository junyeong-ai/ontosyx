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
/// the LLM should prefer over inventing parallel labels. Empty
/// glossary → empty string.
pub fn render_glossary_section(glossary: &[GlossaryTermDef]) -> String {
    if glossary.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "## Available Domain Terms\n\
         The workspace already defines these business terms. Prefer their canonical \
         labels (or the closest match) over inventing new node / edge / property \
         names for the same concept.\n\n",
    );
    for term in glossary {
        let aliases = if term.aliases.is_empty() {
            String::new()
        } else {
            format!(" (aliases: {})", term.aliases.join(", "))
        };
        let desc = term.description.default.clone();
        let desc_part = if desc.is_empty() {
            String::new()
        } else {
            format!(" — {desc}")
        };
        out.push_str(&format!(
            "- `{}`{}{}\n",
            term.term, aliases, desc_part,
        ));
    }
    out
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
            term: name.to_string(),
            display_name: LocalizedText::default(),
            aliases: aliases.iter().map(|s| s.to_uppercase()).collect(),
            description: LocalizedText::new(desc),
            category: None,
            parent_term_id: None,
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
