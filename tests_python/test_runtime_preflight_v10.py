from pathlib import Path

from tools.runtime_preflight import run_preflight


def test_gateway_preflight_validates_current_python_runtime(tmp_path: Path) -> None:
    report = run_preflight(tmp_path, "gateway")
    assert report["ready"] is True
    assert report["summary"]["blocked"] == 0


def test_interactive_preflight_fails_closed_without_vendored_runtime(tmp_path: Path) -> None:
    report = run_preflight(tmp_path, "interactive")
    assert report["ready"] is False
    blocked = {item["name"] for item in report["checks"] if item["status"] == "BLOCKED"}
    assert "cesium_local_vendor" in blocked
    assert "aedifex_vendor_and_build" in blocked
