//! Cypher tokenizer.
//!
//! A single-pass, zero-dependency scanner that classifies each input
//! character into one of a small set of [`CypherToken`] variants. The
//! tokenizer is not Cypher-aware in the grammatical sense — it only knows
//! enough to keep string literals, comments, backtick-quoted identifiers,
//! and bracket types apart from "raw" keyword / identifier text.
//!
//! Why not reuse a parser crate? openCypher is sprawling, and most of its
//! grammar (expression precedence, numeric literal exponents, backtick
//! escape rules) is irrelevant to isolation / ACL / safety rewriters. We
//! need four things the string-based predecessor got wrong:
//!
//! 1. Don't treat `MATCH` inside a string literal as a keyword.
//! 2. Don't mistake `//` comments for Cypher text.
//! 3. Recognise bracket kind so we can pair `(...)` / `[...]` / `{...}`.
//! 4. Track byte spans so diagnostics can point at the offending source.
//!
//! The tokenizer produces tokens including whitespace and comments so the
//! parser / renderer can reproduce the original source byte-for-byte.
//! Callers that want to skip layout tokens should filter with
//! [`CypherToken::is_trivia`].

use std::fmt;

/// A byte-range within the original input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Slice the source using this span. Caller must pass the original
    /// input the tokens were derived from.
    pub fn slice<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

/// A classified lexical token.
///
/// `text` is the verbatim source slice the token covers — keeping it on
/// the token lets the renderer round-trip formatting exactly, and lets
/// validators surface the offending substring in diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CypherToken {
    pub kind: TokenKind,
    pub text: String,
    pub span: Span,
}

impl CypherToken {
    pub fn new(kind: TokenKind, text: impl Into<String>, span: Span) -> Self {
        Self {
            kind,
            text: text.into(),
            span,
        }
    }

    /// Whitespace or comment — not semantically significant to parsers
    /// but preserved on the token stream for lossless rendering.
    pub fn is_trivia(&self) -> bool {
        matches!(self.kind, TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment)
    }

    /// Is this token a keyword equal (case-insensitively) to `kw`?
    pub fn is_keyword(&self, kw: &str) -> bool {
        matches!(self.kind, TokenKind::Keyword) && self.text.eq_ignore_ascii_case(kw)
    }
}

/// Coarse category of a Cypher token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Uppercase-folded identifier matching the [`KEYWORDS`] table
    /// (`MATCH`, `WHERE`, `CREATE`, …). The token's `text` preserves
    /// original casing so round-tripping is lossless.
    Keyword,
    /// Bare identifier (variable, label, property key, function name).
    Identifier,
    /// Backtick-quoted identifier: `` `odd name` ``. The backticks are
    /// part of `text`; strip them to recover the logical name.
    QuotedIdentifier,
    /// Single- or double-quoted string literal. Quotes are part of `text`.
    StringLiteral,
    /// Parameter reference: `$name` or `$0`. Text includes the `$`.
    Parameter,
    /// Integer or floating-point literal.
    Number,
    /// One of `(`, `)`, `[`, `]`, `{`, `}`.
    Paren,
    /// Multi-character operators (`<=`, `>=`, `<>`, `=~`) or single-char
    /// operators that aren't punctuation (`=`, `<`, `>`, `+`, `-`, `*`,
    /// `/`, `%`, `!`, `:`, `.`, `|`).
    Operator,
    /// Structural punctuation: `,` or `;`.
    Punctuation,
    /// Arrow heads (`->`, `<-`) used in relationship patterns.
    Arrow,
    /// `//` line comment, text includes leading `//`, excludes the trailing newline.
    LineComment,
    /// `/* … */` block comment, text includes delimiters.
    BlockComment,
    /// ASCII whitespace run.
    Whitespace,
    /// Catch-all for characters we don't specifically classify. Rare; the
    /// renderer still emits them verbatim.
    Unknown,
}

