from __future__ import annotations

"""Biblioteca de mídias de referência endereçada por conteúdo.

A API nunca aceita caminho arbitrário: o usuário/instalador materializa o
arquivo em data/imports/media-inbox e solicita a importação pelo nome relativo.
SVG ativo é recusado; imagem raster é decodificada para validar bytes.
"""

from datetime import datetime, timezone
import csv
import io
import json
import mimetypes
import os
import re
import struct
import zipfile
from xml.etree import ElementTree
from pathlib import Path
import shutil
import sqlite3
from typing import Any
import unicodedata
import uuid

from PIL import Image

from .errors import ApiError
from .hashing import sha256_file
from .schema_validation import SchemaRegistry


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


class ReferenceMediaStore:
    MAX_BYTES = 2 * 1024 * 1024 * 1024
    SAFE_EXTENSIONS = {
        # raster / HDR
        ".png", ".jpg", ".jpeg", ".webp", ".tif", ".tiff", ".exr", ".hdr",
        ".avif", ".heic", ".heif",
        # vídeo / áudio
        ".mp4", ".mov", ".webm", ".wav", ".mp3", ".flac",
        # documentos / dados
        ".pdf", ".json", ".geojson", ".csv", ".txt", ".md", ".kml", ".kmz", ".ies",
        # modelos / BIM / CAD / point cloud
        ".glb", ".gltf", ".obj", ".fbx", ".stl", ".ply", ".ifc", ".dxf", ".dwg",
        ".las", ".laz", ".blend",
    }
    JSON_MAX_BYTES = 64 * 1024 * 1024
    XML_MAX_BYTES = 64 * 1024 * 1024
    TEXT_PROBE_BYTES = 4 * 1024 * 1024

    def __init__(self, root: Path, schemas: SchemaRegistry):
        self.root = root.resolve()
        self.schemas = schemas
        self.inbox = self.root / "data" / "imports" / "media-inbox"
        self.store = self.root / "data" / "media"
        self.inbox.mkdir(parents=True, exist_ok=True)
        self.store.mkdir(parents=True, exist_ok=True)
        self.db_path = self.store / "registry.sqlite3"
        self._init_db()

    def _connect(self) -> sqlite3.Connection:
        db = sqlite3.connect(self.db_path, timeout=30)
        db.row_factory = sqlite3.Row
        db.execute("PRAGMA journal_mode=WAL")
        db.execute("PRAGMA synchronous=FULL")
        return db

    def _init_db(self) -> None:
        with self._connect() as db:
            db.executescript("""
            CREATE TABLE IF NOT EXISTS reference_media(
              id TEXT PRIMARY KEY,
              content_hash TEXT NOT NULL UNIQUE,
              original_name TEXT NOT NULL,
              stored_path TEXT NOT NULL,
              mime TEXT NOT NULL,
              category TEXT NOT NULL,
              bytes INTEGER NOT NULL,
              width INTEGER,
              height INTEGER,
              duration_seconds REAL,
              roles_json TEXT NOT NULL,
              license_json TEXT NOT NULL,
              provenance_json TEXT NOT NULL,
              metadata_json TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            """)

    def _safe_inbox_path(self, relative: str) -> Path:
        relative = str(relative).strip("/\\")
        lexical = Path(relative)
        if (not relative or lexical.is_absolute() or ".." in lexical.parts
                or any(part in {"", "."} for part in lexical.parts)):
            raise ApiError("MEDIA_PATH_ESCAPE", relative, status=403)
        cursor = self.inbox
        for part in lexical.parts:
            cursor = cursor / part
            # resolve() follows symlinks; check each lexical component first so
            # an in-inbox link never becomes invisible to the policy.
            if cursor.is_symlink():
                raise ApiError("MEDIA_SYMLINK_DENIED", relative, status=403)
        candidate = cursor.resolve()
        try:
            candidate.relative_to(self.inbox)
        except ValueError as error:
            raise ApiError("MEDIA_PATH_ESCAPE", relative, status=403) from error
        if not candidate.is_file():
            raise ApiError("MEDIA_FILE_NOT_FOUND", relative, status=404)
        return candidate

    @classmethod
    def _read_text_probe(cls, path: Path, *, limit: int | None = None) -> str:
        maximum = int(limit or cls.TEXT_PROBE_BYTES)
        with path.open("rb") as stream:
            raw = stream.read(maximum + 1)
        if len(raw) > maximum:
            raw = raw[:maximum]
        try:
            return raw.decode("utf-8-sig")
        except UnicodeDecodeError as error:
            raise ApiError("MEDIA_TEXT_ENCODING_INVALID", path.name, status=400) from error

    @staticmethod
    def _bmff_brand(head: bytes) -> str | None:
        if len(head) < 12 or head[4:8] != b"ftyp":
            return None
        return head[8:12].decode("ascii", errors="replace")

    @staticmethod
    def _safe_xml(raw: bytes, *, kind: str) -> ElementTree.Element:
        upper = raw[:4096].upper()
        if b"<!DOCTYPE" in upper or b"<!ENTITY" in upper:
            raise ApiError(f"MEDIA_{kind}_ACTIVE_XML_DENIED", "DOCTYPE/ENTITY não permitido", status=400)
        try:
            return ElementTree.fromstring(raw)
        except ElementTree.ParseError as error:
            raise ApiError(f"MEDIA_{kind}_INVALID", str(error), status=400) from error

    @classmethod
    def _detect(cls, path: Path) -> tuple[str, str, dict[str, Any]]:
        ext = path.suffix.lower()
        size = path.stat().st_size
        with path.open("rb") as stream:
            head = stream.read(512)
        metadata: dict[str, Any] = {"format": ext.lstrip("."), "validation": "header"}

        if ext in {".png", ".jpg", ".jpeg", ".webp", ".tif", ".tiff"}:
            try:
                with Image.open(path) as image:
                    image.verify()
                with Image.open(path) as image:
                    metadata.update(width=int(image.width), height=int(image.height), mode=str(image.mode),
                                    validation="decoded")
                    mime = Image.MIME.get(image.format, mimetypes.guess_type(path.name)[0] or "application/octet-stream")
                return mime, "image", metadata
            except Exception as error:
                raise ApiError("MEDIA_IMAGE_INVALID", str(error), status=400) from error
        if ext == ".exr":
            if head[:4] != bytes.fromhex("762f3101"):
                raise ApiError("MEDIA_EXR_INVALID", path.name, status=400)
            return "image/x-exr", "image", metadata
        if ext == ".hdr":
            if not (head.startswith(b"#?RADIANCE") or head.startswith(b"#?RGBE")):
                raise ApiError("MEDIA_HDR_INVALID", path.name, status=400)
            return "image/vnd.radiance", "image", metadata
        if ext in {".avif", ".heic", ".heif"}:
            brand = cls._bmff_brand(head)
            allowed = {"avif", "avis", "heic", "heix", "hevc", "hevx", "mif1", "msf1"}
            if brand not in allowed and not any(code.encode() in head[:128] for code in allowed):
                raise ApiError("MEDIA_HEIF_INVALID", path.name, status=400)
            metadata["brand"] = brand
            return ("image/avif" if ext == ".avif" else "image/heic"), "image", metadata

        if ext == ".glb":
            if len(head) < 12 or head[:4] != b"glTF" or int.from_bytes(head[4:8], "little") != 2:
                raise ApiError("MEDIA_GLB_INVALID", path.name, status=400)
            declared = int.from_bytes(head[8:12], "little")
            if declared != size:
                raise ApiError("MEDIA_GLB_LENGTH_MISMATCH", path.name, status=400,
                               details={"declared": declared, "actual": size})
            return "model/gltf-binary", "model3d", metadata
        if ext in {".gltf", ".json", ".geojson"}:
            if size > cls.JSON_MAX_BYTES:
                raise ApiError("MEDIA_JSON_TOO_LARGE", path.name, status=413,
                               details={"bytes": size, "max_bytes": cls.JSON_MAX_BYTES})
            try:
                value = json.loads(path.read_text(encoding="utf-8-sig"))
            except Exception as error:
                raise ApiError("MEDIA_JSON_INVALID", str(error), status=400) from error
            metadata["validation"] = "parsed"
            if ext == ".gltf":
                if not isinstance(value, dict) or str(value.get("asset", {}).get("version")) != "2.0":
                    raise ApiError("MEDIA_GLTF_INVALID", "asset.version 2.0 ausente", status=400)
                return "model/gltf+json", "model3d", metadata
            if ext == ".geojson":
                allowed_types = {"Feature", "FeatureCollection", "Point", "MultiPoint", "LineString",
                                 "MultiLineString", "Polygon", "MultiPolygon", "GeometryCollection"}
                if not isinstance(value, dict) or value.get("type") not in allowed_types:
                    raise ApiError("MEDIA_GEOJSON_INVALID", "type GeoJSON inválido", status=400)
                return "application/geo+json", "geodata", metadata
            return "application/json", "document", metadata

        if ext == ".pdf":
            if not head.startswith(b"%PDF-"):
                raise ApiError("MEDIA_PDF_INVALID", path.name, status=400)
            return "application/pdf", "document", metadata
        if ext in {".mp4", ".mov"}:
            brand = cls._bmff_brand(head)
            if not brand:
                raise ApiError("MEDIA_VIDEO_INVALID", path.name, status=400)
            metadata["brand"] = brand
            return ("video/mp4" if ext == ".mp4" else "video/quicktime"), "video", metadata
        if ext == ".webm":
            if not head.startswith(bytes.fromhex("1a45dfa3")):
                raise ApiError("MEDIA_WEBM_INVALID", path.name, status=400)
            return "video/webm", "video", metadata
        if ext == ".wav":
            if not (head.startswith(b"RIFF") and head[8:12] == b"WAVE"):
                raise ApiError("MEDIA_WAV_INVALID", path.name, status=400)
            return "audio/wav", "audio", metadata
        if ext == ".flac":
            if not head.startswith(b"fLaC"):
                raise ApiError("MEDIA_FLAC_INVALID", path.name, status=400)
            return "audio/flac", "audio", metadata
        if ext == ".mp3":
            if not (head.startswith(b"ID3") or (len(head) >= 2 and head[0] == 0xFF and (head[1] & 0xE0) == 0xE0)):
                raise ApiError("MEDIA_MP3_INVALID", path.name, status=400)
            return "audio/mpeg", "audio", metadata

        if ext == ".ifc":
            text = cls._read_text_probe(path)
            upper = text.upper()
            if "ISO-10303-21" not in upper or "FILE_SCHEMA" not in upper:
                raise ApiError("MEDIA_IFC_INVALID", path.name, status=400)
            schema = re.search(r"FILE_SCHEMA\s*\(\s*\(\s*'([^']+)'", upper)
            if schema:
                metadata["ifc_schema"] = schema.group(1)
            return "application/x-step", "bim", metadata
        if ext == ".dxf":
            if head.startswith(b"AutoCAD Binary DXF\r\n\x1a\x00"):
                metadata["encoding"] = "binary"
            else:
                text = cls._read_text_probe(path)
                normalized = re.sub(r"\s+", "", text[:8192]).upper()
                if "0SECTION" not in normalized or "2HEADER" not in normalized:
                    raise ApiError("MEDIA_DXF_INVALID", path.name, status=400)
                metadata["encoding"] = "ascii"
            return "image/vnd.dxf", "cad", metadata
        if ext == ".dwg":
            if not re.match(br"^AC10[0-9]{2}", head):
                raise ApiError("MEDIA_DWG_INVALID", path.name, status=400)
            metadata["dwg_signature"] = head[:6].decode("ascii", errors="replace")
            return "image/vnd.dwg", "cad", metadata
        if ext == ".fbx":
            if head.startswith(b"Kaydara FBX Binary  \x00\x1a\x00"):
                metadata["encoding"] = "binary"
            else:
                text = cls._read_text_probe(path)
                if "FBX" not in text[:2048].upper():
                    raise ApiError("MEDIA_FBX_INVALID", path.name, status=400)
                metadata["encoding"] = "ascii"
            return "application/x-fbx", "model3d", metadata
        if ext == ".stl":
            valid = False
            if size >= 84:
                count = struct.unpack("<I", head[80:84])[0] if len(head) >= 84 else 0
                valid = 84 + count * 50 == size
                if valid:
                    metadata.update(encoding="binary", triangles=count)
            if not valid:
                text = cls._read_text_probe(path)
                valid = text.lstrip().lower().startswith("solid") and "facet normal" in text.lower()
                if valid:
                    metadata["encoding"] = "ascii"
            if not valid:
                raise ApiError("MEDIA_STL_INVALID", path.name, status=400)
            return "model/stl", "model3d", metadata
        if ext == ".ply":
            text = cls._read_text_probe(path, limit=1024 * 1024)
            if not text.startswith("ply") or "end_header" not in text:
                raise ApiError("MEDIA_PLY_INVALID", path.name, status=400)
            fmt = re.search(r"^format\s+([^\s]+)", text, re.MULTILINE)
            metadata["encoding"] = fmt.group(1) if fmt else "unknown"
            return "application/x-ply", "pointcloud", metadata
        if ext in {".las", ".laz"}:
            if not head.startswith(b"LASF"):
                raise ApiError("MEDIA_LAS_INVALID", path.name, status=400)
            if len(head) >= 26:
                metadata["las_version"] = f"{head[24]}.{head[25]}"
            return "application/vnd.las", "pointcloud", metadata
        if ext == ".blend":
            if not head.startswith(b"BLENDER"):
                raise ApiError("MEDIA_BLEND_INVALID", path.name, status=400)
            metadata["pointer_size"] = chr(head[7]) if len(head) > 7 else None
            metadata["endianness"] = chr(head[8]) if len(head) > 8 else None
            metadata["blend_version"] = head[9:12].decode("ascii", errors="replace") if len(head) >= 12 else None
            return "application/x-blender", "model3d", metadata
        if ext == ".obj":
            text = cls._read_text_probe(path)
            if not re.search(r"(?m)^\s*v\s+[-+0-9.]", text) or not re.search(r"(?m)^\s*(f|l|p)\s+", text):
                raise ApiError("MEDIA_OBJ_INVALID", path.name, status=400)
            return "model/obj", "model3d", metadata

        if ext == ".kml":
            if size > cls.XML_MAX_BYTES:
                raise ApiError("MEDIA_KML_TOO_LARGE", path.name, status=413)
            root = cls._safe_xml(path.read_bytes(), kind="KML")
            if not root.tag.lower().endswith("kml"):
                raise ApiError("MEDIA_KML_INVALID", "raiz kml ausente", status=400)
            metadata["validation"] = "parsed"
            return "application/vnd.google-earth.kml+xml", "geodata", metadata
        if ext == ".kmz":
            try:
                with zipfile.ZipFile(path) as archive:
                    entries = archive.infolist()
                    if len(entries) > 10000:
                        raise ApiError("MEDIA_KMZ_TOO_MANY_ENTRIES", str(len(entries)), status=400)
                    total = 0
                    kml_names = []
                    for info in entries:
                        name = info.filename.replace("\\", "/")
                        parts = Path(name).parts
                        if name.startswith("/") or ".." in parts:
                            raise ApiError("MEDIA_KMZ_PATH_ESCAPE", name, status=400)
                        total += int(info.file_size)
                        if total > cls.MAX_BYTES:
                            raise ApiError("MEDIA_KMZ_EXPANDED_TOO_LARGE", str(total), status=413)
                        if name.lower().endswith(".kml"):
                            kml_names.append(name)
                    if not kml_names:
                        raise ApiError("MEDIA_KMZ_KML_MISSING", path.name, status=400)
                    raw = archive.read(kml_names[0])
                    cls._safe_xml(raw, kind="KML")
                    metadata.update(validation="archive+parsed", kml_entry=kml_names[0], entries=len(entries))
            except ApiError:
                raise
            except (zipfile.BadZipFile, OSError) as error:
                raise ApiError("MEDIA_KMZ_INVALID", str(error), status=400) from error
            return "application/vnd.google-earth.kmz", "geodata", metadata
        if ext == ".csv":
            text = cls._read_text_probe(path)
            try:
                sample = text[:65536]
                dialect = csv.Sniffer().sniff(sample)
                rows = list(csv.reader(io.StringIO(sample), dialect))[:5]
            except csv.Error as error:
                raise ApiError("MEDIA_CSV_INVALID", str(error), status=400) from error
            if not rows:
                raise ApiError("MEDIA_CSV_EMPTY", path.name, status=400)
            metadata.update(validation="parsed-sample", delimiter=dialect.delimiter, columns=len(rows[0]))
            return "text/csv", "dataset", metadata
        if ext == ".ies":
            text = cls._read_text_probe(path)
            if "TILT=" not in text.upper() or not (text.upper().startswith("IES") or "IESNA" in text[:256].upper()):
                raise ApiError("MEDIA_IES_INVALID", path.name, status=400)
            return "application/x-ies", "lighting", metadata
        if ext in {".txt", ".md"}:
            cls._read_text_probe(path)
            return ("text/markdown" if ext == ".md" else "text/plain"), "document", metadata
        raise ApiError("MEDIA_TYPE_UNSUPPORTED", ext or path.name, status=415)

    def import_bytes(self, filename: str, content: bytes, payload: dict[str, Any] | None = None) -> dict[str, Any]:
        """Importa upload binário sem expor caminho arbitrário ao cliente.

        Os bytes são primeiro materializados de forma atômica dentro do inbox,
        passam pelo mesmo detector/decoder e são removidos após o commit no
        content store. Um upload inválido nunca cria registro parcial.
        """
        payload = dict(payload or {})
        raw_name = unicodedata.normalize("NFC", str(filename or "")).strip()
        if (not raw_name or Path(raw_name).name != raw_name
                or any(ch in raw_name for ch in ("/", "\\", "\x00"))
                or any(ord(ch) < 32 or ord(ch) == 127 for ch in raw_name)):
            raise ApiError("MEDIA_FILENAME_INVALID", raw_name, status=400)
        # Preserve the exact, normalized display name in metadata. Use an ASCII
        # storage name only for temporary/content-store paths so Windows, ZIPs
        # and cross-platform backups remain predictable.
        safe_name = re.sub(r"[^A-Za-z0-9._ -]+", "_", raw_name).strip(" .")
        if not safe_name or Path(raw_name).suffix.lower() not in self.SAFE_EXTENSIONS:
            raise ApiError("MEDIA_TYPE_UNSUPPORTED", Path(raw_name).suffix.lower(), status=415)
        size = len(content)
        max_bytes = min(int(payload.get("max_bytes", self.MAX_BYTES)), self.MAX_BYTES)
        if size <= 0 or size > max_bytes:
            raise ApiError("MEDIA_SIZE_INVALID", safe_name, status=413,
                           details={"bytes": size, "max_bytes": max_bytes})
        upload_dir = self.inbox / ".uploads"
        upload_dir.mkdir(parents=True, exist_ok=True)
        token = uuid.uuid4().hex
        temporary = upload_dir / f".{token}.partial"
        published = upload_dir / f"{token}-{safe_name}"
        try:
            with temporary.open("wb") as stream:
                stream.write(content); stream.flush(); os.fsync(stream.fileno())
            os.replace(temporary, published)
            payload["path"] = published.relative_to(self.inbox).as_posix()
            payload["original_name"] = raw_name
            payload.setdefault("provenance", {"source": "browser_upload", "source_ref": raw_name})
            return self.import_from_inbox(payload)
        finally:
            temporary.unlink(missing_ok=True)
            published.unlink(missing_ok=True)

    def import_from_inbox(self, payload: dict[str, Any]) -> dict[str, Any]:
        source = self._safe_inbox_path(str(payload.get("path", "")))
        if source.suffix.lower() not in self.SAFE_EXTENSIONS:
            raise ApiError("MEDIA_TYPE_UNSUPPORTED", source.suffix, status=415)
        size = source.stat().st_size
        max_bytes = min(int(payload.get("max_bytes", self.MAX_BYTES)), self.MAX_BYTES)
        if size <= 0 or size > max_bytes:
            raise ApiError("MEDIA_SIZE_INVALID", source.name, status=413,
                           details={"bytes": size, "max_bytes": max_bytes})
        mime, category, detected = self._detect(source)
        digest = sha256_file(source)
        original_name = unicodedata.normalize("NFC", str(payload.get("original_name") or source.name)).strip()
        if (not original_name or Path(original_name).name != original_name
                or any(ch in original_name for ch in ("/", "\\", "\x00"))
                or any(ord(ch) < 32 or ord(ch) == 127 for ch in original_name)):
            raise ApiError("MEDIA_FILENAME_INVALID", original_name, status=400)
        storage_name = re.sub(r"[^A-Za-z0-9._ -]+", "_", original_name).strip(" .")
        if not storage_name:
            storage_name = f"media{source.suffix.lower()}"
        destination = self.store / digest[:2] / digest / storage_name
        destination.parent.mkdir(parents=True, exist_ok=True)
        if not destination.is_file():
            temporary = destination.with_name(f".{destination.name}.{uuid.uuid4().hex}.partial")
            shutil.copyfile(source, temporary)
            with temporary.open("rb") as stream: os.fsync(stream.fileno())
            os.replace(temporary, destination)
        record = {
            "schema_version": 1, "id": str(uuid.uuid4()), "content_hash": digest,
            "original_name": original_name, "stored_path": destination.relative_to(self.root).as_posix(),
            "mime": mime, "category": category, "bytes": size,
            "width": detected.get("width"), "height": detected.get("height"),
            "duration_seconds": payload.get("duration_seconds"),
            "roles": payload.get("roles", ["reference"]),
            "license": payload.get("license", {"id": "LicenseRef-UserProvided", "redistribution_allowed": False}),
            "provenance": payload.get("provenance", {"source": "user_inbox", "source_ref": source.name}),
            "metadata": {**detected, **(payload.get("metadata") or {})}, "created_at": utc_now(),
        }
        self.schemas.validate("reference-media.schema.json", record)
        with self._connect() as db:
            existing = db.execute("SELECT * FROM reference_media WHERE content_hash=?", (digest,)).fetchone()
            if existing: return self._decode(existing)
            db.execute("""INSERT INTO reference_media VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)""", (
                record["id"], digest, record["original_name"], record["stored_path"], mime, category, size,
                record["width"], record["height"], record["duration_seconds"],
                json.dumps(record["roles"], ensure_ascii=False), json.dumps(record["license"], ensure_ascii=False),
                json.dumps(record["provenance"], ensure_ascii=False), json.dumps(record["metadata"], ensure_ascii=False),
                record["created_at"],
            ))
        return record

    @staticmethod
    def _decode(row: sqlite3.Row) -> dict[str, Any]:
        value = dict(row)
        value["schema_version"] = 1
        for column, target in (("roles_json", "roles"), ("license_json", "license"),
                               ("provenance_json", "provenance"), ("metadata_json", "metadata")):
            value[target] = json.loads(value.pop(column))
        return value

    def list(self, *, category: str | None = None, limit: int = 200) -> list[dict[str, Any]]:
        sql = "SELECT * FROM reference_media"; args: list[Any] = []
        if category: sql += " WHERE category=?"; args.append(category)
        sql += " ORDER BY created_at DESC LIMIT ?"; args.append(max(1, min(int(limit), 1000)))
        with self._connect() as db:
            values = [self._decode(row) for row in db.execute(sql, args)]
        for value in values:
            value["content_url"] = f"/api/v2/reference-media/{value['id']}/content"
        return values

    def update_metadata(self, identifier: str, payload: dict[str, Any]) -> dict[str, Any]:
        current = self.get(identifier, verify=False)
        roles = payload.get("roles", current["roles"])
        metadata = payload.get("metadata", current["metadata"])
        license_value = payload.get("license", current["license"])
        if (not isinstance(roles, list) or not roles
                or any(not isinstance(item, str) or not item.strip() for item in roles)):
            raise ApiError("MEDIA_ROLES_INVALID", "roles precisa ser lista não vazia de strings", status=400)
        roles = list(dict.fromkeys(item.strip() for item in roles))
        if not isinstance(metadata, dict):
            raise ApiError("MEDIA_METADATA_INVALID", "metadata precisa ser objeto", status=400)
        if not isinstance(license_value, dict) or not license_value.get("id"):
            raise ApiError("MEDIA_LICENSE_INVALID", "license.id obrigatório", status=400)
        candidate = {
            **current,
            "roles": roles,
            "metadata": metadata,
            "license": license_value,
        }
        candidate.pop("integrity", None)
        candidate.pop("content_url", None)
        self.schemas.validate("reference-media.schema.json", candidate)
        with self._connect() as db:
            db.execute(
                "UPDATE reference_media SET roles_json=?,license_json=?,metadata_json=? WHERE id=?",
                (json.dumps(roles, ensure_ascii=False), json.dumps(license_value, ensure_ascii=False),
                 json.dumps(metadata, ensure_ascii=False), current["id"]),
            )
        return self.get(current["id"], verify=True)

    def content_path(self, identifier: str) -> tuple[Path, str, int]:
        value = self.get(identifier, verify=True)
        if not value.get("integrity", {}).get("ok"):
            raise ApiError("REFERENCE_MEDIA_CORRUPT", identifier, status=409)
        path = (self.root / value["stored_path"]).resolve()
        try:
            path.relative_to(self.store)
        except ValueError as error:
            raise ApiError("MEDIA_PATH_ESCAPE", value["stored_path"], status=403) from error
        return path, value["mime"], int(value["bytes"])

    def get(self, identifier: str, *, verify: bool = True) -> dict[str, Any]:
        with self._connect() as db:
            row = db.execute("SELECT * FROM reference_media WHERE id=? OR content_hash=?", (identifier, identifier)).fetchone()
        if not row: raise ApiError("REFERENCE_MEDIA_NOT_FOUND", identifier, status=404)
        value = self._decode(row)
        if verify:
            path = (self.root / value["stored_path"]).resolve()
            ok = path.is_file() and path.stat().st_size == value["bytes"] and sha256_file(path) == value["content_hash"]
            value["integrity"] = {"ok": ok}
            if not ok: value["integrity"]["error"] = "bytes/hash mismatch"
        value["content_url"] = f"/api/v2/reference-media/{value['id']}/content"
        return value
