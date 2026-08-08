#!/usr/bin/env python3
"""Teste ponta a ponta do motor procedural com um pacote local real/validado.

O teste é deliberadamente estrito:

* exige o binário Rust real;
* importa o pacote por manifesto, hash e licença;
* monta inputs pelo mesmo ``LocalInputAssembler`` usado pela API;
* executa jobs persistentes reais;
* valida manifests, checksums e arquivos gerados;
* repete o primeiro gerador solicitado e compara todos os hashes de saída.

Não há worker Python substituto, resultado pré-gravado ou sucesso simulado.
Ausência do binário encerra com erro explícito.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import shutil
import sys
import tempfile
from typing import Iterable

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from arcz_server.generation_workers import RustGenerationWorker
from arcz_server.generator_contracts import GeneratorContracts
from arcz_server.input_assembler import LocalInputAssembler
from arcz_server.jobs import JobManager
from arcz_server.schema_validation import SchemaRegistry
from arcz_server.source_registry import SourceRegistry

GENERATOR_ALIASES = {
    "terrain": "terrain.generate",
    "parcels": "parcels.generate",
    "roads": "roads.generate",
    "houses": "houses.generate",
    "buildings": "buildings.generate",
    "vegetation": "vegetation.generate",
    "materials": "materials.generate",
}
DEFAULT_KINDS = tuple(GENERATOR_ALIASES.values())


def resolve_generator_kinds(values: Iterable[str] | None) -> list[str]:
    """Normaliza aliases CLI em kinds de job, preservando ordem sem duplicatas."""
    raw = list(values or [])
    if not raw or "all" in raw:
        return list(DEFAULT_KINDS)
    result: list[str] = []
    for value in raw:
        kind = GENERATOR_ALIASES.get(value, value)
        if kind not in DEFAULT_KINDS:
            allowed = ", ".join(["all", *GENERATOR_ALIASES.keys()])
            raise ValueError(f"gerador inválido: {value}; permitidos: {allowed}")
        if kind not in result:
            result.append(kind)
    return result


def locate_binary(explicit: Path | None) -> Path:
    candidates: list[Path] = []
    if explicit:
        candidates.append(explicit.expanduser())
    for profile in ("release", "debug"):
        candidates.extend([
            ROOT / "target" / profile / "arcz-generation-cli",
            ROOT / "target" / profile / "arcz-generation-cli.exe",
        ])
    for candidate in candidates:
        resolved = candidate.resolve()
        if resolved.is_file():
            return resolved
    raise SystemExit("Worker ausente. Execute: python tools/build_generation_worker.py")


def active_region() -> dict:
    """Região da fixture de teste; não representa dado territorial de produção."""
    return {
        "request": {
            "schema_version": 1,
            "region_id": "fixture-minimal",
            "bbox_wgs84": [-48.501, -27.151, -48.499, -27.149],
            "polygon_wgs84": [],
            "focus": {"lat": -27.15, "lon": -48.5},
            "scale": "quarteirao",
            "requested_radius_m": 200.0,
            "sources": {"osm": True, "overture": False, "dem": True, "imagery": False, "street": False},
            "generation_epoch": 1,
        },
        "context": {
            "schema_version": 1,
            "region_id": "fixture-minimal",
            "crs_work": "ENU_LOCAL",
            "origin_wgs84": [-48.5, -27.15, 0.0],
            "terrain": {
                "min_m": 0.0,
                "max_m": 0.9,
                "mean_slope_deg": 0.5,
                "slope_classes": {},
                "confidence": 1.0,
                "vertical_error_m": 0.01,
            },
            "urban": {
                "density": "low",
                "block_pattern": "fixture",
                "road_hierarchy": {},
                "building_height_distribution": {},
                "landuse_distribution": {},
            },
            "environment": {
                "biome": "generic_temperate_urban",
                "climate_profile": "fixture",
                "soil_profile": "fixture",
            },
            "evidence": [],
            "warnings": ["fixture sintética de teste; proibida como fonte territorial de produção"],
            "source_packages": [],
        },
    }


def run_job(
    manager: JobManager,
    contracts: GeneratorContracts,
    assembler: LocalInputAssembler,
    kind: str,
    region: dict,
    user_params: dict | None = None,
) -> tuple[dict, dict]:
    assembled = assembler.resolve(kind, region, user_params or {})
    request = {
        "plugin_id": f"arcz.smoke.{kind}",
        "plugin_version": "1.0.0",
        "params": assembled["params"],
        "region": region,
        "source_versions": assembled["source_versions"],
        "source_packages": assembled["packages"],
    }
    contracts.validate_job(kind, request)
    created = manager.create(kind, request, generation_epoch=1)
    completed = manager.wait(created["id"], timeout=120.0)
    if completed["status"] != "COMPLETED":
        raise RuntimeError(json.dumps(completed, ensure_ascii=False, indent=2))
    manifest_path = manager.root / completed["result_manifest"]
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if not manifest["outputs"]:
        raise RuntimeError(f"{kind}: manifest sem outputs")
    for output in manifest["outputs"]:
        path = manager.root / output["path"]
        if not path.is_file() or path.stat().st_size != output["bytes"]:
            raise RuntimeError(f"{kind}: output ausente ou tamanho divergente: {output['path']}")
    return completed, manifest


def output_signature(manifest: dict) -> list[tuple[str, str, int]]:
    """Assinatura determinística independente do diretório/job id."""
    return sorted(
        (str(item["kind"]), str(item["sha256"]), int(item["bytes"]))
        for item in manifest["outputs"]
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", type=Path, help="binário local arcz-generation-cli")
    parser.add_argument(
        "--package",
        type=Path,
        default=ROOT / "examples" / "source-package" / "minimal-package",
        help="diretório local com package.json e arquivos verificados",
    )
    parser.add_argument(
        "--generator",
        action="append",
        choices=["all", *GENERATOR_ALIASES.keys()],
        help="gerador a provar; pode repetir. Padrão: all",
    )
    parser.add_argument("--keep", action="store_true", help="preserva artefatos em validation/last-smoke-generation")
    args = parser.parse_args()

    worker_binary = locate_binary(args.worker)
    package = args.package.expanduser().resolve()
    if not package.is_dir() or not (package / "package.json").is_file():
        raise SystemExit(f"Pacote local inválido: {package}")
    kinds = resolve_generator_kinds(args.generator)

    keep_path = ROOT / "validation" / "last-smoke-generation" if args.keep else None
    with tempfile.TemporaryDirectory(prefix="arcz-smoke-") as temporary_name:
        temp = Path(temporary_name)
        schemas = SchemaRegistry(ROOT / "schemas")
        sources = SourceRegistry(temp / "data", schemas)
        imported = sources.import_directory(package)
        contracts = GeneratorContracts(schemas)
        assembler = LocalInputAssembler(schemas, sources, contracts)
        manager = JobManager(temp, schemas, workers=1)
        worker = RustGenerationWorker(temp, executable=worker_binary, timeout_seconds=120)
        for kind in kinds:
            manager.register(kind, worker)

        region = active_region()
        region["context"]["source_packages"] = [imported["content_hash"]]
        results: dict[str, dict] = {}
        manifests: dict[str, dict] = {}
        try:
            for kind in kinds:
                _, manifest = run_job(manager, contracts, assembler, kind, region)
                manifests[kind] = manifest
                results[kind] = {
                    "outputs": manifest["outputs"],
                    "seed": manifest["seed"],
                    "inputs_hash": manifest["inputs_hash"],
                }

            replay_kind = kinds[0]
            _, replay = run_job(manager, contracts, assembler, replay_kind, region)
            if output_signature(manifests[replay_kind]) != output_signature(replay):
                raise RuntimeError(f"replay determinístico divergente para {replay_kind}")
            if manifests[replay_kind]["seed"] != replay["seed"]:
                raise RuntimeError(f"seed divergente no replay de {replay_kind}")
        finally:
            manager.stop()

        report = {
            "schema_version": 1,
            "ok": True,
            "worker": str(worker_binary),
            "package": str(package),
            "package_hash": imported["content_hash"],
            "generators": kinds,
            "replay_verified": kinds[0],
            "jobs": results,
        }
        (temp / "smoke-report.json").write_text(
            json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        if keep_path:
            if keep_path.exists():
                shutil.rmtree(keep_path)
            shutil.copytree(temp, keep_path)
            print(keep_path)
        else:
            print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
