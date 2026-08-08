from __future__ import annotations

from io import BytesIO
import json
from pathlib import Path
import shutil
import zipfile

from PIL import Image
import pytest

from arcz_server.errors import ApiError
from arcz_server.prompt_library import PromptLibrary
from arcz_server.reference_media import ReferenceMediaStore
from arcz_server.schema_validation import SchemaRegistry

ROOT = Path(__file__).resolve().parents[1]
SCHEMAS = SchemaRegistry(ROOT / "schemas")


class NoModel:
    def request(self, *args, **kwargs):
        raise ApiError("MODEL_NOT_INSTALLED", "modelo local ausente", status=503)


def png_bytes(size=(9, 7)) -> bytes:
    stream = BytesIO()
    Image.new("RGB", size, (20, 40, 80)).save(stream, "PNG")
    return stream.getvalue()


def prepare_root(root: Path) -> None:
    shutil.copytree(ROOT / "resources", root / "resources")


def user_prompt(slug="user.exterior.hero") -> dict:
    return {
        "slug": slug,
        "title": "Exterior Hero",
        "category": "render",
        "purpose": "photoreal",
        "language": "pt-BR",
        "template": "{{project.name}} · exterior 8K",
        "negative_template": "geometria deformada",
        "tags": ["exterior", "8k"],
        "variables": {"project.name": {"required": True}},
    }


def test_prompt_versions_duplicate_archive_and_bundle_are_real(tmp_path: Path) -> None:
    prepare_root(tmp_path)
    library = PromptLibrary(tmp_path, SCHEMAS, NoModel())
    created = library.upsert(user_prompt())
    assert created["version"] == 1 and created["builtin"] is False

    updated = library.upsert({**created, "template": "{{project.name}} · exterior cinematográfico 8K"})
    assert updated["version"] == 2
    versions = library.versions(created["id"])
    assert [item["version"] for item in versions] == [2, 1]
    assert versions[0]["snapshot"]["content_hash"] == updated["content_hash"]

    duplicate = library.duplicate(created["id"], {"slug": "user.exterior.hero.en", "language": "en-US"})
    assert duplicate["id"] != created["id"]
    assert duplicate["builtin"] is False
    assert duplicate["language"] == "en-US"

    archived = library.archive(duplicate["id"])
    assert archived["active"] is False
    assert all(item["id"] != duplicate["id"] for item in library.list(limit=1000))

    bundle = library.export_bundle({"identifiers": [created["id"]], "include_versions": True})
    assert bundle["prompt_count"] == 1
    assert len(bundle["bundle_hash"]) == 64
    imported = library.import_bundle(bundle, conflict="duplicate")
    assert imported["ok"] is True
    assert imported["imported"][0]["id"] != created["id"]

    tampered = json.loads(json.dumps(bundle))
    tampered["entries"][0]["prompt"]["title"] = "alterado"
    with pytest.raises(ApiError) as caught:
        library.import_bundle(tampered)
    assert caught.value.code == "PROMPT_BUNDLE_HASH_MISMATCH"


def test_builtin_prompt_is_immutable_but_can_be_duplicated(tmp_path: Path) -> None:
    prepare_root(tmp_path)
    library = PromptLibrary(tmp_path, SCHEMAS, NoModel())
    builtin = library.get("render.archviz.exterior.photoreal")
    assert builtin["builtin"] is True
    with pytest.raises(ApiError) as caught:
        library.upsert({**builtin, "template": "mutação proibida"})
    assert caught.value.code == "BUILTIN_PROMPT_READ_ONLY"
    copy = library.duplicate(builtin["id"], {"slug": "user.builtin.copy"})
    assert copy["builtin"] is False


