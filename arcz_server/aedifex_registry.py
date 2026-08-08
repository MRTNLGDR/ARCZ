from __future__ import annotations

"""Strict registry for the pinned Aedifex authoring kernel.

Presence is not readiness. The registry verifies source identity, package
versions, license, complete source inventory, conversion coverage, overlay
manifest, build manifest, IFC WASM assets and content hashes before the runtime
manager may start the sidecar.
"""

import hashlib
import json
import logging
from pathlib import Path
import subprocess
from typing import Any

from .aedifex_inventory import inventory_upstream, validate_coverage
from .errors import ApiError
from .hashing import sha256_file
from .schema_validation import SchemaRegistry

LOGGER = logging.getLogger(__name__)


def _tree_integrity(root: Path, *, excluded: set[str]) -> dict[str, object]:
    digest = hashlib.sha256()
    count = 0
    total = 0
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.is_symlink():
            continue
        rel = path.relative_to(root).as_posix()
        if rel in excluded:
            continue
        size = path.stat().st_size
        value = sha256_file(path)
        digest.update(rel.encode("utf-8")); digest.update(b"\0")
        digest.update(str(size).encode("ascii")); digest.update(b"\0")
        digest.update(value.encode("ascii")); digest.update(b"\n")
        count += 1
        total += size
    return {"file_count": count, "total_bytes": total, "tree_sha256": digest.hexdigest()}


def _read_object(path: Path) -> dict[str, Any] | None:
    if not path.is_file():
        return None
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    return value if isinstance(value, dict) else None


