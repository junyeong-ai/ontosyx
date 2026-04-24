//! [`HealthStore`] — readiness probe — a 1-row SELECT to confirm the connection.

use super::*;

#[async_trait]
impl HealthStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn health_check(&self) -> bool {
        sqlx::query("SELECT 1").execute(&self.pool).await.is_ok()
    }
}
