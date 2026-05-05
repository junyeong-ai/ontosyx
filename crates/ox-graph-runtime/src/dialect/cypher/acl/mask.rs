//! `Mask` action — replace property accesses on the projection
//! surface (RETURN / WITH) with the policy's `mask_pattern`. Three
//! shapes covered: direct `<var>.<prop>`, chained `<var>.<prop>.<sub>`,
//! and bare `<var>` (rewritten to a Cypher map projection that
//! overrides every masked property).

use std::collections::HashMap;

use crate::cypher::ast::{
    ClauseKind, CypherClause, CypherPatternElement, CypherStatement,
};
use crate::cypher::token::{CypherToken, TokenKind};

use super::snapshot::{AclAction, AclSnapshot};

const DEFAULT_MASK_PATTERN: &str = "'***'";

/// Per-policy mask spec collected from the snapshot. `label = None`
/// is the wildcard form (`resource_value: None | "*"`) and applies
/// to every variable's labels in the query.
#[derive(Debug, Clone)]
pub(super) struct MaskSpec {
    label: Option<String>,
    properties: Vec<String>,
    pattern: String,
}

pub(super) fn collect_mask_specs(snapshot: &AclSnapshot) -> Vec<MaskSpec> {
    let mut out = Vec::new();
    for policy in &snapshot.policies {
        if policy.action != AclAction::Mask {
            continue;
        }
        if policy.resource_type != "label" {
            continue;
        }
        let properties = match policy.properties.as_ref() {
            Some(p) if !p.is_empty() => p.clone(),
            _ => continue,
        };
        let pattern = policy
            .mask_pattern
            .as_deref()
            .map(quote_pattern_literal)
            .unwrap_or_else(|| DEFAULT_MASK_PATTERN.to_string());
        let label = match policy.resource_value.as_deref() {
            Some(v) if !v.is_empty() && v != "*" => Some(v.to_string()),
            _ => None,
        };
        out.push(MaskSpec {
            label,
            properties,
            pattern,
        });
    }
    out
}

/// The DB stores `mask_pattern` as a free-form string (`***`,
/// `XXXX-XXXX`, etc.) but the rewriter has to splice it into Cypher
/// as a single-quoted literal. Cypher's escape rules (per openCypher
/// 9.0 §2.5.4) mark backslash as the escape character: `\\`, `\'`,
/// `\"`, `\b`, `\f`, `\n`, `\r`, `\t`, `\uXXXX`. Anything else after
/// `\` is a parse error. So a raw `mask_pattern` containing a
/// literal `\` MUST be doubled or it produces an unrecognised escape
/// sequence at runtime.
///
/// Order matters: escape `\` first so the `\'` we introduce in the
/// next pass doesn't turn into `\\\'` and re-trigger the rule.
fn quote_pattern_literal(raw: &str) -> String {
    let escaped = raw.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

pub(super) fn apply_mask_in_statement(
    statement: &mut CypherStatement,
    specs: &[MaskSpec],
) -> bool {
    if specs.is_empty() {
        return false;
    }
    let var_labels = build_var_label_map(statement);
    if var_labels.is_empty() {
        return false;
    }

    let mut any = false;
    for clause in &mut statement.clauses {
        if !matches!(clause.kind, ClauseKind::Return | ClauseKind::With) {
            continue;
        }
        if rewrite_property_accesses(clause, &var_labels, specs) {
            any = true;
        }
    }
    any
}

/// Walk MATCH / OPTIONAL MATCH / CREATE / MERGE clauses and build
/// `variable -> labels` for every bound node. A variable bound in
/// multiple patterns accumulates the union of its labels.
fn build_var_label_map(statement: &CypherStatement) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for clause in &statement.clauses {
        if !matches!(
            clause.kind,
            ClauseKind::Match
                | ClauseKind::OptionalMatch
                | ClauseKind::Create
                | ClauseKind::Merge
        ) {
            continue;
        }
        for pattern in &clause.patterns {
            for element in &pattern.elements {
                if let CypherPatternElement::Node(node) = element
                    && let Some(var) = &node.variable
                {
                    let entry = map.entry(var.clone()).or_default();
                    for label in &node.labels {
                        if !entry.contains(label) {
                            entry.push(label.clone());
                        }
                    }
                }
            }
        }
    }
    map
}

