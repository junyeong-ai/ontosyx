#!/usr/bin/env bash
# Download jinaai/jina-embeddings-v5-text-small-retrieval (ONNX) from HuggingFace.
#
# Uses mise-managed Python + huggingface_hub. Run from repo root:
#   ./scripts/download-jina-v5.sh
#
# The ONNX model is ~2.4 GB (model.onnx_data) — ensure sufficient disk space.
# License: CC-BY-NC-4.0 (non-commercial use only).
set -euo pipefail

MODEL_ID="jinaai/jina-embeddings-v5-text-small-retrieval"
MODEL_DIR="${HOME}/.cache/ontosyx/models/jina-embeddings-v5-text-small-retrieval"

echo "==> Downloading ${MODEL_ID}"
echo "    Target: ${MODEL_DIR}"
echo ""

# Ensure huggingface_hub is installed in mise python
mise exec -- pip install -q huggingface_hub 2>/dev/null

mise exec -- python -c "
from huggingface_hub import snapshot_download

path = snapshot_download(
    '${MODEL_ID}',
    local_dir='${MODEL_DIR}',
    allow_patterns=[
        'onnx/model.onnx',
        'onnx/model.onnx_data',
        'tokenizer.json',
        'tokenizer_config.json',
        'config.json',
        'config_sentence_transformers.json',
        '1_Pooling/config.json',
        'modules.json',
    ],
)
print(f'Done: {path}')
"

echo ""
echo "==> Model saved to: ${MODEL_DIR}"
