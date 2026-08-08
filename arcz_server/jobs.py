from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass
from datetime import datetime, timezone
import json
from pathlib import Path
import queue
import sqlite3
import threading
import time
import traceback
import uuid
from typing import Any, Protocol

from .atomic_io import atomic_write_json
from .errors import ApiError, as_api_error
from .hashing import sha256_file
from .schema_validation import SchemaRegistry

STATUSES = frozenset({
    "QUEUED", "RUNNING", "CANCEL_REQUESTED", "CANCELLED", "COMPLETED",
    "FAILED_RETRYABLE", "FAILED_PERMANENT",
})
STAGES = (
    "VALIDATE_REQUEST", "RESOLVE_REGION", "ACQUIRE_INPUTS", "BUILD_CONTEXT",
    "RESOLVE_STYLE", "PLAN_TILES", "ESTIMATE_BUDGET", "GENERATE",
    "VALIDATE_OUTPUT", "STAGE_RESULT", "APPLY_TRANSACTION", "INDEX", "PERSIST", "DONE",
)
TERMINAL_STATUSES = frozenset({"CANCELLED", "COMPLETED", "FAILED_RETRYABLE", "FAILED_PERMANENT"})


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


class JobCancelled(RuntimeError):
    pass


class Worker(Protocol):
    def __call__(self, context: "JobContext", request: dict[str, Any]) -> str | Path | dict[str, Any]: ...


