/**
 * Pure helpers for the globe + Floorplanner site-authoring split.
 * Kept free of DOM/Cesium dependencies so the contract can be unit tested.
 */

export const DEFAULT_SITE_AUTHORING_LAYOUT = Object.freeze({
  schema_version: 1,
  show_globe: true,
  split_ratio: 0.38,
  auto_publish: true,
  auto_publish_delay_ms: 1800,
});

export function clampNumber(value, min, max, fallback) {
  const number = Number(value);
  if (!Number.isFinite(number)) return fallback;
  return Math.min(max, Math.max(min, number));
}

export function normalizeSiteAuthoringLayout(value = {}) {
  return {
    schema_version: 1,
    show_globe: value.show_globe !== false,
    split_ratio: clampNumber(value.split_ratio, 0.2, 0.68, DEFAULT_SITE_AUTHORING_LAYOUT.split_ratio),
    auto_publish: value.auto_publish !== false,
    auto_publish_delay_ms: Math.round(clampNumber(
      value.auto_publish_delay_ms,
      400,
      10000,
      DEFAULT_SITE_AUTHORING_LAYOUT.auto_publish_delay_ms,
    )),
  };
}

function finitePoint(point) {
  const lon = Number(Array.isArray(point) ? point[0] : point?.lon);
  const lat = Number(Array.isArray(point) ? point[1] : point?.lat);
  return Number.isFinite(lon) && Number.isFinite(lat) ? [lon, lat] : null;
}

export function bboxFromRegionState(state) {
  const recorte = state?.recorte?.perimetro;
  const source = Array.isArray(recorte) && recorte.length >= 3
    ? recorte
    : state?.active_region?.request?.polygon_wgs84;
  const points = (Array.isArray(source) ? source : []).map(finitePoint).filter(Boolean);
  if (points.length) {
    const lon = points.map(point => point[0]);
    const lat = points.map(point => point[1]);
    return [Math.min(...lon), Math.min(...lat), Math.max(...lon), Math.max(...lat)];
  }
  const bbox = state?.active_region?.request?.bbox_wgs84;
  if (Array.isArray(bbox) && bbox.length === 4 && bbox.every(value => Number.isFinite(Number(value)))) {
    return bbox.map(Number);
  }
  return null;
}

export function regionSummary(state) {
  const request = state?.active_region?.request || {};
  const context = state?.active_region?.context || {};
  const parcel = Array.isArray(state?.recorte?.perimetro) && state.recorte.perimetro.length >= 3;
  const scale = parcel ? "lote desenhado" : String(request.scale || "região");
  const label = String(
    request.label || request.display_name || context.name || context.region_name || request.region_id || "Região Ativa",
  );
  return {
    label,
    scale,
    region_id: request.region_id || context.region_id || null,
    source: parcel ? "recorte manual bloqueável" : "Região Ativa georreferenciada",
    bbox_wgs84: bboxFromRegionState(state),
  };
}
