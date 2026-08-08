#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
AEDIFEX_SOURCE=""; CESIUM_SOURCE=""; CESIUM_LICENSE=""; IMPORT_ASSISTED=0; CLONE_AEDIFEX=0; BLENDER=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --aedifex-source) AEDIFEX_SOURCE="$2"; shift 2;;
    --clone-aedifex) CLONE_AEDIFEX=1; shift;;
    --cesium-source) CESIUM_SOURCE="$2"; shift 2;;
    --cesium-license) CESIUM_LICENSE="$2"; shift 2;;
    --blender) BLENDER="$2"; shift 2;;
    --import-assisted) IMPORT_ASSISTED=1; shift;;
    *) echo "Argumento desconhecido: $1" >&2; exit 2;;
  esac
done
cd "$PROJECT_ROOT"
PYTHON="$(command -v python3 || command -v python)"
[[ -n "$PYTHON" ]] || { echo "Python 3.11+ ausente" >&2; exit 2; }
[[ -x .venv/bin/python ]] || "$PYTHON" -m venv .venv
PYTHON="$PROJECT_ROOT/.venv/bin/python"
if [[ -d vendor/python/wheelhouse ]]; then
  "$PYTHON" -m pip install --no-index --find-links vendor/python/wheelhouse -r requirements.txt
elif [[ $IMPORT_ASSISTED -eq 1 ]]; then
  "$PYTHON" -m pip install -r requirements.txt
else
  echo "Wheelhouse local ausente; use --import-assisted explicitamente." >&2; exit 2
fi
export ARCZ_NETWORK_MODE=$([[ $IMPORT_ASSISTED -eq 1 ]] && echo import_assisted || echo offline_strict)
if [[ -n "$CESIUM_SOURCE" ]]; then
  [[ -n "$CESIUM_LICENSE" ]] || { echo "--cesium-license obrigatório" >&2; exit 2; }
  "$PYTHON" tools/vendor_cesium.py --source "$CESIUM_SOURCE" --license-file "$CESIUM_LICENSE" --version 1.143.0 --force
fi
if [[ -n "$AEDIFEX_SOURCE" ]]; then "$PYTHON" tools/vendor_aedifex.py --source "$AEDIFEX_SOURCE"; fi
if [[ $CLONE_AEDIFEX -eq 1 ]]; then [[ $IMPORT_ASSISTED -eq 1 ]] || { echo "--clone-aedifex exige --import-assisted" >&2; exit 2; }; "$PYTHON" tools/vendor_aedifex.py --clone; fi
if [[ -d opensources/forks/aedifex-arcz ]]; then "$PYTHON" tools/build_aedifex_sidecar.py $([[ $IMPORT_ASSISTED -eq 1 ]] && echo --allow-network); fi
if command -v cargo >/dev/null 2>&1; then cargo build --release --workspace; else echo "AVISO: Cargo ausente" >&2; fi
{
  echo ARCZ_NETWORK_MODE=offline_strict
  echo ARCZ_PORT=8123
  echo ARCZ_SEM_NAVEGADOR=0
  [[ -n "$BLENDER" ]] && echo "ARCZ_BLENDER=$BLENDER"
} > .env.local
"$PYTHON" tools/runtime_preflight.py --profile interactive --output logs/runtime-preflight.json
