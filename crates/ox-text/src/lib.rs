#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

//! Workspace-aware morphological tokenizer + glossary-driven
//! user-dictionary substrate.
//!
//! # Architecture
//!
//! ```text
//!                Glossary (workspace SSOT) ──┐
//!                                            │ commit_version hook
//!                                            ▼
//!  WorkspaceTokenizerRegistry  ◄─── glossary→user_dict CSV compiler
//!  (Lazy + LRU + ArcSwap)             (Phase 0d)
//!         │
//!         │ Arc<dyn Tokenizer> per workspace
//!         ▼
//!  Index path  /  Query path  /  Promote path
//!         │
//!         ▼
//!     tokenized_text
//!     └─ tsvector GENERATED → GIN
//! ```
//!
//! Same `Arc<dyn Tokenizer>` is consumed by index-time and
//! query-time call sites — recall consistency guaranteed by
//! construction (one function, two callsites).
//!
//! # Korean ↔ Multi-language
//!
//! `LinderaTokenizer` ships with `mecab-ko-dic` embedded
//! (lindera `embed-ko-dic` feature). The trait surface
//! abstracts the engine — Japanese / Chinese / Vietnamese
//! workspaces register a different impl on the same registry
//! without touching consumers.
//!
//! # Glossary as SSOT
//!
//! [`compile_glossary_to_user_dict`] converts the workspace's
//! `GlossaryTermDef` set into a lindera user dictionary CSV.
//! Each term's surfaces (default + translations) are recognised
//! as compounds; `term_pos` drives the POS tag; `concept_id`
//! drives canonical-lemma collapse so synonym terms (LTV ≡
//! 고객 생애 가치 ≡ Customer Lifetime Value) tokenize to a
//! single canonical token, giving the tsvector ranker
//! synonym-recall at the lexical layer.
//!
//! # Determinism
//!
//! [`glossary_tokenizer_fingerprint`] computes a stable sha256
//! over tokenizer-relevant glossary state (ignoring metadata
//! that doesn't affect tokenization — descriptions, examples,
//! audit timestamps). The commit-path hook diffs this
//! fingerprint against the previous version's stamp and
//! short-circuits the rebuild when the glossary's token-shape
//! is unchanged.

mod fingerprint;
mod glossary_dict;
mod registry;
mod tokenizer;

pub use fingerprint::{GlossaryFingerprint, glossary_tokenizer_fingerprint};
pub use glossary_dict::{UserDictCompileError, compile_glossary_to_user_dict};
pub use registry::{RegistryConfig, RegistryError, WorkspaceTokenizerRegistry};
pub use tokenizer::{
    KoreanEnglishTokenizer, PassthroughTokenizer, Token, TokenizeError, Tokenizer,
};
