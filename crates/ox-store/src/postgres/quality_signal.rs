//! [`QualitySignalStore`] — per-query signal log + dashboard
//! aggregation for the six adaptive quality windows (SHACL
//! pass/failure, anchor hit rate, glossary term hits, ambiguity
//! resolution uptake, query reproducibility, stale-type pressure).
//!
//! Reads go through windowed CTEs that compare the current window's
//! counters against the previous window for a Wilson-interval
//! trend calculation; writes are fire-and-forget so the agent's
//! hot path never pays the signal-capture latency.

use super::*;

fn shacl_failure_from_str(
    s: &str,
) -> OxResult<crate::quality_signal::ShaclFailureKind> {
    use crate::quality_signal::ShaclFailureKind;
    Ok(match s {
        "cardinality_violation" => ShaclFailureKind::CardinalityViolation,
        "measure_group_by" => ShaclFailureKind::MeasureGroupBy,
        "unknown_coded_value" => ShaclFailureKind::UnknownCodedValue,
        "mandatory_property_missing" => ShaclFailureKind::MandatoryPropertyMissing,
        "temporal_grain_mismatch" => ShaclFailureKind::TemporalGrainMismatch,
        "other" => ShaclFailureKind::Other,
        other => {
            return Err(OxError::Runtime {
                message: format!("unknown shacl_failure_kind: {other}"),
            });
        }
    })
}

fn shacl_failure_to_str(
    k: crate::quality_signal::ShaclFailureKind,
) -> &'static str {
    use crate::quality_signal::ShaclFailureKind;
    match k {
        ShaclFailureKind::CardinalityViolation => "cardinality_violation",
        ShaclFailureKind::MeasureGroupBy => "measure_group_by",
        ShaclFailureKind::UnknownCodedValue => "unknown_coded_value",
        ShaclFailureKind::MandatoryPropertyMissing => "mandatory_property_missing",
        ShaclFailureKind::TemporalGrainMismatch => "temporal_grain_mismatch",
        ShaclFailureKind::Other => "other",
    }
}

/// Row used only inside `aggregate_quality_metrics` — flat numeric
/// counters so a single SQL round-trip collects every window stat.
/// Not exposed outside this file.
#[derive(Debug, sqlx::FromRow)]
struct WindowCounters {
    samples: i64,
    anchor_matched: i64,
    glossary_hit: i64,
    clarified: i64,
    clarified_success: i64,
    reproducible: i64,
    shacl_passed: i64,
}

async fn list_window_counters(
    pool: &PgPool,
    days: i64,
    older_than_days: i64,
) -> OxResult<WindowCounters> {
    // `older_than_days > 0` picks the PREVIOUS window (for trend
    // calc): rows older than `older_than_days` days but still
    // within `days + older_than_days` days. `older_than_days == 0`
    // picks the CURRENT window (last `days` days).
    //
    // Reproducibility = count of signal rows whose
    // `query_ir_normalized_hash` appears more than once in the
    // window (meaning "the same plan ran at least twice" → the
    // question is reproducible). Computed against the window's
    // signal set so a one-off query never counts against itself.
    let sql = "WITH window_rows AS ( \
                   SELECT * FROM query_execution_signals \
                   WHERE captured_at >= now() - ($1::bigint || ' days')::interval \
                         - ($2::bigint || ' days')::interval \
                     AND captured_at < now() - ($2::bigint || ' days')::interval \
               ), hashes AS ( \
                   SELECT query_ir_normalized_hash, COUNT(*) AS c \
                   FROM window_rows \
                   GROUP BY query_ir_normalized_hash \
               ) \
               SELECT \
                 (SELECT COUNT(*) FROM window_rows)::bigint AS samples, \
                 (SELECT COUNT(*) FROM window_rows \
                    WHERE anchor_top_score IS NOT NULL AND anchor_top_score >= 0.5)::bigint AS anchor_matched, \
                 (SELECT COUNT(*) FROM window_rows \
                    WHERE array_length(glossary_term_hits, 1) > 0)::bigint AS glossary_hit, \
                 (SELECT COUNT(*) FROM window_rows \
                    WHERE ambiguity_was_clarified)::bigint AS clarified, \
                 (SELECT COUNT(*) FROM window_rows \
                    WHERE ambiguity_was_clarified AND shacl_passed)::bigint AS clarified_success, \
                 COALESCE((SELECT SUM(c) FROM hashes WHERE c > 1), 0)::bigint AS reproducible, \
                 (SELECT COUNT(*) FROM window_rows WHERE shacl_passed)::bigint AS shacl_passed";
    sqlx::query_as::<_, WindowCounters>(sql)
        .bind(days)
        .bind(older_than_days)
        .fetch_one(pool)
        .await
        .map_err(to_ox_error)
}

