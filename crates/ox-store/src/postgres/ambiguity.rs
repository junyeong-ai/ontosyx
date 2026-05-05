//! [`AmbiguityStore`] — detector outputs + resolver history.
//!
//! All access is workspace-scoped via PostgreSQL RLS. The
//! "at most one active resolution per context" invariant is enforced
//! by the partial unique index `uq_ambiguity_resolutions_active` plus
//! an in-transaction `revoked_at = now()` step executed before every
//! create. Schema declarations live in the `ambiguity_contexts` +
//! `ambiguity_resolutions` sections of `migrations/0001_schema.sql`.

use super::*;

fn ambiguity_context_from_row(row: &sqlx::postgres::PgRow) -> OxResult<ox_ontology::ambiguity::AmbiguityContext> {
    use sqlx::Row;
    let id_text: &str = row.try_get("id").map_err(to_ox_error)?;
    let id_uuid: uuid::Uuid = id_text.parse().map_err(|e: uuid::Error| OxError::Runtime {
        message: format!("ambiguity context id parse: {e}"),
    })?;
    let source_id: &str = row.try_get("source_id").map_err(to_ox_error)?;
    let relation: &str = row.try_get("relation").map_err(to_ox_error)?;
    let column_name: &str = row.try_get("column_name").map_err(to_ox_error)?;
    let kind_text: &str = row.try_get("kind").map_err(to_ox_error)?;
    let kind = match kind_text {
        "numeric_code" => ox_ontology::ambiguity::AmbiguityKind::NumericCode,
        "opaque_short_code" => ox_ontology::ambiguity::AmbiguityKind::OpaqueShortCode,
        "overloaded_name" => ox_ontology::ambiguity::AmbiguityKind::OverloadedName,
        other => {
            return Err(OxError::Runtime {
                message: format!("unknown ambiguity kind in DB row: {other}"),
            });
        }
    };
    let sample_values_json: serde_json::Value =
        row.try_get("sample_values").map_err(to_ox_error)?;
    let sample_values: Vec<String> =
        serde_json::from_value(sample_values_json).map_err(|e| OxError::Runtime {
            message: format!("sample_values decode: {e}"),
        })?;
    let distinct_estimate: Option<i64> = row.try_get("distinct_estimate").map_err(to_ox_error)?;
    let nullable: bool = row.try_get("nullable").map_err(to_ox_error)?;
    let clarification_prompt: &str =
        row.try_get("clarification_prompt").map_err(to_ox_error)?;
    let detection_source_hash: &str =
        row.try_get("detection_source_hash").map_err(to_ox_error)?;
    let repo_hint_json: Option<serde_json::Value> =
        row.try_get("repo_hint").map_err(to_ox_error)?;
    let repo_hint = match repo_hint_json {
        Some(v) => Some(
            serde_json::from_value::<ox_ontology::ambiguity::RepoHint>(v).map_err(|e| {
                OxError::Runtime {
                    message: format!("repo_hint decode: {e}"),
                }
            })?,
        ),
        None => None,
    };
    let detected_at: DateTime<Utc> = row.try_get("detected_at").map_err(to_ox_error)?;

    Ok(ox_ontology::ambiguity::AmbiguityContext {
        id: ox_ontology::ambiguity::AmbiguityId::new(id_uuid.to_string()),
        source_id: ox_ontology::mapping::refs::SourceId::new(source_id),
        column: ox_ontology::mapping::refs::ColumnRef {
            relation: relation.to_string(),
            column: column_name.to_string(),
        },
        kind,
        sample_values,
        distinct_estimate: distinct_estimate.map(|v| v as u64),
        nullable,
        clarification_prompt: clarification_prompt.to_string(),
        detection_source_hash: detection_source_hash.to_string(),
        repo_hint,
        detected_at,
    })
}

