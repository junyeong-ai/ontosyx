//! Shared low-level helpers used by every rewriter pass.
//!
//! Three rewriters (`WorkspaceScopeRewriter`, `AclRewriter`,
//! `SoftDeleteRewriter`) used to carry their own copies of the same
//! string + token utilities. The duplication started small and
//! drifted (`strip_keyword` vs `strip_leading_keyword`,
//! `leading_whitespace` vs `split_leading_whitespace`) — exactly the
//! shape of debt the workspace lints want gone.
//!
//! Everything in here is a pure function over the AST / tokens; no
//! state, no allocator surprises. Add a new helper here when a
//! second rewriter needs the same operation; never inline-copy.

use crate::cypher::ast::{ClauseKind, CypherStatement};

/// Split `text` into its leading-whitespace prefix and the rest.
/// Returns `("", text)` if the input has no leading whitespace.
pub(crate) fn split_leading_whitespace(text: &str) -> (&str, &str) {
    let trimmed_len = text.len() - text.trim_start().len();
    text.split_at(trimmed_len)
}

/// Convenience that drops the prefix half of [`split_leading_whitespace`].
pub(crate) fn leading_whitespace(text: &str) -> &str {
    let (lead, _) = split_leading_whitespace(text);
    lead
}

/// If `text` (already left-trimmed) begins with `keyword` followed
/// by whitespace or end-of-string, return the remainder after the
/// keyword. Case-insensitive match — Cypher keywords come in mixed
/// case in practice (`MATCH`, `Match`, `match`).
pub(crate) fn strip_leading_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let kw = keyword.as_bytes();
    let bytes = text.as_bytes();
    if bytes.len() < kw.len() {
        return None;
    }
    for (i, k) in kw.iter().enumerate() {
        if !bytes[i].eq_ignore_ascii_case(k) {
            return None;
        }
    }
    let rest = &text[kw.len()..];
    if rest.is_empty() || rest.starts_with(|c: char| c.is_whitespace()) {
        Some(rest)
    } else {
        None
    }
}

/// Find the WHERE clause that immediately follows the clause at
/// `idx`. WHERE in Cypher binds to the preceding MATCH / OPTIONAL
/// MATCH; an intervening clause means the WHERE belongs to
/// something else. Returns `None` if the next clause is not a WHERE.
pub(crate) fn find_following_where_clause(
    statement: &CypherStatement,
    idx: usize,
) -> Option<usize> {
    let next = idx + 1;
    statement
        .clauses
        .get(next)
        .filter(|c| c.kind == ClauseKind::Where)
        .map(|_| next)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn split_leading_whitespace_handles_no_prefix() {
        let (lead, rest) = split_leading_whitespace("WHERE foo");
        assert_eq!(lead, "");
        assert_eq!(rest, "WHERE foo");
    }

    #[test]
    fn split_leading_whitespace_extracts_full_prefix() {
        let (lead, rest) = split_leading_whitespace("   \tWHERE foo");
        assert_eq!(lead, "   \t");
        assert_eq!(rest, "WHERE foo");
    }

    #[test]
    fn strip_leading_keyword_case_insensitive() {
        assert_eq!(strip_leading_keyword("WHERE x", "WHERE"), Some(" x"));
        assert_eq!(strip_leading_keyword("where x", "WHERE"), Some(" x"));
        assert_eq!(strip_leading_keyword("Where x", "WHERE"), Some(" x"));
    }

    #[test]
    fn strip_leading_keyword_requires_following_whitespace_or_eof() {
        assert_eq!(strip_leading_keyword("WHEREVER", "WHERE"), None);
        assert_eq!(strip_leading_keyword("WHERE", "WHERE"), Some(""));
        assert_eq!(strip_leading_keyword("WHERE\n", "WHERE"), Some("\n"));
    }
}
