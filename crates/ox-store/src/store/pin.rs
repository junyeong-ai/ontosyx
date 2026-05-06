use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::PinboardItem;

use super::{CursorPage, CursorParams};

#[async_trait]
pub trait PinStore: Send + Sync {
    async fn create_pin(&self, user_id: &str, item: &PinboardItem) -> OxResult<()>;

    async fn list_pins(
        &self,
        user_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<PinboardItem>>;

    async fn delete_pin(&self, user_id: &str, id: Uuid) -> OxResult<bool>;
}
