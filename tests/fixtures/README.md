# Test fixtures

Workspace-agnostic fixture data for tests + evaluation seeds.

## `korean_ecommerce.csv`

Synthetic Korean e-commerce dataset used by `scripts/e2e-korean.sh` and the
golden lifecycle test. Drives ontology design / mapping / commit through
the canonical Korean column glossary.

## `retrieval_comparison_golden.korean.json`

Golden dataset template for `retrieval_comparison` evaluation cases —
hybrid (RRF) vs trigram-only baseline lift measurement across the three
retrieval surfaces (`verified_query` / `community_summary` /
`knowledge_entry`).

### How to use

The template ships with placeholder `expected_ids` strings (each carries
`REPLACE-with-…`). Before importing, populate them with the gold-standard
ids your workspace produces:

| Surface             | Id shape                                    | Where it comes from                                    |
|---------------------|---------------------------------------------|--------------------------------------------------------|
| `verified_query`    | `vq-{question_hash}`                        | Server-generated when promoted via `POST /api/verified-queries` |
| `community_summary` | `leiden:{level}:{cluster}`                  | Cron-emitted (community detection sweep, 6h cadence)   |
| `knowledge_entry`   | UUID                                        | Server-generated on `POST /api/knowledge`              |

Then import via the bulk-upsert endpoint:

```bash
curl -X POST "$ONTOSYX_API_URL/evaluation/runs/$RUN_ID/cases/bulk-upsert" \
  -H 'Content-Type: application/json' \
  -d @tests/fixtures/retrieval_comparison_golden.korean.json
```

Every case replaces by `(run_id, case_key)` so re-importing with the same
keys updates the gold ids in place — operational convenience for iterating
on the gold set as the workspace's data drifts.

After import, execute each case via the `/cases/{case_key}/execute`
endpoint (or the run-detail page's bulk-execute affordance). The eight
metric rows (`<surface>.<leg>.<axis>`) the case-execute path emits feed
the dashboard's `RunComparisonAggregate` matrix.
