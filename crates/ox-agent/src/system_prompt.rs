//! Agent system prompt assembly.
//!
//! The base prompt is loaded from the `prompt_templates` table
//! (workspace-scoped lookup, falling back to the global template).
//! Role + ontology + repo-insight + knowledge-base context appended
//! deterministically so the assembled string is stable for caching
//! and audit-hash purposes.
//!
//! `prompts/agent_system.toml` seeds the table on first boot; after
//! seeding the DB is authoritative — admins edit through
//! `/api/admin/prompts`, not by editing TOML.

use crate::context::DomainContext;
use crate::tools;

/// Build the agent's system prompt.
///
/// Loads the base prompt from DB (`prompt_templates`,
/// `name="agent_system"`, workspace-scoped lookup with global
/// fallback) and appends role + ontology + repo-insight + learned
/// knowledge context. The output is deterministic for fixed inputs,
/// so the chat handler can hash it for replay-audit purposes — an
/// admin editing the template in the DB invalidates prior sessions
/// because the hash changes.
pub async fn build_system_prompt(domain: &DomainContext, user_role: &str) -> String {
    // Workspace-scoped lookup: prefer this workspace's override, fall
    // back to the global template. A workspace admin can override the
    // base agent_system prompt without affecting other tenants.
    let lookup = domain
        .store
        .find_active_prompt_for_workspace("agent_system", Some(domain.workspace_id))
        .await;
    let base = match lookup {
        Ok(Some(row)) => row.content,
        Ok(None) => {
            tracing::error!("agent_system prompt missing from DB — using minimal fallback");
            "You are Ontosyx, a knowledge graph assistant.".to_string()
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to load agent_system prompt — using minimal fallback");
            "You are Ontosyx, a knowledge graph assistant.".to_string()
        }
    };

    let mut prompt = base;

    match user_role {
        "viewer" => {
            prompt.push_str(
                "\nThe current user has **viewer** role. \
                 You can query and explain data, but cannot modify the ontology or execute analyses.\n",
            );
        }
        "designer" => {
            prompt.push_str(
                "\nThe current user has **designer** role. \
                 You have full access to all tools.\n",
            );
        }
        "admin" => {
            prompt.push_str(
                "\nThe current user has **admin** role. \
                 You have full access to all tools and system configuration.\n",
            );
        }
        _ => {}
    }

    if let Some(ontology) = domain.current_ontology() {
        prompt.push_str(&format!(
            "\nCurrent ontology: '{}' (v{})\n\
             Node types: {}\n\
             Edge types: {}\n",
            ontology.name,
            ontology.version.number,
            ontology
                .node_types()
                .iter()
                .map(|n| n.label.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            ontology
                .edge_types()
                .iter()
                .map(|e| e.label.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }

    if let Some(insights) = &domain.repo_insights {
        prompt.push_str("\n\n--- Source Code Insights ---\n");
        if let Ok(formatted) = serde_json::to_string_pretty(insights) {
            prompt.push_str(&formatted);
        }
    }

    if let (Some(kb), Some(ontology)) = (&domain.knowledge_store, domain.current_ontology()) {
        match kb
            .list_active_knowledge(
                &ontology.name,
                ontology.version.number as i32,
                &["correction", "hint"],
                10,
            )
            .await
        {
            Ok(entries) if !entries.is_empty() => {
                prompt.push_str("\n\n--- Learned Knowledge ---\n");
                for e in &entries {
                    prompt.push_str(&format!("- [{}] {}\n", e.kind, e.content));
                }
            }
            _ => {}
        }
    }

    prompt
}

/// Read-only tool surface — viewers, plus the strict subset of
/// designer / admin work that doesn't mutate the ontology or
/// dispatch heavy analysis.
const VIEWER_TOOLS: &[&str] = &[
    tools::QUERY_GRAPH,
    tools::EXPLAIN_ONTOLOGY,
    tools::VISUALIZE,
    tools::RECALL_MEMORY,
    tools::SEARCH_RECIPES,
    tools::INTROSPECT_SOURCE,
    tools::CONSULT_KNOWLEDGE,
];

/// Full tool surface — designer / admin. Superset of `VIEWER_TOOLS`,
/// extended with mutating + analysis + ambiguity-resolution tools.
const DESIGNER_TOOLS: &[&str] = &[
    tools::QUERY_GRAPH,
    tools::EDIT_ONTOLOGY,
    tools::APPLY_ONTOLOGY,
    tools::EXECUTE_ANALYSIS,
    tools::EXPLAIN_ONTOLOGY,
    tools::VISUALIZE,
    tools::RECALL_MEMORY,
    tools::SEARCH_RECIPES,
    tools::INTROSPECT_SOURCE,
    tools::SCHEMA_EVOLUTION,
    tools::CONSULT_KNOWLEDGE,
    tools::RESOLVE_AMBIGUITY,
];

/// Determine which tools are available based on user role. The match
/// is exhaustive across the closed [`crate::PlatformRole`] set —
/// `Admin` and `Designer` both reach the full surface; `Viewer` is
/// limited to the read-only subset; an unrecognised role string (a
/// future role variant, a misconfigured DB, a typo upstream) reaches
/// an *empty* tool surface, never the full one. Failsafe: the worst
/// case for an unknown role is "the agent has nothing to call",
/// which surfaces immediately during testing rather than a silent
/// over-privilege escalation.
pub(crate) fn tool_names_for_role(role: &str) -> Vec<&'static str> {
    match role {
        "viewer" => VIEWER_TOOLS.to_vec(),
        "designer" | "admin" => DESIGNER_TOOLS.to_vec(),
        unknown => {
            tracing::warn!(
                role = unknown,
                "unrecognised role on agent build — granting empty tool surface"
            );
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_tools_subset_of_designer_tools() {
        for tool in VIEWER_TOOLS {
            assert!(
                DESIGNER_TOOLS.contains(tool),
                "VIEWER_TOOLS must be a subset of DESIGNER_TOOLS — '{tool}' missing from DESIGNER_TOOLS"
            );
        }
    }

    #[test]
    fn unknown_role_gets_empty_tool_surface() {
        assert!(tool_names_for_role("auditor").is_empty());
        assert!(tool_names_for_role("").is_empty());
    }

    #[test]
    fn known_roles_resolve_to_documented_surface() {
        assert_eq!(tool_names_for_role("viewer"), VIEWER_TOOLS.to_vec());
        assert_eq!(tool_names_for_role("designer"), DESIGNER_TOOLS.to_vec());
        assert_eq!(tool_names_for_role("admin"), DESIGNER_TOOLS.to_vec());
    }
}
