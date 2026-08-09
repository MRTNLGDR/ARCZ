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


def query_runtime_environment(env: dict[str, str]) -> dict[str, str]:
    code = (
        "import json,arcz_local; "
        "print(json.dumps(arcz_local.apply_runtime_environment()))"
    )
    completed = subprocess.run(
        [sys.executable, "-c", code],
        cwd=ROOT,
        env=env,
        text=True,
        capture_output=True,
        check=True,
    )
    return json.loads(completed.stdout.strip())


def test_official_python_runtime_defaults_asset_bank_inside_repo() -> None:
    env = python_env()
    env.pop("ARCZ_BANCO", None)
    applied = query_runtime_environment(env)
    bank = Path(applied["ARCZ_BANCO"]).resolve()
    assert bank == (ROOT / "resources" / "assets").resolve()
    bank.relative_to(ROOT)
    assert applied["ARCZ_NETWORK_MODE"] == "offline_strict"


def test_official_python_runtime_rejects_external_asset_bank_override(tmp_path: Path) -> None:
    external = (tmp_path / "outside-assets").resolve()
    applied = query_runtime_environment(python_env({"ARCZ_BANCO": str(external)}))
    bank = Path(applied["ARCZ_BANCO"]).resolve()
    assert bank == (ROOT / "resources" / "assets").resolve()
    bank.relative_to(ROOT)
    assert applied["ARCZ_NETWORK_MODE"] == "offline_strict"


def test_official_runtime_cannot_be_promoted_to_import_assisted() -> None:
    applied = query_runtime_environment(
        python_env({"ARCZ_NETWORK_MODE": "import_assisted"})
    )
    assert applied["ARCZ_NETWORK_MODE"] == "offline_strict"


def test_environment_builder_is_pure_and_repo_local(tmp_path: Path) -> None:
    code = (
        "import json,arcz_local; "
        "print(json.dumps(arcz_local.runtime_environment({"
        "'ARCZ_NETWORK_MODE':'import_assisted','ARCZ_BANCO':'%s'})))"
        % str(tmp_path).replace("\\", "\\\\")
    )
    completed = subprocess.run(
        [sys.executable, "-c", code],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=True,
    )
    env = json.loads(completed.stdout.strip())
    assert env["ARCZ_NETWORK_MODE"] == "offline_strict"
    assert Path(env["ARCZ_BANCO"]).resolve() == (ROOT / "resources" / "assets").resolve()
