//! Cypher partial parser.
//!
//! Turns a [`crate::cypher::token`] stream into a [`CypherAst`]. This is a
//! recursive-descent parser that recognises clause boundaries and the
//! pattern grammar needed by downstream rewriters / validators.
//!
//! What we parse structurally:
//! - Statement boundaries (`UNION` / `UNION ALL`).
//! - Clause headers: the keywords listed in [`ClauseKind`].
//! - Patterns inside MATCH / OPTIONAL MATCH / CREATE / MERGE:
//!   node patterns `(var:Label:Label2 {k: v})`, relationship patterns
//!   `-[var:TYPE|TYPE2 *1..3 {k: v}]->` with direction, and
//!   variable-length bounds.
//!
//! What we don't parse:
//! - Full expressions. A clause's non-pattern body stays as the raw token
//!   slice, because rewriting / validating doesn't need the expression tree.
//! - Function call arguments, list / map literals inside projections.
//!
//! The parser is lenient: on ambiguity it prefers to keep the source
//! intact (as `ClauseKind::Unknown` or an empty pattern list) rather than
//! error out. A lossless round trip of the raw text is the invariant;
//! structural accuracy is best-effort on surfaces that rewriters touch.

use crate::cypher::ast::{
    ClauseKind, CypherAst, CypherClause, CypherPattern, CypherPatternElement, CypherStatement,
    NodePattern, RelDirection, RelationshipPattern, RemoveItem, SetItem, UnionKind,
};
use crate::cypher::token::{CypherToken, Span, TokenKind, tokenize};

