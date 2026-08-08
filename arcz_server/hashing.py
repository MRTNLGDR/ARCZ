from __future__ import annotations
import hashlib, json
from pathlib import Path
from typing import Any, BinaryIO

def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

def sha256_file(path: Path, chunk_size: int = 1024 * 1024) -> str:
    h = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(chunk_size):
            h.update(chunk)
    return h.hexdigest()

def canonical_json_bytes(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")

def canonical_json_hash(value: Any) -> str:
    return sha256_bytes(canonical_json_bytes(value))
