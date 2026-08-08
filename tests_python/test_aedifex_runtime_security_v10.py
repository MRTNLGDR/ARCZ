from __future__ import annotations

from pathlib import Path

import pytest

from arcz_server.aedifex_registry import AedifexRegistry
from arcz_server.aedifex_runtime import AedifexRuntimeManager
from arcz_server.errors import ApiError


def test_runtime_rejects_non_loopback_arcz_api(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    monkeypatch.setenv("ARCZ_API_URL", "https://api.example.invalid")
    with pytest.raises(ApiError) as caught:
        AedifexRuntimeManager(tmp_path, AedifexRegistry(tmp_path))
    assert caught.value.code == "AEDIFEX_ARCZ_API_URL_DENIED"


def test_runtime_rejects_credentials_paths_queries_and_invalid_ports() -> None:
    invalid = (
        "http://user:secret@127.0.0.1:8123",
        "http://127.0.0.1:8123/api",
        "http://127.0.0.1:8123/?token=x",
        "http://127.0.0.1:99999",
    )
    for value in invalid:
        with pytest.raises(ApiError) as caught:
            AedifexRuntimeManager._loopback_origin(value)
        assert caught.value.code == "AEDIFEX_ARCZ_API_URL_INVALID"


def test_runtime_normalizes_loopback_origins() -> None:
    assert AedifexRuntimeManager._loopback_origin("http://LOCALHOST:8123/") == "http://localhost:8123"
    assert AedifexRuntimeManager._loopback_origin("http://[::1]:8123") == "http://[::1]:8123"
    assert AedifexRuntimeManager._loopback_origin("http://127.0.0.1") == "http://127.0.0.1"


def test_sidecar_environment_does_not_inherit_provider_or_proxy_secrets(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("PATH", "/safe/bin")
    monkeypatch.setenv("OPENAI_API_KEY", "secret")
    monkeypatch.setenv("ANTHROPIC_API_KEY", "secret")
    monkeypatch.setenv("AWS_SECRET_ACCESS_KEY", "secret")
    monkeypatch.setenv("HTTP_PROXY", "http://proxy.invalid")
    monkeypatch.setenv("HTTPS_PROXY", "http://proxy.invalid")
    monkeypatch.setenv("ARCZ_AEDIFEX_BRIDGE_TOKEN", "parent-token")
    child = AedifexRuntimeManager._sanitized_child_env()
    assert child["PATH"] == "/safe/bin"
    for forbidden in (
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "AWS_SECRET_ACCESS_KEY",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ARCZ_AEDIFEX_BRIDGE_TOKEN",
    ):
        assert forbidden not in child
    assert set(child).issubset(AedifexRuntimeManager.CHILD_ENV_ALLOWLIST)
