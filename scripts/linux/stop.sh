#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"; PID_FILE="$LOGS/arcz.pid"
[[ -f "$PID_FILE" ]] || { echo "Nenhum PID ARCZ registrado."; exit 0; }
PID="$(cat "$PID_FILE")"; if kill -0 "$PID" 2>/dev/null; then kill "$PID"; for _ in {1..30}; do kill -0 "$PID" 2>/dev/null || break; sleep .1; done; kill -9 "$PID" 2>/dev/null || true; fi
rm -f "$PID_FILE"; echo "ARCZ encerrado."
