function bboxFromPolygon(points) {
  if (!Array.isArray(points) || points.length < 1) return null;
  const lon = points.map(p => Number(Array.isArray(p) ? p[0] : p.lon));
  const lat = points.map(p => Number(Array.isArray(p) ? p[1] : p.lat));
  if (![...lon, ...lat].every(Number.isFinite)) return null;
  return [Math.min(...lon), Math.min(...lat), Math.max(...lon), Math.max(...lat)];
}

function recorteWgs84(state) {
  const points = state?.recorte?.perimetro;
  if (!Array.isArray(points) || points.length < 3) return [];
  return points.map(p => [Number(p.lon), Number(p.lat), Number(p.alt || 0)]);
}

const CONTEXT_FORMATS = new Set(["glb", "geojson"]);
const HASH = /^[a-f0-9]{64}$/;

function outputFormat(output) {
  const kind = String(output?.kind || output?.format || "").toLowerCase();
  if (CONTEXT_FORMATS.has(kind)) return kind;
  const match = String(output?.path || "").toLowerCase().match(/\.([a-z0-9]+)$/);
  return match && CONTEXT_FORMATS.has(match[1]) ? match[1] : null;
}

function roleFrom(value) {
  const text = String(value || "").toLowerCase();
  if (/terrain|relevo/.test(text)) return "terrain";
  if (/road|via/.test(text)) return "roads";
  if (/veget|tree|arvore/.test(text)) return "vegetation";
  if (/building|house|edific|casa|predio/.test(text)) return "buildings";
  if (/imagery|image|ortho/.test(text)) return "imagery";
  if (/survey|scan|lidar/.test(text)) return "survey";
  return "surroundings";
}

function assertLocalAssetPath(value, label) {
  const path = String(value || "").replaceAll("\\", "/");
  if (!path || /^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(path) || path.split("/").includes("..")) {
    const error = new Error(`${label}: caminho de contexto deve ser local e relativo ao ARCZ`);
    error.code = "CONTEXT_LAYER_PATH_INVALID";
    throw error;
  }
  return path.startsWith("/") ? path : `/${path}`;
}

function normalizeContextLayer(value, index, owner = "user") {
  if (!value || typeof value !== "object") throw new TypeError(`context layer ${index} inválida`);
  const format = outputFormat(value);
  if (!format) return null;
  const assetPath = assertLocalAssetPath(value.asset_path || value.path || value.uri, `context layer ${index}`);
  const sha256 = String(value.sha256 || value.content_hash || "").toLowerCase();
  if (!HASH.test(sha256)) {
    const error = new Error(`context layer ${index}: sha256 obrigatório`);
    error.code = "CONTEXT_LAYER_HASH_REQUIRED";
    throw error;
  }
  return {
    id: String(value.id || `context:${sha256.slice(0, 20)}:${index}`),
    role: roleFrom(value.role || value.owner || owner),
    format,
    asset_path: assetPath,
    sha256,
    readonly: true,
    visible: value.visible !== false,
    opacity: Number.isFinite(Number(value.opacity)) ? Math.max(0, Math.min(1, Number(value.opacity))) : 1,
    lod: String(value.lod || "reference"),
    coordinate_space: value.coordinate_space || "AEDIFEX_LOCAL",
    transform: value.transform,
    geo_placement: value.geo_placement || value.placement,
    provenance: value.provenance || {
      owner: value.owner || owner,
      generator: value.generator || null,
      manifest_hash: value.manifest_hash || null
    },
    metadata: value.metadata || {}
  };
}

/**
 * Turns committed procedural outputs into immutable reference layers for the
 * Aedifex authoring kernel. The authoritative editable building is never fed
 * back as context, preventing recursive duplication on globe ↔ floorplanner
 * round-trips.
 */
export function collectModelingContextLayers(state) {
  const candidates = [];
  for (const layer of state?.procedural_layers || []) {
    const manifest = layer?.manifest;
    if (!manifest || typeof manifest !== "object") continue;
    for (const [outputIndex, output] of (manifest.outputs || []).entries()) {
      const format = outputFormat(output);
      if (!format) continue;
      candidates.push({
        ...output,
        id: `procedural:${layer.id || manifest.job_id || "unknown"}:${outputIndex}`,
        format,
        role: roleFrom(layer.owner || manifest.generator),
        owner: layer.owner,
        generator: manifest.generator,
        manifest_hash: manifest.inputs_hash || null,
        provenance: {
          owner: layer.owner,
          generator: manifest.generator,
          job_id: manifest.job_id,
          source_versions: manifest.source_versions || {},
          source_packages: manifest.source_packages || []
        }
      });
    }
  }
  for (const layer of state?.floorplanner_context_layers || []) candidates.push(layer);

  const result = [];
  const seen = new Set();
  for (const [index, candidate] of candidates.entries()) {
    const normalized = normalizeContextLayer(candidate, index, candidate?.owner || "project");
    if (!normalized) continue;
    const key = `${normalized.asset_path}|${normalized.sha256}`;
    if (seen.has(key)) continue;
    seen.add(key);
    result.push(normalized);
  }
  return result.sort((a, b) => `${a.role}:${a.id}`.localeCompare(`${b.role}:${b.id}`));
}

export function buildModelingContextRequest(state, { referenceMedia = [] } = {}) {
  const active = state?.active_region;
  if (!active?.request || !active?.context) {
    const error = new Error("Selecione uma Região Ativa antes de abrir o Floorplanner");
    error.code = "ACTIVE_REGION_REQUIRED"; throw error;
  }
  const parcel = recorteWgs84(state);
  const requestPolygon = active.request.polygon_wgs84 || [];
  const selectedPolygon = parcel.length ? parcel : requestPolygon;
  const bbox = bboxFromPolygon(selectedPolygon) || active.request.bbox_wgs84;
  if (!Array.isArray(bbox) || bbox.length !== 4) throw new Error("Região Ativa sem bbox_wgs84 válido");
  const profileValues = Object.entries(state.region_profiles || {}).map(([id, value]) =>
    value && typeof value === "object" ? { id, ...value } : String(value || id));
  return {
    active_region: active,
    north_rotation_deg: Number(state.floorplanner_north_rotation_deg || 0),
    vertical_offset_m: Number(state.floorplanner_vertical_offset_m || 0),
    selection: {
      selection_id: parcel.length ? `recorte:${active.request.region_id}` : active.request.region_id,
      kind: parcel.length ? "lote" : active.request.scale,
      bbox_wgs84: bbox,
      parcel_polygon_wgs84: selectedPolygon,
      source: {
        kind: parcel.length ? "user_drawn_recorte" : "active_region",
        estimated: !parcel.length && !requestPolygon.length
      }
    },
    regional_profiles: profileValues,
    constraints: state.floorplanner_constraints || {},
    reference_media: [...new Set(referenceMedia.length ? referenceMedia : (state.reference_media || []))],
    context_layers: collectModelingContextLayers(state)
  };
}
