use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::IdempotencyRecord;

/// Backing store for the `Idempotency-Key` middleware. Records are
/// scoped to `(workspace_id, user_id, method, path, key)` so the
/// same client-supplied key cannot accidentally cross routes or
/// tenants.
#[async_trait]
pub trait IdempotencyStore: Send + Sync {
    /// Look up a prior response for this scope. `Ok(None)` means
    /// the middleware proceeds with the live handler and records
    /// the result on the way out via [`create_idempotency_record`].
    async fn find_idempotency_record(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        method: &str,
        path: &str,
        key: &str,
    ) -> OxResult<Option<IdempotencyRecord>>;

    /// Persist a new response. The PK is `(workspace_id, user_id,
    /// method, path, key)`; concurrent writers race and only one
    /// wins (`ON CONFLICT DO NOTHING`), which is the documented
    /// Stripe behaviour — second writer's response is dropped.
    async fn create_idempotency_record(
        &self,
        record: &IdempotencyRecord,
    ) -> OxResult<()>;

    /// Drop expired rows. The middleware never reads them, so
    /// keeping a backlog only costs disk; the cleanup cron uses
    /// this to keep the table bounded. Returns rows removed for
    /// the metric line.
    async fn delete_expired_idempotency_records(&self) -> OxResult<u64>;
}
