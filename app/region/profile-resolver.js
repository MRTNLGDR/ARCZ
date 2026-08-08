function clone(v) { return globalThis.structuredClone ? structuredClone(v) : JSON.parse(JSON.stringify(v)); }
function merge(base, layer, path="$") {
  if (layer === undefined) return clone(base);
  if (base === undefined) return clone(layer);
  if (Array.isArray(base)||Array.isArray(layer)||typeof base!=="object"||typeof layer!=="object"||!base||!layer) return clone(layer);
  const out=clone(base); for (const [k,v] of Object.entries(layer)) out[k]=merge(out[k],v,`${path}.${k}`); return out;
}
function normalizeWeights(obj,path) {
  const entries=Object.entries(obj||{}); const sum=entries.reduce((s,[,v])=>s+Number(v||0),0);
  if (!(sum>0)) throw new Error(`Distribuição vazia: ${path}`);
  return Object.fromEntries(entries.map(([k,v])=>[k,Number(v)/sum]));
}

export function composeRegionalProfile(layers) {
  const applied=[]; let result={};
  for (const layer of layers.filter(Boolean)) { result=merge(result,layer); applied.push(`${layer.id}@${layer.version}`); }
  result.architecture.building_mix=normalizeWeights(result.architecture.building_mix,"architecture.building_mix");
  result.roofs.types=normalizeWeights(result.roofs.types,"roofs.types");
  result.roofs.materials=normalizeWeights(result.roofs.materials,"roofs.materials");
  result.facades.materials=normalizeWeights(result.facades.materials,"facades.materials");
  result.resolution_report={ applied, resolved_at:new Date().toISOString() };
  return result;
}
