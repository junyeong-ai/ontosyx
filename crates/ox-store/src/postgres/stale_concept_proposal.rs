//! [`StaleConceptProposalStore`] — durable deprecation proposals
//! surfaced by the nightly stale-type scan.
//!
//! The proposal lifecycle is strictly `pending → approved|dismissed`
//! — terminal states never flow back to `pending` (enforced by
//! [`record_decision`]). Upserts that race against a terminal state
//! carry the existing decision forward so a later rescan doesn't
//! re-open a ticket the admin already closed.

use super::*;

fn proposal_from_row(
    row: &sqlx::postgres::PgRow,
) -> OxResult<crate::quality_signal::StaleConceptProposal> {
    use sqlx::Row;
    let id: Uuid = row.try_get("id").map_err(to_ox_error)?;
    let workspace_id: Uuid = row.try_get("workspace_id").map_err(to_ox_error)?;
    let type_id: Uuid = row.try_get("type_id").map_err(to_ox_error)?;
    let type_kind: String = row.try_get("type_kind").map_err(to_ox_error)?;
    let last_used_at: Option<DateTime<Utc>> = row.try_get("last_used_at").map_err(to_ox_error)?;
    let days_since_last_use: i32 = row.try_get("days_since_last_use").map_err(to_ox_error)?;
    let proposed_at: DateTime<Utc> = row.try_get("proposed_at").map_err(to_ox_error)?;
    let decision_text: String = row.try_get("decision").map_err(to_ox_error)?;
    let decision = crate::quality_signal::StaleProposalDecision::try_from_db(&decision_text)
        .ok_or_else(|| OxError::Runtime {
            message: format!("unknown stale_concept decision: {decision_text}"),
        })?;
    let decided_at: Option<DateTime<Utc>> = row.try_get("decided_at").map_err(to_ox_error)?;
    let decided_by_user_id: Option<Uuid> =
        row.try_get("decided_by_user_id").map_err(to_ox_error)?;
    let reason: Option<String> = row.try_get("reason").map_err(to_ox_error)?;

    Ok(crate::quality_signal::StaleConceptProposal {
        id,
        workspace_id,
        type_id,
        type_kind,
        last_used_at,
        days_since_last_use: days_since_last_use as i64,
        proposed_at,
        decision,
        decided_at,
        decided_by_user_id,
        reason,
    })
}

#[async_trait]
impl StaleConceptProposalStore for PostgresStore {
    async fn list_stale_concept_proposals(
        &self,
        pending_only: bool,
    ) -> OxResult<Vec<crate::quality_signal::StaleConceptProposal>> {
        // RLS scopes workspace automatically; the `pending_only`
        // filter feeds the admin dashboard's "open work" view.
        let sql = if pending_only {
            "SELECT id, workspace_id, type_id, type_kind, last_used_at, \
                    days_since_last_use, proposed_at, decision, decided_at, \
                    decided_by_user_id, reason \
             FROM stale_concept_proposals \
             WHERE decision = 'pending' \
             ORDER BY proposed_at DESC \
             LIMIT 500"
        } else {
            "SELECT id, workspace_id, type_id, type_kind, last_used_at, \
                    days_since_last_use, proposed_at, decision, decided_at, \
                    decided_by_user_id, reason \
             FROM stale_concept_proposals \
             ORDER BY proposed_at DESC \
             LIMIT 500"
        };
        let rows = sqlx::query(sql)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?;
        rows.iter().map(proposal_from_row).collect()
    }

