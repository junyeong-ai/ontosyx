use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;
use ox_core::i18n::LocalizedText;
use ox_query_ir::insight::{InsightDef, InsightId};

use super::{CursorPage, CursorParams};

/// Input bundle for [`InsightStore::create_insight`]. Carries
/// every author-supplied field; `id`, `created_at`, `updated_at`
/// are stamped server-side and returned on the resulting
/// [`InsightDef`].
#[derive(Debug, Clone)]
pub struct CreateInsightInput {
    pub author_id: Uuid,
    pub question: LocalizedText,
    pub description: LocalizedText,
    pub tags: Vec<String>,
    /// `ConceptId` strings — see `InsightDef::concept_anchors`.
    pub concept_anchors: Vec<String>,
    pub query_ir: serde_json::Value,
    pub original_provenance: Option<serde_json::Value>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Input bundle for [`InsightStore::update_insight`]. `expected_updated_at`
/// is the optimistic-CAS handle: a stale write is rejected with
/// `OxError::Conflict` so two concurrent edits cannot silently
/// overwrite each other.
#[derive(Debug, Clone)]
pub struct UpdateInsightInput {
    pub question: LocalizedText,
    pub description: LocalizedText,
    pub tags: Vec<String>,
    pub concept_anchors: Vec<String>,
    pub query_ir: serde_json::Value,
    pub original_provenance: Option<serde_json::Value>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
}

/// Composable filter for [`InsightStore::list_insights`]. Empty is
/// the "all visible" sentinel; populating any axis narrows the
/// result. Axes AND together; values within an axis OR (array
/// overlap on the Postgres side).
#[derive(Debug, Clone, Default)]
pub struct InsightFilter {
    /// Restrict to insights authored by this user. `None` widens to
    /// every visible insight in the workspace (RLS still applies).
    pub author_id: Option<Uuid>,
    /// `ConceptId` strings. When
    /// non-empty, the insight must carry at least one of these
    /// anchors.
    pub concept_anchors: Vec<String>,
    /// Freeform tags (admin shorthand). When non-empty, the insight
    /// must carry at least one of these tags.
    pub tags: Vec<String>,
}

impl InsightFilter {
    /// Convenience: filter scoped to a single author and nothing else.
    pub fn for_author(author_id: Uuid) -> Self {
        Self {
            author_id: Some(author_id),
            ..Self::default()
        }
    }
}

#[async_trait]
pub trait InsightStore: Send + Sync {
    /// Create a new insight. Server stamps a UUID v7 id (timestamp-
    /// ordered, sortable) plus `created_at` / `updated_at = now()`,
    /// returning the materialised row so the caller never has to
    /// re-fetch.
    async fn create_insight(&self, input: CreateInsightInput) -> OxResult<InsightDef>;

    /// Replace an existing insight in place. CAS on
    /// `expected_updated_at` — a stale call yields
    /// `OxError::Conflict` so concurrent admins don't trample each
    /// other. Returns the updated row.
    async fn update_insight(
        &self,
        id: &InsightId,
        input: UpdateInsightInput,
    ) -> OxResult<InsightDef>;

    /// Single-row read by id. `None` when the workspace has no
    /// matching row (RLS filters cross-workspace ids).
    async fn get_insight(&self, id: &InsightId) -> OxResult<Option<InsightDef>>;

    /// Cursor-paginated list, ordered by `updated_at DESC`. The
    /// [`InsightFilter`] composes every dimension the admin UI / API
    /// surface filters by — author scope plus any subset of stable
    /// `concept_anchors` and freeform `tags`.
    /// Each filter slot is array-overlap (`&&`) — non-empty argument
    /// means "any of these"; empty means "don't filter on this
    /// axis". Multiple slots AND together.
    async fn list_insights(
        &self,
        filter: &InsightFilter,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<InsightDef>>;

    /// Delete by id. Returns `true` when a row was removed, `false`
    /// when the id wasn't visible to the caller (already deleted /
    /// cross-workspace).
    async fn delete_insight(&self, id: &InsightId) -> OxResult<bool>;
}
