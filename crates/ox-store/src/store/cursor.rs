//! Cursor-based pagination primitives.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Cursor-based pagination parameters.
/// Cursor is an opaque compound string: "timestamp|uuid".
#[derive(Debug, Clone, Deserialize)]
pub struct CursorParams {
    /// Max items to return (default 50, max 100)
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Opaque cursor from a previous response's `next_cursor`
    pub cursor: Option<String>,
}

fn default_limit() -> u32 {
    50
}

impl CursorParams {
    /// Clamp limit to [1, 100].
    pub fn effective_limit(&self) -> i64 {
        self.limit.clamp(1, 100) as i64
    }

    /// Parse compound cursor "timestamp|uuid" into its parts.
    pub fn cursor_parts(&self) -> Option<(DateTime<Utc>, Uuid)> {
        let s = self.cursor.as_deref()?;
        let (ts_str, id_str) = s.split_once('|')?;
        let ts: DateTime<Utc> = ts_str.parse().ok().or_else(|| {
            tracing::warn!(cursor = s, "Malformed cursor: invalid timestamp");
            None
        })?;
        let id: Uuid = id_str.parse().ok().or_else(|| {
            tracing::warn!(cursor = s, "Malformed cursor: invalid UUID");
            None
        })?;
        Some((ts, id))
    }
}

impl Default for CursorParams {
    fn default() -> Self {
        Self {
            limit: 50,
            cursor: None,
        }
    }
}

/// Cursor-paginated result.
#[derive(Debug, Serialize)]
pub struct CursorPage<T: Serialize> {
    pub items: Vec<T>,
    /// Pass this value as `cursor` in the next request. `None` means no more pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
