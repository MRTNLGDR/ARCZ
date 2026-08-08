from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import sqlite3
import tempfile
from typing import Any

from .atomic_io import atomic_write_json
from .errors import ApiError
from .hashing import canonical_json_hash, sha256_file
from .schema_validation import SchemaRegistry


class SourceRegistry:
    """Registro de pacotes geoespaciais materializados no disco local.

    A identidade imutável é ``package_id + version``. O conteúdo é endereçado
    por SHA-256 do manifesto, que por sua vez contém tamanho e SHA-256 de cada
    arquivo. Uma mesma versão nunca pode apontar para bytes diferentes.
    """

    def __init__(self, root: Path, schemas: SchemaRegistry):
        self.root = root.resolve()
        self.root.mkdir(parents=True, exist_ok=True)
        self.schemas = schemas
        self.db_path = self.root / "registry.sqlite3"
        self.packages_dir = self.root / "packages"
        self.packages_dir.mkdir(exist_ok=True)
        self._init_db()

    def _connect(self) -> sqlite3.Connection:
        db = sqlite3.connect(self.db_path, timeout=30)
        db.row_factory = sqlite3.Row
        db.execute("PRAGMA journal_mode=WAL")
        db.execute("PRAGMA synchronous=FULL")
        db.execute("PRAGMA foreign_keys=ON")
        db.execute("PRAGMA busy_timeout=30000")
        return db

    def _init_db(self) -> None:
        with self._connect() as db:
            db.executescript(
                """
                CREATE TABLE IF NOT EXISTS packages(
                  content_hash TEXT PRIMARY KEY,
                  package_id TEXT NOT NULL,
                  version TEXT NOT NULL,
                  kind TEXT NOT NULL,
                  west REAL NOT NULL,
                  south REAL NOT NULL,
                  east REAL NOT NULL,
                  north REAL NOT NULL,
                  manifest_path TEXT NOT NULL,
                  imported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                  UNIQUE(package_id, version)
                );
                CREATE INDEX IF NOT EXISTS idx_packages_kind_bbox
                  ON packages(kind, west, south, east, north);
                """
            )

    @staticmethod
    def _safe_member(base: Path, relative: str) -> Path:
        if not relative or relative.startswith(("/", "\\")):
            raise ApiError("PACKAGE_PATH_INVALID", f"Caminho inválido: {relative}", status=400)
        target = (base / relative).resolve()
        try:
            target.relative_to(base.resolve())
        except ValueError as error:
            raise ApiError(
                "PACKAGE_PATH_ESCAPE",
                f"Caminho escapa do pacote: {relative}",
                status=400,
            ) from error
        if target.is_symlink():
            raise ApiError("PACKAGE_SYMLINK_DENIED", f"Symlink não permitido: {relative}", status=400)
        return target

    def import_directory(self, source_dir: Path) -> dict[str, Any]:
        source_dir = source_dir.resolve()
        manifest_path = source_dir / "package.json"
        if not manifest_path.is_file():
            raise ApiError("PACKAGE_MANIFEST_MISSING", "package.json ausente", status=400)

        try:
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            raise ApiError("PACKAGE_MANIFEST_INVALID", str(error), status=400) from error
        self.schemas.validate("source-package.schema.json", manifest)

        # Verifique todos os bytes antes de tocar no catálogo ou no armazenamento.
        for item in manifest["files"]:
            file_path = self._safe_member(source_dir, item["path"])
            if not file_path.is_file():
                raise ApiError("PACKAGE_FILE_MISSING", item["path"], status=400)
            actual_size = file_path.stat().st_size
            if actual_size != item["bytes"]:
                raise ApiError(
                    "PACKAGE_SIZE_MISMATCH",
                    item["path"],
                    status=400,
                    details={"expected": item["bytes"], "actual": actual_size},
                )
            actual_hash = sha256_file(file_path)
            if actual_hash != item["sha256"]:
                raise ApiError(
                    "PACKAGE_HASH_MISMATCH",
                    item["path"],
                    status=400,
                    details={"expected": item["sha256"], "actual": actual_hash},
                )

        content_hash = canonical_json_hash(manifest)
        with self._connect() as db:
            existing = db.execute(
                "SELECT content_hash FROM packages WHERE package_id=? AND version=?",
                (manifest["package_id"], manifest["version"]),
            ).fetchone()
        if existing and existing["content_hash"] != content_hash:
            raise ApiError(
                "PACKAGE_CONFLICT",
                "package_id/version já existe com conteúdo diferente; incremente a versão",
                status=409,
                details={
                    "package_id": manifest["package_id"],
                    "version": manifest["version"],
                    "existing_hash": existing["content_hash"],
                    "new_hash": content_hash,
                },
            )

        destination = self.packages_dir / content_hash
        if not destination.exists():
            temporary = Path(tempfile.mkdtemp(prefix=".package-", dir=self.packages_dir))
            try:
                for item in manifest["files"]:
                    source = self._safe_member(source_dir, item["path"])
                    target = temporary / item["path"]
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(source, target)
                atomic_write_json(temporary / "package.json", manifest)
                os.replace(temporary, destination)
            finally:
                if temporary.exists():
                    shutil.rmtree(temporary, ignore_errors=True)

        bbox = manifest["bbox_wgs84"]
        with self._connect() as db:
            db.execute(
                """
                INSERT OR IGNORE INTO packages(
                  content_hash, package_id, version, kind,
                  west, south, east, north, manifest_path
                ) VALUES(?,?,?,?,?,?,?,?,?)
                """,
                (
                    content_hash,
                    manifest["package_id"],
                    manifest["version"],
                    manifest["kind"],
                    *bbox,
                    str(destination / "package.json"),
                ),
            )
        return {"content_hash": content_hash, "path": str(destination), "manifest": manifest}

    def list(self, kind: str | None = None) -> list[dict[str, Any]]:
        sql = "SELECT * FROM packages"
        params: tuple[Any, ...] = ()
        if kind:
            sql += " WHERE kind=?"
            params = (kind,)
        sql += " ORDER BY imported_at DESC"
        with self._connect() as db:
            return [dict(row) for row in db.execute(sql, params)]

    def resolve_bbox(self, kind: str, bbox: list[float]) -> list[dict[str, Any]]:
        west, south, east, north = bbox
        with self._connect() as db:
            rows = db.execute(
                """
                SELECT * FROM packages
                WHERE kind=? AND west < ? AND east > ? AND south < ? AND north > ?
                ORDER BY imported_at DESC
                """,
                (kind, east, west, north, south),
            ).fetchall()
        return [dict(row) for row in rows]

    def manifest(self, content_hash: str) -> dict[str, Any]:
        with self._connect() as db:
            row = db.execute(
                "SELECT manifest_path FROM packages WHERE content_hash=?",
                (content_hash,),
            ).fetchone()
        if not row:
            raise ApiError("PACKAGE_NOT_FOUND", content_hash, status=404)
        return json.loads(Path(row["manifest_path"]).read_text(encoding="utf-8"))

    def verify(self, content_hash: str) -> dict[str, Any]:
        """Revalida manifesto e bytes já instalados, sem rede."""
        with self._connect() as db:
            row = db.execute(
                "SELECT manifest_path FROM packages WHERE content_hash=?",
                (content_hash,),
            ).fetchone()
        if not row:
            raise ApiError("PACKAGE_NOT_FOUND", content_hash, status=404)
        manifest_path = Path(row["manifest_path"]).resolve()
        package_dir = manifest_path.parent
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        self.schemas.validate("source-package.schema.json", manifest)
        errors: list[dict[str, Any]] = []
        for item in manifest["files"]:
            file_path = self._safe_member(package_dir, item["path"])
            if not file_path.is_file():
                errors.append({"path": item["path"], "code": "MISSING"})
                continue
            actual_size = file_path.stat().st_size
            actual_hash = sha256_file(file_path)
            if actual_size != item["bytes"]:
                errors.append({
                    "path": item["path"], "code": "SIZE_MISMATCH",
                    "expected": item["bytes"], "actual": actual_size,
                })
            if actual_hash != item["sha256"]:
                errors.append({
                    "path": item["path"], "code": "HASH_MISMATCH",
                    "expected": item["sha256"], "actual": actual_hash,
                })
        return {"ok": not errors, "content_hash": content_hash, "errors": errors}
