//! Saved canvas-editable PatternIR — positions + zoom preserved.
//!
//! The payload is the PatternIR itself rather than a compiled
//! QueryIR so a reopen restores the user's node layout and viewport
//! without re-layout.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::SavedQueryPattern;

use super::{CursorPage, CursorParams};

#[async_trait]
pub trait PatternStore: Send + Sync {
    async fn create_pattern(&self, pattern: &SavedQueryPattern) -> OxResult<()>;
    async fn get_pattern(&self, id: Uuid) -> OxResult<Option<SavedQueryPattern>>;
    async fn list_patterns(
        &self,
        user_id: &str,
        ontology_lineage_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<SavedQueryPattern>>;
    async fn update_pattern(
        &self,
        id: Uuid,
        name: &str,
        description: Option<&str>,
        pattern_ir: &serde_json::Value,
    ) -> OxResult<bool>;
    async fn delete_pattern(&self, id: Uuid) -> OxResult<bool>;
}
