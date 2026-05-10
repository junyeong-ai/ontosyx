#!/usr/bin/env bash
# Idempotent git-hooks installer. Wires `core.hooksPath` to
# `.githooks/` so every `git commit` runs through `pre-commit`.
# Safe to invoke from any caller (dev.sh, manual run, CI bootstrap)
# — re-running is a no-op once the config is in place.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

target=".githooks"
current="$(git config --local --get core.hooksPath || true)"

if [ "$current" != "$target" ]; then
    git config --local core.hooksPath "$target"
fi

# Local clones don't always preserve the +x bit on hook files
# (Windows checkouts, fresh clones over filesystems that strip
# exec). Re-applying the bit here keeps the hook executable
# regardless of how the working tree got materialised.
chmod +x .githooks/* 2>/dev/null || true