class AedifexRegistry:
    # Public for test/build tooling. The live status also reads the lock, so a
    # future pin cannot silently narrow this baseline.
    REQUIRED_PACKAGES = (
        "LICENSE",
        "package.json",
        "bun.lock",
        "apps/editor/package.json",
        "apps/ifc-converter/package.json",
        "packages/core/package.json",
        "packages/viewer/package.json",
        "packages/editor/package.json",
        "packages/mcp/package.json",
        "packages/nodes/package.json",
        "packages/plugin-trees/package.json",
        "packages/ifc-converter/package.json",
    )

    def __init__(self, root: Path):
        self.root = root.resolve()
        self.integration = self.root / "integrations/aedifex"
        self.lock_path = self.integration / "UPSTREAM_LOCK.json"
        self.patch_manifest_path = self.integration / "PATCH_MANIFEST.json"
        self.coverage_policy_path = self.integration / "CONVERSION_COVERAGE.json"
        self.generated_inventory_path = self.integration / "generated/UPSTREAM_INVENTORY.json"
        self.generated_coverage_path = self.integration / "generated/CONVERSION_COVERAGE_REPORT.json"
        self.upstream = self.root / "opensources/upstream/aedifex"
        self.fork = self.root / "opensources/forks/aedifex-arcz"
        self.dist = self.root / "vendor/aedifex-floorplanner"
        self.schemas = SchemaRegistry(self.root / "schemas")
        self._deep_integrity_cache: tuple[int, int, bool, dict[str, object]] | None = None

    def lock(self) -> dict[str, Any]:
        value = _read_object(self.lock_path)
        if value is None:
            raise ApiError("AEDIFEX_LOCK_INVALID", str(self.lock_path), status=500)
        if int(value.get("schema_version", 0)) < 3:
            raise ApiError(
                "AEDIFEX_LOCK_SCHEMA_UNSUPPORTED",
                repr(value.get("schema_version")),
                status=500,
            )
        return value

    @staticmethod
    def _git_head(path: Path) -> str | None:
        marker = path / "UPSTREAM_COMMIT"
        if marker.is_file():
            value = marker.read_text(encoding="utf-8").strip()
            if value:
                return value
        if not (path / ".git").exists():
            return None
        try:
            completed = subprocess.run(
                ["git", "-C", str(path), "rev-parse", "HEAD"],
                capture_output=True,
                text=True,
                timeout=10,
                check=False,
            )
            return completed.stdout.strip() if completed.returncode == 0 else None
        except (OSError, subprocess.SubprocessError) as error:
            LOGGER.debug("Unable to resolve Aedifex git head at %s: %s", path, error)
            return None

    @staticmethod
    def _package_status(base: Path, name: str, spec: dict[str, Any]) -> dict[str, Any]:
        path = base / str(spec["path"])
        result: dict[str, Any] = {
            "name": name,
            "path": str(spec["path"]),
            "expected_version": str(spec["version"]),
            "exists": path.is_file(),
            "valid": False,
        }
        if not path.is_file():
            result["error"] = "PACKAGE_MANIFEST_MISSING"
            return result
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
        except Exception as error:
            result["error"] = "PACKAGE_MANIFEST_INVALID"
            result["detail"] = str(error)
            return result
        result["actual_name"] = value.get("name")
        result["actual_version"] = value.get("version")
        result["valid"] = value.get("name") == name and value.get("version") == spec["version"]
        if not result["valid"]:
            result["error"] = "PACKAGE_IDENTITY_MISMATCH"
        return result

    def _coverage_status(
        self,
        required_commit: str,
        lock: dict[str, Any],
        *,
        verify_live_source: bool,
    ) -> tuple[dict[str, Any] | None, dict[str, Any] | None, list[dict[str, Any]]]:
        blockers: list[dict[str, Any]] = []
        inventory = _read_object(self.generated_inventory_path)
        coverage = _read_object(self.generated_coverage_path)
        policy = _read_object(self.coverage_policy_path)
        if inventory is None:
            blockers.append({"code": "AEDIFEX_UPSTREAM_INVENTORY_MISSING"})
            return None, coverage, blockers
        if coverage is None:
            blockers.append({"code": "AEDIFEX_CONVERSION_COVERAGE_MISSING"})
            return inventory, None, blockers
        if policy is None:
            blockers.append({"code": "AEDIFEX_CONVERSION_POLICY_MISSING"})
            return inventory, coverage, blockers
        try:
            self.schemas.validate("aedifex-upstream-inventory.schema.json", inventory)
            self.schemas.validate("aedifex-conversion-policy.schema.json", policy)
            self.schemas.validate("aedifex-conversion-coverage.schema.json", coverage)
            recomputed = validate_coverage(inventory, policy)
        except Exception as error:
            blockers.append({"code": "AEDIFEX_CONVERSION_EVIDENCE_INVALID", "detail": str(error)})
            return inventory, coverage, blockers
        if inventory.get("commit") != required_commit or coverage.get("upstream_commit") != required_commit:
            blockers.append({
                "code": "AEDIFEX_CONVERSION_COMMIT_MISMATCH",
                "expected": required_commit,
                "inventory": inventory.get("commit"),
                "coverage": coverage.get("upstream_commit"),
            })
        if recomputed.get("report_hash") != coverage.get("report_hash"):
            blockers.append({"code": "AEDIFEX_CONVERSION_REPORT_HASH_MISMATCH"})
        if coverage.get("ready") is not True or coverage.get("blockers"):
            blockers.append({
                "code": "AEDIFEX_CONVERSION_COVERAGE_BLOCKED",
                "count": len(coverage.get("blockers") or []),
            })
        discovered_kinds = {str(item.get("id")) for item in inventory.get("node_kinds", []) if isinstance(item, dict)}
        missing_kinds = sorted(set(lock.get("required_node_kinds", [])) - discovered_kinds)
        if missing_kinds:
            blockers.append({"code": "AEDIFEX_REQUIRED_NODE_KINDS_MISSING", "kinds": missing_kinds})
        if verify_live_source and self.upstream.is_dir():
            try:
                live = inventory_upstream(self.upstream, expected_commit=required_commit)
                if live.get("inventory_hash") != inventory.get("inventory_hash"):
                    blockers.append({"code": "AEDIFEX_UPSTREAM_INVENTORY_STALE"})
            except Exception as error:
                blockers.append({"code": "AEDIFEX_UPSTREAM_LIVE_AUDIT_FAILED", "detail": str(error)})
        return inventory, coverage, blockers

    def _verify_build_integrity(
        self,
        build: dict[str, Any],
        runtime_entry: Path | None,
        *,
        deep: bool,
        inventory: dict[str, Any] | None,
        coverage: dict[str, Any] | None,
    ) -> tuple[bool, list[dict[str, Any]]]:
        blockers: list[dict[str, Any]] = []
        try:
            self.schemas.validate("aedifex-build-manifest.schema.json", build)
        except Exception as error:
            return False, [{"code": "AEDIFEX_BUILD_MANIFEST_INVALID", "detail": str(error)}]
        if inventory and build.get("inventory_hash") != inventory.get("inventory_hash"):
            blockers.append({"code": "AEDIFEX_BUILD_INVENTORY_HASH_MISMATCH"})
        if coverage and build.get("coverage_report_hash") != coverage.get("report_hash"):
            blockers.append({"code": "AEDIFEX_BUILD_COVERAGE_HASH_MISMATCH"})
        integrity = build.get("integrity")
        if not isinstance(integrity, dict):
            return False, [{"code": "AEDIFEX_BUILD_INTEGRITY_MISSING"}]
        if not runtime_entry or not runtime_entry.is_file():
            return False, [{"code": "AEDIFEX_RUNTIME_ENTRY_MISSING"}]
        expected_entry = integrity.get("entry_sha256")
        if not isinstance(expected_entry, str) or sha256_file(runtime_entry) != expected_entry:
            blockers.append({"code": "AEDIFEX_RUNTIME_ENTRY_HASH_MISMATCH"})

        runtime_cwd = runtime_entry.parent
        wasm = build.get("wasm_integrity") if isinstance(build.get("wasm_integrity"), dict) else {}
        for name in ("web-ifc.wasm", "web-ifc-mt.wasm"):
            expected = wasm.get(name) if isinstance(wasm.get(name), dict) else {}
            candidate = runtime_cwd / "public" / name
            if not candidate.is_file():
                blockers.append({"code": "AEDIFEX_IFC_WASM_MISSING", "file": name})
                continue
            if candidate.stat().st_size != expected.get("bytes") or sha256_file(candidate) != expected.get("sha256"):
                blockers.append({"code": "AEDIFEX_IFC_WASM_HASH_MISMATCH", "file": name})

        if deep and not blockers:
            manifest_path = self.dist / "arcz-aedifex-build.json"
            stamp = (manifest_path.stat().st_mtime_ns, manifest_path.stat().st_size)
            cached = self._deep_integrity_cache
            if cached and cached[0:2] == stamp:
                deep_ok, actual = cached[2], cached[3]
            else:
                actual = _tree_integrity(self.dist, excluded={"arcz-aedifex-build.json"})
                deep_ok = all(
                    actual.get(key) == integrity.get(key)
                    for key in ("file_count", "total_bytes", "tree_sha256")
                )
                self._deep_integrity_cache = (stamp[0], stamp[1], deep_ok, actual)
            if not deep_ok:
                blockers.append({"code": "AEDIFEX_BUILD_TREE_HASH_MISMATCH", "actual": actual})
        return not blockers, blockers

    def status(self, *, verify_tree: bool = False) -> dict[str, Any]:
        lock = self.lock()
        required_commit = str(lock["commit"])
        paths = list(dict.fromkeys([*self.REQUIRED_PACKAGES, *[str(item) for item in lock.get("required_workspace_paths", [])]]))
        upstream_missing = [item for item in paths if not (self.upstream / item).is_file()]
        fork_missing = [item for item in paths if not (self.fork / item).is_file()]
        package_specs = lock.get("packages", {}) if isinstance(lock.get("packages"), dict) else {}
        upstream_packages = [self._package_status(self.upstream, name, spec) for name, spec in package_specs.items()]
        fork_packages = [self._package_status(self.fork, name, spec) for name, spec in package_specs.items()]

        license_path = self.upstream / "LICENSE"
        license_ok = license_path.is_file() and "MIT License" in license_path.read_text(
            encoding="utf-8", errors="ignore"
        )
        head = self._git_head(self.upstream)
        fork_head = self._git_head(self.fork)
        commit_ok = head == required_commit
        fork_commit_ok = fork_head == required_commit

        patch = _read_object(self.patch_manifest_path)
        patch_ok = False
        if patch is not None:
            try:
                self.schemas.validate("aedifex-patch-manifest.schema.json", patch)
                patch_ids = [item.get("id") for item in patch.get("patches", []) if isinstance(item, dict)]
                patch_ok = (
                    patch.get("upstream_commit") == required_commit
                    and len(patch_ids) == len(set(patch_ids))
                )
            except Exception:
                patch_ok = False

        inventory, coverage, coverage_blockers = self._coverage_status(
            required_commit,
            lock,
            verify_live_source=verify_tree and not upstream_missing,
        )

        dist_manifest = self.dist / "arcz-aedifex-build.json"
        build = _read_object(dist_manifest)
        runtime_entry = None
        if isinstance(build, dict):
            runtime = build.get("runtime")
            if isinstance(runtime, dict):
                command = runtime.get("command")
                cwd = (self.dist / str(runtime.get("cwd", "."))).resolve()
                if isinstance(command, list) and len(command) >= 2 and isinstance(command[1], str):
                    candidate = (cwd / command[1]).resolve()
                    try:
                        candidate.relative_to(self.dist.resolve())
                        runtime_entry = candidate
                    except ValueError:
                        runtime_entry = None

        integrity_ok = False
        integrity_blockers: list[dict[str, Any]] = []
        if isinstance(build, dict) and build.get("upstream_commit") == required_commit:
            integrity_ok, integrity_blockers = self._verify_build_integrity(
                build,
                runtime_entry,
                deep=verify_tree,
                inventory=inventory,
                coverage=coverage,
            )
        build_ok = bool(integrity_ok and isinstance(build, dict))

        blockers: list[dict[str, Any]] = []
        if upstream_missing:
            blockers.append({"code": "AEDIFEX_UPSTREAM_MISSING", "files": upstream_missing})
        if fork_missing:
            blockers.append({"code": "AEDIFEX_FORK_MISSING", "files": fork_missing})
        for scope, packages in (("upstream", upstream_packages), ("fork", fork_packages)):
            invalid = [item for item in packages if not item["valid"]]
            if invalid:
                blockers.append({"code": "AEDIFEX_PACKAGE_MATRIX_INVALID", "scope": scope, "packages": invalid})
        if not license_ok:
            blockers.append({"code": "AEDIFEX_LICENSE_UNVERIFIED"})
        if not head:
            blockers.append({"code": "AEDIFEX_COMMIT_UNVERIFIED"})
        elif not commit_ok:
            blockers.append({"code": "AEDIFEX_COMMIT_MISMATCH", "expected": required_commit, "actual": head})
        if not fork_head:
            blockers.append({"code": "AEDIFEX_FORK_COMMIT_UNVERIFIED"})
        elif not fork_commit_ok:
            blockers.append({"code": "AEDIFEX_FORK_COMMIT_MISMATCH", "expected": required_commit, "actual": fork_head})
        if not patch_ok:
            blockers.append({"code": "AEDIFEX_PATCH_MANIFEST_INVALID"})
        blockers.extend(coverage_blockers)
        if not build_ok:
            blockers.extend(integrity_blockers or [{"code": "AEDIFEX_BRIDGE_BUILD_MISSING"}])

        return {
            "schema_version": 3,
            "upstream": lock,
            "paths": {
                "upstream": str(self.upstream),
                "fork": str(self.fork),
                "dist": str(self.dist),
            },
            "head": head,
            "fork_head": fork_head,
            "commit_verified": commit_ok,
            "fork_commit_verified": fork_commit_ok,
            "license_verified": license_ok,
            "patch_manifest_verified": patch_ok,
            "conversion": {
                "inventory": inventory,
                "coverage": coverage,
                "ready": not coverage_blockers,
            },
            "packages": {"upstream": upstream_packages, "fork": fork_packages},
            "bridge_build_ready": build_ok,
            "build": build,
            "ready": not blockers,
            "blockers": blockers,
        }

    def require_ready(self) -> dict[str, Any]:
        status = self.status(verify_tree=True)
        if not status["ready"]:
            raise ApiError(
                "AEDIFEX_RUNTIME_NOT_READY",
                "Aedifex local ainda não foi vendorizado/compilado",
                status=503,
                details=status,
            )
        return status
