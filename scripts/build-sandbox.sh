#!/usr/bin/env bash
set -euo pipefail

# Build the analysis sandbox Docker image
# Used by ExecuteAnalysisTool for Python data science code execution

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "==> Building analysis sandbox image..."
docker build \
  -f "$PROJECT_DIR/Dockerfile.analysis-sandbox" \
  -t ontosyx-analysis-sandbox \
  "$PROJECT_DIR"

echo "==> Verifying sandbox..."
docker run --rm ontosyx-analysis-sandbox python -c "
import pandas, numpy, sklearn, statsmodels
print(f'pandas={pandas.__version__}')
print(f'numpy={numpy.__version__}')
print(f'scikit-learn={sklearn.__version__}')
print(f'statsmodels={statsmodels.__version__}')
print('Sandbox ready.')
"