fn ambiguity_resolution_from_row(
    row: &sqlx::postgres::PgRow,
) -> OxResult<ox_ontology::ambiguity::AmbiguityResolution> {
    use sqlx::Row;
    let id_text: &str = row.try_get("id").map_err(to_ox_error)?;
    let id_uuid: uuid::Uuid = id_text.parse().map_err(|e: uuid::Error| OxError::Runtime {
        message: format!("ambiguity resolution id parse: {e}"),
    })?;
    let context_id_text: &str = row.try_get("context_id").map_err(to_ox_error)?;
    let context_uuid: uuid::Uuid = context_id_text.parse().map_err(|e: uuid::Error| {
        OxError::Runtime {
            message: format!("context_id parse: {e}"),
        }
    })?;
    let context_source_hash: &str =
        row.try_get("context_source_hash").map_err(to_ox_error)?;
    let mapping_json: serde_json::Value = row.try_get("mapping").map_err(to_ox_error)?;
    let mapping = serde_json::from_value::<ox_ontology::ambiguity::AmbiguityMapping>(mapping_json)
        .map_err(|e| OxError::Runtime {
            message: format!("ambiguity mapping decode: {e}"),
        })?;
    let resolved_at: DateTime<Utc> = row.try_get("resolved_at").map_err(to_ox_error)?;
    let resolved_by_user_id: Option<Uuid> =
        row.try_get("resolved_by_user_id").map_err(to_ox_error)?;
    let supersedes_uuid: Option<Uuid> = row.try_get("supersedes_id").map_err(to_ox_error)?;
    let revoked_at: Option<DateTime<Utc>> = row.try_get("revoked_at").map_err(to_ox_error)?;

    Ok(ox_ontology::ambiguity::AmbiguityResolution {
        id: ox_ontology::ambiguity::AmbiguityResolutionId::new(id_uuid.to_string()),
        context_id: ox_ontology::ambiguity::AmbiguityId::new(context_uuid.to_string()),
        context_source_hash: context_source_hash.to_string(),
        mapping,
        resolved_at,
        resolved_by_user_id,
        supersedes: supersedes_uuid
            .map(|u| ox_ontology::ambiguity::AmbiguityResolutionId::new(u.to_string())),
        revoked_at,
    })
}