class JobStore:
    """Fila persistente local.

    A tabela de eventos é append-only. O servidor pode reiniciar sem perder a
    explicação do que ocorreu; jobs interrompidos são marcados como retryable,
    nunca deixados eternamente em RUNNING.
    """

    def __init__(self, db_path: Path):
        self.db_path = db_path.resolve()
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self._write_lock = threading.RLock()
        self._init_db()
        self.recover_interrupted()

    def _connect(self) -> sqlite3.Connection:
        db = sqlite3.connect(self.db_path, timeout=30, isolation_level=None)
        db.row_factory = sqlite3.Row
        db.execute("PRAGMA journal_mode=WAL")
        db.execute("PRAGMA synchronous=FULL")
        db.execute("PRAGMA foreign_keys=ON")
        db.execute("PRAGMA busy_timeout=30000")
        return db

    def _init_db(self) -> None:
        with self._connect() as db:
            db.executescript("""
            CREATE TABLE IF NOT EXISTS jobs(
              id TEXT PRIMARY KEY,
              kind TEXT NOT NULL,
              status TEXT NOT NULL,
              stage TEXT NOT NULL,
              progress REAL NOT NULL,
              generation_epoch INTEGER NOT NULL,
              request_json TEXT NOT NULL,
              error_json TEXT,
              result_manifest TEXT,
              cancel_reason TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_jobs_status_created ON jobs(status, created_at);
            CREATE TABLE IF NOT EXISTS job_events(
              seq INTEGER PRIMARY KEY AUTOINCREMENT,
              job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
              timestamp TEXT NOT NULL,
              event_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_job_events_job_seq ON job_events(job_id, seq);
            """)

    def recover_interrupted(self) -> int:
        now = utc_now()
        error = json.dumps({
            "code": "SERVER_RESTARTED",
            "message": "O processo local reiniciou durante o job. O job pode ser reexecutado.",
            "retryable": True,
            "details": {},
        }, ensure_ascii=False, separators=(",", ":"))
        with self._write_lock, self._connect() as db:
            rows = db.execute("SELECT id FROM jobs WHERE status IN ('RUNNING','CANCEL_REQUESTED')").fetchall()
            db.execute(
                "UPDATE jobs SET status='FAILED_RETRYABLE', error_json=?, updated_at=? "
                "WHERE status IN ('RUNNING','CANCEL_REQUESTED')", (error, now),
            )
            for row in rows:
                self._append_event_connection(db, row["id"], {
                    "type": "terminal", "status": "FAILED_RETRYABLE", "stage": "DONE",
                    "progress": 1.0, "error": json.loads(error),
                })
        return len(rows)

    @staticmethod
    def _row_to_dict(row: sqlite3.Row) -> dict[str, Any]:
        return {
            "id": row["id"], "kind": row["kind"], "status": row["status"],
            "stage": row["stage"], "progress": float(row["progress"]),
            "generation_epoch": int(row["generation_epoch"]),
            "created_at": row["created_at"], "updated_at": row["updated_at"],
            "request": json.loads(row["request_json"]),
            "error": json.loads(row["error_json"]) if row["error_json"] else None,
            "result_manifest": row["result_manifest"], "cancel_reason": row["cancel_reason"],
        }

    def create(self, kind: str, request: dict[str, Any], generation_epoch: int = 0) -> dict[str, Any]:
        job_id = uuid.uuid4().hex
        now = utc_now()
        with self._write_lock, self._connect() as db:
            db.execute(
                "INSERT INTO jobs(id,kind,status,stage,progress,generation_epoch,request_json,error_json,"
                "result_manifest,cancel_reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
                (job_id, kind, "QUEUED", "VALIDATE_REQUEST", 0.0, int(generation_epoch),
                 json.dumps(request, ensure_ascii=False, separators=(",", ":")), None, None, None, now, now),
            )
            self._append_event_connection(db, job_id, {
                "type": "created", "status": "QUEUED", "stage": "VALIDATE_REQUEST", "progress": 0.0,
            })
        return self.get(job_id)

    def get(self, job_id: str) -> dict[str, Any]:
        with self._connect() as db:
            row = db.execute("SELECT * FROM jobs WHERE id=?", (job_id,)).fetchone()
        if row is None:
            raise ApiError("JOB_NOT_FOUND", f"Job não encontrado: {job_id}", status=404)
        return self._row_to_dict(row)

    def list(self, *, status: str | None = None, limit: int = 100) -> list[dict[str, Any]]:
        limit = max(1, min(int(limit), 1000))
        sql = "SELECT * FROM jobs"
        params: list[Any] = []
        if status:
            if status not in STATUSES:
                raise ApiError("JOB_STATUS_INVALID", status, status=400)
            sql += " WHERE status=?"
            params.append(status)
        sql += " ORDER BY created_at DESC LIMIT ?"
        params.append(limit)
        with self._connect() as db:
            return [self._row_to_dict(row) for row in db.execute(sql, params)]

    def update(self, job_id: str, *, status: str | None = None, stage: str | None = None,
               progress: float | None = None, error: dict[str, Any] | None = None,
               result_manifest: str | None = None, cancel_reason: str | None = None,
               event_type: str = "progress") -> dict[str, Any]:
        if status is not None and status not in STATUSES:
            raise ValueError(f"status inválido: {status}")
        if stage is not None and stage not in STAGES:
            raise ValueError(f"stage inválido: {stage}")
        if progress is not None and not 0.0 <= float(progress) <= 1.0:
            raise ValueError("progress fora de 0..1")
        fields: list[str] = ["updated_at=?"]
        values: list[Any] = [utc_now()]
        for column, value in (("status", status), ("stage", stage), ("progress", progress),
                              ("result_manifest", result_manifest), ("cancel_reason", cancel_reason)):
            if value is not None:
                fields.append(f"{column}=?")
                values.append(value)
        if error is not None:
            fields.append("error_json=?")
            values.append(json.dumps(error, ensure_ascii=False, separators=(",", ":")))
        values.append(job_id)
        with self._write_lock, self._connect() as db:
            current = db.execute("SELECT status FROM jobs WHERE id=?", (job_id,)).fetchone()
            if current is None:
                raise ApiError("JOB_NOT_FOUND", job_id, status=404)
            if current["status"] in TERMINAL_STATUSES and status not in (None, current["status"]):
                raise ApiError("JOB_TERMINAL", "Job terminal não pode mudar de estado", status=409)
            db.execute(f"UPDATE jobs SET {', '.join(fields)} WHERE id=?", values)
            updated = db.execute("SELECT * FROM jobs WHERE id=?", (job_id,)).fetchone()
            event = {
                "type": event_type, "status": updated["status"], "stage": updated["stage"],
                "progress": float(updated["progress"]),
            }
            if error is not None:
                event["error"] = error
            if result_manifest is not None:
                event["result_manifest"] = result_manifest
            self._append_event_connection(db, job_id, event)
        return self._row_to_dict(updated)

    def request_cancel(self, job_id: str, reason: str) -> dict[str, Any]:
        job = self.get(job_id)
        if job["status"] in TERMINAL_STATUSES:
            return job
        if job["status"] == "QUEUED":
            return self.update(job_id, status="CANCELLED", stage="DONE", progress=1.0,
                               cancel_reason=reason, event_type="terminal")
        return self.update(job_id, status="CANCEL_REQUESTED", cancel_reason=reason,
                           event_type="cancel_requested")

    def cancel_requested(self, job_id: str) -> tuple[bool, str | None]:
        job = self.get(job_id)
        return job["status"] in {"CANCEL_REQUESTED", "CANCELLED"}, job["cancel_reason"]

    def append_event(self, job_id: str, event: dict[str, Any]) -> int:
        with self._write_lock, self._connect() as db:
            return self._append_event_connection(db, job_id, event)

    @staticmethod
    def _append_event_connection(db: sqlite3.Connection, job_id: str, event: dict[str, Any]) -> int:
        event = {"job_id": job_id, "timestamp": utc_now(), **event}
        cur = db.execute(
            "INSERT INTO job_events(job_id,timestamp,event_json) VALUES(?,?,?)",
            (job_id, event["timestamp"], json.dumps(event, ensure_ascii=False, separators=(",", ":"))),
        )
        return int(cur.lastrowid)

    def events_after(self, job_id: str, after: int = 0, limit: int = 200) -> list[dict[str, Any]]:
        self.get(job_id)
        with self._connect() as db:
            rows = db.execute(
                "SELECT seq,event_json FROM job_events WHERE job_id=? AND seq>? ORDER BY seq LIMIT ?",
                (job_id, int(after), max(1, min(int(limit), 1000))),
            ).fetchall()
        result = []
        for row in rows:
            event = json.loads(row["event_json"])
            event["seq"] = int(row["seq"])
            result.append(event)
        return result


