from __future__ import annotations
from pathlib import Path
import os, shutil, tempfile
from .hashing import sha256_bytes, sha256_file
from .atomic_io import atomic_write_bytes
from .errors import ApiError

class ContentStore:
    def __init__(self, root: Path): self.root=root.resolve(); self.root.mkdir(parents=True,exist_ok=True)
    def path_for(self, digest: str) -> Path:
        if len(digest)!=64 or any(c not in "0123456789abcdef" for c in digest): raise ApiError("HASH_INVALID","SHA-256 inválido",status=400)
        return self.root/digest[:2]/digest[2:]
    def put_bytes(self,data:bytes)->tuple[str,Path]:
        digest=sha256_bytes(data); path=self.path_for(digest)
        if not path.is_file(): path.parent.mkdir(parents=True,exist_ok=True); atomic_write_bytes(path,data)
        return digest,path
    def put_file(self,source:Path)->tuple[str,Path]:
        source=source.resolve()
        if not source.is_file(): raise ApiError("SOURCE_FILE_NOT_FOUND",str(source),status=404)
        digest=sha256_file(source); dest=self.path_for(digest)
        if not dest.is_file():
            dest.parent.mkdir(parents=True,exist_ok=True)
            fd,tmp=tempfile.mkstemp(prefix=".import-",dir=dest.parent); os.close(fd); temp=Path(tmp)
            try: shutil.copyfile(source,temp); os.replace(temp,dest)
            finally: temp.unlink(missing_ok=True)
        return digest,dest
    def verify(self,digest:str)->bool:
        path=self.path_for(digest); return path.is_file() and sha256_file(path)==digest
