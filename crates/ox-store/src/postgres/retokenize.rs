//! `Retokenizable` postgres impls — one per retrieval surface
//! (`ontology_community_summaries` / `verified_queries` /
//! `knowledge_entries`) plus the `retokenizable_surfaces`
//! helper the publish pipeline iterates.
//!
//! The shape is uniform: each surface carries
//! `(tokenized_text, tokenizer_dict_fingerprint, ...source
//! columns)`; the source-column projection differs per table
//! and the per-row tokenisation runs on it. We keep the
//! generic skeleton in one helper and parameterise by:
//! - SQL `SELECT id, source_text` (per table)
//! - SQL `UPDATE ... SET tokenized_text = $1, tokenizer_dict_fingerprint = $2 WHERE id = $3`
//!   (per table)
//!
//! Stream-and-update keeps memory bounded — we never load the
//! whole stale set into RAM, even for workspaces with tens of
//! thousands of rows.

use async_trait::async_trait;

use ox_core::error::OxResult;
use ox_text::Tokenizer;

use crate::store::Retokenizable;

use super::{PostgresStore, to_ox_error};

/// Concrete adapter — one per retrieval surface — that the
/// `retokenizable_surfaces` helper hands back. The structure
/// captures the surface name + the source-column projection
/// the per-row tokenisation reads, so each call site stays
/// SQL-native (no dynamic table names) while the algorithm
/// stays in one place.
pub struct RetokenizableSurface<'a> {
    store: &'a PostgresStore,
    name: &'static str,
    select_stale: &'static str,
    update_row: &'static str,
}

#[async_trait]
impl<'a> Retokenizable for RetokenizableSurface<'a> {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn retokenize_workspace(
        &self,
        tokenizer: &dyn Tokenizer,
        target_fingerprint: &str,
    ) -> OxResult<usize> {
        super::require_workspace_context()?;

        // Stream stale rows. RLS-bound read scopes to the
        // calling workspace.
        let rows: Vec<(String, String)> = sqlx::query_as(self.select_stale)
            .bind(target_fingerprint)
            .fetch_all(&self.store.pool)
            .await
            .map_err(to_ox_error)?;

        let mut touched = 0_usize;
        for (id, source_text) in rows {
            let tokenized =
                tokenizer
                    .tokenize(&source_text)
                    .map_err(|e| ox_core::error::OxError::Runtime {
                        message: format!(
                            "tokenize failed for `{name}` row {id}: {e}",
                            name = self.name
                        ),
                    })?;
            sqlx::query(self.update_row)
                .bind(&tokenized)
                .bind(target_fingerprint)
                .bind(&id)
                .execute(&self.store.pool)
                .await
                .map_err(to_ox_error)?;
            touched += 1;
        }
        Ok(touched)
    }
}

impl crate::store::RetokenizableStore for PostgresStore {
    fn retokenizable_surfaces(&self) -> Vec<Box<dyn crate::store::Retokenizable + '_>> {
        retokenizable_surfaces_inner(self)
            .into_iter()
            .map(|s| Box::new(s) as Box<dyn crate::store::Retokenizable + '_>)
            .collect()
    }
}

fn retokenizable_surfaces_inner(store: &PostgresStore) -> Vec<RetokenizableSurface<'_>> {
    vec![
        RetokenizableSurface {
            store,
            name: "ontology_community_summaries",
            // `summary` carries the LLM prose that's the
            // primary retrieval anchor; `title` augments. The
            // tokenizer sees the combined text so multi-token
            // phrase matches cross the boundary.
            select_stale: "
                SELECT id::text, title || ' ' || summary
                FROM ontology_community_summaries
                WHERE tokenizer_dict_fingerprint <> $1
            ",
            update_row: "
                UPDATE ontology_community_summaries
                SET tokenized_text = $1, tokenizer_dict_fingerprint = $2
                WHERE id::text = $3
            ",
        },
        RetokenizableSurface {
            store,
            name: "verified_queries",
            select_stale: "
                SELECT id, question
                FROM verified_queries
                WHERE tokenizer_dict_fingerprint <> $1
            ",
            update_row: "
                UPDATE verified_queries
                SET tokenized_text = $1, tokenizer_dict_fingerprint = $2
                WHERE id = $3
            ",
        },
        RetokenizableSurface {
            store,
            name: "knowledge_entries",
            // Knowledge entries persist the failure-driven
            // correction body in `content`; `title` is the
            // concise lead. Same combine-then-tokenize policy
            // as community_summaries.
            select_stale: "
                SELECT id::text, title || ' ' || content
                FROM knowledge_entries
                WHERE tokenizer_dict_fingerprint <> $1
            ",
            update_row: "
                UPDATE knowledge_entries
                SET tokenized_text = $1, tokenizer_dict_fingerprint = $2
                WHERE id::text = $3
            ",
        },
    ]
}
