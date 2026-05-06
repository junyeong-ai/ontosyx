//! Change-type routing rules — one per `(workspace_id?, change_type)`.
//!
//! Global defaults live with `workspace_id IS NULL` and are seeded
//! by the migration. Workspace overrides go through
//! [`ChangeRoutingStore::upsert_change_routing_rule`]; the resolve
//! path returns the higher-priority row (workspace override > global).
//!
//! The `change_*` prefix on every method disambiguates from
//! [`super::ModelConfigStore`]'s `list_routing_rules` (LLM model
//! routing — a different concept routing between providers).

use async_trait::async_trait;

use ox_core::error::OxResult;

#[async_trait]
pub trait ChangeRoutingStore: Send + Sync {
    /// List every rule visible to the current workspace (global +
    /// overrides), ordered by `change_type` then `priority DESC`.
    /// Used by the admin UI to render the full routing table.
    async fn list_change_routing_rules(
        &self,
    ) -> OxResult<Vec<ox_ontology::change_routing::ChangeRoutingRule>>;

    /// Resolve the single active rule for `change_type`. Workspace
    /// override wins over global default via higher `priority`.
    /// Returns `None` when no rule matches — caller treats that as
    /// "require approval" by policy, not "silently auto-apply".
    async fn resolve_change_routing(
        &self,
        change_type: ox_ontology::change_routing::ChangeType,
    ) -> OxResult<Option<ox_ontology::change_routing::ChangeRoutingRule>>;

    /// Upsert a workspace override. Natural key is
    /// `(workspace_id, change_type)` so a workspace has at most one
    /// override per change type. The store fills `workspace_id` from
    /// `app.workspace_id` — callers don't pass it (a caller writing
    /// global defaults uses the SYSTEM_BYPASS path at migration time).
    async fn upsert_change_routing_rule(
        &self,
        rule: ox_ontology::change_routing::ChangeRoutingRule,
    ) -> OxResult<ox_ontology::change_routing::ChangeRoutingRule>;

    /// Drop a workspace override, reverting to the global default.
    /// Returns `true` when a row was deleted.
    async fn delete_change_routing_rule(
        &self,
        change_type: ox_ontology::change_routing::ChangeType,
    ) -> OxResult<bool>;
}
