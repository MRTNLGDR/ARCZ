from __future__ import annotations

"""Chat global local com ferramentas reais, preview, aprovação e auditoria.

O modelo nunca recebe autoridade para escolher o projeto ou a revisão. Esses
valores vêm do contexto persistido da sessão e dos metadados enviados pela UI.
Leituras podem ser executadas automaticamente; exportações, mutações e ações
destrutivas exigem preview e aprovação explícita.
"""

from datetime import datetime, timezone
import json
from pathlib import Path
import sqlite3
from typing import Any, Callable
import uuid

from .ai_broker import LocalAIBroker
from .errors import ApiError, as_api_error
from .hashing import canonical_json_hash
from .reference_media import ReferenceMediaStore
from .schema_validation import SchemaRegistry


ToolInvoker = Callable[[str, dict[str, Any], dict[str, Any]], Any]
TOOL_RUN_STATUSES = frozenset({
    "PROPOSED", "PREVIEWING", "AWAITING_APPROVAL", "APPROVED", "RUNNING",
    "SUCCEEDED", "FAILED", "REJECTED", "CANCELLED",
})
TERMINAL_TOOL_RUN_STATUSES = frozenset({"SUCCEEDED", "FAILED", "REJECTED", "CANCELLED"})
MAX_TOOL_RESULT_BYTES = 8 * 1024 * 1024


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def normalize_tool_calls(value: Any) -> list[dict[str, Any]]:
    if value in (None, []):
        return []
    if not isinstance(value, list):
        raise ApiError("CHAT_TOOL_CALLS_INVALID", "tool_calls precisa ser lista", status=500)
    if len(value) > 32:
        raise ApiError("CHAT_TOOL_CALLS_LIMIT", "máximo de 32 tool calls por resposta", status=500)
    result: list[dict[str, Any]] = []
    for index, item in enumerate(value):
        if not isinstance(item, dict):
            raise ApiError("CHAT_TOOL_CALL_INVALID", f"tool_calls[{index}] precisa ser objeto", status=500)
        nested = item.get("function") if isinstance(item.get("function"), dict) else {}
        name = str(nested.get("name") or item.get("name") or "").strip()
        if not name or len(name) > 180:
            raise ApiError("CHAT_TOOL_NAME_INVALID", f"tool_calls[{index}]", status=500)
        raw_args = nested.get("arguments", item.get("arguments", item.get("input", {})))
        if isinstance(raw_args, str):
            try:
                arguments = json.loads(raw_args)
            except Exception as error:
                raise ApiError("CHAT_TOOL_ARGUMENTS_INVALID", name, status=500) from error
        else:
            arguments = raw_args
        if not isinstance(arguments, dict):
            raise ApiError("CHAT_TOOL_ARGUMENTS_INVALID", name, status=500)
        call_id = str(item.get("id") or f"call-{uuid.uuid4()}")
        result.append({"id": call_id, "name": name, "arguments": arguments, "status": "PROPOSED"})
    return result


def _bounded_json(value: Any, *, code: str = "CHAT_TOOL_RESULT_TOO_LARGE") -> str:
    try:
        raw = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False)
    except (TypeError, ValueError) as error:
        raise ApiError("CHAT_TOOL_RESULT_NOT_JSON", str(error), status=500) from error
    if len(raw.encode("utf-8")) > MAX_TOOL_RESULT_BYTES:
        raise ApiError(code, f"resultado excede {MAX_TOOL_RESULT_BYTES} bytes", status=413)
    return raw


def _safe_error(error: BaseException) -> dict[str, Any]:
    api = as_api_error(error, default_code="CHAT_TOOL_EXECUTION_FAILED")
    return api.payload()["error"]


def _tool_result_failed(value: Any) -> bool:
    if not isinstance(value, dict):
        return False
    if value.get("isError") is True:
        return True
    tool_result = value.get("tool_result")
    return isinstance(tool_result, dict) and tool_result.get("isError") is True


