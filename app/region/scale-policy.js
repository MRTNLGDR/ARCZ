export const REGION_SCALES = Object.freeze([
  "mundo","continente","pais","estado","cidade","bairro","quarteirao","endereco","lote","poligono_manual"
]);

const POLICIES = Object.freeze({
  mundo: { generationRadiusMaxM: 0, generationAllowed: false, defaultZoom: 2 },
  continente: { generationRadiusMaxM: 0, generationAllowed: false, defaultZoom: 4 },
  pais: { generationRadiusMaxM: 0, generationAllowed: false, defaultZoom: 5 },
  estado: { generationRadiusMaxM: 0, generationAllowed: false, defaultZoom: 7 },
  cidade: { generationRadiusMaxM: 3_000, generationAllowed: true, defaultZoom: 13 },
  bairro: { generationRadiusMaxM: 2_000, generationAllowed: true, defaultZoom: 15 },
  quarteirao: { generationRadiusMaxM: 750, generationAllowed: true, defaultZoom: 17 },
  endereco: { generationRadiusMaxM: 500, generationAllowed: true, defaultZoom: 18 },
  lote: { generationRadiusMaxM: 250, generationAllowed: true, defaultZoom: 19 },
  poligono_manual: { generationRadiusMaxM: 3_000, generationAllowed: true, defaultZoom: 17 }
});

export function policyForScale(scale) {
  const policy = POLICIES[scale];
  if (!policy) throw new Error(`Escala desconhecida: ${scale}`);
  return policy;
}

export function clampGenerationRadius(scale, requestedM) {
  const policy = policyForScale(scale);
  if (!policy.generationAllowed) return 0;
  if (!Number.isFinite(requestedM) || requestedM <= 0) throw new TypeError("raio de geração inválido");
  return Math.min(requestedM, policy.generationRadiusMaxM);
}

export function assertPluginScale(manifest, scale) {
  if (!manifest?.escalas?.includes(scale)) {
    throw new Error(`Plugin ${manifest?.id || "desconhecido"} não aceita escala ${scale}`);
  }
}