/// Every Cypher keyword we recognise. Case-insensitive match. Order is
/// insertion order — callers that need longest-match-first should sort.
///
/// Not exhaustive: we only list keywords that (a) head a clause, (b) show
/// up in pattern syntax, or (c) are relevant to safety validators. Adding
/// a new keyword is a one-line change and does not require touching the
/// parser unless a new clause kind needs structured payload.
pub const KEYWORDS: &[&str] = &[
    // Clause heads
    "MATCH",
    "OPTIONAL",
    "CREATE",
    "MERGE",
    "WHERE",
    "SET",
    "DELETE",
    "DETACH",
    "REMOVE",
    "RETURN",
    "WITH",
    "UNWIND",
    "CALL",
    "YIELD",
    "UNION",
    "ALL",
    "ORDER",
    "BY",
    "SKIP",
    "LIMIT",
    "FOREACH",
    // Logical operators (expression-level; we tokenise as keyword for
    // easier validator scanning, not for grammar decisions)
    "AND",
    "OR",
    "NOT",
    "XOR",
    "IN",
    "IS",
    "NULL",
    "TRUE",
    "FALSE",
    "AS",
    "DISTINCT",
    "ASC",
    "ASCENDING",
    "DESC",
    "DESCENDING",
    "ON",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    // Schema / admin — listed so safety validators can flag them
    "DROP",
    "CONSTRAINT",
    "INDEX",
    "USING",
];

/// Tokenise `input` into a `Vec<CypherToken>`. Always succeeds; unknown
/// characters become [`TokenKind::Unknown`] tokens. The returned tokens
/// cover every byte of the input, and concatenating their `text` fields
/// reproduces the original source.
pub fn tokenize(input: &str) -> Vec<CypherToken> {
    let bytes = input.as_bytes();
    let mut out: Vec<CypherToken> = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let start = i;
        let b = bytes[i];

        // Whitespace run.
        if b.is_ascii_whitespace() {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            out.push(CypherToken::new(
                TokenKind::Whitespace,
                &input[start..i],
                Span::new(start, i),
            ));
            continue;
        }

        // Line comment `// …` up to end-of-line (newline excluded).
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            out.push(CypherToken::new(
                TokenKind::LineComment,
                &input[start..i],
                Span::new(start, i),
            ));
            continue;
        }

        // Block comment `/* … */` — spans lines; terminated by `*/` or EOF.
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            if i + 1 < bytes.len() {
                i += 2; // consume closing */
            }
            out.push(CypherToken::new(
                TokenKind::BlockComment,
                &input[start..i],
                Span::new(start, i),
            ));
            continue;
        }

        // String literals: single or double quoted, backslash-escape aware.
        if b == b'\'' || b == b'"' {
            let quote = b;
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < bytes.len() {
                i += 1; // closing quote
            }
            out.push(CypherToken::new(
                TokenKind::StringLiteral,
                &input[start..i],
                Span::new(start, i),
            ));
            continue;
        }

        // Backtick-quoted identifier.
        if b == b'`' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'`' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            out.push(CypherToken::new(
                TokenKind::QuotedIdentifier,
                &input[start..i],
                Span::new(start, i),
            ));
            continue;
        }

        // Parameter reference `$name` or `$0`.
        if b == b'$' {
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
            {
                i += 1;
            }
            out.push(CypherToken::new(
                TokenKind::Parameter,
                &input[start..i],
                Span::new(start, i),
            ));
            continue;
        }

        // Number (with optional decimal and exponent).
        if b.is_ascii_digit() || (b == b'.' && next_is_digit(bytes, i + 1)) {
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' && next_is_digit(bytes, i + 1) {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                i += 1;
                if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                    i += 1;
                }
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push(CypherToken::new(
                TokenKind::Number,
                &input[start..i],
                Span::new(start, i),
            ));
            continue;
        }

        // Arrow heads: `->`, `<-`, `<->` (left-angle is ambiguous with <= / <>,
        // disambiguate via lookahead).
        if b == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'>' {
            i += 2;
            out.push(CypherToken::new(
                TokenKind::Arrow,
                "->",
                Span::new(start, i),
            ));
            continue;
        }
        if b == b'<' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            i += 2;
            out.push(CypherToken::new(
                TokenKind::Arrow,
                "<-",
                Span::new(start, i),
            ));
            continue;
        }

        // Parens / brackets / braces.
        if matches!(b, b'(' | b')' | b'[' | b']' | b'{' | b'}') {
            i += 1;
            out.push(CypherToken::new(
                TokenKind::Paren,
                &input[start..i],
                Span::new(start, i),
            ));
            continue;
        }

        // Structural punctuation.
        if matches!(b, b',' | b';') {
            i += 1;
            out.push(CypherToken::new(
                TokenKind::Punctuation,
                &input[start..i],
                Span::new(start, i),
            ));
            continue;
        }

        // Multi-char operators: `<=`, `>=`, `<>`, `=~`, `..`.
        if let Some(op) = scan_multi_char_operator(bytes, i) {
            i += op.len();
            out.push(CypherToken::new(
                TokenKind::Operator,
                op,
                Span::new(start, i),
            ));
            continue;
        }

        // Single-char operator-ish bytes.
        if matches!(
            b,
            b'=' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%' | b'!' | b':' | b'.' | b'|' | b'^'
        ) {
            i += 1;
            out.push(CypherToken::new(
                TokenKind::Operator,
                &input[start..i],
                Span::new(start, i),
            ));
            continue;
        }

        // Identifier / keyword.
        if b.is_ascii_alphabetic() || b == b'_' {
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
            {
                i += 1;
            }
            let text = &input[start..i];
            let kind = if is_keyword(text) {
                TokenKind::Keyword
            } else {
                TokenKind::Identifier
            };
            out.push(CypherToken::new(kind, text, Span::new(start, i)));
            continue;
        }

        // Unknown byte — preserve as Unknown so round-trip stays lossless.
        // Advance by the full UTF-8 character width so we never land
        // inside a multi-byte code point when slicing the source.
        let ch_len = input[i..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(1);
        i += ch_len;
        out.push(CypherToken::new(
            TokenKind::Unknown,
            &input[start..i],
            Span::new(start, i),
        ));
    }

    out
}

