from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]


def python_env(extra: dict[str, str] | None = None) -> dict[str, str]:
    env = dict(os.environ)
    env.update(extra or {})
    return env


def query_asset_bank(env: dict[str, str]) -> Path:
    code = "import json,os; print(json.dumps({'bank': os.environ.get('ARCZ_BANCO')}))"
    completed = subprocess.run(
        [sys.executable, "-c", code],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
        check=True,
    )
    return Path(json.loads(completed.stdout.strip())["bank"]).resolve()


def test_python_server_default_asset_bank_is_inside_repo() -> None:
    env = python_env()
    env.pop("ARCZ_BANCO", None)
    bank = query_asset_bank(env)
    assert bank == (ROOT / "resources" / "assets").resolve()
    bank.relative_to(ROOT)


def test_external_asset_bank_override_is_clamped_to_repo(tmp_path: Path) -> None:
    external = (tmp_path / "outside-assets").resolve()
    bank = query_asset_bank(python_env({"ARCZ_BANCO": str(external)}))
    assert bank == (ROOT / "resources" / "assets").resolve()
    bank.relative_to(ROOT)


def test_import_assisted_mode_is_not_overwritten_by_bootstrap() -> None:
    code = "import os; print(os.environ.get('ARCZ_NETWORK_MODE'))"
    completed = subprocess.run(
        [sys.executable, "-c", code],
        cwd=ROOT,
        env=python_env({"ARCZ_NETWORK_MODE": "import_assisted"}),
        text=True,
        capture_output=True,
        check=True,
    )
    assert completed.stdout.strip() == "import_assisted"
