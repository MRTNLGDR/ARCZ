from __future__ import annotations

from pathlib import Path

from arcz_server.aedifex_catalog_localizer import localize_catalog_assets


def _catalog(root: Path, body: str) -> Path:
    path = root / "packages/editor/src/components/ui/item-catalog/catalog-items.tsx"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        "export type CatalogItem = { id: string }\n"
        "export const CATALOG_ITEMS: CatalogItem[] = [\n"
        + body
        + "\n]\n",
        encoding="utf-8",
    )
    return path


def _resolver(_root: Path, url: str) -> tuple[str | None, str, str]:
    slug = url.split("/system/", 1)[1].split("/", 1)[0]
    if url.endswith("model.glb"):
        return (None if slug == "remote-only" else f"/items/{slug}/model.glb", slug, "model")
    if "thumbnail." in url:
        return (None if slug == "remote-only" else f"/items/{slug}/thumbnail.webp", slug, "thumbnail")
    return None, slug, "floorplan"


def test_localizes_available_assets_and_omits_optional_floorplan(tmp_path: Path) -> None:
    path = _catalog(
        tmp_path,
        """  {
    id: 'chair',
    thumbnail: 'https://bucket.supabase.co/storage/v1/object/public/items/system/chair/thumbnail.png',
    src: 'https://bucket.supabase.co/storage/v1/object/public/items/system/chair/model.glb',
    floorPlanUrl: 'https://bucket.supabase.co/storage/v1/object/public/items/system/chair/floor-plan.png',
  },""",
    )

    report = localize_catalog_assets(tmp_path, _resolver)
    text = path.read_text(encoding="utf-8")

    assert "/items/chair/model.glb" in text
    assert "/items/chair/thumbnail.webp" in text
    assert "floorPlanUrl: undefined" in text
    assert "supabase.co" not in text
    assert report["excluded_remote_only_count"] == 0


def test_removes_complete_remote_only_entry_without_leaving_broken_tile(tmp_path: Path) -> None:
    path = _catalog(
        tmp_path,
        """  {
    id: 'local-chair',
    src: 'https://bucket.supabase.co/storage/v1/object/public/items/system/local-chair/model.glb',
    thumbnail: 'https://bucket.supabase.co/storage/v1/object/public/items/system/local-chair/thumbnail.png',
  },
  {
    id: 'remote-only',
    src: 'https://bucket.supabase.co/storage/v1/object/public/items/system/remote-only/model.glb',
    thumbnail: 'https://bucket.supabase.co/storage/v1/object/public/items/system/remote-only/thumbnail.png',
    surface: { height: 0.4 },
  },""",
    )

    report = localize_catalog_assets(tmp_path, _resolver)
    text = path.read_text(encoding="utf-8")

    assert "local-chair" in text
    assert "remote-only" not in text
    assert "supabase.co" not in text
    assert report["excluded_remote_only_count"] == 1
    assert report["excluded_remote_only_entries"][0]["id"] == "remote-only"