/// Parse an input string. Always succeeds: unrecognised constructs fall
/// through to `ClauseKind::Unknown` with tokens preserved.
pub fn parse(input: &str) -> CypherAst {
    let tokens = tokenize(input);
    Parser::new(input, tokens).parse_ast()
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<CypherToken>,
    /// Indices into `tokens` that represent non-trivia tokens, for
    /// convenient step-by-step matching without losing whitespace in the
    /// preserved slices.
    idx: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, tokens: Vec<CypherToken>) -> Self {
        Self {
            source,
            tokens,
            idx: 0,
        }
    }

    fn parse_ast(&mut self) -> CypherAst {
        let mut statements: Vec<CypherStatement> = Vec::new();
        let mut unions: Vec<UnionKind> = Vec::new();

        loop {
            let stmt = self.parse_statement();
            statements.push(stmt);

            match self.peek_union() {
                Some(kind) => {
                    unions.push(kind);
                    self.consume_union();
                }
                None => break,
            }
        }

        CypherAst { statements, unions }
    }

    fn parse_statement(&mut self) -> CypherStatement {
        let mut clauses: Vec<CypherClause> = Vec::new();
        while self.idx < self.tokens.len() {
            if self.peek_union().is_some() {
                break;
            }
            let clause = self.parse_clause();
            if clause.tokens.is_empty() {
                // Defensive: a zero-token clause would loop forever.
                break;
            }
            clauses.push(clause);
        }
        CypherStatement { clauses }
    }

    /// Read one clause: everything from (and including) the current
    /// clause-head keyword up to but not including the next clause-head
    /// keyword or a UNION separator or EOF.
    fn parse_clause(&mut self) -> CypherClause {
        let start_tok_idx = self.idx;
        let start_byte = self
            .tokens
            .get(self.idx)
            .map(|t| t.span.start)
            .unwrap_or(self.source.len());

        let kind = self.classify_clause_head();
        // Advance past the clause-head keyword(s). `OPTIONAL MATCH` and
        // `DETACH DELETE` consume two tokens; everything else consumes one.
        self.advance_non_trivia();
        if matches!(kind, ClauseKind::OptionalMatch | ClauseKind::DetachDelete) {
            self.advance_non_trivia();
        }
        if matches!(kind, ClauseKind::OrderBy) {
            // ORDER BY is two keywords; we already advanced ORDER, now BY.
            self.advance_non_trivia();
        }

        // Collect tokens until the next clause head / UNION / EOF.
        // A clause head inside brackets / parens / braces belongs to a
        // subquery (CALL { … }), a map literal, or a pattern body —
        // never a new outer clause. Track bracket depth and only look
        // for the next head at depth 0.
        let mut depth_paren: i32 = 0;
        let mut depth_bracket: i32 = 0;
        let mut depth_brace: i32 = 0;
        while self.idx < self.tokens.len() {
            let at_top_level = depth_paren == 0 && depth_bracket == 0 && depth_brace == 0;
            if at_top_level {
                if self.peek_union().is_some() {
                    break;
                }
                if self.at_clause_head() {
                    break;
                }
            }
            let tok = &self.tokens[self.idx];
            if tok.kind == crate::cypher::token::TokenKind::Paren {
                match tok.text.as_str() {
                    "(" => depth_paren += 1,
                    ")" => depth_paren -= 1,
                    "[" => depth_bracket += 1,
                    "]" => depth_bracket -= 1,
                    "{" => depth_brace += 1,
                    "}" => depth_brace -= 1,
                    _ => {}
                }
            }
            self.idx += 1;
        }

        let end_tok_idx = self.idx;
        let end_byte = if end_tok_idx == 0 {
            start_byte
        } else if end_tok_idx <= self.tokens.len() {
            self.tokens
                .get(end_tok_idx - 1)
                .map(|t| t.span.end)
                .unwrap_or(start_byte)
        } else {
            self.source.len()
        };

        let clause_tokens: Vec<CypherToken> = self.tokens[start_tok_idx..end_tok_idx].to_vec();
        let text = self.source[start_byte..end_byte].to_string();
        let span = Span::new(start_byte, end_byte);

        let patterns = if kind.has_patterns() {
            parse_patterns(&clause_tokens, start_tok_idx, self.source)
        } else {
            Vec::new()
        };
        let set_items = if matches!(kind, ClauseKind::Set) {
            parse_set_items(&clause_tokens, self.source)
        } else {
            Vec::new()
        };
        let remove_items = if matches!(kind, ClauseKind::Remove) {
            parse_remove_items(&clause_tokens, self.source)
        } else {
            Vec::new()
        };

        CypherClause {
            kind,
            tokens: clause_tokens,
            text,
            span,
            patterns,
            set_items,
            remove_items,
        }
    }

    /// Look at the current non-trivia token (without consuming) and
    /// classify which clause we're about to parse.
    fn classify_clause_head(&self) -> ClauseKind {
        let Some(first) = self.peek_non_trivia(0) else {
            return ClauseKind::Unknown;
        };
        if !matches!(first.kind, TokenKind::Keyword) {
            return ClauseKind::Unknown;
        }
        let head = first.text.to_ascii_uppercase();
        match head.as_str() {
            "MATCH" => ClauseKind::Match,
            "OPTIONAL" => match self.peek_non_trivia(1) {
                Some(t) if t.is_keyword("MATCH") => ClauseKind::OptionalMatch,
                _ => ClauseKind::Unknown,
            },
            "CREATE" => ClauseKind::Create,
            "MERGE" => ClauseKind::Merge,
            "WHERE" => ClauseKind::Where,
            "SET" => ClauseKind::Set,
            "DELETE" => ClauseKind::Delete,
            "DETACH" => match self.peek_non_trivia(1) {
                Some(t) if t.is_keyword("DELETE") => ClauseKind::DetachDelete,
                _ => ClauseKind::Unknown,
            },
            "REMOVE" => ClauseKind::Remove,
            "RETURN" => ClauseKind::Return,
            "WITH" => ClauseKind::With,
            "UNWIND" => ClauseKind::Unwind,
            "CALL" => ClauseKind::Call,
            "ORDER" => match self.peek_non_trivia(1) {
                Some(t) if t.is_keyword("BY") => ClauseKind::OrderBy,
                _ => ClauseKind::Unknown,
            },
            "SKIP" => ClauseKind::Skip,
            "LIMIT" => ClauseKind::Limit,
            _ => ClauseKind::Unknown,
        }
    }

    /// True if the current position is at the start of a recognisable
    /// clause (used to stop clause-body accumulation).
    fn at_clause_head(&self) -> bool {
        !matches!(self.classify_clause_head(), ClauseKind::Unknown) || self.peek_keyword("UNION")
    }

    /// Peek whether the next non-trivia tokens constitute a UNION boundary.
    fn peek_union(&self) -> Option<UnionKind> {
        let first = self.peek_non_trivia(0)?;
        if !first.is_keyword("UNION") {
            return None;
        }
        match self.peek_non_trivia(1) {
            Some(t) if t.is_keyword("ALL") => Some(UnionKind::All),
            _ => Some(UnionKind::Distinct),
        }
    }

    /// Consume a UNION separator.
    fn consume_union(&mut self) {
        // UNION keyword
        self.advance_non_trivia();
        // Optional ALL
        if let Some(t) = self.peek_non_trivia(0)
            && t.is_keyword("ALL")
        {
            self.advance_non_trivia();
        }
    }

    fn peek_non_trivia(&self, offset: usize) -> Option<&CypherToken> {
        let mut remaining = offset;
        for tok in &self.tokens[self.idx..] {
            if tok.is_trivia() {
                continue;
            }
            if remaining == 0 {
                return Some(tok);
            }
            remaining -= 1;
        }
        None
    }

    fn peek_keyword(&self, kw: &str) -> bool {
        self.peek_non_trivia(0)
            .map(|t| t.is_keyword(kw))
            .unwrap_or(false)
    }

    /// Advance past trivia and one non-trivia token.
    fn advance_non_trivia(&mut self) {
        while self.idx < self.tokens.len() && self.tokens[self.idx].is_trivia() {
            self.idx += 1;
        }
        if self.idx < self.tokens.len() {
            self.idx += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Pattern parsing
// ---------------------------------------------------------------------------

/// Parse the pattern list inside a MATCH / OPTIONAL MATCH / CREATE / MERGE
/// clause body. Accepts a clause's token slice (which starts with the
/// clause-head keyword); ignores the head + any leading trivia, then walks
/// patterns separated by commas.
fn parse_patterns(
    clause_tokens: &[CypherToken],
    _clause_start_token_idx: usize,
    source: &str,
) -> Vec<CypherPattern> {
    // Skip the clause-head keyword (and OPTIONAL / DETACH if applicable).
    let mut i = 0usize;
    while i < clause_tokens.len() && clause_tokens[i].is_trivia() {
        i += 1;
    }
    // First keyword
    if i < clause_tokens.len() && matches!(clause_tokens[i].kind, TokenKind::Keyword) {
        let kw = clause_tokens[i].text.to_ascii_uppercase();
        i += 1;
        if kw == "OPTIONAL" {
            // Skip MATCH that follows
            while i < clause_tokens.len() && clause_tokens[i].is_trivia() {
                i += 1;
            }
            if i < clause_tokens.len() && clause_tokens[i].is_keyword("MATCH") {
                i += 1;
            }
        }
    }

    let mut patterns: Vec<CypherPattern> = Vec::new();
    loop {
        // Skip whitespace / commas between patterns.
        while i < clause_tokens.len()
            && (clause_tokens[i].is_trivia()
                || (clause_tokens[i].kind == TokenKind::Punctuation
                    && clause_tokens[i].text == ","))
        {
            i += 1;
        }
        if i >= clause_tokens.len() {
            break;
        }
        // A pattern must start with `(` (node) — if we see anything else
        // we've hit expressions (WHERE body, projections inside CALL, …)
        // and should stop.
        if !(clause_tokens[i].kind == TokenKind::Paren && clause_tokens[i].text == "(") {
            break;
        }

        let (pattern, consumed) = parse_single_pattern(&clause_tokens[i..], source);
        if consumed == 0 {
            // Defensive against infinite loops if pattern parsing bailed.
            break;
        }
        if !pattern.elements.is_empty() {
            patterns.push(pattern);
        }
        i += consumed;
    }
    patterns
}

/// Parse one pattern (alternating node / relationship). Returns the
/// pattern plus the number of tokens consumed.
fn parse_single_pattern(tokens: &[CypherToken], source: &str) -> (CypherPattern, usize) {
    let mut elements: Vec<CypherPatternElement> = Vec::new();
    let mut i = 0usize;
    let mut pattern_start_byte: Option<usize> = None;
    let mut pattern_end_byte: usize = 0;

    loop {
        while i < tokens.len() && tokens[i].is_trivia() {
            i += 1;
        }
        if i >= tokens.len() {
            break;
        }
        // Node
        if tokens[i].kind == TokenKind::Paren && tokens[i].text == "(" {
            let (node, consumed) = parse_node_pattern(&tokens[i..]);
            if consumed == 0 {
                break;
            }
            if pattern_start_byte.is_none() {
                pattern_start_byte = Some(node.span.start);
            }
            pattern_end_byte = node.span.end;
            elements.push(CypherPatternElement::Node(node));
            i += consumed;
            continue;
        }
        // Relationship: `-`, `-[`, `<-`, `->`, or the tail `-` / `->` of the previous one.
        // Detect by the presence of `-`, `<-`, `->` as the head.
        if is_relationship_start(&tokens[i..]) {
            let (rel, consumed) = parse_relationship_pattern(&tokens[i..]);
            if consumed == 0 {
                break;
            }
            pattern_end_byte = rel.span.end;
            if pattern_start_byte.is_none() {
                pattern_start_byte = Some(rel.span.start);
            }
            elements.push(CypherPatternElement::Relationship(rel));
            i += consumed;
            continue;
        }
        break;
    }

    let span = match pattern_start_byte {
        Some(start) => Span::new(start, pattern_end_byte),
        None => Span::default(),
    };
    let _ = source; // currently unused beyond span math
    (CypherPattern { elements, span }, i)
}

/// Look at the head of a slice and decide whether it's the start of a
/// relationship pattern. Cypher relationship syntax variants:
///   `-[...]->`, `-[...]-`, `<-[...]-`, `-[...]-`, `--`, `->`, `<-`.
fn is_relationship_start(tokens: &[CypherToken]) -> bool {
    let mut i = 0;
    while i < tokens.len() && tokens[i].is_trivia() {
        i += 1;
    }
    if i >= tokens.len() {
        return false;
    }
    matches!(
        (tokens[i].kind, tokens[i].text.as_str()),
        (TokenKind::Operator, "-") | (TokenKind::Arrow, "<-") | (TokenKind::Arrow, "->")
    )
}

/// Parse a `(var:Label:Label2 {k: v, …})` node pattern. Returns the
/// pattern and the number of tokens consumed (including the closing `)`).
fn parse_node_pattern(tokens: &[CypherToken]) -> (NodePattern, usize) {
    let mut i = 0usize;
    // Opening `(`
    while i < tokens.len() && tokens[i].is_trivia() {
        i += 1;
    }
    if !(i < tokens.len() && tokens[i].kind == TokenKind::Paren && tokens[i].text == "(") {
        return (NodePattern::default(), 0);
    }
    let start_byte = tokens[i].span.start;
    i += 1;

    let mut variable: Option<String> = None;
    let mut labels: Vec<String> = Vec::new();
    let mut properties: Vec<(String, String)> = Vec::new();

    // Optional variable.
    while i < tokens.len() && tokens[i].is_trivia() {
        i += 1;
    }
    if i < tokens.len()
        && matches!(
            tokens[i].kind,
            TokenKind::Identifier | TokenKind::QuotedIdentifier
        )
    {
        variable = Some(strip_backticks(&tokens[i].text));
        i += 1;
    }

    // Zero or more `:Label` repetitions.
    loop {
        while i < tokens.len() && tokens[i].is_trivia() {
            i += 1;
        }
        if i < tokens.len() && tokens[i].kind == TokenKind::Operator && tokens[i].text == ":" {
            i += 1;
            while i < tokens.len() && tokens[i].is_trivia() {
                i += 1;
            }
            if i < tokens.len()
                && matches!(
                    tokens[i].kind,
                    TokenKind::Identifier | TokenKind::QuotedIdentifier
                )
            {
                labels.push(strip_backticks(&tokens[i].text));
                i += 1;
            }
        } else {
            break;
        }
    }

    // Optional inline property map `{ k: v, ... }`.
    while i < tokens.len() && tokens[i].is_trivia() {
        i += 1;
    }
    if i < tokens.len() && tokens[i].kind == TokenKind::Paren && tokens[i].text == "{" {
        let (props, consumed) = parse_property_map(&tokens[i..]);
        properties = props;
        i += consumed;
    }

    // Consume up to the matching `)`. We balance parens in case something
    // weird sits between us and the closer (function call in value, etc.).
    let mut depth = 1;
    while i < tokens.len() && depth > 0 {
        if tokens[i].kind == TokenKind::Paren {
            if tokens[i].text == "(" {
                depth += 1;
            } else if tokens[i].text == ")" {
                depth -= 1;
                if depth == 0 {
                    i += 1;
                    break;
                }
            }
        }
        i += 1;
    }

    let end_byte = if i == 0 {
        start_byte
    } else {
        tokens[i - 1].span.end
    };

    (
        NodePattern {
            variable,
            labels,
            properties,
            span: Span::new(start_byte, end_byte),
        },
        i,
    )
}

/// Parse a relationship pattern. Supported shapes (Cypher normalises
/// arrows on either side of `[…]`):
///   `-[var:TYPE|TYPE2 *1..3 {k: v}]->`, `-[]-`, `--`, `->`, `<-`.
fn parse_relationship_pattern(tokens: &[CypherToken]) -> (RelationshipPattern, usize) {
    let mut i = 0usize;
    let mut variable: Option<String> = None;
    let mut types: Vec<String> = Vec::new();
    let mut var_length: Option<(Option<u32>, Option<u32>)> = None;
    let mut properties: Vec<(String, String)> = Vec::new();

    while i < tokens.len() && tokens[i].is_trivia() {
        i += 1;
    }
    if i >= tokens.len() {
        return (RelationshipPattern::default(), 0);
    }
    let start_byte = tokens[i].span.start;

    // Left side: `<-` (incoming), `-` (outgoing candidate / undirected).
    let mut left_incoming = false;
    if tokens[i].kind == TokenKind::Arrow && tokens[i].text == "<-" {
        left_incoming = true;
        i += 1;
    } else if tokens[i].kind == TokenKind::Operator && tokens[i].text == "-" {
        i += 1;
    } else {
        return (RelationshipPattern::default(), 0);
    }

    // Optional `[ var:TYPE { ... } ]` spec.
    while i < tokens.len() && tokens[i].is_trivia() {
        i += 1;
    }
    if i < tokens.len() && tokens[i].kind == TokenKind::Paren && tokens[i].text == "[" {
        i += 1;
        // Variable name (optional).
        while i < tokens.len() && tokens[i].is_trivia() {
            i += 1;
        }
        if i < tokens.len()
            && matches!(
                tokens[i].kind,
                TokenKind::Identifier | TokenKind::QuotedIdentifier
            )
        {
            variable = Some(strip_backticks(&tokens[i].text));
            i += 1;
        }
        // Types after `:`. Cypher supports alternatives via `|`.
        while i < tokens.len() && tokens[i].is_trivia() {
            i += 1;
        }
        if i < tokens.len() && tokens[i].kind == TokenKind::Operator && tokens[i].text == ":" {
            i += 1;
            loop {
                while i < tokens.len() && tokens[i].is_trivia() {
                    i += 1;
                }
                if i < tokens.len()
                    && matches!(
                        tokens[i].kind,
                        TokenKind::Identifier | TokenKind::QuotedIdentifier
                    )
                {
                    types.push(strip_backticks(&tokens[i].text));
                    i += 1;
                }
                while i < tokens.len() && tokens[i].is_trivia() {
                    i += 1;
                }
                if i < tokens.len()
                    && tokens[i].kind == TokenKind::Operator
                    && tokens[i].text == "|"
                {
                    i += 1;
                    continue;
                }
                break;
            }
        }
        // Variable-length: `*`, `*3`, `*1..5`, `*..10`, `*2..`.
        while i < tokens.len() && tokens[i].is_trivia() {
            i += 1;
        }
        if i < tokens.len() && tokens[i].kind == TokenKind::Operator && tokens[i].text == "*" {
            i += 1;
            let mut lower: Option<u32> = None;
            let mut upper: Option<u32> = None;
            while i < tokens.len() && tokens[i].is_trivia() {
                i += 1;
            }
            if i < tokens.len() && tokens[i].kind == TokenKind::Number {
                lower = tokens[i].text.parse::<u32>().ok();
                i += 1;
            }
            while i < tokens.len() && tokens[i].is_trivia() {
                i += 1;
            }
            if i < tokens.len() && tokens[i].kind == TokenKind::Operator && tokens[i].text == ".." {
                i += 1;
                while i < tokens.len() && tokens[i].is_trivia() {
                    i += 1;
                }
                if i < tokens.len() && tokens[i].kind == TokenKind::Number {
                    upper = tokens[i].text.parse::<u32>().ok();
                    i += 1;
                }
            } else if lower.is_some() {
                // `*3` → exact bound, upper == lower.
                upper = lower;
            }
            var_length = Some((lower, upper));
        }
        // Optional property map.
        while i < tokens.len() && tokens[i].is_trivia() {
            i += 1;
        }
        if i < tokens.len() && tokens[i].kind == TokenKind::Paren && tokens[i].text == "{" {
            let (props, consumed) = parse_property_map(&tokens[i..]);
            properties = props;
            i += consumed;
        }
        // Advance past `]`, balancing nested brackets defensively.
        let mut depth = 1;
        while i < tokens.len() && depth > 0 {
            if tokens[i].kind == TokenKind::Paren {
                if tokens[i].text == "[" {
                    depth += 1;
                } else if tokens[i].text == "]" {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
            }
            i += 1;
        }
    }

    // Right side: `->` (outgoing), `-` (undirected if left was `-`, incoming
    // if left was `<-`).
    while i < tokens.len() && tokens[i].is_trivia() {
        i += 1;
    }
    let mut right_outgoing = false;
    if i < tokens.len() {
        if tokens[i].kind == TokenKind::Arrow && tokens[i].text == "->" {
            right_outgoing = true;
            i += 1;
        } else if tokens[i].kind == TokenKind::Operator && tokens[i].text == "-" {
            i += 1;
        }
    }

    let direction = match (left_incoming, right_outgoing) {
        (true, false) => RelDirection::Incoming,
        (false, true) => RelDirection::Outgoing,
        _ => RelDirection::Undirected,
    };

    let end_byte = if i == 0 {
        start_byte
    } else {
        tokens[i - 1].span.end
    };

    (
        RelationshipPattern {
            variable,
            types,
            direction,
            var_length,
            properties,
            span: Span::new(start_byte, end_byte),
        },
        i,
    )
}

/// Parse `{ k: expr, k2: expr2 }` into `(key, raw_value_text)` pairs. The
/// value is preserved as raw source text so callers can compare / inspect
/// without our having to parse arbitrary expressions.
fn parse_property_map(tokens: &[CypherToken]) -> (Vec<(String, String)>, usize) {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut i = 0usize;
    while i < tokens.len() && tokens[i].is_trivia() {
        i += 1;
    }
    if !(i < tokens.len() && tokens[i].kind == TokenKind::Paren && tokens[i].text == "{") {
        return (out, 0);
    }
    i += 1;
    loop {
        while i < tokens.len() && tokens[i].is_trivia() {
            i += 1;
        }
        if i >= tokens.len() {
            break;
        }
        if tokens[i].kind == TokenKind::Paren && tokens[i].text == "}" {
            i += 1;
            break;
        }
        // Expect identifier / quoted-identifier key.
        let key = if matches!(
            tokens[i].kind,
            TokenKind::Identifier | TokenKind::QuotedIdentifier
        ) {
            let k = strip_backticks(&tokens[i].text);
            i += 1;
            k
        } else {
            // Can't parse key — skip to next `,` or `}` to recover.
            while i < tokens.len()
                && !(tokens[i].kind == TokenKind::Paren && tokens[i].text == "}")
                && !(tokens[i].kind == TokenKind::Punctuation && tokens[i].text == ",")
            {
                i += 1;
            }
            continue;
        };
        // Consume `:`.
        while i < tokens.len() && tokens[i].is_trivia() {
            i += 1;
        }
        if !(i < tokens.len() && tokens[i].kind == TokenKind::Operator && tokens[i].text == ":") {
            continue;
        }
        i += 1;
        // Capture value tokens until top-level `,` or `}`.
        let value_start_byte = tokens.get(i).map(|t| t.span.start).unwrap_or_default();
        let mut depth_paren = 0i32;
        let mut depth_bracket = 0i32;
        let mut depth_brace = 0i32;
        let mut value_end_token_idx = i;
        while value_end_token_idx < tokens.len() {
            let t = &tokens[value_end_token_idx];
            if t.kind == TokenKind::Paren {
                match t.text.as_str() {
                    "(" => depth_paren += 1,
                    ")" => depth_paren -= 1,
                    "[" => depth_bracket += 1,
                    "]" => depth_bracket -= 1,
                    "{" => depth_brace += 1,
                    "}" => {
                        if depth_brace == 0 {
                            break;
                        }
                        depth_brace -= 1;
                    }
                    _ => {}
                }
            }
            if t.kind == TokenKind::Punctuation
                && t.text == ","
                && depth_paren == 0
                && depth_bracket == 0
                && depth_brace == 0
            {
                break;
            }
            value_end_token_idx += 1;
        }
        let value_end_byte = if value_end_token_idx == i {
            value_start_byte
        } else {
            tokens[value_end_token_idx - 1].span.end
        };
        // Build the raw value text from the original slice range.
        // (We preserve exact source text, including whitespace.)
        let raw_value = if value_end_byte >= value_start_byte {
            // We only have spans; reconstruct by concatenating token texts.
            let mut buf = String::new();
            for t in &tokens[i..value_end_token_idx] {
                buf.push_str(&t.text);
            }
            buf.trim().to_string()
        } else {
            String::new()
        };
        out.push((key, raw_value));
        i = value_end_token_idx;
        // Consume comma if present.
        while i < tokens.len() && tokens[i].is_trivia() {
            i += 1;
        }
        if i < tokens.len() && tokens[i].kind == TokenKind::Punctuation && tokens[i].text == "," {
            i += 1;
        }
    }
    (out, i)
}

fn strip_backticks(s: &str) -> String {
    if s.starts_with('`') && s.ends_with('`') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// SET / REMOVE clause body parsing
// ---------------------------------------------------------------------------

/// Walk a `SET` clause's tokens (keyword already consumed by the
/// caller — but still present in `clause_tokens`) and pull every
/// `<var>.<prop> = <value>` assignment into a [`SetItem`]. Comma-
/// separated assignments parse as multiple items; non-property-target
/// SET forms (`SET n += {…}`, `SET n :Label`) are skipped silently
/// because the SHACL surface only enforces per-property writes.
fn parse_set_items(clause_tokens: &[CypherToken], source: &str) -> Vec<SetItem> {
    let mut items = Vec::new();
    let mut i = 0;
    // Skip the leading SET keyword and any trivia.
    while i < clause_tokens.len()
        && (clause_tokens[i].is_trivia()
            || (clause_tokens[i].kind == TokenKind::Keyword
                && clause_tokens[i].text.eq_ignore_ascii_case("SET")))
    {
        i += 1;
    }
    while i < clause_tokens.len() {
        let segment_start = i;
        // Find the next top-level comma (or end of clause). Track
        // nesting so commas inside maps / parentheses don't split.
        let mut depth_paren = 0i32;
        let mut depth_bracket = 0i32;
        let mut depth_brace = 0i32;
        let mut segment_end = clause_tokens.len();
        while i < clause_tokens.len() {
            let tok = &clause_tokens[i];
            if tok.kind == TokenKind::Paren {
                match tok.text.as_str() {
                    "(" => depth_paren += 1,
                    ")" => depth_paren -= 1,
                    "[" => depth_bracket += 1,
                    "]" => depth_bracket -= 1,
                    "{" => depth_brace += 1,
                    "}" => depth_brace -= 1,
                    _ => {}
                }
            }
            if depth_paren == 0
                && depth_bracket == 0
                && depth_brace == 0
                && tok.kind == TokenKind::Punctuation
                && tok.text == ","
            {
                segment_end = i;
                break;
            }
            i += 1;
        }
        if let Some(item) = parse_set_segment(&clause_tokens[segment_start..segment_end], source) {
            items.push(item);
        }
        // Skip the comma we landed on (if any) before the next segment.
        if i < clause_tokens.len()
            && clause_tokens[i].kind == TokenKind::Punctuation
            && clause_tokens[i].text == ","
        {
            i += 1;
        }
    }
    items
}

fn parse_set_segment(tokens: &[CypherToken], source: &str) -> Option<SetItem> {
    let non_trivia: Vec<&CypherToken> =
        tokens.iter().filter(|t| !t.is_trivia()).collect();
    // Need at least: ident . ident = value
    if non_trivia.len() < 5 {
        return None;
    }
    if !matches!(
        non_trivia[0].kind,
        TokenKind::Identifier | TokenKind::QuotedIdentifier
    ) {
        return None;
    }
    if non_trivia[1].kind != TokenKind::Operator || non_trivia[1].text != "." {
        return None;
    }
    if !matches!(
        non_trivia[2].kind,
        TokenKind::Identifier | TokenKind::QuotedIdentifier
    ) {
        return None;
    }
    // Equality assignment only — `+=` (map merge) targets the whole
    // node, not a single property, so SHACL per-property enforcement
    // doesn't apply.
    if non_trivia[3].kind != TokenKind::Operator || non_trivia[3].text != "=" {
        return None;
    }

    let variable = strip_backticks(&non_trivia[0].text);
    let property = strip_backticks(&non_trivia[2].text);

    // The value is everything from after the `=` to the end of the
    // segment. Take the source slice between the `=` end and the last
    // non-trivia token's end so callers see the original expression
    // verbatim (including parens, function calls, parameters).
    let value_start = non_trivia[3].span.end;
    let value_end = non_trivia.last()?.span.end;
    let value_text = source[value_start..value_end].trim().to_string();

    let span = Span::new(non_trivia[0].span.start, value_end);
    Some(SetItem {
        variable,
        property,
        value_text,
        span,
    })
}

/// Walk a `REMOVE` clause's tokens and pull every `<var>.<prop>`
/// target into a [`RemoveItem`]. Label removals (`REMOVE n:Label`)
/// are skipped — those don't engage the SHACL property-shape surface.
fn parse_remove_items(clause_tokens: &[CypherToken], _source: &str) -> Vec<RemoveItem> {
    let mut items = Vec::new();
    let mut i = 0;
    while i < clause_tokens.len()
        && (clause_tokens[i].is_trivia()
            || (clause_tokens[i].kind == TokenKind::Keyword
                && clause_tokens[i].text.eq_ignore_ascii_case("REMOVE")))
    {
        i += 1;
    }
    while i < clause_tokens.len() {
        let segment_start = i;
        let mut depth_paren = 0i32;
        let mut depth_bracket = 0i32;
        let mut depth_brace = 0i32;
        let mut segment_end = clause_tokens.len();
        while i < clause_tokens.len() {
            let tok = &clause_tokens[i];
            if tok.kind == TokenKind::Paren {
                match tok.text.as_str() {
                    "(" => depth_paren += 1,
                    ")" => depth_paren -= 1,
                    "[" => depth_bracket += 1,
                    "]" => depth_bracket -= 1,
                    "{" => depth_brace += 1,
                    "}" => depth_brace -= 1,
                    _ => {}
                }
            }
            if depth_paren == 0
                && depth_bracket == 0
                && depth_brace == 0
                && tok.kind == TokenKind::Punctuation
                && tok.text == ","
            {
                segment_end = i;
                break;
            }
            i += 1;
        }
        if let Some(item) = parse_remove_segment(&clause_tokens[segment_start..segment_end]) {
            items.push(item);
        }
        if i < clause_tokens.len()
            && clause_tokens[i].kind == TokenKind::Punctuation
            && clause_tokens[i].text == ","
        {
            i += 1;
        }
    }
    items
}

fn parse_remove_segment(tokens: &[CypherToken]) -> Option<RemoveItem> {
    let non_trivia: Vec<&CypherToken> =
        tokens.iter().filter(|t| !t.is_trivia()).collect();
    // Need exactly: ident . ident
    if non_trivia.len() != 3 {
        return None;
    }
    if !matches!(
        non_trivia[0].kind,
        TokenKind::Identifier | TokenKind::QuotedIdentifier
    ) {
        return None;
    }
    if non_trivia[1].kind != TokenKind::Operator || non_trivia[1].text != "." {
        return None;
    }
    if !matches!(
        non_trivia[2].kind,
        TokenKind::Identifier | TokenKind::QuotedIdentifier
    ) {
        return None;
    }
    Some(RemoveItem {
        variable: strip_backticks(&non_trivia[0].text),
        property: strip_backticks(&non_trivia[2].text),
        span: Span::new(non_trivia[0].span.start, non_trivia[2].span.end),
    })
}

// ---------------------------------------------------------------------------
// AST → source rendering
// ---------------------------------------------------------------------------

impl CypherAst {
    /// Reassemble the AST into Cypher source. For an AST produced by
    /// [`parse`], this is a lossless round trip: `render(parse(src)) == src`.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (i, stmt) in self.statements.iter().enumerate() {
            for clause in &stmt.clauses {
                out.push_str(&clause.text);
            }
            if let Some(kind) = self.unions.get(i) {
                // Use a single space separator between clause and UNION —
                // the next statement's leading whitespace (captured inside
                // its first clause's `text`) takes care of the rest.
                match kind {
                    UnionKind::Distinct => out.push_str(" UNION "),
                    UnionKind::All => out.push_str(" UNION ALL "),
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn clause_kinds(stmt: &CypherStatement) -> Vec<ClauseKind> {
        stmt.clauses.iter().map(|c| c.kind).collect()
    }

    #[test]
    fn empty_input_produces_single_empty_statement() {
        let ast = parse("");
        assert_eq!(ast.statements.len(), 1);
        assert!(ast.statements[0].clauses.is_empty());
    }

    #[test]
    fn single_match_return() {
        let ast = parse("MATCH (n:Person) RETURN n");
        assert_eq!(ast.statements.len(), 1);
        assert_eq!(
            clause_kinds(&ast.statements[0]),
            vec![ClauseKind::Match, ClauseKind::Return]
        );
        let match_clause = &ast.statements[0].clauses[0];
        assert_eq!(match_clause.patterns.len(), 1);
        assert_eq!(match_clause.patterns[0].elements.len(), 1);
        if let CypherPatternElement::Node(n) = &match_clause.patterns[0].elements[0] {
            assert_eq!(n.variable.as_deref(), Some("n"));
            assert_eq!(n.labels, vec!["Person".to_string()]);
        } else {
            panic!("expected node element");
        }
    }

    #[test]
    fn optional_match_is_classified() {
        let ast = parse("OPTIONAL MATCH (n) RETURN n");
        assert_eq!(
            clause_kinds(&ast.statements[0]),
            vec![ClauseKind::OptionalMatch, ClauseKind::Return]
        );
    }

    #[test]
    fn detach_delete_is_classified() {
        let ast = parse("MATCH (n) DETACH DELETE n");
        assert_eq!(
            clause_kinds(&ast.statements[0]),
            vec![ClauseKind::Match, ClauseKind::DetachDelete]
        );
    }

    #[test]
    fn order_by_is_single_clause() {
        let ast = parse("MATCH (n) RETURN n ORDER BY n.name LIMIT 10");
        assert_eq!(
            clause_kinds(&ast.statements[0]),
            vec![
                ClauseKind::Match,
                ClauseKind::Return,
                ClauseKind::OrderBy,
                ClauseKind::Limit,
            ]
        );
    }

    #[test]
    fn union_splits_statements() {
        let ast = parse("MATCH (a) RETURN a UNION MATCH (b) RETURN b");
        assert_eq!(ast.statements.len(), 2);
        assert_eq!(ast.unions, vec![UnionKind::Distinct]);
    }

    #[test]
    fn union_all_preserved() {
        let ast = parse("MATCH (a) RETURN a UNION ALL MATCH (b) RETURN b");
        assert_eq!(ast.unions, vec![UnionKind::All]);
    }

    #[test]
    fn multi_label_node_pattern() {
        let ast = parse("MATCH (n:Person:Employee) RETURN n");
        if let CypherPatternElement::Node(n) = &ast.statements[0].clauses[0].patterns[0].elements[0]
        {
            assert_eq!(n.labels, vec!["Person".to_string(), "Employee".to_string()]);
        } else {
            panic!("expected node");
        }
    }

    #[test]
    fn relationship_with_var_length() {
        let ast = parse("MATCH (a)-[:KNOWS*1..3]->(b) RETURN a, b");
        let pattern = &ast.statements[0].clauses[0].patterns[0];
        assert_eq!(pattern.elements.len(), 3);
        if let CypherPatternElement::Relationship(r) = &pattern.elements[1] {
            assert_eq!(r.types, vec!["KNOWS".to_string()]);
            assert_eq!(r.direction, RelDirection::Outgoing);
            assert_eq!(r.var_length, Some((Some(1), Some(3))));
        } else {
            panic!("expected relationship");
        }
    }

    #[test]
    fn relationship_unbounded_var_length() {
        let ast = parse("MATCH (a)-[:R*]->(b) RETURN a, b");
        if let CypherPatternElement::Relationship(r) =
            &ast.statements[0].clauses[0].patterns[0].elements[1]
        {
            assert_eq!(r.var_length, Some((None, None)));
        } else {
            panic!("expected relationship");
        }
    }

    #[test]
    fn relationship_alternative_types() {
        let ast = parse("MATCH (a)-[:KNOWS|LIKES]->(b) RETURN a, b");
        if let CypherPatternElement::Relationship(r) =
            &ast.statements[0].clauses[0].patterns[0].elements[1]
        {
            assert_eq!(r.types, vec!["KNOWS".to_string(), "LIKES".to_string()]);
        } else {
            panic!("expected relationship");
        }
    }

    #[test]
    fn incoming_relationship_direction() {
        let ast = parse("MATCH (a)<-[:R]-(b) RETURN a, b");
        if let CypherPatternElement::Relationship(r) =
            &ast.statements[0].clauses[0].patterns[0].elements[1]
        {
            assert_eq!(r.direction, RelDirection::Incoming);
        } else {
            panic!("expected relationship");
        }
    }

    #[test]
    fn undirected_relationship_direction() {
        let ast = parse("MATCH (a)-[:R]-(b) RETURN a, b");
        if let CypherPatternElement::Relationship(r) =
            &ast.statements[0].clauses[0].patterns[0].elements[1]
        {
            assert_eq!(r.direction, RelDirection::Undirected);
        } else {
            panic!("expected relationship");
        }
    }

    #[test]
    fn node_inline_properties_captured_as_raw_text() {
        let ast = parse("MATCH (n:Person {name: 'Alice', age: 30}) RETURN n");
        if let CypherPatternElement::Node(n) = &ast.statements[0].clauses[0].patterns[0].elements[0]
        {
            let keys: Vec<&str> = n.properties.iter().map(|(k, _)| k.as_str()).collect();
            assert_eq!(keys, vec!["name", "age"]);
            // Value preservation is raw text; quotes included on the
            // literal value.
            assert_eq!(n.properties[0].1, "'Alice'");
            assert_eq!(n.properties[1].1, "30");
        } else {
            panic!("expected node");
        }
    }

    #[test]
    fn multiple_patterns_separated_by_comma() {
        let ast = parse("MATCH (a), (b)-[:R]->(c) RETURN a, b, c");
        assert_eq!(ast.statements[0].clauses[0].patterns.len(), 2);
    }

    #[test]
    fn where_clause_captured_separately_without_patterns() {
        let ast = parse("MATCH (n) WHERE n.age > 30 RETURN n");
        let kinds = clause_kinds(&ast.statements[0]);
        assert_eq!(
            kinds,
            vec![ClauseKind::Match, ClauseKind::Where, ClauseKind::Return]
        );
        // WHERE does not carry patterns.
        assert!(ast.statements[0].clauses[1].patterns.is_empty());
    }

    #[test]
    fn string_literal_with_clause_head_keyword_stays_in_clause() {
        let ast = parse("MATCH (n) WHERE n.name = 'MATCH me' RETURN n");
        // Only one MATCH clause; the quoted MATCH inside the string does
        // not start a second one.
        let match_count = ast.statements[0]
            .clauses
            .iter()
            .filter(|c| c.kind == ClauseKind::Match)
            .count();
        assert_eq!(match_count, 1);
    }

    #[test]
    fn render_round_trips_original_source() {
        let samples = [
            "MATCH (n:Person) RETURN n",
            "OPTIONAL MATCH (n) RETURN n",
            "MATCH (a)-[:R*1..3]->(b) WHERE a.id = $id RETURN a, b",
            "MATCH (n) DETACH DELETE n",
            "MATCH (a) RETURN a UNION MATCH (b) RETURN b",
            "MATCH (a) RETURN a UNION ALL MATCH (b) RETURN b",
            "// comment\nMATCH (x) RETURN x",
            "CREATE (a:A {id: 1, name: 'x'}) SET a.updated = timestamp() RETURN a",
        ];
        for src in samples {
            let ast = parse(src);
            // Only require exact round-trip when the AST didn't drop text;
            // whitespace between statements is represented by leading
            // whitespace of the next clause, which the renderer emits a
            // ` UNION ` separator for. We check a weaker property: parse
            // then render then parse yields the same clause shape.
            let rendered = ast.render();
            let reparsed = parse(&rendered);
            assert_eq!(
                reparsed
                    .statements
                    .iter()
                    .map(|s| s.clauses.iter().map(|c| c.kind).collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
                ast.statements
                    .iter()
                    .map(|s| s.clauses.iter().map(|c| c.kind).collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
                "reparsing rendered form must yield the same clause shape for `{src}`",
            );
        }
    }

    #[test]
    fn node_labels_collected_across_patterns() {
        let ast = parse("MATCH (a:Person)-[:KNOWS]->(b:Employee), (c:Person) RETURN a, b, c");
        assert_eq!(
            ast.node_labels(),
            vec!["Person".to_string(), "Employee".to_string()]
        );
    }

    #[test]
    fn relationship_types_collected_across_patterns() {
        let ast = parse("MATCH (a)-[:KNOWS]->(b)-[:LIKES|HATES]->(c) RETURN a, b, c");
        assert_eq!(
            ast.relationship_types(),
            vec![
                "KNOWS".to_string(),
                "LIKES".to_string(),
                "HATES".to_string()
            ]
        );
    }

    #[test]
    fn has_writes_detects_mutation_clauses() {
        assert!(!parse("MATCH (n) RETURN n").has_writes());
        assert!(parse("CREATE (n:T) RETURN n").has_writes());
        assert!(parse("MATCH (n) SET n.x = 1").has_writes());
        assert!(parse("MATCH (n) DETACH DELETE n").has_writes());
    }
}