fn next_is_digit(bytes: &[u8], idx: usize) -> bool {
    idx < bytes.len() && bytes[idx].is_ascii_digit()
}

fn scan_multi_char_operator(bytes: &[u8], i: usize) -> Option<&'static str> {
    let rest = &bytes[i..];
    ["<=", ">=", "<>", "=~", "..", ":="]
        .into_iter()
        .find(|cand| rest.starts_with(cand.as_bytes()))
}

fn is_keyword(text: &str) -> bool {
    KEYWORDS.iter().any(|kw| text.eq_ignore_ascii_case(kw))
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Keyword => f.write_str("keyword"),
            TokenKind::Identifier => f.write_str("identifier"),
            TokenKind::QuotedIdentifier => f.write_str("quoted-identifier"),
            TokenKind::StringLiteral => f.write_str("string"),
            TokenKind::Parameter => f.write_str("parameter"),
            TokenKind::Number => f.write_str("number"),
            TokenKind::Paren => f.write_str("paren"),
            TokenKind::Operator => f.write_str("operator"),
            TokenKind::Punctuation => f.write_str("punctuation"),
            TokenKind::Arrow => f.write_str("arrow"),
            TokenKind::LineComment => f.write_str("line-comment"),
            TokenKind::BlockComment => f.write_str("block-comment"),
            TokenKind::Whitespace => f.write_str("whitespace"),
            TokenKind::Unknown => f.write_str("unknown"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(tokens: &[CypherToken]) -> Vec<TokenKind> {
        tokens.iter().map(|t| t.kind).collect()
    }

    fn texts(tokens: &[CypherToken]) -> Vec<&str> {
        tokens.iter().map(|t| t.text.as_str()).collect()
    }

    /// Every tokenizer run must be lossless: concatenating `text` rebuilds input.
    fn assert_lossless(input: &str) {
        let tokens = tokenize(input);
        let rebuilt: String = tokens.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(rebuilt, input, "tokenizer must be lossless");
    }

    #[test]
    fn empty_input_produces_no_tokens() {
        assert!(tokenize("").is_empty());
    }

    #[test]
    fn whitespace_is_captured_as_a_single_token() {
        let toks = tokenize("  \n\t");
        assert_eq!(kinds(&toks), vec![TokenKind::Whitespace]);
        assert_eq!(toks[0].text, "  \n\t");
    }

    #[test]
    fn line_comment_runs_to_newline_exclusive() {
        let toks = tokenize("// hi\nMATCH");
        assert_eq!(
            kinds(&toks),
            vec![TokenKind::LineComment, TokenKind::Whitespace, TokenKind::Keyword]
        );
        assert_eq!(toks[0].text, "// hi");
    }

    #[test]
    fn block_comment_spans_lines() {
        let toks = tokenize("/* multi\nline */MATCH");
        assert_eq!(
            kinds(&toks),
            vec![TokenKind::BlockComment, TokenKind::Keyword]
        );
        assert_eq!(toks[0].text, "/* multi\nline */");
    }

    #[test]
    fn string_literal_with_embedded_keyword_stays_literal() {
        let toks = tokenize("WHERE n.name = 'MATCH OPTIONAL'");
        let keywords: Vec<&str> = toks
            .iter()
            .filter(|t| t.kind == TokenKind::Keyword)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(
            keywords,
            vec!["WHERE"],
            "tokens inside a string literal must not be classified as keywords"
        );
        let string_tokens: Vec<&str> = toks
            .iter()
            .filter(|t| t.kind == TokenKind::StringLiteral)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(string_tokens, vec!["'MATCH OPTIONAL'"]);
    }

    #[test]
    fn double_quoted_string_with_escaped_quote() {
        let toks = tokenize(r#"SET n.name = "He said \"hi\""#);
        assert!(
            toks.iter().any(|t| t.kind == TokenKind::StringLiteral
                && t.text == "\"He said \\\"hi\\\"")
        );
    }

    #[test]
    fn backtick_quoted_identifier() {
        let toks = tokenize("MATCH (`odd name`:Label)");
        assert!(toks.iter().any(|t| t.kind == TokenKind::QuotedIdentifier
            && t.text == "`odd name`"));
    }

    #[test]
    fn parameter_reference_keeps_dollar_sign() {
        let toks = tokenize("$_ws_id");
        assert_eq!(kinds(&toks), vec![TokenKind::Parameter]);
        assert_eq!(toks[0].text, "$_ws_id");
    }

    #[test]
    fn number_with_decimal_and_exponent() {
        for src in ["42", "3.14", "1e10", "1.5E-3", ".5"] {
            let toks = tokenize(src);
            assert_eq!(kinds(&toks), vec![TokenKind::Number], "input `{src}`");
            assert_eq!(toks[0].text, src);
        }
    }

    #[test]
    fn arrow_heads_recognised_separately_from_operators() {
        let toks = tokenize("(a)-[:X]->(b)");
        let arrows: Vec<&str> = toks
            .iter()
            .filter(|t| t.kind == TokenKind::Arrow)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(arrows, vec!["->"]);
    }

    #[test]
    fn multi_char_operators_are_scanned_atomically() {
        let toks = tokenize("a <= b >= c <> d");
        let ops: Vec<&str> = toks
            .iter()
            .filter(|t| t.kind == TokenKind::Operator)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(ops, vec!["<=", ">=", "<>"]);
    }

    #[test]
    fn parens_and_brackets_are_distinct_tokens() {
        let toks = tokenize("([{}])");
        assert_eq!(texts(&toks), vec!["(", "[", "{", "}", "]", ")"]);
        assert!(toks.iter().all(|t| t.kind == TokenKind::Paren));
    }

    #[test]
    fn identifier_vs_keyword_case_insensitive() {
        let toks = tokenize("match Match MATCH matchmaker");
        let kinds_: Vec<TokenKind> = toks
            .iter()
            .filter(|t| !t.is_trivia())
            .map(|t| t.kind)
            .collect();
        assert_eq!(
            kinds_,
            vec![
                TokenKind::Keyword,
                TokenKind::Keyword,
                TokenKind::Keyword,
                TokenKind::Identifier, // matchmaker — not a keyword
            ]
        );
    }

    #[test]
    fn identifier_with_underscore_and_digits() {
        let toks = tokenize("_ws_id node123 _42abc");
        let idents: Vec<&str> = toks
            .iter()
            .filter(|t| t.kind == TokenKind::Identifier)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(idents, vec!["_ws_id", "node123", "_42abc"]);
    }

    #[test]
    fn unknown_byte_survives_as_unknown_token() {
        let toks = tokenize("a\u{00A0}b");
        assert!(toks.iter().any(|t| t.kind == TokenKind::Unknown));
    }

    #[test]
    fn is_keyword_helper_is_case_insensitive() {
        let toks = tokenize("Match");
        assert_eq!(toks.len(), 1);
        assert!(toks[0].is_keyword("MATCH"));
        assert!(toks[0].is_keyword("match"));
    }

    #[test]
    fn lossless_on_mixed_query() {
        let samples = [
            "MATCH (n:Person) WHERE n.name = 'foo' RETURN n",
            "CREATE (a:A {id:1}) SET a.n = $n",
            "// comment\nMATCH (x) RETURN x // trailing\n",
            "MATCH (a)-[:R*1..5]->(b) RETURN a, b",
            "CALL { MATCH (x) RETURN x } RETURN x",
            "MATCH (n) RETURN n UNION ALL MATCH (m) RETURN m",
            "",
        ];
        for s in samples {
            assert_lossless(s);
        }
    }

    #[test]
    fn span_points_into_original_source() {
        let src = "MATCH (n)";
        let toks = tokenize(src);
        for tok in &toks {
            assert_eq!(tok.span.slice(src), tok.text);
        }
    }
}
