from __future__ import annotations
import json, os, tempfile
from pathlib import Path
from typing import Any
from .hashing import canonical_json_hash


def _fsync_directory(path: Path) -> None:
    if os.name == "nt":
        return
    fd = os.open(path, os.O_RDONLY)
    try: os.fsync(fd)
    finally: os.close(fd)


def atomic_write_bytes(path: Path, data: bytes, *, backup: bool = False) -> None:
    path = path.resolve()
    path.parent.mkdir(parents=True, exist_ok=True)
    if backup and path.is_file():
        backup_path = path.with_name(path.name + ".bak")
        atomic_write_bytes(backup_path, path.read_bytes(), backup=False)
    fd, temp_name = tempfile.mkstemp(prefix=f".{path.name}.", suffix=".partial", dir=path.parent)
    temp = Path(temp_name)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp, path)
        _fsync_directory(path.parent)
    except BaseException:
        try: temp.unlink(missing_ok=True)
        finally: raise


def atomic_write_json(path: Path, value: Any, *, backup: bool = False, indent: int = 2) -> str:
    raw = json.dumps(value, ensure_ascii=False, indent=indent, allow_nan=False).encode("utf-8")
    atomic_write_bytes(path, raw, backup=backup)
    return canonical_json_hash(value)


def read_json(path: Path, default: Any = None) -> Any:
    if not path.is_file(): return default
    return json.loads(path.read_text(encoding="utf-8"))
