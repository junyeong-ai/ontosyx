#!/usr/bin/env bash
# Regenerates web/openapi.json + web/src/types/api.generated.ts from the
# utoipa-derived OpenAPI spec exposed by ox-api.
#
# Run after any change to #[utoipa::path] or #[derive(ToSchema)] types.
# CI enforces freshness via scripts/check-openapi-drift.sh.
set -euo pipefail

cd "$(dirname "$0")/.."

cargo run --quiet --bin dump_openapi -p ox-api > web/openapi.json

cd web
pnpm exec openapi-typescript ./openapi.json -o src/types/api.generated.ts
