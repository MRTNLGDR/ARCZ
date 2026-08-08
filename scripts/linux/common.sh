#!/usr/bin/env bash
set -euo pipefail
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LOGS="$PROJECT_ROOT/logs"
mkdir -p "$LOGS"
load_arcz_env() {
  if [[ -f "$PROJECT_ROOT/.env.local" ]]; then
    set -a; # shellcheck disable=SC1091
    source "$PROJECT_ROOT/.env.local"; set +a
  fi
}
resolve_python() {
  if [[ -x "$PROJECT_ROOT/.venv/bin/python" ]]; then printf '%s\n' "$PROJECT_ROOT/.venv/bin/python"; return; fi
  command -v python3 || command -v python
}
