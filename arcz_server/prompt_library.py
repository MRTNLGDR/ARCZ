from __future__ import annotations

"""Biblioteca local de prompts versionada e compilador determinístico."""

from datetime import datetime, timezone
import json
from pathlib import Path
import re
import sqlite3
from typing import Any
import uuid

from .ai_broker import LocalAIBroker
from .errors import ApiError
from .hashing import canonical_json_hash
from .schema_validation import SchemaRegistry

TOKEN = re.compile(r"\{\{\s*([A-Za-z0-9_.-]+)\s*\}\}")


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


class PromptLibrary:
    def __init__(self, root: Path, schemas: SchemaRegistry, ai: LocalAIBroker):
        self.root = root.resolve(); self.schemas = schemas; self.ai = ai
        self.db_path = self.root / "data" / "prompts" / "prompts.sqlite3"
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self._init_db(); self._load_builtins()

    def _connect(self) -> sqlite3.Connection:
        db = sqlite3.connect(self.db_path, timeout=30)
        db.row_factory = sqlite3.Row
        db.execute("PRAGMA journal_mode=WAL"); db.execute("PRAGMA synchronous=FULL")
        return db

    def _init_db(self) -> None:
        with self._connect() as db:
            db.executescript("""
            CREATE TABLE IF NOT EXISTS prompts(
              id TEXT PRIMARY KEY, slug TEXT NOT NULL UNIQUE, title TEXT NOT NULL,
              category TEXT NOT NULL, purpose TEXT NOT NULL, language TEXT NOT NULL,
              template TEXT NOT NULL, negative_template TEXT NOT NULL,
              tags_json TEXT NOT NULL, variables_json TEXT NOT NULL,
              version INTEGER NOT NULL, builtin INTEGER NOT NULL, active INTEGER NOT NULL,
              content_hash TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS prompt_versions(
              prompt_id TEXT NOT NULL, version INTEGER NOT NULL, snapshot_json TEXT NOT NULL,
              content_hash TEXT NOT NULL, created_at TEXT NOT NULL,
              PRIMARY KEY(prompt_id, version)
            );
            """)

    def _load_builtins(self) -> None:
        base = self.root / "resources" / "prompts"
        if not base.is_dir(): return
        for path in sorted(base.glob("*.json")):
            value = json.loads(path.read_text(encoding="utf-8"))
            value["builtin"] = True
            self.upsert(value, allow_builtin_update=True)

    @staticmethod
    def _decode(row: sqlite3.Row) -> dict[str, Any]:
        value = dict(row)
        value["tags"] = json.loads(value.pop("tags_json")); value["variables"] = json.loads(value.pop("variables_json"))
        value["builtin"] = bool(value["builtin"]); value["active"] = bool(value["active"])
        value["schema_version"] = 1
        return value

    def list(self, *, query: str | None = None, category: str | None = None, language: str | None = None,
             limit: int = 200) -> list[dict[str, Any]]:
        clauses = ["active=1"]; args: list[Any] = []
        if query:
            clauses.append("(lower(title) LIKE ? OR lower(slug) LIKE ? OR lower(tags_json) LIKE ?)")
            q = f"%{query.lower()}%"; args.extend([q, q, q])
        if category: clauses.append("category=?"); args.append(category)
        if language: clauses.append("language=?"); args.append(language)
        args.append(max(1, min(int(limit), 1000)))
        with self._connect() as db:
            rows = db.execute(f"SELECT * FROM prompts WHERE {' AND '.join(clauses)} ORDER BY builtin DESC,title LIMIT ?", args)
            return [self._decode(row) for row in rows]

    def get(self, identifier: str) -> dict[str, Any]:
        with self._connect() as db:
            row = db.execute("SELECT * FROM prompts WHERE id=? OR slug=?", (identifier, identifier)).fetchone()
        if not row: raise ApiError("PROMPT_NOT_FOUND", identifier, status=404)
        return self._decode(row)

    def upsert(self, value: dict[str, Any], *, allow_builtin_update: bool = False) -> dict[str, Any]:
        candidate = dict(value)
        candidate.setdefault("schema_version", 1); candidate.setdefault("id", str(uuid.uuid4()))
        candidate.setdefault("language", "pt-BR"); candidate.setdefault("negative_template", "")
        candidate.setdefault("tags", []); candidate.setdefault("variables", {})
        candidate.setdefault("version", 1); candidate.setdefault("builtin", False); candidate.setdefault("active", True)
        now = utc_now(); candidate.setdefault("created_at", now); candidate["updated_at"] = now
        content_payload = {key: candidate[key] for key in (
            "slug", "title", "category", "purpose", "language", "template", "negative_template", "tags", "variables"
        )}
        candidate["content_hash"] = canonical_json_hash(content_payload)
        self.schemas.validate("prompt-template.schema.json", candidate)
        with self._connect() as db:
            previous = db.execute("SELECT * FROM prompts WHERE slug=?", (candidate["slug"],)).fetchone()
            if previous:
                old = self._decode(previous)
                if old["builtin"] and not allow_builtin_update:
                    raise ApiError("BUILTIN_PROMPT_READ_ONLY", candidate["slug"], status=409)
                if old["content_hash"] == candidate["content_hash"]:
                    return old
                candidate["id"] = old["id"]; candidate["created_at"] = old["created_at"]
                candidate["version"] = int(old["version"]) + 1
            db.execute("BEGIN IMMEDIATE")
            try:
                db.execute("""INSERT INTO prompts VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                    ON CONFLICT(id) DO UPDATE SET slug=excluded.slug,title=excluded.title,category=excluded.category,
                    purpose=excluded.purpose,language=excluded.language,template=excluded.template,
                    negative_template=excluded.negative_template,tags_json=excluded.tags_json,
                    variables_json=excluded.variables_json,version=excluded.version,builtin=excluded.builtin,
                    active=excluded.active,content_hash=excluded.content_hash,updated_at=excluded.updated_at""", (
                    candidate["id"], candidate["slug"], candidate["title"], candidate["category"], candidate["purpose"],
                    candidate["language"], candidate["template"], candidate["negative_template"],
                    json.dumps(candidate["tags"], ensure_ascii=False), json.dumps(candidate["variables"], ensure_ascii=False),
                    candidate["version"], int(candidate["builtin"]), int(candidate["active"]), candidate["content_hash"],
                    candidate["created_at"], candidate["updated_at"],
                ))
                db.execute("INSERT OR REPLACE INTO prompt_versions VALUES(?,?,?,?,?)", (
                    candidate["id"], candidate["version"], json.dumps(candidate, ensure_ascii=False),
                    candidate["content_hash"], now,
                ))
                db.commit()
            except BaseException:
                db.rollback(); raise
        return self.get(candidate["id"])

    def versions(self, identifier: str, *, limit: int = 100) -> list[dict[str, Any]]:
        prompt = self.get(identifier)
        with self._connect() as db:
            rows = db.execute(
                "SELECT version,snapshot_json,content_hash,created_at FROM prompt_versions "
                "WHERE prompt_id=? ORDER BY version DESC LIMIT ?",
                (prompt["id"], max(1, min(int(limit), 1000))),
            ).fetchall()
        result: list[dict[str, Any]] = []
        for row in rows:
            try:
                snapshot = json.loads(row["snapshot_json"])
            except Exception as error:
                raise ApiError("PROMPT_VERSION_CORRUPT", f"{prompt['id']}@{row['version']}", status=500) from error
            result.append({
                "prompt_id": prompt["id"],
                "version": int(row["version"]),
                "content_hash": row["content_hash"],
                "created_at": row["created_at"],
                "snapshot": snapshot,
            })
        return result

    def duplicate(self, identifier: str, payload: dict[str, Any] | None = None) -> dict[str, Any]:
        source = self.get(identifier)
        values = dict(payload or {})
        slug = str(values.get("slug") or f"{source['slug']}-copy-{uuid.uuid4().hex[:6]}").strip().lower()
        title = str(values.get("title") or f"{source['title']} · cópia").strip()
        candidate = {
            **source,
            "id": str(uuid.uuid4()),
            "slug": slug,
            "title": title,
            "category": str(values.get("category") or source["category"]),
            "purpose": str(values.get("purpose") or source["purpose"]),
            "language": str(values.get("language") or source["language"]),
            "template": str(values.get("template") or source["template"]),
            "negative_template": str(values.get("negative_template") if "negative_template" in values else source["negative_template"]),
            "tags": list(values.get("tags") if isinstance(values.get("tags"), list) else source["tags"]),
            "variables": dict(values.get("variables") if isinstance(values.get("variables"), dict) else source["variables"]),
            "version": 1,
            "builtin": False,
            "active": True,
            "created_at": utc_now(),
        }
        candidate.pop("content_hash", None)
        candidate.pop("updated_at", None)
        return self.upsert(candidate)

    def archive(self, identifier: str) -> dict[str, Any]:
        prompt = self.get(identifier)
        if prompt["builtin"]:
            raise ApiError("BUILTIN_PROMPT_READ_ONLY", prompt["slug"], status=409)
        now = utc_now()
        with self._connect() as db:
            db.execute("UPDATE prompts SET active=0,updated_at=? WHERE id=?", (now, prompt["id"]))
        return {"ok": True, "id": prompt["id"], "slug": prompt["slug"], "active": False, "updated_at": now}

    @staticmethod
    def _lookup(values: dict[str, Any], name: str) -> Any:
        current: Any = values
        for part in name.split("."):
            if not isinstance(current, dict) or part not in current: return None
            current = current[part]
        return current

    def compile(self, identifier: str, variables: dict[str, Any], *, context: dict[str, Any] | None = None) -> dict[str, Any]:
        prompt = self.get(identifier)
        if not isinstance(variables, dict): raise ApiError("PROMPT_VARIABLES_INVALID", "variables precisa ser objeto", status=400)
        merged = {**(context or {}), **variables}
        required = [name for name, spec in prompt["variables"].items() if isinstance(spec, dict) and spec.get("required")]
        missing = [name for name in required if self._lookup(merged, name) in (None, "")]
        if missing: raise ApiError("PROMPT_VARIABLES_MISSING", ", ".join(missing), status=400, details={"missing": missing})
        def substitute(text: str) -> str:
            unresolved: list[str] = []
            def repl(match):
                value = self._lookup(merged, match.group(1))
                if value is None: unresolved.append(match.group(1)); return match.group(0)
                if isinstance(value, (dict, list)): return json.dumps(value, ensure_ascii=False, separators=(",", ":"))
                return str(value)
            result = TOKEN.sub(repl, text)
            if unresolved:
                raise ApiError("PROMPT_TOKEN_UNRESOLVED", ", ".join(sorted(set(unresolved))), status=400,
                               details={"tokens": sorted(set(unresolved))})
            return result.strip()
        compiled = {
            "schema_version": 1, "template_id": prompt["id"], "template_slug": prompt["slug"],
            "template_version": prompt["version"], "language": prompt["language"],
            "prompt": substitute(prompt["template"]),
            "negative_prompt": substitute(prompt["negative_template"]) if prompt["negative_template"] else "",
            "variables": variables, "context_hash": canonical_json_hash(context or {}),
        }
        compiled["content_hash"] = canonical_json_hash(compiled)
        return compiled

    def export_bundle(self, payload: dict[str, Any] | None = None) -> dict[str, Any]:
        """Exporta prompts como bundle portátil, íntegro e sem caminhos locais.

        O bundle preserva IDs, slugs, versões e variáveis. Histórico completo é
        opcional porque pode ser grande; por padrão a versão ativa de cada prompt
        é suficiente para round-trip. O hash cobre todo o conteúdo, exceto o
        próprio campo ``bundle_hash``.
        """
        options = dict(payload or {})
        identifiers = options.get("identifiers")
        include_builtins = bool(options.get("include_builtins", True))
        include_versions = bool(options.get("include_versions", False))
        if identifiers is not None and not isinstance(identifiers, list):
            raise ApiError("PROMPT_BUNDLE_IDENTIFIERS_INVALID", "identifiers precisa ser array", status=400)
        prompts = []
        if identifiers:
            prompts = [self.get(str(identifier)) for identifier in identifiers]
        else:
            prompts = self.list(limit=1000)
        if not include_builtins:
            prompts = [item for item in prompts if not item.get("builtin")]
        entries: list[dict[str, Any]] = []
        for prompt in prompts:
            entry: dict[str, Any] = {"prompt": prompt}
            if include_versions:
                entry["versions"] = self.versions(prompt["id"], limit=1000)
            entries.append(entry)
        bundle = {
            "schema_version": 1,
            "kind": "arcz.prompt-library.bundle",
            "exported_at": utc_now(),
            "prompt_count": len(entries),
            "include_versions": include_versions,
            "entries": entries,
        }
        bundle["bundle_hash"] = canonical_json_hash(bundle)
        return bundle

    def import_bundle(self, bundle: dict[str, Any], *, conflict: str = "duplicate") -> dict[str, Any]:
        """Importa bundle validado com política explícita de conflito.

        ``duplicate`` nunca sobrescreve: cria um novo slug/ID quando já existe.
        ``skip`` ignora conflitos. ``update`` cria nova versão apenas de prompts
        editáveis; built-ins continuam imutáveis.
        """
        if not isinstance(bundle, dict):
            raise ApiError("PROMPT_BUNDLE_INVALID", "bundle precisa ser objeto", status=400)
        if bundle.get("schema_version") != 1 or bundle.get("kind") != "arcz.prompt-library.bundle":
            raise ApiError("PROMPT_BUNDLE_VERSION_UNSUPPORTED", repr(bundle.get("schema_version")), status=422)
        received_hash = str(bundle.get("bundle_hash") or "")
        unsigned = dict(bundle); unsigned.pop("bundle_hash", None)
        actual_hash = canonical_json_hash(unsigned)
        if received_hash != actual_hash:
            raise ApiError("PROMPT_BUNDLE_HASH_MISMATCH", "bundle alterado ou corrompido", status=409,
                           details={"expected": received_hash, "actual": actual_hash})
        entries = bundle.get("entries")
        if not isinstance(entries, list):
            raise ApiError("PROMPT_BUNDLE_ENTRIES_INVALID", "entries precisa ser array", status=400)
        conflict = str(conflict or "duplicate").lower()
        if conflict not in {"duplicate", "skip", "update"}:
            raise ApiError("PROMPT_BUNDLE_CONFLICT_POLICY_INVALID", conflict, status=400)
        imported: list[dict[str, Any]] = []
        skipped: list[dict[str, Any]] = []
        errors: list[dict[str, Any]] = []
        for index, entry in enumerate(entries):
            try:
                if not isinstance(entry, dict) or not isinstance(entry.get("prompt"), dict):
                    raise ApiError("PROMPT_BUNDLE_ENTRY_INVALID", f"index={index}", status=400)
                candidate = dict(entry["prompt"])
                # Imported copies are user-owned. Bundles must never mutate the
                # installed immutable base templates.
                candidate["builtin"] = False
                candidate["active"] = True
                existing = None
                try:
                    existing = self.get(str(candidate.get("slug") or candidate.get("id")))
                except ApiError as error:
                    if error.code != "PROMPT_NOT_FOUND":
                        raise
                if existing:
                    if conflict == "skip":
                        skipped.append({"index": index, "slug": candidate.get("slug"), "reason": "conflict"})
                        continue
                    if conflict == "update" and not existing.get("builtin"):
                        candidate["id"] = existing["id"]
                        candidate["created_at"] = existing["created_at"]
                        candidate["version"] = existing["version"]
                    else:
                        suffix = uuid.uuid4().hex[:8]
                        candidate["id"] = str(uuid.uuid4())
                        candidate["slug"] = f"{candidate.get('slug') or 'prompt'}-import-{suffix}"
                        candidate["title"] = f"{candidate.get('title') or 'Prompt'} · importado"
                        candidate["version"] = 1
                        candidate["created_at"] = utc_now()
                else:
                    # Preserve the source ID only when it is not already used by
                    # another row; otherwise SQLite would reject a different slug.
                    try:
                        if candidate.get("id"):
                            self.get(str(candidate["id"]))
                            candidate["id"] = str(uuid.uuid4())
                    except ApiError as error:
                        if error.code != "PROMPT_NOT_FOUND":
                            raise
                candidate.pop("content_hash", None)
                candidate.pop("updated_at", None)
                imported.append(self.upsert(candidate))
            except ApiError as error:
                errors.append({"index": index, "code": error.code, "message": error.message})
            except Exception as error:
                errors.append({"index": index, "code": "PROMPT_BUNDLE_IMPORT_FAILED", "message": str(error)})
        return {
            "ok": not errors,
            "bundle_hash": received_hash,
            "conflict_policy": conflict,
            "imported": imported,
            "skipped": skipped,
            "errors": errors,
        }

    def enhance(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self.ai.request("prompt.enhance", payload, model_id=payload.get("model_id"),
                               timeout_seconds=payload.get("timeout_seconds"))

    def translate(self, payload: dict[str, Any]) -> dict[str, Any]:
        if not payload.get("target_language"): raise ApiError("TARGET_LANGUAGE_REQUIRED", "target_language obrigatório", status=400)
        return self.ai.request("prompt.translate", payload, model_id=payload.get("model_id"),
                               timeout_seconds=payload.get("timeout_seconds"))
