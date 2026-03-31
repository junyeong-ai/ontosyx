#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

# Auto-detect onnxruntime library path (homebrew, any version)
if [ -z "${ORT_LIB_LOCATION:-}" ]; then
  ORT_BASE="/opt/homebrew/Cellar/onnxruntime"
  if [ -d "$ORT_BASE" ]; then
    ORT_VERSION=$(ls -1 "$ORT_BASE" | sort -V | tail -1)
    if [ -n "$ORT_VERSION" ] && [ -d "$ORT_BASE/$ORT_VERSION/lib" ]; then
      export ORT_LIB_LOCATION="$ORT_BASE/$ORT_VERSION/lib"
      export DYLD_LIBRARY_PATH="$ORT_BASE/$ORT_VERSION/lib${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
    fi
  fi
fi
export ORT_PREFER_DYNAMIC_LINK=1

case "${1:-start}" in
  build)
    cargo build --package ox-api
    ;;
  start)
    exec cargo run --package ox-api
    ;;
  release)
    exec cargo run --package ox-api --release
    ;;
  *)
    echo "Usage: ./run.sh [build|start|release]"
    exit 1
    ;;
esac
