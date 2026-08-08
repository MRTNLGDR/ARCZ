from __future__ import annotations

"""Deterministic inventory and admission coverage for a vendored Aedifex tree.

The ARCZ integration must never rely on a hand-written feature checklist alone.
Before a pinned upstream commit can be built, this module enumerates every
workspace package/app, plugin surface, node kind, MCP tool source, API route,
environment variable, URL and network call site. A conversion policy then
classifies each item. Unknown or explicitly blocked items fail closed.
"""

from dataclasses import dataclass
from datetime import datetime, timezone
import fnmatch
import hashlib
import json
from pathlib import Path
import re
import subprocess
from typing import Any, Iterable

from .errors import ApiError

SOURCE_SUFFIXES = {".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"}
IGNORED_PARTS = {"node_modules", ".git", ".next", "dist", "coverage", "build", ".turbo"}
TEST_RE = re.compile(r"\.(?:test|spec)\.[^.]+$", re.I)
URL_RE = re.compile(r"https?://[^\s'\"`<>)}]+", re.I)
ENV_RE = re.compile(r"(?:process\.env|Bun\.env)(?:\.([A-Z][A-Z0-9_]*)|\[['\"]([A-Z][A-Z0-9_]*)['\"]\])")
KIND_RE = re.compile(r"\bkind\s*:\s*['\"]([a-z][a-z0-9_.:-]*)['\"]", re.I)
TOOL_NAME_PATTERNS = (
    re.compile(r"\b(?:name|toolName)\s*:\s*['\"]([a-z][a-z0-9_.:-]*)['\"]", re.I),
    re.compile(r"\bregisterTool\s*\(\s*['\"]([a-z][a-z0-9_.:-]*)['\"]", re.I),
    re.compile(r"\bserver\.tool\s*\(\s*['\"]([a-z][a-z0-9_.:-]*)['\"]", re.I),
)
NETWORK_CALL_RE = re.compile(
    r"\b(fetch|axios\.(?:get|post|put|patch|delete)|new\s+WebSocket|new\s+EventSource)\s*\(",
    re.I,
)
NODE_DIRECTORY_EXCLUDES = {
    "index", "shared", "utils", "types", "registry", "schema", "schemas",
    "test", "tests", "fixtures", "__tests__", "lib", "systems",
}
MCP_FILE_EXCLUDES = {"index", "types", "shared", "utils", "helpers", "constants"}
INVENTORY_CATEGORIES = (
    "packages", "apps", "plugins", "node_kinds", "mcp_tools", "api_routes",
    "environment_variables", "external_urls", "network_call_sites",
)


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _canonical(value: object) -> bytes:
    return json.dumps(
        value,
        sort_keys=True,
        ensure_ascii=False,
        separators=(",", ":"),
        allow_nan=False,
    ).encode("utf-8")


def _sha(value: object) -> str:
    return hashlib.sha256(_canonical(value)).hexdigest()


def _read_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ApiError("AEDIFEX_INVENTORY_JSON_INVALID", str(path), status=422) from error
    if not isinstance(value, dict):
        raise ApiError("AEDIFEX_INVENTORY_JSON_INVALID", str(path), status=422)
    return value


def _source_files(root: Path) -> Iterable[Path]:
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.is_symlink() or path.suffix.lower() not in SOURCE_SUFFIXES:
            continue
        rel = path.relative_to(root)
        if any(part in IGNORED_PARTS for part in rel.parts):
            continue
        yield path


