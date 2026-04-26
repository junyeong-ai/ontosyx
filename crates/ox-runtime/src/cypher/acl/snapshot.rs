//! Policy types the ACL rewriter consumes — independent of the
//! `ox-store` `AclPolicy` row so the rewriter layer carries no
//! persistence dependency. The ox-api loader does the conversion.

/// Concrete actions a row-level policy can take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AclAction {
    /// Reject every row that matches this policy's resource. Inject
    /// `WHERE false` on the matching MATCH clause.
    Deny,
    /// Replace specific properties with `mask_pattern` (or `'***'`
    /// when none is set) at projection time.
    Mask,
}

impl AclAction {
    /// Map an `acl_policies.action` string to its enum form. Returns
    /// `None` for unknown values — the loader is responsible for
    /// surfacing typos; the rewriter silently skips them so an
    /// unrecognised action can't accidentally re-classify a policy
    /// as `Deny`.
    pub fn from_db_string(s: &str) -> Option<Self> {
        match s {
            "deny" => Some(Self::Deny),
            "mask" => Some(Self::Mask),
            _ => None,
        }
    }
}

/// Minimal policy slice the rewriter consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AclPolicySpec {
    pub action: AclAction,
    /// Resource family — `"label"` (node label match) or
    /// `"edge_label"` (edge type match). Other values pass through
    /// unchanged.
    pub resource_type: String,
    /// Specific resource value. `None` is a wildcard across the
    /// entire `resource_type`.
    pub resource_value: Option<String>,
    /// Property whitelist for `Mask`. Unused by `Deny`.
    pub properties: Option<Vec<String>>,
    /// Pattern for `Mask`. Unused by `Deny`.
    pub mask_pattern: Option<String>,
    /// Tie-breaker when more than one policy matches the same
    /// resource. Higher value wins. Sorting is the loader's
    /// responsibility — the rewriter trusts the order.
    pub priority: i32,
}

/// Pre-filtered, priority-sorted policies the rewriter applies.
#[derive(Debug, Clone, Default)]
pub struct AclSnapshot {
    pub policies: Vec<AclPolicySpec>,
}

impl AclSnapshot {
    pub fn empty() -> Self {
        Self::default()
    }

    /// `true` when the snapshot has no policies — the rewriter uses
    /// this to short-circuit the entire pass without walking the AST.
    pub fn is_empty(&self) -> bool {
        self.policies.is_empty()
    }
}