#[async_trait]
impl QualitySignalStore for PostgresStore {
    async fn create_query_execution_signal(
        &self,
        signal: &crate::quality_signal::QueryExecutionSignal,
    ) -> OxResult<()> {
        let failure_text = signal.shacl_failure_kind.map(shacl_failure_to_str);
        // idempotent: `execution_id` uniquely identifies one query
        // execution event. Re-emission carries the same captured
        // signals — DO NOTHING is safe; first writer wins.
        sqlx::query(
            "INSERT INTO query_execution_signals \
             (execution_id, workspace_id, captured_at, anchor_top_score, anchor_hit_kinds, \
              glossary_term_hits, ambiguity_resolution_ids, ambiguity_was_clarified, \
              shacl_passed, shacl_failure_kind, query_ir_normalized_hash, referenced_type_ids) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
             ON CONFLICT (execution_id) DO NOTHING",
        )
        .bind(signal.execution_id)
        .bind(signal.workspace_id)
        .bind(signal.captured_at)
        .bind(signal.anchor_top_score.map(|v| v as f64))
        .bind(&signal.anchor_hit_kinds)
        .bind(&signal.glossary_term_hits)
        .bind(&signal.ambiguity_resolution_ids)
        .bind(signal.ambiguity_was_clarified)
        .bind(signal.shacl_passed)
        .bind(failure_text)
        .bind(&signal.query_ir_normalized_hash)
        .bind(&signal.referenced_type_ids)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    async fn aggregate_quality_metrics(
        &self,
        window: crate::quality_signal::MetricWindow,
    ) -> OxResult<crate::quality_signal::QualityMetricsReport> {
        use crate::quality_signal::{MetricValue, QualityMetricsReport};

        let days = window.as_days();
        let current = list_window_counters(&self.pool, days, 0).await?;
        let previous = list_window_counters(&self.pool, days, days).await?;

        #[derive(sqlx::FromRow)]
        struct StaleRatio {
            total: i64,
            stale: i64,
        }
        let ratio: StaleRatio = sqlx::query_as::<_, StaleRatio>(
            "SELECT COUNT(*)::bigint AS total, \
                    COUNT(*) FILTER (WHERE last_used_at < now() - INTERVAL '180 days')::bigint AS stale \
             FROM ontology_type_last_used",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;

        fn prop(counters: &WindowCounters, numerator: i64) -> f64 {
            if counters.samples == 0 {
                0.0
            } else {
                (numerator as f64) / (counters.samples as f64)
            }
        }

        fn prop_clarified(counters: &WindowCounters) -> f64 {
            if counters.clarified == 0 {
                0.0
            } else {
                (counters.clarified_success as f64) / (counters.clarified as f64)
            }
        }

        let prev_anchor = prop(&previous, previous.anchor_matched);
        let prev_gloss = prop(&previous, previous.glossary_hit);
        let prev_clar = prop_clarified(&previous);
        let prev_repro = prop(&previous, previous.reproducible);
        let prev_shacl = prop(&previous, previous.shacl_passed);

        let report = QualityMetricsReport {
            anchor_match_rate: MetricValue::wilson_proportion(
                current.anchor_matched as u64,
                current.samples as u64,
                prev_anchor,
            ),
            glossary_hit_rate: MetricValue::wilson_proportion(
                current.glossary_hit as u64,
                current.samples as u64,
                prev_gloss,
            ),
            clarification_success_rate: MetricValue::wilson_proportion(
                current.clarified_success as u64,
                current.clarified as u64,
                prev_clar,
            ),
            query_reproducibility: MetricValue::wilson_proportion(
                current.reproducible as u64,
                current.samples as u64,
                prev_repro,
            ),
            shacl_pass_rate: MetricValue::wilson_proportion(
                current.shacl_passed as u64,
                current.samples as u64,
                prev_shacl,
            ),
            stale_concept_ratio: if ratio.total == 0 {
                MetricValue::empty()
            } else {
                MetricValue::wilson_proportion(ratio.stale as u64, ratio.total as u64, 0.0)
            },
            sample_size: current.samples as u64,
            window,
        };
        Ok(report)
    }

    async fn list_shacl_failure_distribution(
        &self,
        window: crate::quality_signal::MetricWindow,
    ) -> OxResult<Vec<crate::quality_signal::ShaclFailureCount>> {
        use crate::quality_signal::ShaclFailureCount;

        #[derive(sqlx::FromRow)]
        struct Row {
            kind: String,
            count: i64,
        }
        let rows: Vec<Row> = sqlx::query_as::<_, Row>(
            "SELECT shacl_failure_kind AS kind, COUNT(*)::bigint AS count \
             FROM query_execution_signals \
             WHERE captured_at >= now() - ($1::bigint || ' days')::interval \
               AND shacl_failure_kind IS NOT NULL \
             GROUP BY shacl_failure_kind \
             ORDER BY count DESC",
        )
        .bind(window.as_days())
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        rows.into_iter()
            .map(|r| {
                Ok(ShaclFailureCount {
                    kind: shacl_failure_from_str(&r.kind)?,
                    count: r.count as u64,
                })
            })
            .collect()
    }

    async fn upsert_type_last_used(
        &self,
        type_ids: &[(uuid::Uuid, &str)],
    ) -> OxResult<()> {
        if type_ids.is_empty() {
            return Ok(());
        }
        for (id, kind) in type_ids {
            sqlx::query(
                "INSERT INTO ontology_type_last_used \
                 (workspace_id, type_id, type_kind, last_used_at, use_count_7d, use_count_30d) \
                 VALUES (current_setting('app.workspace_id', true)::uuid, $1, $2, now(), 1, 1) \
                 ON CONFLICT (workspace_id, type_id) DO UPDATE SET \
                     last_used_at  = now(), \
                     use_count_7d  = ontology_type_last_used.use_count_7d + 1, \
                     use_count_30d = ontology_type_last_used.use_count_30d + 1, \
                     updated_at    = now()",
            )
            .bind(id)
            .bind(kind)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        }
        Ok(())
    }

    async fn list_stale_types(
        &self,
        stale_after_days: i64,
    ) -> OxResult<Vec<crate::quality_signal::StaleTypeEntry>> {
        use crate::quality_signal::StaleTypeEntry;

        #[derive(sqlx::FromRow)]
        struct Row {
            workspace_id: Uuid,
            type_id: Uuid,
            type_kind: String,
            last_used_at: Option<DateTime<Utc>>,
            days_since: Option<f64>,
        }
        let rows: Vec<Row> = sqlx::query_as::<_, Row>(
            "SELECT workspace_id, type_id, type_kind, last_used_at, \
                    EXTRACT(EPOCH FROM (now() - last_used_at)) / 86400.0 AS days_since \
             FROM ontology_type_last_used \
             WHERE last_used_at < now() - ($1::bigint || ' days')::interval \
             ORDER BY last_used_at ASC \
             LIMIT 500",
        )
        .bind(stale_after_days)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(rows
            .into_iter()
            .map(|r| StaleTypeEntry {
                workspace_id: r.workspace_id,
                type_id: r.type_id,
                type_kind: r.type_kind,
                last_used_at: r.last_used_at,
                days_since_last_use: r.days_since.map(|v| v as i64).unwrap_or(0),
            })
            .collect())
    }
}
