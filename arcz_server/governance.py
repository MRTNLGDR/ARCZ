from __future__ import annotations

"""Snapshot de governança sempre derivado dos arquivos reais do handoff."""

from datetime import datetime, timezone
import json
from pathlib import Path
from typing import Any


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


class GovernanceSnapshot:
    def __init__(self, root: Path): self.root = root.resolve()

    def build(self) -> dict[str, Any]:
        tasks_doc = self._json("TASKS.json", {"tasks": []}); status_doc = self._json("IMPLEMENTATION_STATUS.json", {"modules": []})
        tasks = tasks_doc.get("tasks", []) if isinstance(tasks_doc, dict) else []
        modules = status_doc.get("modules", []) if isinstance(status_doc, dict) else []
        done_states = {"DONE", "VERIFIED", "CLOSED"}
        done = sum(1 for task in tasks if str(task.get("state", "")).upper() in done_states)
        alerts = []
        for module in modules:
            status = str(module.get("status", "UNKNOWN"))
            if status in {"BLOCKED", "PARTIAL", "NOT_IMPLEMENTED", "IMPLEMENTED_UNVERIFIED", "CONTRACT_READY"}:
                alerts.append({"id": f"module:{module.get('id')}", "severity": "HIGH" if status == "BLOCKED" else "MEDIUM",
                               "status": "OPEN", "kind": status, "fact": module.get("name", module.get("id")),
                               "action": "; ".join(module.get("limitations", [])) or "Executar gates do módulo"})
        task_values = [{"id": str(t.get("id")), "category": str(t.get("module", "implementation")),
                        "title": str(t.get("title", "")), "source_path": "TASKS.json", "source_line": 1,
                        "status": "DONE" if str(t.get("state", "")).upper() in done_states else "PENDING"} for t in tasks]
        module_values = []
        for module in modules:
            related = [task for task in tasks if task.get("module") == module.get("id")]
            module_values.append({"module_id": str(module.get("id")), "module_title": str(module.get("name", "")),
                                  "done": sum(1 for t in related if str(t.get("state", "")).upper() in done_states),
                                  "total": len(related)})
        total = len(tasks)
        documents = []
        for name in ("LEIA-PRIMEIRO.md", "AGENTS.md", "ROADMAP.md", "CHANGELOG.md", "IMPLEMENTATION_STATUS.json", "TASKS.json"):
            path = self.root / name
            if path.is_file(): documents.append({"name": name, "link": f"/{name}", "updated_at": datetime.fromtimestamp(path.stat().st_mtime, timezone.utc).isoformat().replace("+00:00", "Z")})
        return {"generatedAt": utc_now(), "state": "READY" if not alerts else "DEGRADED",
                "summary": {"totalTasks": total, "doneTasks": done, "pendingTasks": total-done,
                            "openAlerts": len(alerts), "documents": len(documents),
                            "progressPercent": (done / total * 100.0) if total else 0.0},
                "modules": module_values, "tasks": task_values, "alerts": alerts,
                "changelog": self._changelog(), "logs": [], "documents": documents}

    def _json(self, name: str, default: Any) -> Any:
        path = self.root / name
        try: return json.loads(path.read_text(encoding="utf-8"))
        except Exception: return default

    def _changelog(self) -> list[dict[str, Any]]:
        path = self.root / "CHANGELOG.md"
        if not path.is_file(): return []
        changes = []
        for index, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if line.startswith("## "): changes.append({"release": line[3:].strip(), "category": "release", "description": line[3:].strip(), "source_line": index})
        return changes[:50]
