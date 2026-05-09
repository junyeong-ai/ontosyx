#!/usr/bin/env bash
# Eval golden gate — fails the PR when the deterministic retrieval
# pipeline regresses on the frozen fixture in
# `crates/ox-store/tests/integration/eval_retrieval_golden.rs`.
#
# The gate seeds a tiny ontology into a fresh workspace, walks
# `OntologyNavigationStore::search_entry_points` for each golden
# question, scores precision@k / recall@k / MRR / NDCG@k against
# the gold-standard anchor list, and asserts each axis floor.
# No LLM round-trip — the test runs in seconds and the failure
# message names every offending case + axis.
#
# Required env: `OX_TEST_DATABASE_URL` pointing at a Postgres role
# with CREATE permission and the `vector`, `pg_trgm`,
# `uuid-ossp`, `btree_gin` extensions installed. Use a freshly
# dropped or dedicated test database so the single `0001_schema.sql`
# baseline can create the schema from an empty migration ledger.
#
# Local dev:
# ```sh
# OX_TEST_DATABASE_URL=postgres://ontosyx_app:ontosyx-dev@localhost:5436/ontosyx_test \
#     bash scripts/eval-pr-gate.sh
# ```

set -euo pipefail

cd "$(dirname "$0")/.."

if [[ -z "${OX_TEST_DATABASE_URL:-}" ]]; then
    echo "eval-pr-gate: OX_TEST_DATABASE_URL is required (Postgres URL)" >&2
    exit 2
fi

cargo test -p ox-store --test integration --release -- \
    --ignored \
    --nocapture \
    eval_retrieval_golden