    async fn get_stale_concept_proposal(
        &self,
        id: Uuid,
    ) -> OxResult<Option<crate::quality_signal::StaleConceptProposal>> {
        let row = sqlx::query(
            "SELECT id, workspace_id, type_id, type_kind, last_used_at, \
                    days_since_last_use, proposed_at, decision, decided_at, \
                    decided_by_user_id, reason \
             FROM stale_concept_proposals WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.as_ref().map(proposal_from_row).transpose()
    }

    async fn upsert_stale_concept_proposal(
        &self,
        proposal: crate::quality_signal::StaleConceptProposal,
    ) -> OxResult<crate::quality_signal::StaleConceptProposal> {
        // Cron-friendly: natural key dedup. A re-proposal after a
        // previous `dismissed` decision needs the admin to clear the
        // old row first — we don't auto-resurrect, because that
        // would flap every scan.
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let row = sqlx::query(
            "INSERT INTO stale_concept_proposals \
             (id, workspace_id, type_id, type_kind, last_used_at, \
              days_since_last_use, proposed_at, decision) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending') \
             ON CONFLICT (workspace_id, type_id) DO UPDATE SET \
                 last_used_at = EXCLUDED.last_used_at, \
                 days_since_last_use = EXCLUDED.days_since_last_use \
             RETURNING id, workspace_id, type_id, type_kind, last_used_at, \
                       days_since_last_use, proposed_at, decision, decided_at, \
                       decided_by_user_id, reason",
        )
        .bind(proposal.id)
        .bind(workspace_id)
        .bind(proposal.type_id)
        .bind(&proposal.type_kind)
        .bind(proposal.last_used_at)
        .bind(proposal.days_since_last_use as i32)
        .bind(proposal.proposed_at)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        proposal_from_row(&row)
    }

    async fn record_stale_proposal_decision(
        &self,
        id: Uuid,
        decision: crate::quality_signal::StaleProposalDecision,
        decided_by_user_id: Option<Uuid>,
        reason: Option<String>,
    ) -> OxResult<crate::quality_signal::StaleConceptProposal> {
        super::require_workspace_context()?;
        // Only transition from `pending` — repeated decisions are
        // silent no-ops that return the existing row (so the UI can
        // double-click the button without error). Terminal → terminal
        // transitions would erode the audit trail and aren't useful.
        let row = sqlx::query(
            "UPDATE stale_concept_proposals \
             SET decision = $2, \
                 decided_at = now(), \
                 decided_by_user_id = $3, \
                 reason = $4 \
             WHERE id = $1 AND decision = 'pending' \
             RETURNING id, workspace_id, type_id, type_kind, last_used_at, \
                       days_since_last_use, proposed_at, decision, decided_at, \
                       decided_by_user_id, reason",
        )
        .bind(id)
        .bind(decision.as_str())
        .bind(decided_by_user_id)
        .bind(reason.as_deref())
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if let Some(row) = row {
            return proposal_from_row(&row);
        }
        // No row updated → already terminal OR RLS-invisible. Return
        // the current row when visible, otherwise propagate a
        // `NotFound` shape callers already expect from `.get_*`.
        let current = self.get_stale_concept_proposal(id).await?;
        current.ok_or_else(|| OxError::Runtime {
            message: format!("stale_concept_proposal {id} not found"),
        })
    }

    async fn record_stale_proposal_decisions(
        &self,
        ids: &[Uuid],
        decision: crate::quality_signal::StaleProposalDecision,
        decided_by_user_id: Option<Uuid>,
        reason: Option<String>,
    ) -> OxResult<u64> {
        super::require_workspace_context()?;
        if ids.is_empty() {
            return Ok(0);
        }
        // Same `decision = 'pending'` guard as the single-id path
        // — the audit trail of "pending → terminal" is sacred,
        // double-clicks across the cohort silently no-op rather
        // than rewriting the decision metadata.
        let result = sqlx::query(
            "UPDATE stale_concept_proposals \
             SET decision = $2, \
                 decided_at = now(), \
                 decided_by_user_id = $3, \
                 reason = $4 \
             WHERE id = ANY($1) AND decision = 'pending'",
        )
        .bind(ids)
        .bind(decision.as_str())
        .bind(decided_by_user_id)
        .bind(reason.as_deref())
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected())
    }
}
