import test from "node:test";
import assert from "node:assert/strict";

import { Transacao, executarTransacao } from "../app/core/transacao.js";
import { safeCallback, diagnosticoSafeCallbacks, reativarSafeCallback } from "../app/core/safe-callback.js";
import { ResourceTracker } from "../app/core/resource-tracker.js";
import { criarContextoPlugin, CAPABILITIES } from "../app/core/contexto.js";
import { geodesic, slerp, logAltitude } from "../app/cine/interpolation.js";
import { evaluateTrack, upsertKeyframe } from "../app/cine/keyframes.js";
import { policyForScale, clampGenerationRadius, assertPluginScale } from "../app/region/scale-policy.js";
import { PluginRegistry } from "../app/plugins/registry.js";


test("transação confirma em ordem e descarta rollbacks", async () => {
  const events = [];
  const result = await executarTransacao("commit-test", async tx => {
    tx.onRollback(() => events.push("rollback"));
    tx.onCommit(() => events.push("commit-1"));
    tx.onCommit(() => events.push("commit-2"));
    return 42;
  });
  assert.equal(result, 42);
  assert.deepEqual(events, ["commit-1", "commit-2"]);
});


test("falha durante commit executa rollback LIFO", async () => {
  const events = [];
  await assert.rejects(
    executarTransacao("rollback-test", async tx => {
      tx.onRollback(() => events.push("rollback-1"));
      tx.onRollback(() => events.push("rollback-2"));
      tx.onCommit(() => events.push("commit-1"));
      tx.onCommit(() => { throw new Error("commit failed"); });
    }),
    /commit failed/,
  );
  assert.deepEqual(events, ["commit-1", "rollback-2", "rollback-1"]);
});


test("safeCallback nunca relança no laço e desativa após limiar", () => {
  const previous = console.error;
  console.error = () => {};
  try {
    let calls = 0;
    const callback = safeCallback("test.safe.callback", -1, () => {
      calls += 1;
      throw new Error("render failure");
    }, { limiteFalhas: 2, janelaMs: 10_000 });
    assert.equal(callback(), -1);
    assert.equal(callback(), -1);
    assert.equal(callback(), -1);
    assert.equal(calls, 2, "callback desativado não deve chamar função perigosa novamente");
    const state = diagnosticoSafeCallbacks().find(item => item.nome === "test.safe.callback");
    assert.equal(state.desativado, true);
    assert.equal(reativarSafeCallback("test.safe.callback"), true);
    const reactivated = diagnosticoSafeCallbacks().find(item => item.nome === "test.safe.callback");
    assert.equal(reactivated.desativado, false);
  } finally {
    console.error = previous;
  }
});


test("ResourceTracker limpa recursos uma única vez", () => {
  const tracker = new ResourceTracker("fixture");
  let disposed = 0;
  tracker.add(() => { disposed += 1; }, "custom");
  tracker.add(() => { disposed += 1; }, "subscriptions");
  assert.equal(tracker.snapshot().total, 2);
  const snapshot = tracker.disposeAll();
  assert.equal(disposed, 2);
  assert.equal(snapshot.total, 0);
  tracker.disposeAll();
  assert.equal(disposed, 2);
});


test("contexto de plugin bloqueia capabilities não concedidas", () => {
  const ctx = criarContextoPlugin({
    pluginId: "arcz.test.capabilities",
    capabilities: [CAPABILITIES.REGION_READ],
    services: { regionRead: () => ({ id: "r" }) },
  });
  assert.deepEqual(ctx.region.read(), { id: "r" });
  assert.throws(() => ctx.scene.stagePrimitive({}), /Capability não concedida/);
  assert.equal(Object.isFrozen(ctx), true);
  ctx.resources.disposeAll();
});


test("interpolação geográfica, quaternion e altitude permanecem finitas", () => {
  const halfway = geodesic([-48, -27, 10], [-47, -26, 110], 0.5);
  assert.equal(halfway.length, 3);
  assert.ok(halfway.every(Number.isFinite));
  assert.ok(halfway[0] > -48 && halfway[0] < -47);
  assert.ok(halfway[1] > -27 && halfway[1] < -26);
  assert.equal(halfway[2], 60);

  const q = slerp([0, 0, 0, 1], [0, 0, 1, 0], 0.5);
  assert.ok(Math.abs(Math.hypot(...q) - 1) < 1e-9);
  assert.ok(logAltitude(0, 10_000, 0.5) > 0);
});


