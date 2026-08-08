#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"; "$PROJECT_ROOT/scripts/linux/stop.sh" || true
rm -rf "$PROJECT_ROOT/.venv" "$PROJECT_ROOT/.env.local"
echo "Runtime local removido. Código-fonte e dados preservados."
