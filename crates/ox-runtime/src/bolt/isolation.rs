//! Workspace-isolation rewrite shared by every Bolt-driver backend.
//!
//! Reads the `GRAPH_WORKSPACE_ID` / `GRAPH_SYSTEM_BYPASS` task-locals and
//! delegates the actual rewrite to the supplied
//! [`GraphIsolationStrategy`]. The same body used to live duplicated in
//! `Neo4jRuntime::pre_execute` and `MemGraphRuntime::pre_execute`.

use std::collections::HashMap;

use ox_core::types::PropertyValue;

use crate::isolation::GraphIsolationStrategy;
use crate::{GRAPH_SYSTEM_BYPASS, GRAPH_WORKSPACE_ID};

pub(crate) fn scope_with_task_locals(
    strategy: Option<&dyn GraphIsolationStrategy>,
    cypher: &str,
    params: &HashMap<String, PropertyValue>,
) -> (String, HashMap<String, PropertyValue>) {
    let strategy = match strategy {
        Some(s) => s,
        None => return (cypher.to_string(), params.clone()),
    };

    if GRAPH_SYSTEM_BYPASS.try_with(|b| *b).unwrap_or(false) {
        return (cypher.to_string(), params.clone());
    }

    match GRAPH_WORKSPACE_ID.try_with(|id| id.to_string()) {
        Ok(ws_id) => {
            let scoped = strategy.scope(cypher, &ws_id);
            let mut merged = params.clone();
            for (key, value) in scoped.params {
                merged.insert(key.to_string(), PropertyValue::String(value));
            }
            (scoped.query, merged)
        }
        Err(_) => (cypher.to_string(), params.clone()),
    }
}