/// Walk a clause's tokens, build a list of edits, apply them in
/// reverse byte-offset order. Returns true iff any edit landed.
fn rewrite_property_accesses(
    clause: &mut CypherClause,
    var_labels: &HashMap<String, Vec<String>>,
    specs: &[MaskSpec],
) -> bool {
    let clause_start = clause.span.start;
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    for chain in find_property_access_chains(&clause.tokens) {
        let labels = match var_labels.get(&chain.var) {
            Some(l) => l,
            None => continue,
        };
        let pattern = match resolve_mask_pattern(labels, &chain.first_prop, specs) {
            Some(p) => p,
            None => continue,
        };
        let local_start = chain.span_start.saturating_sub(clause_start);
        let local_end = chain.span_end.saturating_sub(clause_start);
        if local_end <= clause.text.len() && local_start < local_end {
            replacements.push((local_start, local_end, pattern));
        }
    }

    for bare in find_bare_variable_references(&clause.tokens) {
        let labels = match var_labels.get(&bare.var) {
            Some(l) => l,
            None => continue,
        };
        let masked = collect_masked_properties_for(labels, specs);
        if masked.is_empty() {
            continue;
        }
        let projection = compose_map_projection_override(&bare.var, &masked);
        let local_start = bare.span_start.saturating_sub(clause_start);
        let local_end = bare.span_end.saturating_sub(clause_start);
        if local_end <= clause.text.len() && local_start < local_end {
            replacements.push((local_start, local_end, projection));
        }
    }

    if replacements.is_empty() {
        return false;
    }
    replacements.sort_by(|a, b| b.0.cmp(&a.0));
    let mut text = std::mem::take(&mut clause.text);
    for (start, end, value) in replacements {
        text.replace_range(start..end, &value);
    }
    clause.text = text;
    true
}

fn resolve_mask_pattern(labels: &[String], prop: &str, specs: &[MaskSpec]) -> Option<String> {
    for spec in specs {
        let label_match = match &spec.label {
            Some(l) => labels.iter().any(|v| v == l),
            None => !labels.is_empty(),
        };
        if !label_match {
            continue;
        }
        if spec.properties.iter().any(|p| p == prop) {
            return Some(spec.pattern.clone());
        }
    }
    None
}

/// Collect every `(property, mask_pattern)` pair that applies to a
/// variable bound to `labels`. Used by the bare-variable rewrite to
/// build the map projection override.
fn collect_masked_properties_for(
    labels: &[String],
    specs: &[MaskSpec],
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for spec in specs {
        let label_match = match &spec.label {
            Some(l) => labels.iter().any(|v| v == l),
            None => !labels.is_empty(),
        };
        if !label_match {
            continue;
        }
        for prop in &spec.properties {
            if !out.iter().any(|(existing, _)| existing == prop) {
                out.push((prop.clone(), spec.pattern.clone()));
            }
        }
    }
    out
}

