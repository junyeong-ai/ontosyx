//! Glossary-driven tokenizer hot-swap pipeline.
//!
//! Every commit-path caller (project draft completion,
//! canonical edit commit, schema-ops adopt / enrichment,
//! auto-enrichment after data load) invokes
//! [`publish_workspace_tokenizer_after_commit`] right after
//! `OntologyVersionStore::commit_version` returns. The helper:
//!
//! 1. Compares the *new* snapshot's
//!    [`ox_text::glossary_tokenizer_fingerprint`] against the
//!    *previous* snapshot's stamped fingerprint.
//! 2. Skips the rebuild when fingerprints match — the glossary's
//!    token-shape is unchanged, the workspace's existing
//!    tokenizer + retrieval index stay current. Most commits
//!    only touch topology / mappings / rules and don't shift
//!    the glossary; the diff makes those commits free.
//! 3. On mismatch:
//!    1. Compiles the IR's glossary into a lindera user dict
//!       CSV (concept-canonicalisation collapses all alias
//!       surfaces to the canonical concept lemma).
//!    2. Builds the binary user dict + assembles a fresh
//!       `KoreanEnglishTokenizer` against the system
//!       `mecab-ko-dic` + the new user dict.
//!    3. Hot-swaps the workspace's
//!       [`ox_text::WorkspaceTokenizerRegistry`] entry via
//!       `ArcSwap`, so in-flight retrieval reads on the prior
//!       tokenizer finish safely and subsequent reads pick up
//!       the new dict.
//!    4. Spawns a workspace-scoped backfill task to
//!       re-tokenize every retrieval surface row whose
//!       `tokenizer_dict_fingerprint` is stale.
//!
//! The helper is **graceful on every failure**:
//!
//! - User dict CSV write failure → log warn, abandon publish
//!   (the prior tokenizer keeps serving — better degraded
//!   recall than no recall).
//! - Backfill task failure → log warn per row, continue.
//!   Stale rows surface in observability via the
//!   `tokenizer_dict_fingerprint != current_fingerprint`
//!   query.
//!
//! No commit-path call is allowed to fail because of a
//! tokenizer build glitch — search infrastructure is observability-
//! grade, not load-bearing for the IR commit itself.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

use ox_core::error::OxResult;
use ox_ontology::OntologyIR;
use ox_store::Store;

use crate::state::AppState;

/// Run the post-commit tokenizer pipeline.
///
/// `parent_version_id` is the snapshot the freshly-committed
/// version branched from (or `None` for the first version of
/// a lineage); the helper fetches its stamped
/// `glossary_tokenizer_fingerprint` and diffs against the new
/// IR's fingerprint. Match → no-op. Mismatch (or absent
/// parent → first commit) → rebuild + publish + backfill.
///
/// Single integration point for every commit-path caller.
/// Pulling diff / rebuild / publish / backfill into one helper
/// keeps the call sites tight and prevents drift between
/// commit paths.
pub async fn publish_workspace_tokenizer_after_commit(
    state: &AppState,
    workspace_id: uuid::Uuid,
    parent_version_id: Option<uuid::Uuid>,
    committed_ir: &OntologyIR,
) -> OxResult<()> {
    let prev_fingerprint = match parent_version_id {
        Some(id) => state
            .store
            .get_version_snapshot(id)
            .await?
            .map(|s| s.glossary_tokenizer_fingerprint)
            .unwrap_or_default(),
        None => String::new(),
    };
    let new_fingerprint = ox_text::glossary_tokenizer_fingerprint(committed_ir);
    if new_fingerprint.as_str() == prev_fingerprint {
        // Token-shape unchanged. Tokenizer stays. No backfill.
        return Ok(());
    }

    let csv = match ox_text::compile_glossary_to_user_dict(committed_ir) {
        Ok(s) => s,
        Err(err) => {
            warn!(
                workspace_id = %workspace_id,
                error = %err,
                "glossary→user-dict CSV compile failed; tokenizer stays on prior dict",
            );
            return Ok(());
        }
    };

    if csv.is_empty() {
        // Empty glossary → run with system dict only. Publish a
        // fresh `system_only` tokenizer so any prior user-dict
        // entries get evicted.
        match ox_text::KoreanEnglishTokenizer::system_only() {
            Ok(tok) => {
                state.tokenizer_registry.publish(workspace_id, tok);
            }
            Err(err) => {
                warn!(
                    workspace_id = %workspace_id,
                    error = %err,
                    "system tokenizer rebuild failed; prior tokenizer retained",
                );
                return Ok(());
            }
        }
    } else {
        let tokenizer = match build_tokenizer_with_user_dict(workspace_id, &csv).await {
            Ok(t) => t,
            Err(err) => {
                warn!(
                    workspace_id = %workspace_id,
                    error = %err,
                    "user-dict tokenizer build failed; prior tokenizer retained",
                );
                return Ok(());
            }
        };
        state.tokenizer_registry.publish(workspace_id, tokenizer);
    }

    info!(
        workspace_id = %workspace_id,
        prev_fingerprint = %prev_fingerprint,
        new_fingerprint = %new_fingerprint.as_str(),
        "workspace tokenizer published; scheduling retokenize backfill",
    );

    // Backfill — re-tokenize every retrieval surface row whose
    // `tokenizer_dict_fingerprint` is stale. Spawned scoped so
    // the inner task inherits the workspace's task-locals
    // (`WORKSPACE_ID`); RLS-bound reads + writes therefore land
    // on the correct tenant.
    let store = Arc::clone(&state.store);
    let tokenizer = state.tokenizer_registry.for_workspace(workspace_id);
    let target_fingerprint = new_fingerprint.as_str().to_string();
    crate::spawn_scoped::spawn_scoped(async move {
        if let Err(err) = run_backfill(
            store.as_ref(),
            tokenizer.as_ref().as_ref(),
            &target_fingerprint,
        )
        .await
        {
            warn!(
                error = %err,
                "tokenizer-dict backfill failed; rows remain on stale fingerprint until next sweep",
            );
        }
    });

    Ok(())
}

