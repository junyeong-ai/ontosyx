//! [`OntologyVersionStore`] — ontology identity + committed version snapshots, with Level-3 materialisation on commit.

use super::*;
use super::ontology_materialize::{assemble_ir, materialize_level3};

#[async_trait]
impl crate::store::OntologyVersionStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_ontology(
        &self,
        name: &str,
        display_name: &serde_json::Value,
        description: &serde_json::Value,
        lineage_id: Option<&str>,
    ) -> OxResult<crate::models::OntologyRow> {
        super::require_workspace_context()?;
        // Explicit lineage takes precedence; otherwise a fresh UUID
        // v4 goes into the TEXT column. Clients can always overwrite
        // later via a sibling update path — for now creation is the
        // only entry point.
        let lineage = lineage_id
            .map(String::from)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        sqlx::query_as::<_, crate::models::OntologyRow>(
            "INSERT INTO ontologies (lineage_id, name, display_name, description) \
             VALUES ($1, $2, $3, $4) RETURNING *",
        )
        .bind(&lineage)
        .bind(name)
        .bind(display_name)
        .bind(description)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_workspace_ontology(
        &self,
    ) -> OxResult<Option<crate::models::OntologyRow>> {
        // RLS scopes the query to the caller's workspace, and the
        // `ontologies_workspace_singleton_uq` UNIQUE constraint
        // guarantees at most one row matches.
        sqlx::query_as::<_, crate::models::OntologyRow>("SELECT * FROM ontologies")
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_ontology(
        &self,
        id: Uuid,
    ) -> OxResult<Option<crate::models::OntologyRow>> {
        sqlx::query_as::<_, crate::models::OntologyRow>(
            "SELECT * FROM ontologies WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_ontologies(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<crate::models::OntologyRow>> {
        let limit = pagination.effective_limit();

        let rows = if let Some((cursor_ts, cursor_id)) = pagination.cursor_parts() {
            sqlx::query_as::<_, crate::models::OntologyRow>(
                "SELECT * FROM ontologies \
                 WHERE (created_at, id) < ($1, $2) \
                 ORDER BY created_at DESC, id DESC \
                 LIMIT $3",
            )
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(limit + 1)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?
        } else {
            sqlx::query_as::<_, crate::models::OntologyRow>(
                "SELECT * FROM ontologies \
                 ORDER BY created_at DESC, id DESC \
                 LIMIT $1",
            )
            .bind(limit + 1)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?
        };

        Ok(build_cursor_page(rows, limit, |o| (o.created_at, o.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_ontology_by_lineage(
        &self,
        lineage_id: &str,
    ) -> OxResult<Option<crate::models::OntologyRow>> {
        sqlx::query_as::<_, crate::models::OntologyRow>(
            "SELECT * FROM ontologies WHERE lineage_id = $1",
        )
        .bind(lineage_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn find_ontology_by_name(
        &self,
        name: &str,
    ) -> OxResult<Option<crate::models::OntologyRow>> {
        // RLS scopes the row set to the caller's workspace; the
        // singleton UNIQUE constraint makes this a 0-or-1 row lookup.
        sqlx::query_as::<_, crate::models::OntologyRow>(
            "SELECT * FROM ontologies WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn commit_version(
        &self,
        ontology_id: Uuid,
        ir: &ox_ontology::OntologyIR,
        version: &str,
        parent_version_id: Option<Uuid>,
        committed_by: &str,
        commit_message: &str,
    ) -> OxResult<crate::models::OntologyVersionSnapshot> {
        super::require_workspace_context()?;
        // Extract content-addressed entities BEFORE opening the
        // transaction — serialisation failures should not leave a
        // half-open tx behind.
        let entities = ox_ontology::storage::extract_entities(ir)?;

        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;

        // 1) Upsert content-addressed entity rows. ON CONFLICT
        //    (entity_hash) DO NOTHING → auto dedup across versions.
        //    Bulk INSERT via unnest to avoid N round trips.
        let mut hashes: Vec<String> = Vec::with_capacity(entities.len());
        let mut kinds: Vec<String> = Vec::with_capacity(entities.len());
        let mut contents: Vec<serde_json::Value> = Vec::with_capacity(entities.len());
        for ent in &entities {
            hashes.push(ent.hash.clone());
            kinds.push(ent.kind.as_str().to_string());
            contents.push(ent.content.clone());
        }
        // idempotent: `entity_hash` is the SHA-256 of the canonical
        // JSON content, so a conflict means the *same* row is being
        // re-inserted — no information lost on skip.
        sqlx::query(
            "INSERT INTO ontology_entity_versions (entity_hash, entity_kind, content) \
             SELECT * FROM UNNEST($1::text[], $2::ontology_entity_kind[], $3::jsonb[]) \
             ON CONFLICT (entity_hash) DO NOTHING",
        )
        .bind(&hashes)
        .bind(&kinds)
        .bind(&contents)
        .execute(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        // 2) Create the version snapshot row. Default bitemporal
        //    columns: valid_from=now, valid_to=NULL, sys_from=now,
        //    sys_to=NULL. Callers that need retrospective windows
        //    land later as a separate route; this is the common
        //    "commit the current state" path.
        let snapshot = sqlx::query_as::<_, crate::models::OntologyVersionSnapshot>(
            "INSERT INTO ontology_version_snapshots \
                (ontology_id, version, parent_version_id, committed_by, commit_message) \
             VALUES ($1, $2, $3, $4, $5) RETURNING *",
        )
        .bind(ontology_id)
        .bind(version)
        .bind(parent_version_id)
        .bind(committed_by)
        .bind(commit_message)
        .fetch_one(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        // 3) Write the pointer set. One row per (kind, logical_id)
        //    → current hash. Bulk insert via unnest.
        let version_id = snapshot.id;
        let mut logical_ids: Vec<String> = Vec::with_capacity(entities.len());
        // Reuse `kinds` and `hashes` from step 1 — same set, same order.
        for ent in &entities {
            logical_ids.push(ent.logical_id.clone());
        }
        sqlx::query(
            "INSERT INTO ontology_version_entities \
                (version_id, entity_kind, entity_logical_id, entity_hash) \
             SELECT $1, k.kind::ontology_entity_kind, k.lid, k.hash \
             FROM UNNEST($2::text[], $3::text[], $4::text[]) \
                AS k(kind, lid, hash)",
        )
        .bind(version_id)
        .bind(&kinds)
        .bind(&logical_ids)
        .bind(&hashes)
        .execute(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        // 4) Materialise Level 3 indexes for this new version.
        //    Inline in the same transaction so a hydrate can see
        //    the flat/navigation/search rows the moment the
        //    version commit returns. Embeddings are intentionally
        //    skipped — they populate asynchronously via a
        //    background job.
        materialize_level3(&mut tx, version_id, ir).await?;

        tx.commit().await.map_err(to_ox_error)?;
        Ok(snapshot)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_ontology_ir(
        &self,
        version_id: Uuid,
    ) -> OxResult<Option<ox_ontology::OntologyIR>> {
        // Hydrate every entity that belongs to this version.
        // Order is not important — the extractor / assembler
        // tolerates arbitrary arrival order and re-keys by
        // (kind, logical_id).
        let rows = sqlx::query_as::<_, crate::models::OntologyEntityJoinRow>(
            // `entity_kind::text` cast: the column is the Postgres
            // ENUM `ontology_entity_kind`, but `OntologyEntityJoinRow`
            // decodes it as a Rust `String`. Casting on the wire
            // side keeps the struct type-agnostic of the ENUM.
            "SELECT vs.entity_kind::text AS entity_kind, \
                    vs.entity_logical_id AS entity_logical_id, \
                    vs.entity_hash AS entity_hash, \
                    ev.content AS content \
             FROM ontology_version_entities vs \
             JOIN ontology_entity_versions ev ON ev.entity_hash = vs.entity_hash \
             WHERE vs.version_id = $1",
        )
        .bind(version_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if rows.is_empty() {
            return Ok(None);
        }

        assemble_ir(&rows)
            .map(Some)
            .map_err(|e| OxError::Runtime {
                message: format!("OntologyIR hydration from version {version_id}: {e:?}"),
            })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_version_snapshot(
        &self,
        version_id: Uuid,
    ) -> OxResult<Option<crate::models::OntologyVersionSnapshot>> {
        sqlx::query_as::<_, crate::models::OntologyVersionSnapshot>(
            "SELECT * FROM ontology_version_snapshots WHERE id = $1",
        )
        .bind(version_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_versions(
        &self,
        ontology_id: Uuid,
        limit: u32,
    ) -> OxResult<Vec<crate::models::OntologyVersionSnapshot>> {
        sqlx::query_as::<_, crate::models::OntologyVersionSnapshot>(
            "SELECT * FROM ontology_version_snapshots \
             WHERE ontology_id = $1 \
             ORDER BY created_at DESC \
             LIMIT $2",
        )
        .bind(ontology_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn resolve_version_at(
        &self,
        ontology_id: Uuid,
        as_of: DateTime<Utc>,
    ) -> OxResult<Option<crate::models::OntologyVersionSnapshot>> {
        // "Live at as_of": valid_from <= as_of AND (valid_to IS
        // NULL OR valid_to > as_of). Newest-first tiebreak by
        // valid_from.
        sqlx::query_as::<_, crate::models::OntologyVersionSnapshot>(
            "SELECT * FROM ontology_version_snapshots \
             WHERE ontology_id = $1 \
               AND valid_from <= $2 \
               AND (valid_to IS NULL OR valid_to > $2) \
             ORDER BY valid_from DESC \
             LIMIT 1",
        )
        .bind(ontology_id)
        .bind(as_of)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_current_version(
        &self,
        ontology_id: Uuid,
    ) -> OxResult<Option<crate::models::OntologyVersionSnapshot>> {
        sqlx::query_as::<_, crate::models::OntologyVersionSnapshot>(
            "SELECT * FROM ontology_version_snapshots \
             WHERE ontology_id = $1 AND valid_to IS NULL \
             ORDER BY created_at DESC \
             LIMIT 1",
        )
        .bind(ontology_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn diff_versions(
        &self,
        from_version: Uuid,
        to_version: Uuid,
    ) -> OxResult<Vec<crate::models::EntityChange>> {
        // FULL OUTER JOIN on (kind, logical_id) — rows where
        // `from_hash != to_hash`, or one side is NULL, are
        // changes. Stable ordering by (kind, logical_id) so the
        // diff reads predictably in the admin UI.
        let rows = sqlx::query_as::<_, crate::models::DiffRow>(
            // `entity_kind::text` cast mirrors `get_ontology_ir` —
            // DiffRow decodes the column as a Rust `String`.
            "SELECT COALESCE(f.entity_kind, t.entity_kind)::text        AS entity_kind, \
                    COALESCE(f.entity_logical_id, t.entity_logical_id) AS entity_logical_id, \
                    f.entity_hash                                       AS from_hash, \
                    t.entity_hash                                       AS to_hash \
             FROM (SELECT * FROM ontology_version_entities WHERE version_id = $1) f \
             FULL OUTER JOIN \
                  (SELECT * FROM ontology_version_entities WHERE version_id = $2) t \
               ON f.entity_kind = t.entity_kind \
              AND f.entity_logical_id = t.entity_logical_id \
             WHERE f.entity_hash IS DISTINCT FROM t.entity_hash \
             ORDER BY entity_kind, entity_logical_id",
        )
        .bind(from_version)
        .bind(to_version)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let kind = match (row.from_hash.clone(), row.to_hash.clone()) {
                    (None, Some(to_hash)) => crate::models::EntityChangeKind::Added { to_hash },
                    (Some(from_hash), None) => {
                        crate::models::EntityChangeKind::Removed { from_hash }
                    }
                    (Some(from_hash), Some(to_hash)) => {
                        crate::models::EntityChangeKind::Modified { from_hash, to_hash }
                    }
                    // The SQL WHERE hash DISTINCT filter guarantees
                    // at least one side is populated. Surface as
                    // Modified with empty hashes so the admin UI
                    // still shows the row instead of panicking on a
                    // theoretically-impossible row the DB could in
                    // principle produce.
                    (None, None) => crate::models::EntityChangeKind::Modified {
                        from_hash: String::new(),
                        to_hash: String::new(),
                    },
                };
                crate::models::EntityChange {
                    entity_kind: row.entity_kind,
                    entity_logical_id: row.entity_logical_id,
                    kind,
                }
            })
            .collect())
    }
}