def test_reference_media_content_metadata_and_corruption_gate(tmp_path: Path) -> None:
    store = ReferenceMediaStore(tmp_path, SCHEMAS)
    raw = png_bytes()
    item = store.import_bytes("referência fachada.png", raw, {
        "roles": ["style", "composition"],
        "license": {"id": "LicenseRef-UserProvided", "redistribution_allowed": False},
        "provenance": {"source": "test_upload", "source_ref": "local"},
        "metadata": {"weight": 0.75},
    })
    path, mime, size = store.content_path(item["id"])
    assert path.read_bytes() == raw
    assert mime == "image/png" and size == len(raw)

    updated = store.update_metadata(item["id"], {
        "roles": ["material", "lighting", "material"],
        "metadata": {"weight": 1.25, "notes": "preservar pedra"},
        "license": item["license"],
    })
    assert updated["roles"] == ["material", "lighting"]
    assert updated["metadata"]["notes"] == "preservar pedra"
    assert updated["integrity"]["ok"] is True
    assert updated["content_url"].endswith("/content")

    path.write_bytes(b"corrompido")
    with pytest.raises(ApiError) as caught:
        store.content_path(item["id"])
    assert caught.value.code == "REFERENCE_MEDIA_CORRUPT"


def _kmz_bytes(kml: bytes) -> bytes:
    stream = BytesIO()
    with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("doc.kml", kml)
    return stream.getvalue()


def _minimal_glb() -> bytes:
    document = json.dumps({"asset": {"version": "2.0"}, "scene": 0, "scenes": [{}]}, separators=(",", ":")).encode()
    document += b" " * ((4 - len(document) % 4) % 4)
    chunk = len(document).to_bytes(4, "little") + (0x4E4F534A).to_bytes(4, "little") + document
    length = 12 + len(chunk)
    return b"glTF" + (2).to_bytes(4, "little") + length.to_bytes(4, "little") + chunk


def test_reference_media_validates_bim_cad_geodata_pointcloud_and_glb(tmp_path: Path) -> None:
    store = ReferenceMediaStore(tmp_path, SCHEMAS)
    fixtures = [
        ("building.ifc", b"ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;", "bim"),
        ("survey.dxf", b"0\nSECTION\n2\nHEADER\n0\nENDSEC\n0\nEOF\n", "cad"),
        ("parcel.geojson", json.dumps({"type": "FeatureCollection", "features": []}).encode(), "geodata"),
        ("route.kml", b'<?xml version="1.0"?><kml xmlns="http://www.opengis.net/kml/2.2"><Document/></kml>', "geodata"),
        ("cloud.ply", b"ply\nformat ascii 1.0\nelement vertex 0\nproperty float x\nproperty float y\nproperty float z\nend_header\n", "pointcloud"),
        ("scene.glb", _minimal_glb(), "model3d"),
        ("luminaire.ies", b"IESNA:LM-63-2002\n[TEST] ARCZ\nTILT=NONE\n1 1000 1 1 1 1 1 1 1 1 1 1 1\n0\n0\n", "lighting"),
    ]
    for name, raw, category in fixtures:
        record = store.import_bytes(name, raw)
        assert record["category"] == category, name
        assert record["metadata"]["validation"] in {"header", "parsed"}, name
        assert record["content_hash"] == __import__("hashlib").sha256(raw).hexdigest()

    kml = b'<?xml version="1.0"?><kml xmlns="http://www.opengis.net/kml/2.2"><Document/></kml>'
    kmz = store.import_bytes("context.kmz", _kmz_bytes(kml))
    assert kmz["category"] == "geodata"
    assert kmz["metadata"]["validation"] == "archive+parsed"


def test_reference_media_rejects_active_xml_zip_traversal_and_fake_cad(tmp_path: Path) -> None:
    store = ReferenceMediaStore(tmp_path, SCHEMAS)
    with pytest.raises(ApiError) as xml:
        store.import_bytes("unsafe.kml", b'<?xml version="1.0"?><!DOCTYPE x [<!ENTITY e SYSTEM "file:///etc/passwd">]><kml>&e;</kml>')
    assert xml.value.code in {"MEDIA_KML_ACTIVE_XML_DENIED", "MEDIA_XML_DANGEROUS", "MEDIA_KML_INVALID"}

    stream = BytesIO()
    with zipfile.ZipFile(stream, "w") as archive:
        archive.writestr("../escape.kml", "<kml/>")
    with pytest.raises(ApiError) as traversal:
        store.import_bytes("unsafe.kmz", stream.getvalue())
    assert traversal.value.code == "MEDIA_KMZ_PATH_ESCAPE"

    with pytest.raises(ApiError) as fake_dwg:
        store.import_bytes("fake.dwg", b"not a dwg")
    assert fake_dwg.value.code == "MEDIA_DWG_INVALID"
