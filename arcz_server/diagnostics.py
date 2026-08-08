from __future__ import annotations

from datetime import datetime, timezone
import os
from pathlib import Path
import platform
from typing import Any

from .hardware import detect_hardware
from .jobs import JobManager
from .network_policy import NetworkPolicy
from .source_registry import SourceRegistry
from .ai_broker import ModelRegistry


def diagnostics(root: Path, *, policy: NetworkPolicy, jobs: JobManager,
                sources: SourceRegistry, models: ModelRegistry) -> dict[str, Any]:
    hardware = detect_hardware().as_dict()
    return {
        "timestamp": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "runtime": {"python": platform.python_version(), "platform": platform.platform(),
                    "pid": os.getpid(), "root": str(root.resolve())},
        "network": {"mode": policy.mode.value, "allow_loopback": policy.allow_loopback,
                    "local_lan_cidrs": list(policy.local_lan_cidrs),
                    "import_allowlist": sorted(policy.import_allowlist)},
        "hardware": hardware,
        "jobs": {"supported_kinds": jobs.supported_kinds(),
                 "active": len(jobs.store.list(status="RUNNING", limit=1000)),
                 "queued": len(jobs.store.list(status="QUEUED", limit=1000)),
                 "recent": jobs.store.list(limit=20)},
        "sources": {"packages": len(sources.list()), "by_kind": _count_by(sources.list(), "kind")},
        "models": {"registered": len(models.list(verify=False)),
                   "installed": sum(1 for model in models.list(verify=True) if model["status"]["installed"])},
    }


def _count_by(items: list[dict[str, Any]], key: str) -> dict[str, int]:
    result: dict[str, int] = {}
    for item in items:
        value = str(item.get(key, "unknown"))
        result[value] = result.get(value, 0) + 1
    return result
