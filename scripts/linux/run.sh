#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"; load_arcz_env; cd "$PROJECT_ROOT"
PYTHON="$(resolve_python)"; "$PYTHON" tools/runtime_preflight.py --profile interactive --output logs/runtime-preflight.json
PID_FILE="$LOGS/arcz.pid"
if [[ -f "$PID_FILE" ]] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then echo "ARCZ já está em execução (PID $(cat "$PID_FILE"))."; exit 0; fi
nohup "$PYTHON" servidor.py "${ARCZ_PORT:-8123}" >"$LOGS/server.stdout.log" 2>"$LOGS/server.stderr.log" & echo $! > "$PID_FILE"
echo "ARCZ iniciado (PID $!)."
