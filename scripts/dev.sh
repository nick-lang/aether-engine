#!/usr/bin/env bash
# Start a full local Aether session from the repo root.
set -euo pipefail
cd "$(dirname "$0")/.."
exec cargo run -p aether_cli -- dev "$@"
