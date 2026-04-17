#!/usr/bin/env bash
# CI guard: fails if committed web/openapi.json or web/src/types/api.generated.ts
# drift from what the current backend would produce.
#
# Locally, if this fails, run `scripts/gen-openapi-types.sh` and commit.
set -euo pipefail

cd "$(dirname "$0")/.."

bash scripts/gen-openapi-types.sh

if ! git diff --exit-code web/openapi.json web/src/types/api.generated.ts; then
  echo
  echo "OpenAPI drift detected." >&2
  echo "Run: scripts/gen-openapi-types.sh, then commit the result." >&2
  exit 1
fi
