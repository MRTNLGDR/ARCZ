// ARCZ Core · Validação síncrona dos contratos usados antes de qualquer I/O.
export class ValidationError extends Error {
  constructor(message, { path = "$", code = "SCHEMA_INVALID", details = {} } = {}) {
    super(message); this.name = "ValidationError"; this.path = path; this.code = code; this.details = details;
  }
}

export function isPlainObject(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const proto = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
}

export function assertPlainObject(value, path = "$") {
  if (!isPlainObject(value)) throw new ValidationError("objeto esperado", { path });
  return value;
}

export function assertFiniteNumber(value, path, { min = -Infinity, max = Infinity } = {}) {
  if (typeof value !== "number" || !Number.isFinite(value)) throw new ValidationError("número finito esperado", { path });
  if (value < min || value > max) throw new ValidationError(`valor fora de [${min}, ${max}]`, { path });
  return value;
}

export function assertString(value, path, { pattern = null, minLength = 1 } = {}) {
  if (typeof value !== "string" || value.length < minLength) throw new ValidationError("texto inválido", { path });
  if (pattern && !pattern.test(value)) throw new ValidationError("texto fora do padrão", { path });
  return value;
}

export function validateRegionRequest(value) {
  assertPlainObject(value);
  assertString(value.region_id, "$.region_id");
  const scales = new Set(["mundo","continente","pais","estado","cidade","bairro","quarteirao","endereco","lote","poligono_manual"]);
  if (!scales.has(value.scale)) throw new ValidationError("escala desconhecida", { path: "$.scale" });
  assertPlainObject(value.focus, "$.focus");
  assertFiniteNumber(value.focus.lat, "$.focus.lat", { min: -85.0511287798066, max: 85.0511287798066 });
  assertFiniteNumber(value.focus.lon, "$.focus.lon", { min: -180, max: 180 });
  assertFiniteNumber(value.requested_radius_m, "$.requested_radius_m", { min: Number.EPSILON, max: 10_000_000 });
  if (!Array.isArray(value.bbox_wgs84) || value.bbox_wgs84.length !== 4) throw new ValidationError("bbox com 4 números esperada", { path: "$.bbox_wgs84" });
  value.bbox_wgs84.forEach((n, i) => assertFiniteNumber(n, `$.bbox_wgs84[${i}]`));
  if (value.bbox_wgs84[0] >= value.bbox_wgs84[2] || value.bbox_wgs84[1] >= value.bbox_wgs84[3]) {
    throw new ValidationError("bbox invertida", { path: "$.bbox_wgs84" });
  }
  assertPlainObject(value.sources, "$.sources");
  for (const key of ["osm","overture","dem","imagery","street"]) {
    if (typeof value.sources[key] !== "boolean") throw new ValidationError("boolean esperado", { path: `$.sources.${key}` });
  }
  return value;
}

export function validatePluginManifest(m) {
  assertPlainObject(m);
  if (!new Set(["gerador","ferramenta"]).has(m.tipo)) throw new ValidationError("tipo de plugin inválido", { path: "$.tipo" });
  assertString(m.id, "$.id", { pattern: /^arcz\.[a-z0-9._-]+$/ });
  assertString(m.nome, "$.nome");
  assertString(m.versao, "$.versao", { pattern: /^\d+\.\d+\.\d+$/ });
  if (m.apiVersion !== "2") throw new ValidationError("apiVersion precisa ser 2", { path: "$.apiVersion" });
  for (const key of ["escalas","modos","capacidades"]) if (!Array.isArray(m[key])) throw new ValidationError("lista esperada", { path: `$.${key}` });
  if (!new Set(["javascript","python","rust","local_ai"]).has(m.worker)) throw new ValidationError("worker inválido", { path: "$.worker" });
  assertPlainObject(m.custoBase, "$.custoBase");
  for (const key of ["triangulos","memoriaMB","texturasMB","drawCalls"]) assertFiniteNumber(m.custoBase[key], `$.custoBase.${key}`, { min: 0 });
  return m;
}

export function deepFreeze(value, seen = new WeakSet()) {
  if (!value || typeof value !== "object" || seen.has(value)) return value;
  seen.add(value);
  for (const child of Object.values(value)) deepFreeze(child, seen);
  return Object.freeze(value);
}
