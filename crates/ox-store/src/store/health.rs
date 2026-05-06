use async_trait::async_trait;

#[async_trait]
pub trait HealthStore: Send + Sync {
    async fn health_check(&self) -> bool;
}