/// Build a `KoreanEnglishTokenizer` with the workspace's
/// freshly-compiled user dict.
///
/// Lindera's CSV → binary path requires a filesystem path;
/// we materialise the CSV to a process-local temp file inside
/// `std::env::temp_dir()` per build (workspace id + timestamp
/// uniqueness), feed it to lindera, then drop the file. The
/// dict bytes are owned by the resulting tokenizer.
async fn build_tokenizer_with_user_dict(
    workspace_id: uuid::Uuid,
    csv: &str,
) -> OxResult<ox_text::KoreanEnglishTokenizer> {
    let path = csv_temp_path(workspace_id);
    {
        let mut f =
            tokio::fs::File::create(&path)
                .await
                .map_err(|e| ox_core::error::OxError::Runtime {
                    message: format!("user-dict csv create failed: {e}"),
                })?;
        f.write_all(csv.as_bytes())
            .await
            .map_err(|e| ox_core::error::OxError::Runtime {
                message: format!("user-dict csv write failed: {e}"),
            })?;
        f.flush()
            .await
            .map_err(|e| ox_core::error::OxError::Runtime {
                message: format!("user-dict csv flush failed: {e}"),
            })?;
    }

    // Compile CSV → UserDictionary on a blocking thread —
    // lindera dictionary build does file IO + heavy parsing
    // and would otherwise block the runtime.
    let path_for_blocking = path.clone();
    let tokenizer = tokio::task::spawn_blocking(move || -> OxResult<_> {
        ox_text::KoreanEnglishTokenizer::from_user_dict_csv_path(&path_for_blocking).map_err(|e| {
            ox_core::error::OxError::Runtime {
                message: format!("tokenizer assembly failed: {e}"),
            }
        })
    })
    .await
    .map_err(|e| ox_core::error::OxError::Runtime {
        message: format!("user-dict build join failed: {e}"),
    })??;

    // Best-effort cleanup of the temp CSV. Lindera no longer
    // needs the file once the binary dict is in memory.
    if let Err(err) = tokio::fs::remove_file(&path).await {
        tracing::debug!(
            path = %path.display(),
            error = %err,
            "user-dict temp CSV cleanup failed; left for OS tmp reaper",
        );
    }

    Ok(tokenizer)
}

fn csv_temp_path(workspace_id: uuid::Uuid) -> PathBuf {
    let nanos = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    std::env::temp_dir().join(format!("ontosyx-userdict-{workspace_id}-{nanos}.csv"))
}

/// Stamped on every retrieval-surface row at write time. The
/// pair is what the backfill cron diffs against the workspace's
/// current fingerprint to detect rows that need retokenizing.
#[derive(Debug, Clone)]
pub struct WorkspaceTokens {
    /// Morphologically tokenised projection of the input text
    /// using the workspace's current lindera + glossary
    /// user-dict tokenizer. Drives the GENERATED `searchable_tsv`
    /// column on every retrieval surface.
    pub tokenized_text: String,
    /// SHA-256 of the glossary state that produced the current
    /// tokenizer. Equality with the workspace's current
    /// fingerprint means the row's tokens are still authoritative;
    /// inequality flags it for the next backfill sweep.
    pub tokenizer_dict_fingerprint: String,
}

/// Tokenize a write-path string using the workspace's current
/// tokenizer + return the canonical tokenizer dict fingerprint.
///
/// Greenfield workspaces (no canonical ontology yet) return an
/// empty `tokenizer_dict_fingerprint` — every row stamps the
/// system-only tokenizer's outputs, and the next commit hooks
/// the backfill cron to re-tokenize them.
///
/// On tokenize failure the raw input is preserved so a row never
/// lands indexable-yet-empty.
pub async fn tokenize_for_workspace(
    state: &AppState,
    workspace_id: uuid::Uuid,
    text: &str,
) -> WorkspaceTokens {
    let tokenizer_dict_fingerprint = current_workspace_fingerprint(state).await;
    let tokenizer = state.tokenizer_registry.for_workspace(workspace_id);
    let tokenized_text = match tokenizer.tokenize(text) {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => text.to_string(),
        Err(err) => {
            warn!(
                workspace_id = %workspace_id,
                error = %err,
                "write-path tokenize failed; storing raw surface",
            );
            text.to_string()
        }
    };
    WorkspaceTokens {
        tokenized_text,
        tokenizer_dict_fingerprint,
    }
}

async fn current_workspace_fingerprint(state: &AppState) -> String {
    match state.store.get_workspace_ontology().await {
        Ok(Some(row)) => match state.store.find_current_version(row.id).await {
            Ok(Some(snap)) => snap.glossary_tokenizer_fingerprint,
            _ => String::new(),
        },
        _ => String::new(),
    }
}

async fn run_backfill(
    store: &dyn Store,
    tokenizer: &dyn ox_text::Tokenizer,
    target_fingerprint: &str,
) -> OxResult<()> {
    let mut total: usize = 0;
    for surface in store.retokenizable_surfaces() {
        let touched = surface
            .retokenize_workspace(tokenizer, target_fingerprint)
            .await?;
        info!(surface = surface.name(), touched, "retokenized surface",);
        total += touched;
    }
    info!(
        target_fingerprint = %target_fingerprint,
        rows_retokenized = total,
        "tokenizer-dict backfill complete",
    );
    Ok(())
}
