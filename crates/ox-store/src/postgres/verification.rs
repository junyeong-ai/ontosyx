//! [`VerificationStore`] — ontology element verifications (who verified what, when, and invalidations).

use super::*;

#[async_trait]
impl VerificationStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn verify_element(&self, v: &ElementVerification) -> OxResult<Uuid> {
        let row: (Uuid,) = sqlx::query_as(
            "INSERT INTO ontology_verifications
             (ontology_lineage_id, element_id, element_kind, verified_by, review_notes)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (ontology_lineage_id, element_id, verified_by)
                WHERE invalidated_at IS NULL
             DO UPDATE SET review_notes = EXCLUDED.review_notes
             RETURNING id",
        )
        .bind(&v.ontology_lineage_id)
        .bind(&v.element_id)
        .bind(&v.element_kind)
        .bind(v.verified_by)
        .bind(&v.review_notes)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.0)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_verifications(
        &self,
        ontology_lineage_id: &str,
    ) -> OxResult<Vec<ElementVerification>> {
        sqlx::query_as(
            "SELECT v.id, v.ontology_lineage_id, v.element_id, v.element_kind,
                    v.verified_by, COALESCE(u.name, u.email) AS verified_by_name,
                    v.review_notes, v.invalidated_at, v.invalidation_reason, v.created_at
             FROM ontology_verifications v
             LEFT JOIN users u ON u.id = v.verified_by
             WHERE v.ontology_lineage_id = $1 AND v.invalidated_at IS NULL
             ORDER BY v.created_at DESC",
        )
        .bind(ontology_lineage_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn invalidate_for_elements(
        &self,
        ontology_lineage_id: &str,
        element_ids: &[&str],
        reason: &str,
    ) -> OxResult<u64> {
        let result = sqlx::query(
            "UPDATE ontology_verifications
             SET invalidated_at = NOW(), invalidation_reason = $3
             WHERE ontology_lineage_id = $1
               AND element_id = ANY($2)
               AND invalidated_at IS NULL",
        )
        .bind(ontology_lineage_id)
        .bind(element_ids)
        .bind(reason)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_verification(
        &self,
        ontology_lineage_id: &str,
        element_id: &str,
        user_id: Uuid,
    ) -> OxResult<bool> {
        let result = sqlx::query(
            "UPDATE ontology_verifications
             SET invalidated_at = NOW(), invalidation_reason = 'manually_revoked'
             WHERE ontology_lineage_id = $1 AND element_id = $2 AND verified_by = $3
               AND invalidated_at IS NULL",
        )
        .bind(ontology_lineage_id)
        .bind(element_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}
