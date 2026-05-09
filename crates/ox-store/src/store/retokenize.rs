//! Retokenization contract for workspace-scoped retrieval
//! surfaces.
//!
//! Every store that persists `tokenized_text` +
//! `tokenizer_dict_fingerprint` columns implements
//! [`Retokenizable`]. The commit-path tokenizer publish
//! pipeline (in `ox-api::tokenizer_publish`) iterates
//! [`retokenizable_surfaces`] over the active store and
//! invokes each impl to refresh stale rows after a glossary
//! change reshapes the workspace's user dictionary.
//!
//! The trait is intentionally narrow — index operations
//! (insert / update at write time) live on each surface's
//! domain-specific store; retokenization is the cross-cutting
//! "make these rows match the current tokenizer dict" job.

use async_trait::async_trait;

use ox_core::error::OxResult;
use ox_text::Tokenizer;

/// Retokenization surface contract.
///
/// `target_fingerprint` is the tokenizer dict fingerprint the
/// rows must end up matching after the call. Rows already at
/// `target_fingerprint` are skipped — only stale rows pay the
/// per-row tokenize + UPDATE cost.
///
/// Returns the number of rows actually retokenized — surfaces
/// in observability so operators see how many surfaces needed
/// realignment after a glossary change.
#[async_trait]
pub trait Retokenizable: Send + Sync {
    fn name(&self) -> &'static str;

    async fn retokenize_workspace(
        &self,
        tokenizer: &dyn Tokenizer,
        target_fingerprint: &str,
    ) -> OxResult<usize>;
}

/// Surface roster — every retrieval table that carries a
/// `tokenized_text` + `tokenizer_dict_fingerprint` pair. Adding
/// a new retrieval surface = one entry in the impl + the
/// matching schema columns + index. The publish pipeline picks
/// up the new surface automatically.
pub trait RetokenizableStore: Send + Sync {
    fn retokenizable_surfaces(&self) -> Vec<Box<dyn Retokenizable + '_>>;
}
