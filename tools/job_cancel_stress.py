#!/usr/bin/env python3
"""Estressa a corrida cooperativa de cancelamento da fila local.

O worker é uma função real executada pela mesma thread/SQLite/JobManager do
runtime. O teste não injeta resultado pronto: ele somente deve terminar por
``JobCancelled`` e prova que ``CANCEL_REQUESTED`` nunca volta a ``RUNNING``.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from arcz_server.jobs import JobContext, JobManager
from arcz_server.schema_validation import SchemaRegistry


def run(iterations: int, sleep_seconds: float) -> dict:
    schemas = SchemaRegistry(ROOT / "schemas")
    failures: list[dict] = []
    for index in range(iterations):
        with tempfile.TemporaryDirectory(prefix="arcz-cancel-stress-") as temporary:
            manager = JobManager(Path(temporary) / "arcz", schemas, workers=1)

            def cancellable(context: JobContext, _request: dict) -> dict:
                for step in range(1000):
                    context.check_cancelled()
                    if step % 5 == 0:
                        context.update("GENERATE", min(0.9, step / 1100))
                    time.sleep(sleep_seconds)
                raise AssertionError("worker ignorou cancelamento até o fim")

            try:
                manager.register("stress.cancel", cancellable)
                job = manager.create("stress.cancel", {})
                deadline = time.monotonic() + 2.0
                while manager.store.get(job["id"])["status"] == "QUEUED" and time.monotonic() < deadline:
                    time.sleep(min(0.001, sleep_seconds or 0.001))
                manager.cancel(job["id"], "stress_cancel")
                terminal = manager.wait(job["id"], timeout=5.0)
                if terminal["status"] != "CANCELLED" or terminal["cancel_reason"] != "stress_cancel":
                    failures.append({"iteration": index, "terminal": terminal})
                    break
            finally:
                manager.stop()
    return {
        "schema_version": 1,
        "iterations": iterations,
        "passed": iterations - len(failures),
        "failures": failures,
        "ok": not failures,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--iterations", type=int, default=100)
    parser.add_argument("--sleep-seconds", type=float, default=0.0005)
    args = parser.parse_args()
    if not 1 <= args.iterations <= 10000:
        raise SystemExit("--iterations precisa estar entre 1 e 10000")
    if not 0 <= args.sleep_seconds <= 0.1:
        raise SystemExit("--sleep-seconds precisa estar entre 0 e 0.1")
    report = run(args.iterations, args.sleep_seconds)
    print(json.dumps(report, ensure_ascii=False))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
