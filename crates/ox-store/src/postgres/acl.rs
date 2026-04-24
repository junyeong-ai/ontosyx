//! [`AclStore`] — workspace-scoped ACL policies (subject × resource × action), priority-sorted.

use super::*;

#[async_trait]
impl AclStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_acl_policy(&self, p: &AclPolicy) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO acl_policies
             (id, name, description, subject_type, subject_value,
              resource_type, resource_value, action, properties,
              mask_pattern, priority, is_active, created_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(p.id)
        .bind(&p.name)
        .bind(&p.description)
        .bind(&p.subject_type)
        .bind(&p.subject_value)
        .bind(&p.resource_type)
        .bind(&p.resource_value)
        .bind(&p.action)
        .bind(&p.properties)
        .bind(&p.mask_pattern)
        .bind(p.priority)
        .bind(p.is_active)
        .bind(p.created_by)
        .bind(p.created_at)
        .bind(p.updated_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_acl_policy(&self, id: Uuid) -> OxResult<Option<AclPolicy>> {
        sqlx::query_as("SELECT * FROM acl_policies WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_acl_policies(
        &self,
        subject_type: Option<&str>,
        resource_value: Option<&str>,
    ) -> OxResult<Vec<AclPolicy>> {
        // Build dynamic query based on optional filters
        match (subject_type, resource_value) {
            (Some(st), Some(rv)) => sqlx::query_as(
                "SELECT * FROM acl_policies
                     WHERE is_active = true AND subject_type = $1 AND resource_value = $2
                     ORDER BY priority DESC, name",
            )
            .bind(st)
            .bind(rv)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error),
            (Some(st), None) => sqlx::query_as(
                "SELECT * FROM acl_policies
                     WHERE is_active = true AND subject_type = $1
                     ORDER BY priority DESC, name",
            )
            .bind(st)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error),
            (None, Some(rv)) => sqlx::query_as(
                "SELECT * FROM acl_policies
                     WHERE is_active = true AND resource_value = $1
                     ORDER BY priority DESC, name",
            )
            .bind(rv)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error),
            (None, None) => sqlx::query_as(
                "SELECT * FROM acl_policies
                     WHERE is_active = true
                     ORDER BY priority DESC, name",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error),
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn update_acl_policy(
        &self,
        id: Uuid,
        name: &str,
        action: &str,
        properties: Option<&[String]>,
        mask_pattern: Option<&str>,
        priority: i32,
        is_active: bool,
    ) -> OxResult<()> {
        let result = sqlx::query(
            "UPDATE acl_policies
             SET name = $2, action = $3, properties = $4, mask_pattern = $5,
                 priority = $6, is_active = $7, updated_at = NOW()
             WHERE id = $1",
        )
        .bind(id)
        .bind(name)
        .bind(action)
        .bind(properties)
        .bind(mask_pattern)
        .bind(priority)
        .bind(is_active)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("ACL policy {id}"),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_acl_policy(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM acl_policies WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_effective_policies(
        &self,
        platform_role: &str,
        workspace_role: &str,
        user_id: Option<Uuid>,
    ) -> OxResult<Vec<AclPolicy>> {
        if let Some(uid) = user_id {
            sqlx::query_as(
                "SELECT * FROM acl_policies
                 WHERE is_active = true AND (
                     (subject_type = 'role' AND subject_value = $1)
                     OR (subject_type = 'workspace_role' AND subject_value = $2)
                     OR (subject_type = 'user' AND subject_value = $3)
                 )
                 ORDER BY priority DESC",
            )
            .bind(platform_role)
            .bind(workspace_role)
            .bind(uid.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)
        } else {
            sqlx::query_as(
                "SELECT * FROM acl_policies
                 WHERE is_active = true AND (
                     (subject_type = 'role' AND subject_value = $1)
                     OR (subject_type = 'workspace_role' AND subject_value = $2)
                 )
                 ORDER BY priority DESC",
            )
            .bind(platform_role)
            .bind(workspace_role)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)
        }
    }
}
