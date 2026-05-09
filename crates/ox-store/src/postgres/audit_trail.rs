//! [`AuditTrailStore`] — workspace-wide stream of PROV-O records.
//!
//! Provenance is stored content-addressed in
//! `ontology_entity_versions` and pointed at by per-version rows in
//! `ontology_version_entities` (`entity_kind = 'provenance'`). The
//! stream joins those two against the live snapshot of every
//! ontology in the workspace, applies jsonb-path filters on the
//! activity / agent kind, and orders by `at_time` desc with the
//! entity hash as the deterministic tiebreak so cursor pagination
//! is stable across requests.

use super::*;

/// Cursor encoding: `<rfc3339>|<entity_hash>`. Both sides are
/// stable identifiers — the timestamp comes from the record's
/// `at_time` field, the hash is the immutable content address.
fn parse_cursor(cursor: Option<&str>) -> Option<(DateTime<Utc>, String)> {
    let s = cursor?;
    let (ts, hash) = s.split_once('|')?;
    let ts: DateTime<Utc> = ts.parse().ok()?;
    Some((ts, hash.to_string()))
}

fn encode_cursor(record: &AuditRecord, entity_hash: &str) -> String {
    format!("{}|{}", record.at_time.to_rfc3339(), entity_hash)
}

#[async_trait]
impl AuditTrailStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_audit_records(
        &self,
        filter: AuditTrailFilter,
        cursor: Option<&str>,
        limit: i64,
    ) -> OxResult<CursorPage<AuditRecord>> {
        let (cursor_ts, cursor_hash) = match parse_cursor(cursor) {
            Some((ts, hash)) => (Some(ts), Some(hash)),
            None => (None, None),
        };

        // The query asks for `limit + 1` rows so we can detect
        // whether a next page exists without a separate count
        // query. The `(at_time, entity_hash)` keyset cursor is the
        // strict-less-than half-pair: rows whose `at_time` is
        // strictly older, OR whose `at_time` matches the cursor and
        // `entity_hash` is lexicographically smaller. Tiebreaking
        // on the immutable hash keeps the order deterministic
        // across calls even when many records share a timestamp.
        let rows: Vec<(
            Uuid,
            String,
            String,
            serde_json::Value,
            DateTime<Utc>,
            String,
        )> = sqlx::query_as(
            "SELECT
                 o.id                               AS ontology_id,
                 o.lineage_id                       AS ontology_lineage_id,
                 o.name                             AS ontology_name,
                 eve.content                        AS provenance,
                 (eve.content->>'at_time')::timestamptz AS at_time,
                 eve.entity_hash                    AS entity_hash
             FROM ontologies o
             JOIN ontology_version_snapshots ovs
               ON ovs.ontology_id = o.id
              AND ovs.valid_to IS NULL
              AND ovs.sys_to   IS NULL
             JOIN ontology_version_entities ove
               ON ove.version_id = ovs.id
              AND ove.entity_kind = 'provenance'
             JOIN ontology_entity_versions eve
               ON eve.entity_hash = ove.entity_hash
             WHERE ($1::uuid       IS NULL OR o.id = $1)
               AND ($2::text       IS NULL OR eve.content->'activity'->>'kind' = $2)
               AND ($3::text       IS NULL OR eve.content->'agent'->>'kind'    = $3)
               AND ($4::timestamptz IS NULL OR (eve.content->>'at_time')::timestamptz >= $4)
               AND ($5::timestamptz IS NULL OR (eve.content->>'at_time')::timestamptz <= $5)
               AND ($6::timestamptz IS NULL
                    OR (eve.content->>'at_time')::timestamptz < $6
                    OR ((eve.content->>'at_time')::timestamptz = $6
                        AND eve.entity_hash < $7))
             ORDER BY (eve.content->>'at_time')::timestamptz DESC,
                      eve.entity_hash DESC
             LIMIT $8",
        )
        .bind(filter.ontology_id)
        .bind(filter.activity_kind.as_deref())
        .bind(filter.agent_kind.as_deref())
        .bind(filter.since)
        .bind(filter.until)
        .bind(cursor_ts)
        .bind(cursor_hash.as_deref())
        .bind(limit + 1)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        // Has-more detection — we asked for `limit + 1`, so if the
        // result is full we know another page exists. The cursor is
        // built from the LAST kept row, not the overflow row, so the
        // next page begins exactly where this one ended.
        let has_more = rows.len() > limit as usize;
        let kept = rows.into_iter().take(limit as usize);

        let mut items: Vec<AuditRecord> = Vec::with_capacity(limit as usize);
        let mut last_hash: Option<String> = None;
        for (ontology_id, lineage_id, name, provenance, at_time, hash) in kept {
            items.push(AuditRecord {
                ontology_id,
                ontology_lineage_id: lineage_id,
                ontology_name: name,
                provenance,
                at_time,
            });
            last_hash = Some(hash);
        }

        let next_cursor = if has_more {
            items
                .last()
                .zip(last_hash.as_deref())
                .map(|(last, hash)| encode_cursor(last, hash))
        } else {
            None
        };

        Ok(CursorPage { items, next_cursor })
    }
}