def _git_head(root: Path) -> str | None:
    marker = root / "UPSTREAM_COMMIT"
    if marker.is_file():
        value = marker.read_text(encoding="utf-8").strip()
        if value:
            return value
    if not (root / ".git").exists():
        return None
    try:
        process = subprocess.run(
            ["git", "-C", str(root), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return process.stdout.strip() if process.returncode == 0 else None


def _package_record(root: Path, path: Path, kind: str) -> dict[str, Any]:
    value = _read_json(path)
    dependencies: dict[str, str] = {}
    for field in ("dependencies", "devDependencies", "peerDependencies", "optionalDependencies"):
        raw = value.get(field)
        if not isinstance(raw, dict):
            continue
        for name, version in raw.items():
            dependencies[str(name)] = str(version)
    return {
        "id": str(value.get("name") or path.parent.relative_to(root).as_posix()),
        "kind": kind,
        "path": path.relative_to(root).as_posix(),
        "version": str(value.get("version") or "0.0.0"),
        "private": bool(value.get("private", False)),
        "license": value.get("license"),
        "scripts": sorted((value.get("scripts") or {}).keys()),
        "dependencies": dict(sorted(dependencies.items())),
        "exports": value.get("exports"),
    }


def _record_sources(record: dict[str, Any]) -> list[str]:
    values: list[str] = []
    source = record.get("source") or record.get("path")
    if isinstance(source, str) and source:
        values.append(source)
    raw_sources = record.get("sources")
    if isinstance(raw_sources, list):
        values.extend(str(value) for value in raw_sources if isinstance(value, str) and value)
    return sorted(set(values))


def _dedupe(records: list[dict[str, Any]], key: str = "id") -> list[dict[str, Any]]:
    values: dict[str, dict[str, Any]] = {}
    for record in records:
        identity = str(record[key])
        previous = values.get(identity)
        if previous is None:
            values[identity] = record
            continue
        sources = sorted(set(_record_sources(previous) + _record_sources(record)))
        merged = {**previous, **{k: v for k, v in record.items() if k not in {"source", "sources"}}}
        if sources:
            merged.pop("source", None)
            merged["sources"] = sources
        values[identity] = merged
    return [values[identity] for identity in sorted(values)]


def _line_number(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def _tool_ids(rel: str, path: Path, text: str) -> set[str]:
    ids: set[str] = set()
    for pattern in TOOL_NAME_PATTERNS:
        ids.update(pattern.findall(text))
    stem = path.stem
    if stem not in MCP_FILE_EXCLUDES:
        path_id = rel.removeprefix("packages/mcp/src/tools/").rsplit(".", 1)[0]
        ids.add(path_id)
    return ids


def inventory_upstream(root: Path, *, expected_commit: str | None = None) -> dict[str, Any]:
    root = root.resolve()
    if not root.is_dir():
        raise ApiError("AEDIFEX_UPSTREAM_MISSING", str(root), status=409)
    if not (root / "package.json").is_file():
        raise ApiError("AEDIFEX_ROOT_PACKAGE_MISSING", str(root), status=422)
    commit = _git_head(root)
    if expected_commit and commit != expected_commit:
        raise ApiError(
            "AEDIFEX_COMMIT_MISMATCH",
            str(root),
            status=409,
            details={"expected": expected_commit, "actual": commit},
        )

    packages = [_package_record(root, path, "package") for path in sorted(root.glob("packages/*/package.json"))]
    apps = [_package_record(root, path, "app") for path in sorted(root.glob("apps/*/package.json"))]

    node_kinds: list[dict[str, Any]] = []
    mcp_tools: list[dict[str, Any]] = []
    api_routes: list[dict[str, Any]] = []
    plugins: list[dict[str, Any]] = []
    env_vars: dict[str, set[str]] = {}
    urls: dict[str, set[str]] = {}
    network_calls: list[dict[str, Any]] = []

    for package in packages:
        package_id = str(package["id"])
        if package_id.startswith("@aedifex/plugin-") or "/plugin-" in str(package["path"]):
            plugins.append({"id": package_id, "source": package["path"], "kind": "package"})

    for path in _source_files(root):
        rel = path.relative_to(root).as_posix()
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        is_test = bool(TEST_RE.search(rel))

        if rel.startswith("packages/nodes/src/") and not is_test:
            tail = rel.removeprefix("packages/nodes/src/")
            first = tail.split("/", 1)[0].rsplit(".", 1)[0]
            if first and first not in NODE_DIRECTORY_EXCLUDES:
                node_kinds.append({"id": first, "source": rel, "discovered_by": "directory"})
            for kind in sorted(set(KIND_RE.findall(text))):
                node_kinds.append({"id": kind, "source": rel, "discovered_by": "declaration"})

        if rel.startswith("packages/mcp/src/tools/") and not is_test:
            for tool_id in sorted(_tool_ids(rel, path, text)):
                mcp_tools.append({"id": tool_id, "source": rel})

        if "/app/api/" in f"/{rel}" and path.name.startswith("route."):
            route = "/" + rel.split("/app/api/", 1)[1].rsplit("/route.", 1)[0]
            api_routes.append({"id": f"/api/{route.lstrip('/')}", "source": rel})

        if "plugin" in rel.lower() and path.name in {"plugin.ts", "plugin.tsx", "index.ts", "index.tsx"}:
            for kind in sorted(set(KIND_RE.findall(text))):
                plugins.append({"id": f"{rel}:{kind}", "source": rel, "kind": "source"})

        for first, second in ENV_RE.findall(text):
            name = first or second
            env_vars.setdefault(name, set()).add(rel)

        for url in URL_RE.findall(text):
            urls.setdefault(url.rstrip(".,;"), set()).add(rel)

        for match in NETWORK_CALL_RE.finditer(text):
            line = _line_number(text, match.start())
            network_calls.append({
                "id": f"{rel}:{line}",
                "source": rel,
                "line": line,
                "call": match.group(1),
                "test_only": is_test,
            })

    root_license = None
    license_path = root / "LICENSE"
    if license_path.is_file():
        license_text = license_path.read_text(encoding="utf-8", errors="ignore")
        root_license = "MIT" if "MIT License" in license_text else "UNCLASSIFIED"

    body: dict[str, Any] = {
        "schema_version": 1,
        "repository": "TangSY/aedifex",
        "commit": commit,
        "root_license": root_license,
        "root_package": _package_record(root, root / "package.json", "workspace-root"),
        "packages": sorted(packages, key=lambda item: item["id"]),
        "apps": sorted(apps, key=lambda item: item["id"]),
        "plugins": _dedupe(plugins),
        "node_kinds": _dedupe(node_kinds),
        "mcp_tools": _dedupe(mcp_tools),
        "api_routes": _dedupe(api_routes),
        "environment_variables": [
            {"id": name, "sources": sorted(sources)} for name, sources in sorted(env_vars.items())
        ],
        "external_urls": [
            {"id": url, "sources": sorted(sources)} for url, sources in sorted(urls.items())
        ],
        "network_call_sites": sorted(network_calls, key=lambda item: item["id"]),
    }
    # Hash excludes volatile audit time/count summaries.
    body["inventory_hash"] = _sha(body)
    body["generated_at"] = _utc_now()
    body["counts"] = {key: len(body[key]) for key in INVENTORY_CATEGORIES}
    return body


@dataclass(frozen=True, slots=True)
class CoverageDecision:
    status: str
    owner: str
    integration: str
    rule: str
    rationale: str


def _matches_sources(item: dict[str, Any], rule: dict[str, Any]) -> bool:
    include = rule.get("source_patterns")
    exclude = rule.get("source_exclude_patterns")
    sources = _record_sources(item)
    if include:
        if not sources:
            return False
        patterns = [str(value) for value in include]
        # All appearances must be in the admitted source scope. A URL present in
        # both tests and runtime cannot be misclassified as test-only.
        if not all(any(fnmatch.fnmatchcase(source, pattern) for pattern in patterns) for source in sources):
            return False
    if exclude:
        patterns = [str(value) for value in exclude]
        if any(any(fnmatch.fnmatchcase(source, pattern) for pattern in patterns) for source in sources):
            return False
    return True


def classify_item(item: dict[str, Any], rules: list[dict[str, Any]]) -> CoverageDecision | None:
    item_id = str(item.get("id"))
    for rule in rules:
        pattern = str(rule.get("pattern") or "")
        if not pattern or not fnmatch.fnmatchcase(item_id, pattern) or not _matches_sources(item, rule):
            continue
        return CoverageDecision(
            status=str(rule.get("status") or "UNMAPPED"),
            owner=str(rule.get("owner") or "unassigned"),
            integration=str(rule.get("integration") or ""),
            rule=pattern,
            rationale=str(rule.get("rationale") or ""),
        )
    return None


def validate_coverage(inventory: dict[str, Any], policy: dict[str, Any]) -> dict[str, Any]:
    if int(policy.get("schema_version", 0)) != 1:
        raise ApiError(
            "AEDIFEX_COVERAGE_POLICY_VERSION_UNSUPPORTED",
            repr(policy.get("schema_version")),
            status=422,
        )
    expected = policy.get("upstream_commit")
    if expected and inventory.get("commit") != expected:
        raise ApiError(
            "AEDIFEX_COVERAGE_COMMIT_MISMATCH",
            str(inventory.get("commit")),
            status=409,
            details={"expected": expected},
        )
    categories = policy.get("categories")
    if not isinstance(categories, dict):
        raise ApiError("AEDIFEX_COVERAGE_CATEGORIES_MISSING", "categories", status=422)
    missing_categories = [name for name in INVENTORY_CATEGORIES if name not in categories]
    if missing_categories:
        raise ApiError(
            "AEDIFEX_COVERAGE_CATEGORIES_INCOMPLETE",
            ",".join(missing_categories),
            status=422,
        )
    blocking = set(policy.get("blocking_statuses") or ["UNMAPPED", "REVIEW_REQUIRED", "BLOCKED"])
    entries: list[dict[str, Any]] = []
    blockers: list[dict[str, Any]] = []
    for category in INVENTORY_CATEGORIES:
        items = inventory.get(category)
        if not isinstance(items, list):
            raise ApiError("AEDIFEX_INVENTORY_CATEGORY_INVALID", category, status=422)
        rules = categories[category]
        if not isinstance(rules, list):
            raise ApiError("AEDIFEX_COVERAGE_RULES_INVALID", category, status=422)
        for item in items:
            if not isinstance(item, dict):
                raise ApiError("AEDIFEX_INVENTORY_ITEM_INVALID", category, status=422)
            item_id = str(item.get("id"))
            decision = classify_item(item, rules)
            source = item.get("source") or item.get("path") or item.get("sources")
            if decision is None:
                row = {
                    "category": category,
                    "id": item_id,
                    "status": "UNMAPPED",
                    "source": source,
                }
            else:
                row = {
                    "category": category,
                    "id": item_id,
                    "status": decision.status,
                    "owner": decision.owner,
                    "integration": decision.integration,
                    "matched_rule": decision.rule,
                    "rationale": decision.rationale,
                    "source": source,
                }
            entries.append(row)
            if row["status"] in blocking:
                blockers.append(row)
    report = {
        "schema_version": 1,
        "inventory_hash": inventory.get("inventory_hash"),
        "policy_hash": _sha(policy),
        "upstream_commit": inventory.get("commit"),
        "ready": not blockers,
        "entries": entries,
        "blockers": blockers,
        "counts": {
            "items": len(entries),
            "blockers": len(blockers),
            "by_status": {
                status: sum(1 for item in entries if item["status"] == status)
                for status in sorted({item["status"] for item in entries})
            },
            "by_category": {
                category: sum(1 for item in entries if item["category"] == category)
                for category in INVENTORY_CATEGORIES
            },
        },
    }
    report["report_hash"] = _sha(report)
    report["generated_at"] = _utc_now()
    return report