@dataclass(slots=True)
class JobContext:
    manager: "JobManager"
    job_id: str
    root: Path

    @property
    def job(self) -> dict[str, Any]:
        return self.manager.store.get(self.job_id)

    @property
    def staging_dir(self) -> Path:
        directory = (self.root / "scene" / "staging" / self.job_id).resolve()
        directory.mkdir(parents=True, exist_ok=True)
        return directory

    def update(self, stage: str, progress: float, *, message: str | None = None,
               metrics: dict[str, Any] | None = None) -> None:
        # O status RUNNING é publicado uma única vez pelo JobManager antes de
        # chamar o worker. Uma atualização de progresso NÃO pode reescrever o
        # status: havia uma corrida em que CANCEL_REQUESTED era sobrescrito por
        # RUNNING entre este check e o UPDATE SQL, fazendo o cancelamento sumir.
        self.check_cancelled()
        self.manager.store.update(self.job_id, stage=stage, progress=progress)
        # Fecha a janela entre o primeiro check e a gravação de progresso.
        self.check_cancelled()
        if message is not None or metrics is not None:
            self.manager.store.append_event(self.job_id, {
                "type": "detail", "stage": stage, "progress": progress,
                "message": message, "metrics": metrics or {},
            })

    def check_cancelled(self) -> None:
        requested, reason = self.manager.store.cancel_requested(self.job_id)
        if requested:
            raise JobCancelled(reason or "cancelled")

    def write_manifest(self, manifest: dict[str, Any]) -> Path:
        path = self.staging_dir / "manifest.json"
        atomic_write_json(path, manifest)
        return path