test("track usa keyframes ordenados e hold não interpola", () => {
  const track = { id: "x", value_type: "number", keyframes: [] };
  upsertKeyframe(track, { frame: 20, value: 20, interpolation: "linear" });
  upsertKeyframe(track, { frame: 0, value: 0, interpolation: "hold" });
  assert.deepEqual(track.keyframes.map(k => k.frame), [0, 20]);
  assert.equal(evaluateTrack(track, 10), 0);
  track.keyframes[0].interpolation = "linear";
  assert.equal(evaluateTrack(track, 10), 10);
});


test("política de escala impede geração planetária e limita bairro", () => {
  assert.equal(policyForScale("mundo").generationAllowed, false);
  assert.equal(clampGenerationRadius("mundo", 1000), 0);
  assert.equal(clampGenerationRadius("bairro", 9000), 2000);
  assert.doesNotThrow(() => assertPluginScale({ id: "x", escalas: ["bairro"] }, "bairro"));
  assert.throws(() => assertPluginScale({ id: "x", escalas: ["lote"] }, "cidade"), /não aceita escala/);
});


test("PluginRegistry rejeita contratos incompletos e filtra por modo", () => {
  const registry = new PluginRegistry();
  const manifest = {
    tipo: "ferramenta",
    id: "arcz.test.tool",
    nome: "Ferramenta de teste",
    versao: "1.0.0",
    apiVersion: "2",
    escalas: ["lote"],
    modos: ["globo"],
    capacidades: [],
    worker: "javascript",
    deterministico: true,
    custoBase: { triangulos: 0, memoriaMB: 0, texturasMB: 0, drawCalls: 0 },
  };
  assert.throws(() => registry.register({ manifest }), /sem ativar/);
  registry.register({ manifest, ativar() {}, desativar() {}, serializar() { return {}; } });
  assert.equal(registry.list({ mode: "globo" }).length, 1);
  assert.equal(registry.list({ mode: "render" }).length, 0);
});

import {
  NETWORK_MODES, assertBrowserUrlAllowed, isPrivateLanHost, networkAwareFetch,
} from "../app/core/network-mode.js";

test("política de rede do navegador bloqueia egress em offline_strict", async () => {
  assert.doesNotThrow(() => assertBrowserUrlAllowed(NETWORK_MODES.OFFLINE_STRICT, "/api/v2/health"));
  assert.throws(
    () => assertBrowserUrlAllowed(NETWORK_MODES.OFFLINE_STRICT, "https://example.invalid/data"),
    error => error?.code === "NETWORK_EGRESS_DENIED",
  );
  let called = false;
  const guarded = networkAwareFetch(NETWORK_MODES.OFFLINE_STRICT, async () => { called = true; return {}; });
  await assert.rejects(async () => guarded("https://example.invalid/data"), /Egress bloqueado/);
  assert.equal(called, false);
});

test("local_lan aceita IP privado literal e recusa DNS público", () => {
  assert.equal(isPrivateLanHost("192.168.1.10"), true);
  assert.equal(isPrivateLanHost("10.0.0.2"), true);
  assert.equal(isPrivateLanHost("172.31.4.7"), true);
  assert.equal(isPrivateLanHost("8.8.8.8"), false);
  assert.doesNotThrow(() => assertBrowserUrlAllowed(NETWORK_MODES.LOCAL_LAN, "http://192.168.1.10:8080/model.glb"));
  assert.throws(() => assertBrowserUrlAllowed(NETWORK_MODES.LOCAL_LAN, "https://example.invalid"), /Egress bloqueado/);
  assert.doesNotThrow(() => assertBrowserUrlAllowed(NETWORK_MODES.IMPORT_ASSISTED, "https://example.invalid"));
});

import { estadoInicial } from "../app/estado.js";

test("estado inicial é local-first e não persiste segredo de provider", () => {
  const state = estadoInicial();
  assert.equal(state.network_mode, NETWORK_MODES.OFFLINE_STRICT);
  assert.equal(state.ambiente.imagery, "naturalearth_local");
  assert.equal(state.ambiente.relevo, "ellipsoid");
  assert.equal(Object.hasOwn(state.ambiente, "token_mapbox"), false);
});