class ChatWorkspace:
    SCOPES = frozenset({"global", "world", "region", "floorplanner", "object", "render", "cinema", "street"})

    def __init__(self, root: Path, schemas: SchemaRegistry, ai: LocalAIBroker, media: ReferenceMediaStore):
        self.root = root.resolve()
        self.schemas = schemas
        self.ai = ai
        self.media = media
        self.db_path = self.root / "data" / "chat" / "chat.sqlite3"
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
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
            db.executescript("""
            CREATE TABLE IF NOT EXISTS chat_sessions(
              id TEXT PRIMARY KEY,title TEXT NOT NULL,scope TEXT NOT NULL,language TEXT NOT NULL,
              context_json TEXT NOT NULL,model_id TEXT,created_at TEXT NOT NULL,updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS chat_messages(
              id TEXT PRIMARY KEY,session_id TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
              seq INTEGER NOT NULL,role TEXT NOT NULL,content TEXT NOT NULL,attachments_json TEXT NOT NULL,
              tool_calls_json TEXT NOT NULL,metadata_json TEXT NOT NULL,content_hash TEXT NOT NULL,created_at TEXT NOT NULL,
              UNIQUE(session_id,seq)
            );
            CREATE TABLE IF NOT EXISTS chat_tool_runs(
              id TEXT PRIMARY KEY,
              session_id TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
              assistant_message_id TEXT REFERENCES chat_messages(id) ON DELETE SET NULL,
              call_id TEXT NOT NULL,
              tool_name TEXT NOT NULL,
              arguments_json TEXT NOT NULL,
              side_effect TEXT NOT NULL,
              requires_approval INTEGER NOT NULL,
              status TEXT NOT NULL,
              project_id TEXT,
              expected_revision INTEGER,
              approval_id TEXT,
              preview_json TEXT,
              result_json TEXT,
              error_json TEXT,
              request_hash TEXT NOT NULL,
              result_hash TEXT,
              approved_at TEXT,
              completed_at TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              UNIQUE(session_id,call_id)
            );
            CREATE INDEX IF NOT EXISTS idx_chat_tool_runs_session ON chat_tool_runs(session_id,created_at);
            CREATE INDEX IF NOT EXISTS idx_chat_tool_runs_status ON chat_tool_runs(status,updated_at);
            """)

    def create_session(self, payload: dict[str, Any]) -> dict[str, Any]:
        scope = str(payload.get("scope", "global"))
        if scope not in self.SCOPES:
            raise ApiError("CHAT_SCOPE_INVALID", scope, status=400)
        now = utc_now()
        session_id = str(payload.get("id") or uuid.uuid4())
        context = payload.get("context", {})
        if not isinstance(context, dict):
            raise ApiError("CHAT_CONTEXT_INVALID", "context precisa ser objeto", status=400)
        value = {
            "schema_version": 1,
            "id": session_id,
            "title": str(payload.get("title") or "Nova conversa"),
            "scope": scope,
            "language": str(payload.get("language") or "pt-BR"),
            "context": context,
            "model_id": payload.get("model_id"),
            "created_at": now,
            "updated_at": now,
        }
        self.schemas.validate("chat-session.schema.json", value)
        with self._connect() as db:
            try:
                db.execute(
                    "INSERT INTO chat_sessions VALUES(?,?,?,?,?,?,?,?)",
                    (session_id, value["title"], scope, value["language"], _bounded_json(context),
                     value["model_id"], now, now),
                )
            except sqlite3.IntegrityError as error:
                raise ApiError("CHAT_SESSION_CONFLICT", session_id, status=409) from error
        return value

    @staticmethod
    def _decode_session(row: sqlite3.Row) -> dict[str, Any]:
        value = dict(row)
        value["context"] = json.loads(value.pop("context_json"))
        value["schema_version"] = 1
        return value

    @staticmethod
    def _decode_message(row: sqlite3.Row) -> dict[str, Any]:
        value = dict(row)
        value["attachments"] = json.loads(value.pop("attachments_json"))
        value["tool_calls"] = json.loads(value.pop("tool_calls_json"))
        value["metadata"] = json.loads(value.pop("metadata_json"))
        value["schema_version"] = 1
        return value

    @staticmethod
    def _decode_tool_run(row: sqlite3.Row) -> dict[str, Any]:
        value = dict(row)
        value["arguments"] = json.loads(value.pop("arguments_json"))
        for field in ("preview_json", "result_json", "error_json"):
            raw = value.pop(field)
            value[field.removesuffix("_json")] = json.loads(raw) if raw else None
        value["requires_approval"] = bool(value["requires_approval"])
        value["schema_version"] = 1
        return value

    def list_sessions(self, limit: int = 100) -> list[dict[str, Any]]:
        with self._connect() as db:
            return [self._decode_session(row) for row in db.execute(
                "SELECT * FROM chat_sessions ORDER BY updated_at DESC LIMIT ?",
                (max(1, min(int(limit), 500)),),
            )]

    def get_session(self, session_id: str, *, include_messages: bool = True, limit: int = 500) -> dict[str, Any]:
        with self._connect() as db:
            row = db.execute("SELECT * FROM chat_sessions WHERE id=?", (session_id,)).fetchone()
            if not row:
                raise ApiError("CHAT_SESSION_NOT_FOUND", session_id, status=404)
            value = self._decode_session(row)
            if include_messages:
                value["messages"] = [self._decode_message(item) for item in db.execute(
                    "SELECT * FROM chat_messages WHERE session_id=? ORDER BY seq LIMIT ?",
                    (session_id, max(1, min(limit, 5000))),
                )]
                value["tool_runs"] = self.list_tool_runs(session_id=session_id, limit=limit, db=db)
            return value

    def append_message(self, session_id: str, payload: dict[str, Any]) -> dict[str, Any]:
        role = str(payload.get("role", "user"))
        content = str(payload.get("content", "")).strip()
        if role not in {"system", "user", "assistant", "tool"}:
            raise ApiError("CHAT_ROLE_INVALID", role, status=400)
        if not content:
            raise ApiError("CHAT_CONTENT_REQUIRED", "content vazio", status=400)
        attachments = payload.get("attachments", [])
        if not isinstance(attachments, list):
            raise ApiError("CHAT_ATTACHMENTS_INVALID", "attachments precisa ser lista", status=400)
        resolved = [self.media.get(str(item), verify=True) for item in attachments]
        invalid = [item["content_hash"] for item in resolved if not item.get("integrity", {}).get("ok")]
        if invalid:
            raise ApiError("CHAT_ATTACHMENT_CORRUPT", ",".join(invalid), status=409)
        if role == "assistant":
            tool_calls = normalize_tool_calls(payload.get("tool_calls", []))
            # Chamadas já processadas pelo orquestrador mantêm os campos de
            # auditoria/status acrescentados depois da normalização inicial.
            supplied = payload.get("tool_calls", [])
            if isinstance(supplied, list):
                for index, original in enumerate(supplied):
                    if index < len(tool_calls) and isinstance(original, dict):
                        for key in ("status", "run_id", "side_effect", "requires_approval", "preview", "error"):
                            if key in original:
                                tool_calls[index][key] = original[key]
        else:
            tool_calls = payload.get("tool_calls", [])
        if not isinstance(tool_calls, list):
            raise ApiError("CHAT_TOOL_CALLS_INVALID", "tool_calls precisa ser lista", status=400)
        metadata = payload.get("metadata", {})
        if not isinstance(metadata, dict):
            raise ApiError("CHAT_MESSAGE_METADATA_INVALID", "metadata precisa ser objeto", status=400)
        with self._connect() as db:
            session = db.execute("SELECT 1 FROM chat_sessions WHERE id=?", (session_id,)).fetchone()
            if not session:
                raise ApiError("CHAT_SESSION_NOT_FOUND", session_id, status=404)
            seq = int(db.execute(
                "SELECT COALESCE(MAX(seq),0)+1 n FROM chat_messages WHERE session_id=?", (session_id,),
            ).fetchone()["n"])
            now = utc_now()
            message_id = str(uuid.uuid4())
            record = {
                "schema_version": 1,
                "id": message_id,
                "session_id": session_id,
                "seq": seq,
                "role": role,
                "content": content,
                "attachments": [item["content_hash"] for item in resolved],
                "tool_calls": tool_calls,
                "metadata": metadata,
                "created_at": now,
            }
            record["content_hash"] = canonical_json_hash(record)
            db.execute(
                "INSERT INTO chat_messages VALUES(?,?,?,?,?,?,?,?,?,?)",
                (message_id, session_id, seq, role, content, _bounded_json(record["attachments"]),
                 _bounded_json(tool_calls), _bounded_json(metadata), record["content_hash"], now),
            )
            db.execute("UPDATE chat_sessions SET updated_at=? WHERE id=?", (now, session_id))
        return record

    def _update_message_tool_calls(self, message_id: str, tool_calls: list[dict[str, Any]]) -> None:
        with self._connect() as db:
            row = db.execute("SELECT * FROM chat_messages WHERE id=?", (message_id,)).fetchone()
            if not row:
                raise ApiError("CHAT_MESSAGE_NOT_FOUND", message_id, status=404)
            value = self._decode_message(row)
            value["tool_calls"] = tool_calls
            canonical = {key: value[key] for key in (
                "schema_version", "id", "session_id", "seq", "role", "content", "attachments",
                "tool_calls", "metadata", "created_at",
            )}
            content_hash = canonical_json_hash(canonical)
            db.execute(
                "UPDATE chat_messages SET tool_calls_json=?,content_hash=? WHERE id=?",
                (_bounded_json(tool_calls), content_hash, message_id),
            )

    def _create_tool_run(
        self,
        session_id: str,
        assistant_message_id: str,
        call: dict[str, Any],
        descriptor: dict[str, Any],
        context: dict[str, Any],
    ) -> dict[str, Any]:
        run_id = str(uuid.uuid4())
        now = utc_now()
        side_effect = str(descriptor.get("side_effect") or "mutate")
        request = {
            "session_id": session_id,
            "call_id": call["id"],
            "tool_name": call["name"],
            "arguments": call["arguments"],
            "project_id": context.get("project_id"),
            "expected_revision": context.get("expected_revision"),
        }
        with self._connect() as db:
            try:
                db.execute(
                    """INSERT INTO chat_tool_runs(
                       id,session_id,assistant_message_id,call_id,tool_name,arguments_json,side_effect,
                       requires_approval,status,project_id,expected_revision,approval_id,preview_json,
                       result_json,error_json,request_hash,result_hash,approved_at,completed_at,created_at,updated_at
                       ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)""",
                    (run_id, session_id, assistant_message_id, call["id"], call["name"],
                     _bounded_json(call["arguments"]), side_effect, int(bool(descriptor.get("requires_approval"))),
                     "PROPOSED", context.get("project_id"), context.get("expected_revision"), None, None,
                     None, None, canonical_json_hash(request), None, None, None, now, now),
                )
            except sqlite3.IntegrityError as error:
                raise ApiError("CHAT_TOOL_CALL_CONFLICT", call["id"], status=409) from error
        return self.get_tool_run(run_id)

    def _transition_tool_run(
        self,
        run_id: str,
        status: str,
        *,
        preview: Any = None,
        result: Any = None,
        error: Any = None,
        approval_id: str | None = None,
        approved_at: str | None = None,
        completed: bool = False,
        expected_from: set[str] | None = None,
    ) -> dict[str, Any]:
        if status not in TOOL_RUN_STATUSES:
            raise ValueError(f"status de tool run inválido: {status}")
        with self._connect() as db:
            row = db.execute("SELECT * FROM chat_tool_runs WHERE id=?", (run_id,)).fetchone()
            if not row:
                raise ApiError("CHAT_TOOL_RUN_NOT_FOUND", run_id, status=404)
            current = str(row["status"])
            if expected_from is not None and current not in expected_from:
                raise ApiError(
                    "CHAT_TOOL_RUN_STATE_CONFLICT",
                    f"{run_id}: {current} -> {status}",
                    status=409,
                    details={"current_status": current, "allowed": sorted(expected_from)},
                )
            now = utc_now()
            output_for_hash = result if result is not None else preview
            result_hash = canonical_json_hash(output_for_hash) if output_for_hash is not None else row["result_hash"]
            db.execute(
                """UPDATE chat_tool_runs SET status=?,preview_json=COALESCE(?,preview_json),
                   result_json=COALESCE(?,result_json),error_json=COALESCE(?,error_json),
                   approval_id=COALESCE(?,approval_id),result_hash=COALESCE(?,result_hash),
                   approved_at=COALESCE(?,approved_at),completed_at=CASE WHEN ? THEN ? ELSE completed_at END,
                   updated_at=? WHERE id=?""",
                (status, _bounded_json(preview) if preview is not None else None,
                 _bounded_json(result) if result is not None else None,
                 _bounded_json(error) if error is not None else None,
                 approval_id, result_hash, approved_at, int(completed), now, now, run_id),
            )
        return self.get_tool_run(run_id)

    def get_tool_run(self, run_id: str, *, db: sqlite3.Connection | None = None) -> dict[str, Any]:
        owned = db is None
        connection = db or self._connect()
        try:
            row = connection.execute("SELECT * FROM chat_tool_runs WHERE id=?", (run_id,)).fetchone()
            if not row:
                raise ApiError("CHAT_TOOL_RUN_NOT_FOUND", run_id, status=404)
            return self._decode_tool_run(row)
        finally:
            if owned:
                connection.close()

    def list_tool_runs(
        self,
        *,
        session_id: str | None = None,
        status: str | None = None,
        limit: int = 200,
        db: sqlite3.Connection | None = None,
    ) -> list[dict[str, Any]]:
        if status is not None and status not in TOOL_RUN_STATUSES:
            raise ApiError("CHAT_TOOL_RUN_STATUS_INVALID", status, status=400)
        sql = "SELECT * FROM chat_tool_runs"
        clauses: list[str] = []
        args: list[Any] = []
        if session_id:
            clauses.append("session_id=?")
            args.append(session_id)
        if status:
            clauses.append("status=?")
            args.append(status)
        if clauses:
            sql += " WHERE " + " AND ".join(clauses)
        sql += " ORDER BY created_at DESC LIMIT ?"
        args.append(max(1, min(int(limit), 2000)))
        owned = db is None
        connection = db or self._connect()
        try:
            return [self._decode_tool_run(row) for row in connection.execute(sql, args)]
        finally:
            if owned:
                connection.close()

    @staticmethod
    def _descriptor_map(tool_catalog: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
        result: dict[str, dict[str, Any]] = {}
        for item in tool_catalog:
            if isinstance(item, dict) and isinstance(item.get("name"), str) and item.get("available", True):
                result[item["name"]] = item
        return result

    def _trusted_tool_context(self, session: dict[str, Any], payload: dict[str, Any]) -> dict[str, Any]:
        session_context = session.get("context") if isinstance(session.get("context"), dict) else {}
        metadata = payload.get("metadata") if isinstance(payload.get("metadata"), dict) else {}
        session_project = session_context.get("floorplanner_project_id")
        metadata_project = metadata.get("floorplanner_project_id")
        if session_project and metadata_project and str(session_project) != str(metadata_project):
            raise ApiError("CHAT_TOOL_PROJECT_CONTEXT_CONFLICT", "project id divergente", status=409)
        project_id = str(session_project or metadata_project or "").strip() or None
        revision_value = metadata.get("expected_revision", metadata.get("current_revision", session_context.get("expected_revision")))
        expected_revision: int | None
        if revision_value is None:
            expected_revision = None
        elif isinstance(revision_value, bool) or not isinstance(revision_value, int) or revision_value < 0:
            raise ApiError("CHAT_TOOL_EXPECTED_REVISION_INVALID", repr(revision_value), status=400)
        else:
            expected_revision = revision_value
        return {"project_id": project_id, "expected_revision": expected_revision}

    def _inference_request(self, session: dict[str, Any], tool_catalog: list[dict[str, Any]]) -> dict[str, Any]:
        return {
            "session": {key: session[key] for key in ("id", "scope", "language", "context")},
            "messages": [{
                "role": item["role"],
                "content": item["content"],
                "attachments": item["attachments"],
                "tool_calls": item.get("tool_calls", []),
                "metadata": item.get("metadata", {}),
            } for item in session["messages"]],
            "tools": [{key: item[key] for key in (
                "name", "description", "inputSchema", "side_effect", "requires_approval",
            ) if key in item} for item in tool_catalog if item.get("available", True)],
            "tool_policy": {
                "read_tools_may_execute_automatically": True,
                "export_mutation_and_destructive_require_preview_and_user_approval": True,
                "project_and_revision_are_host_controlled": True,
                "never_claim_execution_without_tool_result": True,
            },
            "response_contract": {"content": "string", "tool_calls": "array"},
        }

    def _run_model_turn(
        self,
        session_id: str,
        payload: dict[str, Any],
        *,
        tool_catalog: list[dict[str, Any]],
        invoke_tool: ToolInvoker,
    ) -> dict[str, Any]:
        session = self.get_session(session_id, include_messages=True, limit=int(payload.get("history_limit", 100)))
        request = self._inference_request(session, tool_catalog)
        envelope = self.ai.request(
            "chat.global",
            request,
            model_id=payload.get("model_id") or session.get("model_id"),
            timeout_seconds=payload.get("timeout_seconds"),
        )
        result = envelope.get("result")
        if not isinstance(result, dict):
            raise ApiError("CHAT_MODEL_OUTPUT_INVALID", "resultado precisa ser objeto", status=500)
        content = result.get("content") or result.get("text")
        calls = normalize_tool_calls(result.get("tool_calls", []))
        if (not isinstance(content, str) or not content.strip()) and not calls:
            raise ApiError("CHAT_MODEL_CONTENT_MISSING", "modelo não retornou content/text nem tool_calls", status=500)
        content = str(content or "Ação proposta. Revise o preview antes de aplicar.").strip()
        assistant = self.append_message(session_id, {
            "role": "assistant",
            "content": content,
            "tool_calls": calls,
            "metadata": {"model": envelope.get("model"), "cache_key": envelope.get("cache_key")},
        })
        descriptor_map = self._descriptor_map(tool_catalog)
        trusted = self._trusted_tool_context(session, payload)
        auto_read = bool(payload.get("auto_execute_read", True))
        enriched: list[dict[str, Any]] = []
        auto_results: list[dict[str, Any]] = []
        pending_approval = False

        for call in calls:
            descriptor = descriptor_map.get(call["name"])
            if descriptor is None:
                descriptor = {
                    "name": call["name"], "side_effect": "mutate", "requires_approval": True,
                    "available": False,
                }
            side_effect = str(descriptor.get("side_effect") or "mutate")
            requires_approval = bool(descriptor.get("requires_approval")) or side_effect in {"export", "mutate", "destructive"}
            run = self._create_tool_run(session_id, assistant["id"], call, descriptor, trusted)
            view = {**call, "run_id": run["id"], "side_effect": side_effect,
                    "requires_approval": requires_approval}
            if not descriptor.get("available", True):
                error = {"code": "CHAT_TOOL_UNAVAILABLE", "message": call["name"], "retryable": False, "details": {}}
                self._transition_tool_run(run["id"], "FAILED", error=error, completed=True,
                                          expected_from={"PROPOSED"})
                view.update({"status": "FAILED", "error": error})
                enriched.append(view)
                continue
            try:
                if requires_approval:
                    self._transition_tool_run(run["id"], "PREVIEWING", expected_from={"PROPOSED"})
                    preview = invoke_tool(call["name"], call["arguments"], {
                        **trusted, "dry_run": True, "approval_id": None,
                    })
                    if _tool_result_failed(preview):
                        raise ApiError("CHAT_TOOL_PREVIEW_FAILED", call["name"], status=422,
                                       details={"result": preview})
                    changed = bool(preview.get("changed", True)) if isinstance(preview, dict) else True
                    if side_effect in {"mutate", "destructive"} and not changed:
                        final = self._transition_tool_run(run["id"], "SUCCEEDED", preview=preview,
                                                          result=preview, completed=True,
                                                          expected_from={"PREVIEWING"})
                        view.update({"status": "SUCCEEDED", "preview": preview})
                        auto_results.append({"call": view, "run": final, "result": preview})
                    else:
                        self._transition_tool_run(run["id"], "AWAITING_APPROVAL", preview=preview,
                                                  expected_from={"PREVIEWING"})
                        view.update({"status": "AWAITING_APPROVAL", "preview": preview})
                        pending_approval = True
                elif auto_read:
                    self._transition_tool_run(run["id"], "RUNNING", expected_from={"PROPOSED"})
                    tool_result = invoke_tool(call["name"], call["arguments"], {
                        **trusted, "dry_run": True, "approval_id": None,
                    })
                    if _tool_result_failed(tool_result):
                        raise ApiError("CHAT_TOOL_EXECUTION_FAILED", call["name"], status=422,
                                       details={"result": tool_result})
                    final = self._transition_tool_run(run["id"], "SUCCEEDED", result=tool_result,
                                                      completed=True, expected_from={"RUNNING"})
                    view.update({"status": "SUCCEEDED"})
                    auto_results.append({"call": view, "run": final, "result": tool_result})
                else:
                    self._transition_tool_run(run["id"], "AWAITING_APPROVAL", expected_from={"PROPOSED"})
                    view["status"] = "AWAITING_APPROVAL"
                    pending_approval = True
            except BaseException as error:
                structured = _safe_error(error)
                current = self.get_tool_run(run["id"])["status"]
                allowed = {current} if current not in TERMINAL_TOOL_RUN_STATUSES else None
                if allowed:
                    self._transition_tool_run(run["id"], "FAILED", error=structured, completed=True,
                                              expected_from=allowed)
                view.update({"status": "FAILED", "error": structured})
            enriched.append(view)

        if calls:
            self._update_message_tool_calls(assistant["id"], enriched)
            assistant["tool_calls"] = enriched
        for item in auto_results:
            tool_message = self.append_message(session_id, {
                "role": "tool",
                "content": _bounded_json({
                    "tool": item["call"]["name"],
                    "status": "SUCCEEDED",
                    "result": item["result"],
                }),
                "attachments": [],
                "metadata": {
                    "tool_call_id": item["call"]["id"],
                    "tool_run_id": item["run"]["id"],
                    "tool_name": item["call"]["name"],
                    "tool_status": "SUCCEEDED",
                    "result_hash": item["run"].get("result_hash"),
                },
            })
            item["message"] = tool_message
        return {
            "assistant": assistant,
            "inference": {"model": envelope.get("model"), "cache_key": envelope.get("cache_key")},
            "auto_results": auto_results,
            "pending_approval": pending_approval,
        }

    def _agent_loop(
        self,
        session_id: str,
        payload: dict[str, Any],
        *,
        tool_catalog: list[dict[str, Any]],
        invoke_tool: ToolInvoker,
    ) -> dict[str, Any]:
        max_steps = max(1, min(int(payload.get("max_agent_steps", 4)), 4))
        turns: list[dict[str, Any]] = []
        for _ in range(max_steps):
            turn = self._run_model_turn(
                session_id, payload, tool_catalog=tool_catalog, invoke_tool=invoke_tool,
            )
            turns.append(turn)
            if turn["pending_approval"] or not turn["assistant"].get("tool_calls"):
                break
            if not turn["auto_results"]:
                break
        latest = turns[-1]
        return {
            "assistant": latest["assistant"],
            "assistant_messages": [item["assistant"] for item in turns],
            "inference": latest["inference"],
            "pending_approval": any(item["pending_approval"] for item in turns),
            "tool_runs": self.list_tool_runs(session_id=session_id, limit=200),
        }

    def respond(
        self,
        session_id: str,
        payload: dict[str, Any],
        *,
        tool_catalog: list[dict[str, Any]],
        invoke_tool: ToolInvoker | None = None,
    ) -> dict[str, Any]:
        # Backward-compatible default for text-only inference. A tool call can
        # never execute without a concrete local invoker; it fails visibly and
        # is recorded as a structured tool-run error instead of being simulated.
        def unavailable_invoker(name: str, arguments: dict[str, Any], context: dict[str, Any]) -> Any:
            raise ApiError(
                "CHAT_TOOL_INVOKER_UNAVAILABLE",
                f"Nenhum executor local foi registrado para {name}",
                status=503,
                details={"tool": name, "arguments": arguments, "context": context},
            )

        user = self.append_message(session_id, {
            "role": "user",
            "content": payload.get("content"),
            "attachments": payload.get("attachments", []),
            "metadata": payload.get("metadata", {}),
        })
        return {"user": user, **self._agent_loop(
            session_id, payload, tool_catalog=tool_catalog, invoke_tool=invoke_tool or unavailable_invoker,
        )}

    def continue_after_tools(
        self,
        session_id: str,
        payload: dict[str, Any],
        *,
        tool_catalog: list[dict[str, Any]],
        invoke_tool: ToolInvoker,
    ) -> dict[str, Any]:
        return self._agent_loop(session_id, payload, tool_catalog=tool_catalog, invoke_tool=invoke_tool)

    def approve_tool_run(
        self,
        run_id: str,
        payload: dict[str, Any],
        *,
        invoke_tool: ToolInvoker,
    ) -> dict[str, Any]:
        run = self.get_tool_run(run_id)
        if run["status"] != "AWAITING_APPROVAL":
            raise ApiError("CHAT_TOOL_RUN_NOT_APPROVABLE", run["status"], status=409)
        expected = payload.get("expected_revision", run.get("expected_revision"))
        revision_required = bool(run.get("project_id")) or str(run.get("tool_name", "")).startswith("aedifex.")
        if expected is None and not revision_required:
            expected = 0
        if isinstance(expected, bool) or not isinstance(expected, int) or expected < 0:
            raise ApiError("CHAT_TOOL_EXPECTED_REVISION_REQUIRED", "expected_revision", status=400)
        if run.get("expected_revision") is not None and expected != run["expected_revision"]:
            raise ApiError(
                "CHAT_TOOL_APPROVAL_REVISION_MISMATCH",
                f"preview={run['expected_revision']} approval={expected}",
                status=409,
            )
        approval_id = str(uuid.uuid4())
        now = utc_now()
        self._transition_tool_run(
            run_id, "APPROVED", approval_id=approval_id, approved_at=now,
            expected_from={"AWAITING_APPROVAL"},
        )
        self._transition_tool_run(run_id, "RUNNING", expected_from={"APPROVED"})
        try:
            result = invoke_tool(run["tool_name"], run["arguments"], {
                "project_id": run.get("project_id"),
                "expected_revision": expected,
                "dry_run": False,
                "approval_id": approval_id,
            })
            if _tool_result_failed(result):
                raise ApiError("CHAT_TOOL_EXECUTION_FAILED", run["tool_name"], status=422,
                               details={"result": result})
            final = self._transition_tool_run(
                run_id, "SUCCEEDED", result=result, completed=True, expected_from={"RUNNING"},
            )
            tool_message = self.append_message(run["session_id"], {
                "role": "tool",
                "content": _bounded_json({"tool": run["tool_name"], "status": "SUCCEEDED", "result": result}),
                "attachments": [],
                "metadata": {
                    "tool_call_id": run["call_id"],
                    "tool_run_id": run_id,
                    "tool_name": run["tool_name"],
                    "tool_status": "SUCCEEDED",
                    "approval_id": approval_id,
                    "result_hash": final.get("result_hash"),
                },
            })
            self._set_call_status(run, "SUCCEEDED")
            return {"tool_run": final, "tool_message": tool_message, "result": result}
        except BaseException as error:
            structured = _safe_error(error)
            final = self._transition_tool_run(
                run_id, "FAILED", error=structured, completed=True, expected_from={"RUNNING"},
            )
            self._set_call_status(run, "FAILED", error=structured)
            raise ApiError(
                str(structured.get("code") or "CHAT_TOOL_EXECUTION_FAILED"),
                str(structured.get("message") or run["tool_name"]),
                status=int(structured.get("status") or 422) if isinstance(structured.get("status"), int) else 422,
                details={"tool_run": final, "cause": structured},
            ) from error

    def reject_tool_run(self, run_id: str, payload: dict[str, Any] | None = None) -> dict[str, Any]:
        run = self.get_tool_run(run_id)
        if run["status"] != "AWAITING_APPROVAL":
            raise ApiError("CHAT_TOOL_RUN_NOT_REJECTABLE", run["status"], status=409)
        reason = str((payload or {}).get("reason") or "explicit_user_rejection")
        result = {"rejected": True, "reason": reason}
        final = self._transition_tool_run(
            run_id, "REJECTED", result=result, completed=True, expected_from={"AWAITING_APPROVAL"},
        )
        tool_message = self.append_message(run["session_id"], {
            "role": "tool",
            "content": _bounded_json({"tool": run["tool_name"], "status": "REJECTED", "result": result}),
            "attachments": [],
            "metadata": {
                "tool_call_id": run["call_id"], "tool_run_id": run_id,
                "tool_name": run["tool_name"], "tool_status": "REJECTED",
            },
        })
        self._set_call_status(run, "REJECTED")
        return {"tool_run": final, "tool_message": tool_message}

    def _set_call_status(self, run: dict[str, Any], status: str, *, error: Any = None) -> None:
        message_id = run.get("assistant_message_id")
        if not message_id:
            return
        with self._connect() as db:
            row = db.execute("SELECT * FROM chat_messages WHERE id=?", (message_id,)).fetchone()
        if not row:
            return
        message = self._decode_message(row)
        changed = False
        for call in message.get("tool_calls", []):
            if str(call.get("id")) == str(run.get("call_id")):
                call["status"] = status
                if error is not None:
                    call["error"] = error
                changed = True
        if changed:
            self._update_message_tool_calls(message_id, message["tool_calls"])