fn compose_map_projection_override(var: &str, masked: &[(String, String)]) -> String {
    let entries = masked
        .iter()
        .map(|(prop, pattern)| format!("{prop}: {pattern}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{var} {{.*, {entries}}}")
}

/// One discovered property-access chain: `<var>.<first>(.<rest>)*`.
/// `span_start..span_end` is the absolute byte range covering the
/// entire chain. A chain whose first hop matches a mask policy gets
/// replaced wholesale — masking the head implies masking everything
/// reachable through it.
struct PropertyAccessChain {
    var: String,
    first_prop: String,
    span_start: usize,
    span_end: usize,
}

/// Walk `tokens` and return every `<var>.<prop>(.<sub>)*` chain.
/// The chain starts on a non-property variable identifier (i.e., the
/// previous non-trivia token is NOT a dot), so an inner triplet
/// `b.c` of `a.b.c` is not double-counted.
fn find_property_access_chains(tokens: &[CypherToken]) -> Vec<PropertyAccessChain> {
    let non_trivia: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| (!t.is_trivia()).then_some(i))
        .collect();

    let mut chains = Vec::new();
    let mut i = 0;
    while i + 2 < non_trivia.len() {
        let a = &tokens[non_trivia[i]];
        let b = &tokens[non_trivia[i + 1]];
        let c = &tokens[non_trivia[i + 2]];

        let is_dot = b.kind == TokenKind::Operator && b.text == ".";
        let var_ok = matches!(a.kind, TokenKind::Identifier | TokenKind::QuotedIdentifier);
        let prop_ok = matches!(c.kind, TokenKind::Identifier | TokenKind::QuotedIdentifier);
        if !is_dot || !var_ok || !prop_ok {
            i += 1;
            continue;
        }

        if i > 0 {
            let prev = &tokens[non_trivia[i - 1]];
            if prev.kind == TokenKind::Operator && prev.text == "." {
                i += 1;
                continue;
            }
        }

        let mut chain_end_idx = i + 2;
        let mut span_end = c.span.end;
        loop {
            if chain_end_idx + 2 >= non_trivia.len() {
                break;
            }
            let dot = &tokens[non_trivia[chain_end_idx + 1]];
            let ident = &tokens[non_trivia[chain_end_idx + 2]];
            let next_is_dot = dot.kind == TokenKind::Operator && dot.text == ".";
            let next_is_ident =
                matches!(ident.kind, TokenKind::Identifier | TokenKind::QuotedIdentifier);
            if !(next_is_dot && next_is_ident) {
                break;
            }
            chain_end_idx += 2;
            span_end = ident.span.end;
        }

        chains.push(PropertyAccessChain {
            var: strip_backticks(&a.text),
            first_prop: strip_backticks(&c.text),
            span_start: a.span.start,
            span_end,
        });
        i = chain_end_idx + 1;
    }
    chains
}

/// One bare variable reference inside a RETURN / WITH clause —
/// `RETURN p`, `WITH p` (with or without `AS alias`). Function-call
/// arguments (`count(p)`, `id(p)`) are excluded.
struct BareVariableReference {
    var: String,
    span_start: usize,
    span_end: usize,
}

/// Walk `tokens` and return every bare variable reference. A
/// reference is "bare" when the token is an Identifier and:
///
/// * the previous non-trivia token is NOT `.` (property access),
/// * the next non-trivia token is NOT `(` (function call),
/// * the next non-trivia token is NOT `.` (chain head — chains are
///   handled by [`find_property_access_chains`]),
/// * the previous non-trivia token is NOT `(` (function-call
///   argument like `count(p)` — scalar / aggregate ops the operator
///   owns).
fn find_bare_variable_references(tokens: &[CypherToken]) -> Vec<BareVariableReference> {
    let non_trivia: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| (!t.is_trivia()).then_some(i))
        .collect();

    let mut out = Vec::new();
    for (pos, &tok_idx) in non_trivia.iter().enumerate() {
        let token = &tokens[tok_idx];
        if !matches!(token.kind, TokenKind::Identifier | TokenKind::QuotedIdentifier) {
            continue;
        }
        if pos > 0 {
            let prev = &tokens[non_trivia[pos - 1]];
            if prev.kind == TokenKind::Operator && prev.text == "." {
                continue;
            }
            if prev.kind == TokenKind::Paren && prev.text == "(" {
                continue;
            }
        }
        if pos + 1 < non_trivia.len() {
            let next = &tokens[non_trivia[pos + 1]];
            if next.kind == TokenKind::Paren && next.text == "(" {
                continue;
            }
            if next.kind == TokenKind::Operator && next.text == "." {
                continue;
            }
        }
        out.push(BareVariableReference {
            var: strip_backticks(&token.text),
            span_start: token.span.start,
            span_end: token.span.end,
        });
    }
    out
}

fn strip_backticks(s: &str) -> String {
    if s.starts_with('`') && s.ends_with('`') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}
