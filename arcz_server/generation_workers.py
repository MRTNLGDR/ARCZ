from __future__ import annotations

import json
import logging
import os
from pathlib import Path
import subprocess
import sys
import time
from typing import Any

from .atomic_io import atomic_write_json
from .errors import ApiError
from .jobs import JobCancelled, JobContext

LOGGER = logging.getLogger(__name__)


class RustGenerationWorker:
    """Adaptador real para o binário local `arcz-generation-cli`.

    Não existe fallback fictício. Sem binário compilado, o job termina com
    WORKER_NOT_INSTALLED e aponta o comando exato de build.
    """

    def __init__(self, root: Path, *, executable: Path | None = None, timeout_seconds: int = 900):
        self.root = root.resolve()
        self.explicit_executable = executable.resolve() if executable else None
        self.timeout_seconds = int(timeout_seconds)

    def locate(self) -> Path:
        candidates: list[Path] = []
        env = os.environ.get("ARCZ_GENERATION_CLI")
        if self.explicit_executable:
            candidates.append(self.explicit_executable)
        if env:
            candidates.append(Path(env))
        for profile in ("release", "debug"):
            for name in ("arcz-generation-cli.exe", "arcz-generation-cli"):
                candidates.append(self.root / "target" / profile / name)
        for candidate in candidates:
            candidate = candidate.expanduser().resolve()
            if candidate.is_file():
                return candidate
        raise ApiError(
            "WORKER_NOT_INSTALLED",
            "Motor procedural Rust não compilado. Execute `cargo build --release -p arcz-generation-cli`.",
            status=503, retryable=False,
            details={"searched": [str(p) for p in candidates]},
        )

    def __call__(self, context: JobContext, request: dict[str, Any]) -> Path:
        executable = self.locate()
        context.update("VALIDATE_REQUEST", 0.03, message="Validando solicitação do motor Rust")
        request_path = context.staging_dir / "request.json"
        enriched = {
            "schema_version": 1,
            "job_id": context.job_id,
            "kind": context.job["kind"],
            "generation_epoch": context.job["generation_epoch"],
            "request": request,
            "root": str(self.root),
            "staging_dir": str(context.staging_dir),
        }
        atomic_write_json(request_path, enriched)
        log_path = context.staging_dir / "worker.log"
        command = [str(executable), "run", "--request", str(request_path),
                   "--output-dir", str(context.staging_dir)]
        environment = os.environ.copy()
        environment["ARCZ_NETWORK_MODE"] = "offline_strict"
        environment["NO_PROXY"] = "*"
        environment["no_proxy"] = "*"
        started = time.monotonic()
        context.update("GENERATE", 0.15, message="Executando motor procedural local")
        with log_path.open("wb") as log:
            process = subprocess.Popen(command, cwd=self.root, stdout=log, stderr=subprocess.STDOUT,
                                       env=environment, shell=False)
            while process.poll() is None:
                try:
                    context.check_cancelled()
                except JobCancelled:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                    raise
                elapsed = time.monotonic() - started
                if elapsed > self.timeout_seconds:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                    raise ApiError("WORKER_TIMEOUT", f"Motor excedeu {self.timeout_seconds}s",
                                   status=504, retryable=True, details={"log": str(log_path)})
                # Progresso é conservador; o worker pode escrever progress.json para maior precisão.
                progress_file = context.staging_dir / "progress.json"
                if progress_file.is_file():
                    try:
                        progress = json.loads(progress_file.read_text(encoding="utf-8"))
                        context.update(progress.get("stage", "GENERATE"), float(progress.get("progress", 0.5)),
                                       message=progress.get("message"), metrics=progress.get("metrics"))
                    except (OSError, UnicodeError, json.JSONDecodeError, TypeError, ValueError) as exc:
                        # A partially-written progress file must never kill the worker.
                        # The real process result/manifest remains authoritative.
                        LOGGER.debug("Ignoring transient invalid progress.json for %s: %s", context.job_id, exc)
                time.sleep(0.1)
        if process.returncode != 0:
            tail = ""
            try:
                tail = "\n".join(log_path.read_text(encoding="utf-8", errors="replace").splitlines()[-60:])
            except OSError as exc:
                LOGGER.debug("Unable to read failed worker log %s: %s", log_path, exc)
            raise ApiError("WORKER_FAILED", f"Motor Rust encerrou com código {process.returncode}",
                           status=500, retryable=False, details={"command": command, "log_tail": tail})
        manifest_path = context.staging_dir / "manifest.json"
        if not manifest_path.is_file():
            raise ApiError("WORKER_MANIFEST_MISSING", "Motor concluiu sem manifest.json", status=500,
                           details={"log": str(log_path)})
        context.update("VALIDATE_OUTPUT", 0.90, message="Validando artefatos e checksums")
        return manifest_path


class CommandJobWorker:
    """Executa um adaptador local declarado, sempre sem shell.

    Serve para renderizadores, conversores e ferramentas técnicas. A ausência
    do executável é erro explícito; não cria frame, planta ou arquivo vazio.
    """

    def __init__(self, root: Path, command: list[str], *, timeout_seconds: int = 3600):
        if not command:
            raise ValueError("command vazio")
        self.root = root.resolve()
        self.command = list(command)
        self.timeout_seconds = timeout_seconds

    def __call__(self, context: JobContext, request: dict[str, Any]) -> Path:
        request_path = context.staging_dir / "request.json"
        atomic_write_json(request_path, {
            "schema_version": 1, "job_id": context.job_id, "kind": context.job["kind"],
            "request": request, "root": str(self.root), "staging_dir": str(context.staging_dir),
        })
        replacements = {
            "{request}": str(request_path), "{output_dir}": str(context.staging_dir),
            "{root}": str(self.root), "{job_id}": context.job_id,
            "{python}": sys.executable,
        }
        command = [replacements.get(token, token) for token in self.command]
        executable = Path(command[0])
        if (executable.is_absolute() or os.sep in command[0]) and not executable.is_file():
            raise ApiError("TOOL_NOT_INSTALLED", command[0], status=503)
        context.update("GENERATE", 0.2, message=f"Executando ferramenta local: {command[0]}")
        log_path = context.staging_dir / "worker.log"
        with log_path.open("wb") as log:
            process = subprocess.Popen(command, cwd=self.root, stdout=log, stderr=subprocess.STDOUT,
                                       shell=False, env={**os.environ, "ARCZ_NETWORK_MODE": "offline_strict"})
            started = time.monotonic()
            while process.poll() is None:
                try:
                    context.check_cancelled()
                except JobCancelled:
                    process.terminate()
                    try: process.wait(timeout=5)
                    except subprocess.TimeoutExpired: process.kill()
                    raise
                if time.monotonic() - started > self.timeout_seconds:
                    process.kill()
                    raise ApiError("TOOL_TIMEOUT", command[0], status=504, retryable=True)
                time.sleep(0.1)
        if process.returncode != 0:
            tail = log_path.read_text(encoding="utf-8", errors="replace")[-12000:]
            raise ApiError("TOOL_FAILED", f"{command[0]} retornou {process.returncode}", status=500,
                           details={"log_tail": tail})
        manifest = context.staging_dir / "manifest.json"
        if not manifest.is_file():
            raise ApiError("TOOL_MANIFEST_MISSING", "Ferramenta não produziu manifest.json", status=500)
        return manifest
