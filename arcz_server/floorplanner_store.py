from __future__ import annotations

"""Persistência local transacional do Floorplanner/Aedifex.

O scene graph do Aedifex é a fonte de verdade do edifício detalhado. O ARCZ
mantém revisões imutáveis, eventos e exports derivados; nunca tenta editar uma
cópia paralela da mesma geometria.
"""

from datetime import datetime, timezone
import json
from pathlib import Path
import sqlite3
from typing import Any
import uuid

from .errors import ApiError
from .atomic_io import atomic_write_bytes
from .hashing import canonical_json_hash, sha256_bytes, sha256_file
from .schema_validation import SchemaRegistry


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False)


class FloorplannerStore:
    def __init__(self, path: Path, schemas: SchemaRegistry):
        self.path = path.resolve()
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.schemas = schemas
        self._init_db()

    def _connect(self) -> sqlite3.Connection:
        db = sqlite3.connect(self.path, timeout=30, isolation_level=None)
        db.row_factory = sqlite3.Row
        db.execute("PRAGMA journal_mode=WAL")
        db.execute("PRAGMA synchronous=FULL")
        db.execute("PRAGMA foreign_keys=ON")
        db.execute("PRAGMA busy_timeout=30000")
        return db

    def _init_db(self) -> None:
        with self._connect() as db:
            db.executescript("""
            CREATE TABLE IF NOT EXISTS floorplanner_projects(
              id TEXT PRIMARY KEY,
              arcz_project_id TEXT,
              name TEXT NOT NULL,
              region_id TEXT NOT NULL,
              context_hash TEXT NOT NULL,
              context_json TEXT NOT NULL,
              current_revision INTEGER NOT NULL DEFAULT 0,
              status TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_floorplanner_region ON floorplanner_projects(region_id, updated_at DESC);
            CREATE TABLE IF NOT EXISTS floorplanner_revisions(
              project_id TEXT NOT NULL REFERENCES floorplanner_projects(id) ON DELETE CASCADE,
              revision INTEGER NOT NULL,
              parent_revision INTEGER,
              scene_hash TEXT NOT NULL,
              scene_json TEXT NOT NULL,
              origin TEXT NOT NULL,
              author TEXT,
              message TEXT,
              metadata_json TEXT NOT NULL,
              created_at TEXT NOT NULL,
              PRIMARY KEY(project_id, revision),
              UNIQUE(project_id, scene_hash)
            );
            CREATE TABLE IF NOT EXISTS floorplanner_events(
              seq INTEGER PRIMARY KEY AUTOINCREMENT,
              project_id TEXT NOT NULL REFERENCES floorplanner_projects(id) ON DELETE CASCADE,
              revision INTEGER,
              event_type TEXT NOT NULL,
              payload_json TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_floorplanner_events ON floorplanner_events(project_id, seq);
            CREATE TABLE IF NOT EXISTS floorplanner_exports(
              id TEXT PRIMARY KEY,
              project_id TEXT NOT NULL REFERENCES floorplanner_projects(id) ON DELETE CASCADE,
              revision INTEGER NOT NULL,
              format TEXT NOT NULL,
              path TEXT NOT NULL,
              sha256 TEXT NOT NULL,
              bytes INTEGER NOT NULL,
              semantic_manifest_json TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            """)

    def _row_project(self, row: sqlite3.Row, *, include_context: bool = True) -> dict[str, Any]:
        value = dict(row)
        context_raw = value.pop("context_json")
        if include_context:
            value["context"] = json.loads(context_raw)
        return value

    def create_project(self, payload: dict[str, Any]) -> dict[str, Any]:
        context = payload.get("context")
        if not isinstance(context, dict):
            raise ApiError("FLOORPLANNER_CONTEXT_REQUIRED", "context obrigatório", status=400)
        self.schemas.validate("modeling-context-package.schema.json", context)
        project_id = str(payload.get("id") or uuid.uuid4())
        name = str(payload.get("name") or "Projeto Floorplanner").strip()
        if not name:
            raise ApiError("FLOORPLANNER_NAME_REQUIRED", "name vazio", status=400)
        now = utc_now()
        scene = payload.get("scene")
        if scene is not None:
            self.schemas.validate("aedifex-scene.schema.json", scene)
        with self._connect() as db:
            try:
                db.execute("BEGIN IMMEDIATE")
                db.execute(
                    "INSERT INTO floorplanner_projects VALUES(?,?,?,?,?,?,?,?,?,?)",
                    (project_id, payload.get("arcz_project_id"), name, context["region_id"],
                     context["context_hash"], _json(context), 0,
                     "READY_WITHOUT_SCENE", now, now),
                )
                self._append_event_db(db, project_id, None, "project.created", {
                    "context_hash": context["context_hash"], "region_id": context["region_id"]
                })
                if scene is not None:
                    self._save_revision_db(db, project_id, scene, expected_revision=0,
                                           origin=str(payload.get("origin", "import")),
                                           author=payload.get("author"), message=payload.get("message"),
                                           metadata=payload.get("metadata", {}))
                db.execute("COMMIT")
            except sqlite3.IntegrityError as error:
                db.execute("ROLLBACK")
                raise ApiError("FLOORPLANNER_PROJECT_CONFLICT", project_id, status=409) from error
            except BaseException:
                db.execute("ROLLBACK")
                raise
        return self.get_project(project_id, include_scene=True)

    def list_projects(self, *, region_id: str | None = None, limit: int = 100) -> list[dict[str, Any]]:
        limit = max(1, min(int(limit), 500))
        sql = "SELECT * FROM floorplanner_projects"
        args: list[Any] = []
        if region_id:
            sql += " WHERE region_id=?"; args.append(region_id)
        sql += " ORDER BY updated_at DESC LIMIT ?"; args.append(limit)
        with self._connect() as db:
            return [self._row_project(row, include_context=False) for row in db.execute(sql, args)]

    def get_project(self, project_id: str, *, include_scene: bool = False,
                    revision: int | None = None) -> dict[str, Any]:
        with self._connect() as db:
            row = db.execute("SELECT * FROM floorplanner_projects WHERE id=?", (project_id,)).fetchone()
            if not row:
                raise ApiError("FLOORPLANNER_PROJECT_NOT_FOUND", project_id, status=404)
            result = self._row_project(row)
            if include_scene:
                target = int(revision if revision is not None else result["current_revision"])
                result["scene_revision"] = self.get_revision(project_id, target, db=db) if target > 0 else None
            result["exports"] = [dict(item) | {"semantic_manifest": json.loads(item["semantic_manifest_json"])}
                                 for item in db.execute(
                                     "SELECT * FROM floorplanner_exports WHERE project_id=? ORDER BY created_at DESC",
                                     (project_id,))]
            for item in result["exports"]:
                item.pop("semantic_manifest_json", None)
            return result

    def get_revision(self, project_id: str, revision: int, *, db: sqlite3.Connection | None = None) -> dict[str, Any]:
        owned = db is None
        connection = db or self._connect()
        try:
            row = connection.execute(
                "SELECT * FROM floorplanner_revisions WHERE project_id=? AND revision=?",
                (project_id, int(revision)),
            ).fetchone()
            if not row:
                raise ApiError("FLOORPLANNER_REVISION_NOT_FOUND", f"{project_id}@{revision}", status=404)
            value = dict(row)
            value["scene"] = json.loads(value.pop("scene_json"))
            value["metadata"] = json.loads(value.pop("metadata_json"))
            return value
        finally:
            if owned:
                connection.close()

    def save_revision(self, project_id: str, payload: dict[str, Any]) -> dict[str, Any]:
        scene = payload.get("scene")
        if not isinstance(scene, dict):
            raise ApiError("AEDIFEX_SCENE_REQUIRED", "scene obrigatório", status=400)
        self.schemas.validate("aedifex-scene.schema.json", scene)
        expected = payload.get("expected_revision")
        if isinstance(expected, bool) or not isinstance(expected, int) or expected < 0:
            raise ApiError("EXPECTED_REVISION_INVALID", repr(expected), status=400)
        with self._connect() as db:
            try:
                db.execute("BEGIN IMMEDIATE")
                result = self._save_revision_db(
                    db, project_id, scene, expected_revision=expected,
                    origin=str(payload.get("origin", "editor")), author=payload.get("author"),
                    message=payload.get("message"), metadata=payload.get("metadata", {}),
                )
                db.execute("COMMIT")
                return result
            except BaseException:
                db.execute("ROLLBACK")
                raise

    def _save_revision_db(self, db: sqlite3.Connection, project_id: str, scene: dict[str, Any], *,
                          expected_revision: int, origin: str, author: Any, message: Any,
                          metadata: dict[str, Any]) -> dict[str, Any]:
        row = db.execute("SELECT current_revision FROM floorplanner_projects WHERE id=?", (project_id,)).fetchone()
        if not row:
            raise ApiError("FLOORPLANNER_PROJECT_NOT_FOUND", project_id, status=404)
        current = int(row["current_revision"])
        if current != expected_revision:
            raise ApiError("FLOORPLANNER_VERSION_CONFLICT", "Recarregue a cena antes de salvar", status=409,
                           details={"expected_revision": expected_revision, "current_revision": current})
        scene_hash = canonical_json_hash(scene)
        previous = db.execute(
            "SELECT revision FROM floorplanner_revisions WHERE project_id=? AND scene_hash=?",
            (project_id, scene_hash),
        ).fetchone()
        if previous:
            return {"project_id": project_id, "revision": int(previous["revision"]),
                    "scene_hash": scene_hash, "changed": False, "current_revision": current}
        revision = current + 1
        now = utc_now()
        db.execute(
            "INSERT INTO floorplanner_revisions VALUES(?,?,?,?,?,?,?,?,?,?)",
            (project_id, revision, current if current else None, scene_hash, _json(scene), origin,
             None if author is None else str(author), None if message is None else str(message),
             _json(metadata if isinstance(metadata, dict) else {}), now),
        )
        db.execute(
            "UPDATE floorplanner_projects SET current_revision=?,status='READY',updated_at=? WHERE id=?",
            (revision, now, project_id),
        )
        self._append_event_db(db, project_id, revision, "scene.committed", {
            "revision": revision, "parent_revision": current or None, "scene_hash": scene_hash,
            "origin": origin, "message": message,
        })
        return {"project_id": project_id, "revision": revision, "scene_hash": scene_hash,
                "changed": True, "current_revision": revision, "created_at": now}

    def _append_event_db(self, db: sqlite3.Connection, project_id: str, revision: int | None,
                         event_type: str, payload: dict[str, Any]) -> None:
        db.execute(
            "INSERT INTO floorplanner_events(project_id,revision,event_type,payload_json,created_at) VALUES(?,?,?,?,?)",
            (project_id, revision, event_type, _json(payload), utc_now()),
        )

    def events_after(self, project_id: str, after: int = 0, limit: int = 200) -> list[dict[str, Any]]:
        limit = max(1, min(int(limit), 1000))
        with self._connect() as db:
            exists = db.execute("SELECT 1 FROM floorplanner_projects WHERE id=?", (project_id,)).fetchone()
            if not exists:
                raise ApiError("FLOORPLANNER_PROJECT_NOT_FOUND", project_id, status=404)
            rows = db.execute(
                "SELECT * FROM floorplanner_events WHERE project_id=? AND seq>? ORDER BY seq LIMIT ?",
                (project_id, int(after), limit),
            )
            result = []
            for row in rows:
                value = dict(row); value["payload"] = json.loads(value.pop("payload_json")); result.append(value)
            return result

    @staticmethod
    def _validate_glb(data: bytes) -> None:
        if len(data) < 20:
            raise ApiError("FLOORPLANNER_EXPORT_GLB_INVALID", "GLB sem chunk JSON", status=400)
        if data[:4] != b"glTF":
            raise ApiError("FLOORPLANNER_EXPORT_GLB_INVALID", "magic glTF ausente", status=400)
        version = int.from_bytes(data[4:8], "little", signed=False)
        declared = int.from_bytes(data[8:12], "little", signed=False)
        if version != 2:
            raise ApiError("FLOORPLANNER_EXPORT_GLB_VERSION_UNSUPPORTED", str(version), status=400)
        if declared != len(data):
            raise ApiError(
                "FLOORPLANNER_EXPORT_GLB_LENGTH_MISMATCH",
                f"declarado={declared}; recebido={len(data)}",
                status=400,
            )
        offset = 12
        chunk_index = 0
        while offset < len(data):
            if offset + 8 > len(data):
                raise ApiError("FLOORPLANNER_EXPORT_GLB_INVALID", "cabeçalho de chunk truncado", status=400)
            chunk_length = int.from_bytes(data[offset:offset + 4], "little", signed=False)
            chunk_type = int.from_bytes(data[offset + 4:offset + 8], "little", signed=False)
            offset += 8
            end = offset + chunk_length
            if chunk_length % 4 != 0 or end > len(data):
                raise ApiError("FLOORPLANNER_EXPORT_GLB_INVALID", "chunk desalinhado ou truncado", status=400)
            if chunk_index == 0:
                if chunk_type != 0x4E4F534A:  # JSON
                    raise ApiError("FLOORPLANNER_EXPORT_GLB_INVALID", "primeiro chunk não é JSON", status=400)
                try:
                    document = json.loads(data[offset:end].rstrip(b" \x00").decode("utf-8"))
                except Exception as error:
                    raise ApiError("FLOORPLANNER_EXPORT_GLB_JSON_INVALID", str(error), status=400) from error
                if not isinstance(document, dict) or document.get("asset", {}).get("version") != "2.0":
                    raise ApiError("FLOORPLANNER_EXPORT_GLB_JSON_INVALID", "asset.version 2.0 ausente", status=400)
            elif chunk_type not in {0x004E4942}:  # BIN; extensões desconhecidas são rejeitadas
                raise ApiError("FLOORPLANNER_EXPORT_GLB_CHUNK_UNSUPPORTED", hex(chunk_type), status=400)
            offset = end
            chunk_index += 1
        if offset != len(data) or chunk_index == 0:
            raise ApiError("FLOORPLANNER_EXPORT_GLB_INVALID", "estrutura de chunks inválida", status=400)

    def import_export_bytes(
        self, project_id: str, revision: int, data: bytes, *, format: str,
        semantic_manifest: dict[str, Any] | None, scene_hash: str | None, root: Path,
    ) -> dict[str, Any]:
        """Materializa um derivado binário enviado pelo editor local.

        O navegador nunca escolhe um caminho. O armazenamento é derivado de
        project/revision/content hash, escrito atomicamente e só publicado no
        catálogo depois de validar bytes, revisão e hash da cena.
        """
        normalized_format = str(format or "glb").lower().lstrip(".")
        if normalized_format != "glb":
            raise ApiError(
                "FLOORPLANNER_EXPORT_FORMAT_UNSUPPORTED", normalized_format, status=400,
                details={"supported": ["glb"]},
            )
        self._validate_glb(data)
        if not isinstance(semantic_manifest, dict):
            raise ApiError("FLOORPLANNER_EXPORT_MANIFEST_INVALID", "semantic_manifest precisa ser objeto", status=400)
        revision_record = self.get_revision(project_id, int(revision))
        if scene_hash and str(scene_hash) != revision_record["scene_hash"]:
            raise ApiError(
                "FLOORPLANNER_EXPORT_SCENE_HASH_MISMATCH",
                "O export não corresponde à revisão informada",
                status=409,
                details={
                    "expected": revision_record["scene_hash"],
                    "received": str(scene_hash),
                    "revision": int(revision),
                },
            )
        digest = sha256_bytes(data)
        project_bucket = sha256_bytes(project_id.encode("utf-8"))[:20]
        relative_path = Path("data") / "floorplanner" / "exports" / project_bucket / str(int(revision)) / f"{digest}.glb"
        destination = (root.resolve() / relative_path).resolve()
        try:
            destination.relative_to(root.resolve())
        except ValueError as error:
            raise ApiError("FLOORPLANNER_EXPORT_PATH_ESCAPE", str(destination), status=403) from error
        if destination.exists():
            if not destination.is_file() or sha256_file(destination) != digest:
                raise ApiError("FLOORPLANNER_EXPORT_STORAGE_CONFLICT", relative_path.as_posix(), status=409)
        else:
            atomic_write_bytes(destination, data)
        now = utc_now()
        with self._connect() as db:
            try:
                db.execute("BEGIN IMMEDIATE")
                current_revision = self.get_revision(project_id, int(revision), db=db)
                if scene_hash and str(scene_hash) != current_revision["scene_hash"]:
                    raise ApiError("FLOORPLANNER_EXPORT_SCENE_HASH_MISMATCH", str(scene_hash), status=409)
                existing = db.execute(
                    "SELECT * FROM floorplanner_exports WHERE project_id=? AND revision=? AND format=? AND sha256=?",
                    (project_id, int(revision), normalized_format, digest),
                ).fetchone()
                if existing:
                    value = dict(existing)
                    value["semantic_manifest"] = json.loads(value.pop("semantic_manifest_json"))
                    value["url"] = "/" + value["path"]
                    value["deduplicated"] = True
                    db.execute("COMMIT")
                    return value
                export_id = str(uuid.uuid4())
                db.execute(
                    "INSERT INTO floorplanner_exports VALUES(?,?,?,?,?,?,?,?,?)",
                    (export_id, project_id, int(revision), normalized_format, relative_path.as_posix(),
                     digest, len(data), _json(semantic_manifest), now),
                )
                self._append_event_db(db, project_id, int(revision), "export.registered", {
                    "export_id": export_id, "path": relative_path.as_posix(), "sha256": digest,
                    "bytes": len(data), "format": normalized_format, "source": "aedifex_scene_export",
                })
                db.execute("COMMIT")
            except BaseException:
                db.execute("ROLLBACK")
                raise
        return {
            "id": export_id, "project_id": project_id, "revision": int(revision),
            "format": normalized_format, "path": relative_path.as_posix(),
            "url": "/" + relative_path.as_posix(), "sha256": digest, "bytes": len(data),
            "semantic_manifest": semantic_manifest, "created_at": now, "deduplicated": False,
        }

    def register_export(self, project_id: str, revision: int, payload: dict[str, Any], root: Path) -> dict[str, Any]:
        path = (root / str(payload["path"])).resolve() if not Path(str(payload["path"])).is_absolute() else Path(str(payload["path"])).resolve()
        try:
            relative = path.relative_to(root.resolve()).as_posix()
        except ValueError as error:
            raise ApiError("FLOORPLANNER_EXPORT_PATH_ESCAPE", str(path), status=403) from error
        if not path.is_file():
            raise ApiError("FLOORPLANNER_EXPORT_MISSING", relative, status=404)
        actual_hash, actual_bytes = sha256_file(path), path.stat().st_size
        expected_hash = payload.get("sha256")
        if expected_hash and expected_hash != actual_hash:
            raise ApiError("FLOORPLANNER_EXPORT_HASH_MISMATCH", relative, status=409)
        export_id = str(uuid.uuid4())
        with self._connect() as db:
            self.get_revision(project_id, revision, db=db)
            db.execute(
                "INSERT INTO floorplanner_exports VALUES(?,?,?,?,?,?,?,?,?)",
                (export_id, project_id, int(revision), str(payload.get("format", path.suffix.lstrip("."))),
                 relative, actual_hash, actual_bytes, _json(payload.get("semantic_manifest", {})), utc_now()),
            )
            self._append_event_db(db, project_id, revision, "export.registered", {
                "export_id": export_id, "path": relative, "sha256": actual_hash, "bytes": actual_bytes,
            })
        return {"id": export_id, "project_id": project_id, "revision": revision,
                "path": relative, "sha256": actual_hash, "bytes": actual_bytes}