class JobManager:
    def __init__(self, root: Path, schemas: SchemaRegistry, *, workers: int = 1):
        self.root = root.resolve()
        self.schemas = schemas
        self.store = JobStore(self.root / "jobs" / "jobs.sqlite3")
        self._workers: dict[str, Worker] = {}
        self._queue: queue.Queue[str | None] = queue.Queue()
        self._stop = threading.Event()
        self._threads: list[threading.Thread] = []
        self._recovered_queued: set[str] = {
            job["id"] for job in self.store.list(status="QUEUED", limit=1000)
        }
        for index in range(max(1, int(workers))):
            thread = threading.Thread(target=self._run, name=f"arcz-job-{index}", daemon=True)
            thread.start()
            self._threads.append(thread)

    def register(self, kind: str, worker: Worker) -> None:
        if not kind or any(ch not in "abcdefghijklmnopqrstuvwxyz0123456789._-" for ch in kind):
            raise ValueError(f"kind inválido: {kind}")
        if kind in self._workers:
            raise ValueError(f"worker duplicado: {kind}")
        self._workers[kind] = worker
        # Jobs QUEUED sobrevivem ao restart. Só entram novamente na fila quando
        # o worker correspondente foi registrado, evitando corrida KeyError.
        for job_id in list(self._recovered_queued):
            job = self.store.get(job_id)
            if job["kind"] == kind and job["status"] == "QUEUED":
                self._queue.put(job_id)
                self._recovered_queued.remove(job_id)

    def supported_kinds(self) -> list[str]:
        return sorted(self._workers)

    def create(self, kind: str, request: dict[str, Any], generation_epoch: int = 0) -> dict[str, Any]:
        if kind not in self._workers:
            raise ApiError("JOB_KIND_UNAVAILABLE", f"Worker local não registrado: {kind}", status=422,
                           details={"supported": self.supported_kinds()})
        job = self.store.create(kind, request, generation_epoch)
        self._queue.put(job["id"])
        return job

    def cancel(self, job_id: str, reason: str = "cancelled_by_user") -> dict[str, Any]:
        return self.store.request_cancel(job_id, reason)

    def wait(self, job_id: str, timeout: float = 300.0) -> dict[str, Any]:
        deadline = time.monotonic() + timeout
        while True:
            job = self.store.get(job_id)
            if job["status"] in TERMINAL_STATUSES:
                return job
            if time.monotonic() >= deadline:
                raise TimeoutError(job_id)
            time.sleep(0.05)

    def stop(self, timeout: float = 2.0) -> None:
        self._stop.set()
        for _ in self._threads:
            self._queue.put(None)
        for thread in self._threads:
            thread.join(timeout=timeout)

    def _run(self) -> None:
        while not self._stop.is_set():
            job_id = self._queue.get()
            if job_id is None:
                return
            try:
                job = self.store.get(job_id)
                if job["status"] == "CANCELLED":
                    continue
                worker = self._workers.get(job["kind"])
                if worker is None:
                    raise ApiError(
                        "JOB_KIND_UNAVAILABLE",
                        f"Worker local não registrado após retomada: {job['kind']}",
                        status=422,
                        retryable=False,
                        details={"supported": self.supported_kinds()},
                    )
                self.store.update(job_id, status="RUNNING", stage="VALIDATE_REQUEST", progress=0.01)
                context = JobContext(self, job_id, self.root)
                result = worker(context, job["request"])
                context.check_cancelled()
                manifest_path = self._normalize_manifest(context, result)
                self.store.update(job_id, status="COMPLETED", stage="DONE", progress=1.0,
                                  result_manifest=str(manifest_path.relative_to(self.root).as_posix()),
                                  event_type="terminal")
            except JobCancelled as error:
                self.store.update(job_id, status="CANCELLED", stage="DONE", progress=1.0,
                                  cancel_reason=str(error), event_type="terminal")
            except BaseException as error:
                api_error = as_api_error(error, default_code="JOB_EXECUTION_FAILED")
                status = "FAILED_RETRYABLE" if api_error.retryable else "FAILED_PERMANENT"
                payload = api_error.payload()["error"]
                payload["details"] = {**payload.get("details", {}), "traceback": traceback.format_exc(limit=30)}
                try:
                    self.store.update(job_id, status=status, stage="DONE", progress=1.0,
                                      error=payload, event_type="terminal")
                except Exception:
                    traceback.print_exc()
            finally:
                self._queue.task_done()

    def _normalize_manifest(self, context: JobContext, result: str | Path | dict[str, Any]) -> Path:
        if isinstance(result, dict):
            path = context.write_manifest(result)
        else:
            path = Path(result)
            if not path.is_absolute():
                path = (self.root / path).resolve()
        try:
            path.relative_to(self.root)
        except ValueError as error:
            raise ApiError("MANIFEST_PATH_ESCAPE", "Manifest fora da raiz do ARCZ", status=500) from error
        if not path.is_file():
            raise ApiError("MANIFEST_NOT_FOUND", str(path), status=500)
        manifest = json.loads(path.read_text(encoding="utf-8"))
        self.schemas.validate("generation-manifest.schema.json", manifest)
        if manifest["job_id"] != context.job_id:
            raise ApiError("MANIFEST_JOB_MISMATCH", "job_id do manifest não corresponde ao job", status=500)
        for output in manifest["outputs"]:
            output_path = Path(output["path"])
            if not output_path.is_absolute():
                output_path = (self.root / output_path).resolve()
            try:
                output_path.relative_to(self.root)
            except ValueError as error:
                raise ApiError("OUTPUT_PATH_ESCAPE", output["path"], status=500) from error
            if not output_path.is_file():
                raise ApiError("OUTPUT_MISSING", output["path"], status=500)
            actual_size = output_path.stat().st_size
            actual_hash = sha256_file(output_path)
            if actual_size != output["bytes"] or actual_hash != output["sha256"]:
                raise ApiError("OUTPUT_INTEGRITY_FAILED", output["path"], status=500,
                               details={"expected_size": output["bytes"], "actual_size": actual_size,
                                        "expected_sha256": output["sha256"], "actual_sha256": actual_hash})
        return path
