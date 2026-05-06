//! DB-backed API key management for programmatic access.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::ApiKey;

#[async_trait]
pub trait ApiKeyStore: Send + Sync {
    /// Create a new API key with a server-generated plaintext. The
    /// plaintext is returned to the caller exactly once; only the
    /// SHA-256 hash is persisted.
    ///
    /// `role` must be one of `admin`, `designer`, or `viewer` — the DB
    /// CHECK constraint rejects any other value. Prefer `viewer` as the
    /// default for automation keys and escalate deliberately.
    async fn create_api_key(
        &self,
        label: &str,
        workspace_id: Option<Uuid>,
        created_by: &str,
        role: &str,
    ) -> OxResult<(ApiKey, String)>;

    /// Create an API key whose hash is already computed by the caller.
    /// First-boot bootstrap path: the operator supplies the plaintext
    /// via `OX_AUTH__BOOTSTRAP_KEY`, so the server persists only the
    /// hash and never sees the plaintext after install.
    async fn create_api_key_with_hash(
        &self,
        label: &str,
        workspace_id: Option<Uuid>,
        created_by: &str,
        key_hash: &[u8],
        role: &str,
    ) -> OxResult<ApiKey>;

    /// Look up an API key by SHA-256 hash. Returns `None` if the key is
    /// unknown OR has been revoked.
    async fn find_api_key_by_hash(&self, hash: &[u8]) -> OxResult<Option<ApiKey>>;

    /// List all non-revoked API keys (admin view).
    async fn list_api_keys(&self) -> OxResult<Vec<ApiKey>>;

    /// Mark an API key as revoked. Returns `true` if a row was updated.
    /// Uses the `update_*` verb per the Store naming convention; the
    /// "revoked" qualifier is in the suffix to keep the verb prefix
    /// stable for `find`/`update`/`delete` greppability.
    async fn update_api_key_revoked(&self, id: Uuid) -> OxResult<bool>;
}