#[async_trait]
impl AmbiguityStore for PostgresStore {
    async fn list_ambiguity_contexts(
        &self,
        source_id: &ox_ontology::mapping::refs::SourceId,
    ) -> OxResult<Vec<ox_ontology::ambiguity::AmbiguityContext>> {
        let rows = sqlx::query(
            "SELECT id::text AS id, source_id, relation, column_name, kind, sample_values, \
             distinct_estimate, nullable, clarification_prompt, detection_source_hash, \
             repo_hint, detected_at \
             FROM ambiguity_contexts \
             WHERE source_id = $1 \
             ORDER BY relation, column_name",
        )
        .bind(source_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.iter().map(ambiguity_context_from_row).collect()
    }

    async fn list_ambiguity_contexts_in_workspace(
        &self,
    ) -> OxResult<Vec<ox_ontology::ambiguity::AmbiguityContext>> {
        // RLS narrows the rows to the current workspace; the query
        // itself carries no workspace bind so the admin dashboard
        // and agent code-paths share one SQL.
        let rows = sqlx::query(
            "SELECT id::text AS id, source_id, relation, column_name, kind, sample_values, \
             distinct_estimate, nullable, clarification_prompt, detection_source_hash, \
             repo_hint, detected_at \
             FROM ambiguity_contexts \
             ORDER BY detected_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.iter().map(ambiguity_context_from_row).collect()
    }

    async fn get_ambiguity_context(
        &self,
        id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<Option<ox_ontology::ambiguity::AmbiguityContext>> {
        let uuid: Uuid = id.as_str().parse().map_err(|e: uuid::Error| OxError::Runtime {
            message: format!("ambiguity id must be a uuid: {e}"),
        })?;
        let row = sqlx::query(
            "SELECT id::text AS id, source_id, relation, column_name, kind, sample_values, \
             distinct_estimate, nullable, clarification_prompt, detection_source_hash, \
             repo_hint, detected_at \
             FROM ambiguity_contexts WHERE id = $1",
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.as_ref().map(ambiguity_context_from_row).transpose()
    }

    async fn find_ambiguity_context_by_column(
        &self,
        source_id: &ox_ontology::mapping::refs::SourceId,
        column: &ox_ontology::mapping::refs::ColumnRef,
    ) -> OxResult<Option<ox_ontology::ambiguity::AmbiguityContext>> {
        let row = sqlx::query(
            "SELECT id::text AS id, source_id, relation, column_name, kind, sample_values, \
             distinct_estimate, nullable, clarification_prompt, detection_source_hash, \
             repo_hint, detected_at \
             FROM ambiguity_contexts \
             WHERE source_id = $1 AND relation = $2 AND column_name = $3",
        )
        .bind(source_id.as_str())
        .bind(&column.relation)
        .bind(&column.column)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.as_ref().map(ambiguity_context_from_row).transpose()
    }

    async fn upsert_ambiguity_context(
        &self,
        context: ox_ontology::ambiguity::AmbiguityContext,
    ) -> OxResult<ox_ontology::ambiguity::AmbiguityContext> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let ctx_uuid: Uuid = context.id.as_str().parse().map_err(|e: uuid::Error| {
            OxError::Runtime {
                message: format!("ambiguity id must be uuid: {e}"),
            }
        })?;
        let kind_text = match context.kind {
            ox_ontology::ambiguity::AmbiguityKind::NumericCode => "numeric_code",
            ox_ontology::ambiguity::AmbiguityKind::OpaqueShortCode => "opaque_short_code",
            ox_ontology::ambiguity::AmbiguityKind::OverloadedName => "overloaded_name",
        };
        let sample_json = serde_json::to_value(&context.sample_values).map_err(|e| {
            OxError::Runtime {
                message: format!("sample_values encode: {e}"),
            }
        })?;
        let repo_hint_json = match &context.repo_hint {
            Some(h) => Some(serde_json::to_value(h).map_err(|e| OxError::Runtime {
                message: format!("repo_hint encode: {e}"),
            })?),
            None => None,
        };

        sqlx::query(
            "INSERT INTO ambiguity_contexts \
             (id, workspace_id, source_id, relation, column_name, kind, sample_values, \
              distinct_estimate, nullable, clarification_prompt, detection_source_hash, \
              repo_hint, detected_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, \
                     $8, $9, $10, $11, $12, $13) \
             ON CONFLICT (workspace_id, source_id, relation, column_name) DO UPDATE SET \
                 id = EXCLUDED.id, \
                 kind = EXCLUDED.kind, \
                 sample_values = EXCLUDED.sample_values, \
                 distinct_estimate = EXCLUDED.distinct_estimate, \
                 nullable = EXCLUDED.nullable, \
                 clarification_prompt = EXCLUDED.clarification_prompt, \
                 detection_source_hash = EXCLUDED.detection_source_hash, \
                 repo_hint = EXCLUDED.repo_hint, \
                 detected_at = EXCLUDED.detected_at",
        )
        .bind(ctx_uuid)
        .bind(workspace_id)
        .bind(context.source_id.as_str())
        .bind(&context.column.relation)
        .bind(&context.column.column)
        .bind(kind_text)
        .bind(&sample_json)
        .bind(context.distinct_estimate.map(|v| v as i64))
        .bind(context.nullable)
        .bind(&context.clarification_prompt)
        .bind(&context.detection_source_hash)
        .bind(repo_hint_json.as_ref())
        .bind(context.detected_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(context)
    }

    async fn delete_ambiguity_context(
        &self,
        id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<bool> {
        super::require_workspace_context()?;
        let uuid: Uuid = id.as_str().parse().map_err(|e: uuid::Error| OxError::Runtime {
            message: format!("ambiguity id must be uuid: {e}"),
        })?;
        let result = sqlx::query("DELETE FROM ambiguity_contexts WHERE id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_ambiguity_resolutions(
        &self,
        context_id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<Vec<ox_ontology::ambiguity::AmbiguityResolution>> {
        let uuid: Uuid = context_id.as_str().parse().map_err(|e: uuid::Error| {
            OxError::Runtime {
                message: format!("context id must be uuid: {e}"),
            }
        })?;
        let rows = sqlx::query(
            "SELECT id::text AS id, context_id::text AS context_id, context_source_hash, \
             mapping, resolved_at, resolved_by_user_id, \
             supersedes_id, revoked_at \
             FROM ambiguity_resolutions WHERE context_id = $1 \
             ORDER BY resolved_at DESC",
        )
        .bind(uuid)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        rows.iter().map(ambiguity_resolution_from_row).collect()
    }

    async fn get_active_ambiguity_resolution(
        &self,
        source_id: &ox_ontology::mapping::refs::SourceId,
        column: &ox_ontology::mapping::refs::ColumnRef,
    ) -> OxResult<Option<ox_ontology::ambiguity::AmbiguityResolution>> {
        let row = sqlx::query(
            "SELECT r.id::text AS id, r.context_id::text AS context_id, r.context_source_hash, \
             r.mapping, r.resolved_at, r.resolved_by_user_id, \
             r.supersedes_id, r.revoked_at \
             FROM ambiguity_resolutions r \
             JOIN ambiguity_contexts c ON c.id = r.context_id \
             WHERE c.source_id = $1 AND c.relation = $2 AND c.column_name = $3 \
               AND r.revoked_at IS NULL \
             LIMIT 1",
        )
        .bind(source_id.as_str())
        .bind(&column.relation)
        .bind(&column.column)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.as_ref().map(ambiguity_resolution_from_row).transpose()
    }

    async fn create_ambiguity_resolution(
        &self,
        resolution: ox_ontology::ambiguity::AmbiguityResolution,
    ) -> OxResult<ox_ontology::ambiguity::AmbiguityResolution> {
        // Atomic: revoke the current active row (if any) and insert the
        // new one in a single transaction so the partial unique index
        // (one active per context) is never violated at read time.
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let res_uuid: Uuid = resolution.id.as_str().parse().map_err(|e: uuid::Error| {
            OxError::Runtime {
                message: format!("resolution id must be uuid: {e}"),
            }
        })?;
        let ctx_uuid: Uuid = resolution.context_id.as_str().parse().map_err(
            |e: uuid::Error| OxError::Runtime {
                message: format!("context id must be uuid: {e}"),
            },
        )?;
        let supersedes_uuid: Option<Uuid> = match &resolution.supersedes {
            Some(s) => Some(s.as_str().parse().map_err(|e: uuid::Error| {
                OxError::Runtime {
                    message: format!("supersedes id must be uuid: {e}"),
                }
            })?),
            None => None,
        };
        let mapping_json = serde_json::to_value(&resolution.mapping).map_err(|e| {
            OxError::Runtime {
                message: format!("mapping encode: {e}"),
            }
        })?;

        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;

        // Revoke the current active resolution, if any. UPDATE ... RETURNING
        // gives us the row id to chain as `supersedes` when the caller
        // didn't supply one explicitly.
        let revoked = sqlx::query(
            "UPDATE ambiguity_resolutions \
             SET revoked_at = now() \
             WHERE context_id = $1 AND revoked_at IS NULL \
             RETURNING id",
        )
        .bind(ctx_uuid)
        .fetch_optional(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        let supersedes_final: Option<Uuid> = supersedes_uuid.or_else(|| {
            use sqlx::Row;
            revoked.as_ref().and_then(|r| r.try_get("id").ok())
        });

        sqlx::query(
            "INSERT INTO ambiguity_resolutions \
             (id, workspace_id, context_id, context_source_hash, mapping, \
              resolved_at, resolved_by_user_id, supersedes_id, revoked_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL)",
        )
        .bind(res_uuid)
        .bind(workspace_id)
        .bind(ctx_uuid)
        .bind(&resolution.context_source_hash)
        .bind(&mapping_json)
        .bind(resolution.resolved_at)
        .bind(resolution.resolved_by_user_id)
        .bind(supersedes_final)
        .execute(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        tx.commit().await.map_err(to_ox_error)?;

        Ok(ox_ontology::ambiguity::AmbiguityResolution {
            supersedes: supersedes_final
                .map(|u| ox_ontology::ambiguity::AmbiguityResolutionId::new(u.to_string())),
            ..resolution
        })
    }

    async fn revoke_active_ambiguity_resolution(
        &self,
        context_id: &ox_ontology::ambiguity::AmbiguityId,
    ) -> OxResult<bool> {
        super::require_workspace_context()?;
        let ctx_uuid: Uuid = context_id.as_str().parse().map_err(|e: uuid::Error| {
            OxError::Runtime {
                message: format!("context id must be uuid: {e}"),
            }
        })?;
        let result = sqlx::query(
            "UPDATE ambiguity_resolutions \
             SET revoked_at = now() \
             WHERE context_id = $1 AND revoked_at IS NULL",
        )
        .bind(ctx_uuid)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    async fn bulk_revoke_active_ambiguity_resolutions(
        &self,
        context_ids: &[ox_ontology::ambiguity::AmbiguityId],
    ) -> OxResult<u64> {
        super::require_workspace_context()?;
        if context_ids.is_empty() {
            return Ok(0);
        }
        let uuids: Vec<Uuid> = context_ids
            .iter()
            .map(|id| {
                id.as_str().parse::<Uuid>().map_err(|e| OxError::Runtime {
                    message: format!("context id must be uuid: {e}"),
                })
            })
            .collect::<OxResult<Vec<_>>>()?;
        let result = sqlx::query(
            "UPDATE ambiguity_resolutions \
             SET revoked_at = now() \
             WHERE context_id = ANY($1) AND revoked_at IS NULL",
        )
        .bind(&uuids)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }
}
