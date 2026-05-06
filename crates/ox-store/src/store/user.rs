use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::User;

use super::{CursorPage, CursorParams};

#[async_trait]
pub trait UserStore: Send + Sync {
    /// Insert or update a user (matched by provider + provider_sub).
    /// On conflict, updates name, picture, and last_login_at.
    async fn upsert_user(&self, user: &User) -> OxResult<User>;

    async fn get_user_by_id(&self, id: Uuid) -> OxResult<Option<User>>;

    async fn find_user_by_provider(
        &self,
        provider: &str,
        provider_sub: &str,
    ) -> OxResult<Option<User>>;

    async fn list_users(&self, pagination: &CursorParams) -> OxResult<CursorPage<User>>;

    async fn update_user_role(&self, id: Uuid, role: &str) -> OxResult<()>;

    async fn count_users(&self) -> OxResult<i64>;

    /// Read just the `token_version` column. The `require_auth`
    /// middleware uses this on every JWT request to detect bulk
    /// invalidation; surface it as a narrow lookup so a 30-second
    /// cache can sit in front without dragging the whole `User`
    /// row's lifecycle around.
    async fn get_user_token_version(&self, id: Uuid) -> OxResult<Option<i64>>;

    /// Increment `token_version` and return the new value. Atomic
    /// (`UPDATE ... SET token_version = token_version + 1
    /// RETURNING ...`) so concurrent calls remain monotonic. Use
    /// when retiring every issued JWT for the user — role downgrade,
    /// password reset, suspected credential theft.
    async fn increment_user_token_version(&self, id: Uuid) -> OxResult<i64>;
}
