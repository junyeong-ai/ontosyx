use std::collections::{HashMap, HashSet};

use ox_core::graph_exploration::{NodeExpansion, SearchResultNode};
use ox_core::query_ir::QueryResult;
use ox_core::types::PropertyValue;
use ox_store::AclPolicy;

/// Apply ACL policies to query results by masking or removing restricted properties.
/// This is the enforcement layer — policies are fetched from AclStore,
/// then applied to result columns/rows before returning to the client.
pub fn apply_acl_policies(result: &mut QueryResult, policies: &[AclPolicy]) {
    if policies.is_empty() {
        return;
    }

    // Build deny and mask sets from policies
    // deny: (property_name) -> completely remove from results
    // mask: (property_name) -> replace value with mask_pattern
    let mut deny_columns: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut mask_columns: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for policy in policies {
        let props = match &policy.properties {
            Some(p) => p.clone(),
            None => continue, // No specific properties = no enforcement needed
        };

        match policy.action.as_str() {
            "deny" => {
                for prop in props {
                    deny_columns.insert(prop);
                }
            }
            "mask" => {
                let pattern = policy
                    .mask_pattern
                    .clone()
                    .unwrap_or_else(|| "***".to_string());
                for prop in props {
                    if !deny_columns.contains(&prop) {
                        mask_columns.insert(prop, pattern.clone());
                    }
                }
            }
            _ => {} // "allow" = no action needed
        }
    }

    if deny_columns.is_empty() && mask_columns.is_empty() {
        return;
    }

    // Find column indices to deny (remove) or mask
    let mut deny_indices: Vec<usize> = Vec::new();
    let mut mask_indices: Vec<(usize, String)> = Vec::new();

    for (i, col) in result.columns.iter().enumerate() {
        if deny_columns.contains(col) {
            deny_indices.push(i);
        } else if let Some(pattern) = mask_columns.get(col) {
            mask_indices.push((i, pattern.clone()));
        }
    }

    // Apply masks first (replace values)
    for row in &mut result.rows {
        for (idx, pattern) in &mask_indices {
            if let Some(cell) = row.get_mut(*idx) {
                *cell = PropertyValue::String(pattern.clone());
            }
        }
    }

    // Remove denied columns (reverse order to preserve indices)
    if !deny_indices.is_empty() {
        deny_indices.sort_unstable();
        deny_indices.reverse();
        for idx in &deny_indices {
            result.columns.remove(*idx);
            for row in &mut result.rows {
                if *idx < row.len() {
                    row.remove(*idx);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ACL enforcement for graph exploration types (SearchResultNode, NodeExpansion)
// ---------------------------------------------------------------------------

/// Build deny/mask sets from ACL policies. Returns (deny_set, mask_map).
fn build_property_rules(policies: &[AclPolicy]) -> (HashSet<String>, HashMap<String, String>) {
    let mut deny: HashSet<String> = HashSet::new();
    let mut mask: HashMap<String, String> = HashMap::new();

    for policy in policies {
        let props = match &policy.properties {
            Some(p) => p.clone(),
            None => continue,
        };

        match policy.action.as_str() {
            "deny" => {
                for prop in props {
                    deny.insert(prop);
                }
            }
            "mask" => {
                let pattern = policy
                    .mask_pattern
                    .clone()
                    .unwrap_or_else(|| "***".to_string());
                for prop in props {
                    if !deny.contains(&prop) {
                        mask.insert(prop, pattern.clone());
                    }
                }
            }
            _ => {}
        }
    }

    (deny, mask)
}

/// Apply ACL deny/mask rules to a property map (used by graph exploration types).
fn enforce_on_props(
    props: &mut HashMap<String, serde_json::Value>,
    deny: &HashSet<String>,
    mask: &HashMap<String, String>,
) {
    // Remove denied properties
    props.retain(|key, _| !deny.contains(key));

    // Mask remaining properties
    for (key, pattern) in mask {
        if let Some(val) = props.get_mut(key) {
            *val = serde_json::Value::String(pattern.clone());
        }
    }
}

/// Apply ACL policies to search result nodes by masking or removing restricted properties.
pub fn apply_acl_to_search_results(results: &mut [SearchResultNode], policies: &[AclPolicy]) {
    if policies.is_empty() {
        return;
    }

    let (deny, mask) = build_property_rules(policies);
    if deny.is_empty() && mask.is_empty() {
        return;
    }

    for node in results.iter_mut() {
        enforce_on_props(&mut node.props, &deny, &mask);
    }
}

/// Apply ACL policies to a node expansion by masking or removing restricted properties.
pub fn apply_acl_to_node_expansion(expansion: &mut NodeExpansion, policies: &[AclPolicy]) {
    if policies.is_empty() {
        return;
    }

    let (deny, mask) = build_property_rules(policies);
    if deny.is_empty() && mask.is_empty() {
        return;
    }

    for neighbor in expansion.neighbors.iter_mut() {
        enforce_on_props(&mut neighbor.props, &deny, &mask);
    }
}
