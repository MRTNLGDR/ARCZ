#!/usr/bin/env python3
"""Materialize the pinned Aedifex source and apply removable ARCZ overlays.

This command is the only supported path for bringing Aedifex into the ARCZ
workspace. It preserves an immutable upstream copy, inventories the complete
source tree, fails closed on unclassified surfaces, then creates a controlled
fork and copies the ARCZ host/bridge packages into it.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from arcz_server.aedifex_inventory import inventory_upstream, validate_coverage
from arcz_server.atomic_io import atomic_write_json
from arcz_server.hashing import sha256_file
from arcz_server.schema_validation import SchemaRegistry

LOCK_PATH = ROOT / "integrations/aedifex/UPSTREAM_LOCK.json"
POLICY_PATH = ROOT / "integrations/aedifex/CONVERSION_COVERAGE.json"
RUNTIME_POLICY_PATH = ROOT / "integrations/aedifex/RUNTIME_ADMISSION_RULES.json"
PATCH_PATH = ROOT / "integrations/aedifex/PATCH_MANIFEST.json"
LOCK = json.loads(LOCK_PATH.read_text(encoding="utf-8"))
SCHEMAS = SchemaRegistry(ROOT / "schemas")

SUPABASE_ITEM_PREFIX = "/storage/v1/object/public/items/"
SUPABASE_ITEM_URL_RE = re.compile(
    r"https://[a-z0-9-]+\.supabase\.co/storage/v1/object/public/items/[^'\"`\s]+",
    re.I,
)
KTX2_REMOTE = "https://cdn.jsdelivr.net/gh/pmndrs/drei-assets@master/basis/"
DRACO_REMOTE = "https://www.gstatic.com/draco/versioned/decoders/1.5.5/"
LOCAL_BASE_URL = "http://127.0.0.1:8124"


def tree_fingerprint(root: Path) -> dict[str, object]:
    import hashlib

    digest = hashlib.sha256()
    count = 0
    total = 0
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.is_symlink():
            continue
        rel = path.relative_to(root).as_posix()
        size = path.stat().st_size
        file_hash = sha256_file(path)
        digest.update(rel.encode("utf-8")); digest.update(b"\0")
        digest.update(str(size).encode("ascii")); digest.update(b"\0")
        digest.update(file_hash.encode("ascii")); digest.update(b"\n")
        count += 1
        total += size
    return {"file_count": count, "total_bytes": total, "tree_sha256": digest.hexdigest()}


def run(args: list[str], cwd: Path | None = None) -> str:
    completed = subprocess.run(args, cwd=cwd, text=True, capture_output=True, check=False)
    if completed.returncode:
        raise RuntimeError(
            f"{' '.join(map(str, args))}\nSTDOUT:\n{completed.stdout}\nSTDERR:\n{completed.stderr}"
        )
    return completed.stdout.strip()


def copytree(source: Path, destination: Path) -> None:
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(
        source,
        destination,
        symlinks=False,
        ignore=shutil.ignore_patterns("node_modules", ".next", "dist", "coverage", ".turbo", ".git"),
    )


def verify_source(source: Path) -> str:
    license_path = source / "LICENSE"
    if not license_path.is_file() or "MIT License" not in license_path.read_text(errors="ignore"):
        raise RuntimeError("Licença MIT do upstream não foi verificada")
    if (source / ".git").exists():
        head = run(["git", "-C", str(source), "rev-parse", "HEAD"])
    else:
        marker = source / "UPSTREAM_COMMIT"
        if not marker.is_file():
            raise RuntimeError("Checkout sem .git e sem UPSTREAM_COMMIT")
        head = marker.read_text(encoding="utf-8").strip()
    if head != LOCK["commit"]:
        raise RuntimeError(f"Commit incorreto: {head}; esperado {LOCK['commit']}")
    for rel in LOCK.get("required_workspace_paths", []):
        if not (source / rel).is_file():
            raise RuntimeError(f"Arquivo obrigatório ausente: {rel}")
    for name, spec in LOCK.get("packages", {}).items():
        path = source / str(spec["path"])
        package = json.loads(path.read_text(encoding="utf-8"))
        if package.get("name") != name or package.get("version") != spec["version"]:
            raise RuntimeError(
                f"Pacote incompatível {name}: {package.get('name')}@{package.get('version')}; "
                f"esperado {spec['version']}"
            )
    return head


def effective_conversion_policy() -> dict:
    policy = json.loads(POLICY_PATH.read_text(encoding="utf-8"))
    runtime = json.loads(RUNTIME_POLICY_PATH.read_text(encoding="utf-8"))
    if runtime.get("schema_version") != 1:
        raise RuntimeError("RUNTIME_ADMISSION_RULES.json usa schema_version não suportado")
    if runtime.get("upstream_commit") != LOCK["commit"]:
        raise RuntimeError("RUNTIME_ADMISSION_RULES.json pertence a outro commit Aedifex")
    additions = runtime.get("categories")
    if not isinstance(additions, dict):
        raise RuntimeError("RUNTIME_ADMISSION_RULES.json sem categories")
    categories = policy.get("categories")
    if not isinstance(categories, dict):
        raise RuntimeError("CONVERSION_COVERAGE.json sem categories")
    for category, extra_rules in additions.items():
        if category not in categories or not isinstance(extra_rules, list):
            raise RuntimeError(f"categoria de runtime inválida: {category}")
        rules = categories[category]
        if not isinstance(rules, list):
            raise RuntimeError(f"categoria de policy inválida: {category}")
        fallback = next(
            (
                index
                for index, rule in enumerate(rules)
                if isinstance(rule, dict)
                and rule.get("pattern") == "*"
                and not rule.get("source_patterns")
            ),
            None,
        )
        if fallback is None:
            raise RuntimeError(f"categoria {category} não possui fallback fail-closed")
        signatures = {
            (
                str(rule.get("pattern")),
                tuple(str(value) for value in rule.get("source_patterns", [])),
                str(rule.get("status")),
            )
            for rule in rules
            if isinstance(rule, dict)
        }
        admitted = []
        for rule in extra_rules:
            if not isinstance(rule, dict):
                raise RuntimeError(f"regra runtime inválida em {category}")
            signature = (
                str(rule.get("pattern")),
                tuple(str(value) for value in rule.get("source_patterns", [])),
                str(rule.get("status")),
            )
            if signature not in signatures:
                admitted.append(rule)
        rules[fallback:fallback] = admitted
    return policy


def audit_source(source: Path) -> tuple[dict, dict]:
    inventory = inventory_upstream(source, expected_commit=LOCK["commit"])
    policy = effective_conversion_policy()
    SCHEMAS.validate("aedifex-conversion-policy.schema.json", policy)
    SCHEMAS.validate("aedifex-upstream-inventory.schema.json", inventory)
    report = validate_coverage(inventory, policy)
    SCHEMAS.validate("aedifex-conversion-coverage.schema.json", report)

    evidence_dir = ROOT / "validation/aedifex"
    atomic_write_json(evidence_dir / "UPSTREAM_INVENTORY.json", inventory)
    atomic_write_json(evidence_dir / "CONVERSION_COVERAGE_REPORT.json", report)
    if not report["ready"]:
        summary = ", ".join(
            f"{item['category']}:{item['id']}={item['status']}" for item in report["blockers"][:25]
        )
        raise RuntimeError(
            "Conversão Aedifex bloqueada por superfícies não admitidas. "
            f"Atualize CONVERSION_COVERAGE.json/RUNTIME_ADMISSION_RULES.json após auditoria: {summary}"
        )
    return inventory, report


def _item_slug_and_role(url: str) -> tuple[str, str]:
    parsed = urlparse(url)
    marker = SUPABASE_ITEM_PREFIX
    if marker not in parsed.path:
        raise RuntimeError(f"URL de catálogo fora do prefixo admitido: {url}")
    tail = parsed.path.split(marker, 1)[1]
    parts = [part for part in tail.split("/") if part]
    if len(parts) < 3:
        raise RuntimeError(f"URL de catálogo incompleta: {url}")
    if parts[0] == "system":
        slug = parts[1]
        rest = parts[2:]
    elif parts[0] == "users" and len(parts) >= 4:
        slug = parts[2]
        rest = parts[3:]
    else:
        raise RuntimeError(f"escopo de catálogo Supabase não reconhecido: {url}")
    leaf = rest[-1].lower()
    if leaf.endswith(".glb"):
        return slug, "model"
    if leaf.startswith("thumbnail."):
        return slug, "thumbnail"
    if leaf.startswith("floor-plan."):
        return slug, "floorplan"
    raise RuntimeError(f"tipo de asset de catálogo não reconhecido: {url}")


def _first_existing(directory: Path, names: tuple[str, ...]) -> Path | None:
    for name in names:
        candidate = directory / name
        if candidate.is_file() and candidate.stat().st_size > 0:
            return candidate
    return None


def _local_catalog_value(fork: Path, url: str) -> tuple[str | None, str, str]:
    slug, role = _item_slug_and_role(url)
    directory = fork / "apps/editor/public/items" / slug
    if role == "model":
        candidate = _first_existing(directory, ("model.glb",))
        if candidate is None:
            glbs = sorted(path for path in directory.glob("*.glb") if path.is_file() and path.stat().st_size > 0)
            candidate = glbs[0] if len(glbs) == 1 else None
    elif role == "thumbnail":
        candidate = _first_existing(
            directory,
            ("thumbnail.webp", "thumbnail.png", "thumbnail.jpg", "thumbnail.jpeg"),
        )
    else:
        candidate = _first_existing(
            directory,
            ("floor-plan.svg", "floor-plan.webp", "floor-plan.png", "floor-plan.jpg"),
        )
    if candidate is None:
        return None, slug, role
    return f"/items/{slug}/{candidate.name}", slug, role


def localize_catalog_assets(fork: Path) -> dict[str, object]:
    catalog = fork / "packages/editor/src/components/ui/item-catalog/catalog-items.tsx"
    text = catalog.read_text(encoding="utf-8")
    urls = sorted(set(SUPABASE_ITEM_URL_RE.findall(text)))
    rewritten = 0
    omitted_floorplans = 0
    unresolved: list[dict[str, str]] = []
    roles: dict[str, int] = {"model": 0, "thumbnail": 0, "floorplan": 0}

    for url in urls:
        local, slug, role = _local_catalog_value(fork, url)
        roles[role] += 1
        quoted = re.compile(r"(['\"])" + re.escape(url) + r"\1")
        if local is not None:
            text, count = quoted.subn(lambda match: f"{match.group(1)}{local}{match.group(1)}", text)
            if count == 0:
                raise RuntimeError(f"URL catalogada não pôde ser substituída: {url}")
            rewritten += count
            continue
        if role == "floorplan":
            text, count = quoted.subn("undefined", text)
            if count == 0:
                raise RuntimeError(f"floorPlan remoto não pôde ser neutralizado: {url}")
            omitted_floorplans += count
            continue
        unresolved.append({"url": url, "slug": slug, "role": role})

    if unresolved:
        preview = ", ".join(f"{item['slug']}:{item['role']}" for item in unresolved[:30])
        raise RuntimeError(
            f"catálogo Aedifex possui {len(unresolved)} assets runtime sem equivalente local: {preview}"
        )
    if "supabase.co/storage/v1/object/public/items/" in text:
        raise RuntimeError("catálogo Aedifex ainda contém URL Supabase após localização")
    catalog.write_text(text, encoding="utf-8")
    return {
        "remote_urls": len(urls),
        "rewritten": rewritten,
        "omitted_floorplans": omitted_floorplans,
        "roles": roles,
    }


def _replace_required(path: Path, old: str, new: str, label: str) -> int:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count == 0:
        raise RuntimeError(f"rewrite obrigatório não encontrado ({label}): {path}")
    path.write_text(text.replace(old, new), encoding="utf-8")
    return count


def _localize_base_url(path: Path) -> int:
    text = path.read_text(encoding="utf-8")
    start = text.find("export const BASE_URL = (() => {")
    if start < 0:
        raise RuntimeError(f"BASE_URL upstream não encontrado: {path}")
    end_marker = "})()"
    end = text.find(end_marker, start)
    if end < 0:
        raise RuntimeError(f"fim de BASE_URL upstream não encontrado: {path}")
    end += len(end_marker)
    replacement = (
        "export const BASE_URL =\n"
        f"  process.env.NEXT_PUBLIC_APP_URL || '{LOCAL_BASE_URL}'"
    )
    path.write_text(text[:start] + replacement + text[end:], encoding="utf-8")
    return 1


def localize_runtime_sources(fork: Path) -> dict[str, object]:
    catalog = localize_catalog_assets(fork)
    rewrites: dict[str, int] = {}
    rewrites["viewer_same_origin_assets"] = _replace_required(
        fork / "packages/viewer/src/lib/asset-url.ts",
        "process.env.NEXT_PUBLIC_ASSETS_CDN_URL || 'https://editor.aedifex.com'",
        "(process.env.NEXT_PUBLIC_ASSETS_CDN_URL || '').replace(/\\/$/, '')",
        "viewer asset CDN",
    )
    rewrites["ktx2_local_transcoder"] = _replace_required(
        fork / "packages/viewer/src/lib/ktx2-loader.ts", KTX2_REMOTE, "/basis/", "KTX2 transcoder"
    )
    rewrites["draco_local_decoder"] = _replace_required(
        fork / "packages/nodes/src/item/renderer.tsx", DRACO_REMOTE, "/draco/", "Draco decoder"
    )
    rewrites["editor_base_url"] = _localize_base_url(fork / "packages/editor/src/lib/utils.ts")
    rewrites["upstream_app_base_url"] = _localize_base_url(fork / "apps/editor/lib/utils.ts")

    forbidden = {
        "supabase_item_catalog": "supabase.co/storage/v1/object/public/items/",
        "remote_ktx2": KTX2_REMOTE,
        "remote_draco": DRACO_REMOTE,
    }
    remaining: list[str] = []
    runtime_roots = [
        fork / "packages/editor/src",
        fork / "packages/viewer/src",
        fork / "packages/nodes/src",
    ]
    for root in runtime_roots:
        for path in sorted(root.rglob("*")):
            if not path.is_file() or path.suffix.lower() not in {".ts", ".tsx", ".js", ".jsx", ".mjs"}:
                continue
            text = path.read_text(encoding="utf-8", errors="replace")
            for label, needle in forbidden.items():
                if needle in text:
                    remaining.append(f"{label}:{path.relative_to(fork).as_posix()}")
    if remaining:
        raise RuntimeError("dependências remotas obrigatórias permaneceram no fork: " + ", ".join(remaining[:30]))
    return {"catalog": catalog, "source_rewrites": rewrites, "remaining_forbidden": []}


def merge_workspace(fork: Path) -> None:
    package_path = fork / "package.json"
    document = json.loads(package_path.read_text(encoding="utf-8"))
    workspaces = document.setdefault("workspaces", [])
    if not isinstance(workspaces, list):
        raise RuntimeError("package.json upstream possui workspaces em formato não suportado")
    for item in (
        "apps/arcz-floorplanner",
        "packages/arcz-bridge",
        "packages/arcz-aedifex-tools",
    ):
        if item not in workspaces:
            workspaces.append(item)
    package_path.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path)
    parser.add_argument("--clone", action="store_true")
    args = parser.parse_args()
    source = args.source
    if args.clone:
        if os.environ.get("ARCZ_NETWORK_MODE") != "import_assisted":
            raise SystemExit("--clone exige ARCZ_NETWORK_MODE=import_assisted")
        source = ROOT / "opensources/.incoming/aedifex"
        source.parent.mkdir(parents=True, exist_ok=True)
        if source.exists():
            shutil.rmtree(source)
        run(["git", "clone", "https://github.com/TangSY/aedifex.git", str(source)])
        run(["git", "-C", str(source), "checkout", "--detach", LOCK["commit"]])
    if not source or not source.resolve().is_dir():
        raise SystemExit("Forneça --source <checkout-local> ou --clone explicitamente")

    source = source.resolve()
    verify_source(source)
    inventory, coverage = audit_source(source)

    upstream = ROOT / "opensources/upstream/aedifex"
    fork = ROOT / "opensources/forks/aedifex-arcz"
    copytree(source, upstream)
    (upstream / "UPSTREAM_COMMIT").write_text(LOCK["commit"] + "\n", encoding="utf-8")
    copytree(upstream, fork)

    localization = localize_runtime_sources(fork)

    overlay = ROOT / "integrations/aedifex/overlay"
    for rel in (
        "apps/arcz-floorplanner",
        "packages/arcz-bridge",
        "packages/arcz-aedifex-tools",
    ):
        source_overlay = overlay / rel
        if not source_overlay.is_dir():
            raise RuntimeError(f"Overlay obrigatório ausente: {source_overlay}")
        copytree(source_overlay, fork / rel)
    merge_workspace(fork)

    generated = ROOT / "integrations/aedifex/generated"
    atomic_write_json(generated / "UPSTREAM_INVENTORY.json", inventory)
    atomic_write_json(generated / "CONVERSION_COVERAGE_REPORT.json", coverage)
    atomic_write_json(generated / "LOCALIZATION_REPORT.json", localization)

    output = ROOT / "opensources/integrations/aedifex-materialization.json"
    atomic_write_json(output, {
        "schema_version": 4,
        "commit": LOCK["commit"],
        "upstream": "opensources/upstream/aedifex",
        "fork": "opensources/forks/aedifex-arcz",
        "license_sha256": sha256_file(upstream / "LICENSE"),
        "upstream_integrity": tree_fingerprint(upstream),
        "fork_integrity": tree_fingerprint(fork),
        "overlay_manifest": "integrations/aedifex/PATCH_MANIFEST.json",
        "overlay_manifest_sha256": sha256_file(PATCH_PATH),
        "runtime_admission_rules": "integrations/aedifex/RUNTIME_ADMISSION_RULES.json",
        "runtime_admission_rules_sha256": sha256_file(RUNTIME_POLICY_PATH),
        "inventory_hash": inventory["inventory_hash"],
        "coverage_report_hash": coverage["report_hash"],
        "coverage_ready": True,
        "localization": localization,
    })
    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
