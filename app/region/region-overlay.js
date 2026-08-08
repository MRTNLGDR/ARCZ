// Desenha a Região Ativa usando apenas uma facade de cena, nunca viewer cru.
export class RegionOverlay {
  constructor({ sceneFacade }) {
    if (!sceneFacade?.addPolygon || !sceneFacade?.remove) throw new Error("sceneFacade incompleta");
    this.scene = sceneFacade; this.handle = null;
  }

  show(region, { color = "#4EC9B0", width = 2 } = {}) {
    this.clear();
    const polygon = region?.request?.polygon_wgs84?.length
      ? region.request.polygon_wgs84
      : bboxToPolygon(region?.request?.bbox_wgs84);
    if (!polygon?.length) return null;
    this.handle = this.scene.addPolygon({ positions: polygon, outlineColor: color, outlineWidth: width, fillAlpha: 0.04, id: `active-region:${region.request.region_id}` });
    return this.handle;
  }

  clear() { if (this.handle) { this.scene.remove(this.handle); this.handle = null; } }
  dispose() { this.clear(); }
}

function bboxToPolygon(bbox) {
  if (!Array.isArray(bbox) || bbox.length !== 4) return [];
  const [w,s,e,n] = bbox; return [[w,s],[e,s],[e,n],[w,n],[w,s]];
}
