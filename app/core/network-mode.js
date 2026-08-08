/**
 * Política de rede do front-end. O servidor aplica uma segunda barreira no
 * socket; esta camada impede que módulos do navegador façam egress por engano.
 *
 * Importante: `local_lan` NÃO significa "qualquer URL". Só aceita loopback,
 * mesma origem e endereços IP privados literais. Nomes DNS arbitrários são
 * recusados porque o navegador não expõe uma resolução DNS confiável para
 * provar que o destino pertence à LAN.
 */
export const NETWORK_MODES = Object.freeze({
  OFFLINE_STRICT: "offline_strict",
  LOCAL_LAN: "local_lan",
  IMPORT_ASSISTED: "import_assisted"
});

export function validateNetworkMode(mode) {
  if (!Object.values(NETWORK_MODES).includes(mode)) throw new Error(`Modo de rede inválido: ${mode}`);
  return mode;
}

export function isLoopbackUrl(url) {
  const parsed = new URL(url, globalThis.location?.href || "http://127.0.0.1/");
  return ["127.0.0.1", "localhost", "localhost.localdomain", "::1", "[::1]"].includes(parsed.hostname);
}

function parseIpv4(hostname) {
  const pieces = hostname.split(".");
  if (pieces.length !== 4 || pieces.some(piece => !/^\d{1,3}$/.test(piece))) return null;
  const octets = pieces.map(Number);
  return octets.every(value => value >= 0 && value <= 255) ? octets : null;
}

export function isPrivateLanHost(hostname) {
  const host = String(hostname || "").replace(/^\[|\]$/g, "").toLowerCase();
  const ipv4 = parseIpv4(host);
  if (ipv4) {
    const [a, b] = ipv4;
    return a === 10 || a === 127 || (a === 172 && b >= 16 && b <= 31) ||
      (a === 192 && b === 168) || (a === 169 && b === 254);
  }
  // ULA fc00::/7, link-local fe80::/10 e loopback. Hostnames comuns não são
  // aceitos aqui: use a mesma origem do ARCZ ou um IP/CIDR autorizado no server.
  return host === "::1" || /^f[cd][0-9a-f]*:/i.test(host) || /^fe[89ab][0-9a-f]*:/i.test(host);
}

export function isSameOriginUrl(url) {
  const base = globalThis.location?.href || "http://127.0.0.1/";
  const parsed = new URL(url, base);
  const current = new URL(base);
  return parsed.origin === current.origin;
}

export function assertBrowserUrlAllowed(mode, url) {
  validateNetworkMode(mode);
  const resolved = new URL(url, globalThis.location?.href || "http://127.0.0.1/");
  if (mode === NETWORK_MODES.IMPORT_ASSISTED) return resolved;
  if (isLoopbackUrl(resolved) || isSameOriginUrl(resolved)) return resolved;
  if (mode === NETWORK_MODES.LOCAL_LAN && isPrivateLanHost(resolved.hostname)) return resolved;
  const error = new Error(`Egress bloqueado em ${mode}: ${resolved.origin}`);
  error.code = "NETWORK_EGRESS_DENIED";
  error.details = { mode, origin: resolved.origin, hostname: resolved.hostname };
  throw error;
}

export function networkAwareFetch(mode, fetchImpl = globalThis.fetch?.bind(globalThis)) {
  validateNetworkMode(mode);
  if (!fetchImpl) throw new Error("fetch indisponível");
  return (input, init = {}) => {
    const url = typeof input === "string" || input instanceof URL ? String(input) : input.url;
    assertBrowserUrlAllowed(mode, url);
    return fetchImpl(input, init);
  };
}
