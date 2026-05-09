#!/usr/bin/env bash
# Regenerates web/openapi.json + web/src/types/api.generated.ts from the
# utoipa-derived OpenAPI spec exposed by ox-api.
#
# Run after any change to #[utoipa::path] or #[derive(ToSchema)] types.
# CI enforces freshness via scripts/check-openapi-drift.sh.
set -euo pipefail

cd "$(dirname "$0")/.."

# Stack bumped to 128MB so `dump_openapi`'s recursive
# JsonSchema/ToSchema generation has room for nested structures
# (`OntologyIR` → `RuleDef` → `ShaclConstraint::Or { branches:
# Vec<ShaclConstraint> }` and similar self-referential variants).
if command -v mise >/dev/null 2>&1 && [ -f mise.toml ]; then
  RUST_MIN_STACK=134217728 mise exec -- cargo run --quiet --bin dump_openapi -p ox-api > web/openapi.json
else
  RUST_MIN_STACK=134217728 cargo run --quiet --bin dump_openapi -p ox-api > web/openapi.json
fi

cd web
pnpm exec openapi-typescript ./openapi.json -o src/types/api.generated.ts
