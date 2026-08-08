from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from .errors import ApiError
from .hashing import sha256_file
from .schema_validation import SchemaRegistry


class PanoramaRegistry:
    def __init__(self, root: Path, schemas: SchemaRegistry):
        self.root = root.resolve()
        self.root.mkdir(parents=True, exist_ok=True)
        self.schemas = schemas

    def list(self) -> list[dict[str, Any]]:
        result = []
        for path in sorted(self.root.rglob("sequence.json")):
            try:
                sequence = self._read(path, verify_images=False)
                relative = path.relative_to(self.root)
                result.append({"sequence_id": sequence["sequence_id"], "frames": len(sequence["frames"]),
                               "manifest": str(relative),
                               "base_url": "/data/panoramas/" + relative.parent.as_posix().strip("/") + "/",
                               "license": sequence["license"]})
            except ApiError:
                continue
        return result

    def get(self, sequence_id: str, *, verify_images: bool = True) -> dict[str, Any]:
        for path in sorted(self.root.rglob("sequence.json")):
            sequence = json.loads(path.read_text(encoding="utf-8"))
            if sequence.get("sequence_id") == sequence_id:
                value = self._read(path, verify_images=verify_images)
                relative = path.relative_to(self.root)
                value["manifest"] = str(relative)
                value["base_url"] = "/data/panoramas/" + relative.parent.as_posix().strip("/") + "/"
                return value
        raise ApiError("PANORAMA_SEQUENCE_NOT_FOUND", sequence_id, status=404)

    def _read(self, manifest_path: Path, *, verify_images: bool) -> dict[str, Any]:
        sequence = json.loads(manifest_path.read_text(encoding="utf-8"))
        self.schemas.validate("panorama-sequence.schema.json", sequence)
        if verify_images:
            for frame in sequence["frames"]:
                image = (manifest_path.parent / frame["image"]).resolve()
                try: image.relative_to(manifest_path.parent.resolve())
                except ValueError as error:
                    raise ApiError("PANORAMA_PATH_ESCAPE", frame["image"], status=400) from error
                if not image.is_file():
                    raise ApiError("PANORAMA_IMAGE_MISSING", frame["image"], status=404)
                if sha256_file(image) != frame["sha256"]:
                    raise ApiError("PANORAMA_HASH_MISMATCH", frame["image"], status=409)
        return sequence
