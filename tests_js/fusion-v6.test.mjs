import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

if (typeof globalThis.CustomEvent === "undefined") {
  globalThis.CustomEvent = class CustomEvent extends Event {
    constructor(type, init = {}) { super(type); this.detail = init.detail; }
  };
}

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const { buildModelingContextRequest } = await import("../app/floorplanner/modeling-context.js");
const { buildPhotorealRequest, PhotorealClient } = await import("../app/render/photoreal-client.js");
const { FusionSharedState } = await import("../app/shell/fusion-shared-state.js");
const { LocalApiClient, ArczApiError } = await import("../app/core/api-client.js");
const { StreetSequence } = await import("../app/walk/street-sequence.js");
const { estadoInicial } = await import("../app/estado.js");

function activeRegion() {
  return {
    request: {
      region_id: "region-js",
      bbox_wgs84: [-48.501, -27.151, -48.499, -27.149],
      polygon_wgs84: [[-48.5005, -27.1505, 10], [-48.4995, -27.1505, 10], [-48.4995, -27.1495, 10]],
      focus: { lon: -48.5, lat: -27.15 },
      scale: "lote",
      generation_epoch: 2,
    },
    context: { region_id: "region-js", origin_wgs84: [-48.5, -27.15, 10] },
    generation_epoch: 2,
  };
}

test("ModelingContextRequest usa recorte desenhado como autoridade do lote", () => {
  const state = {
    active_region: activeRegion(),
    recorte: { perimetro: [
      { lon: -48.5002, lat: -27.1502 },
      { lon: -48.4998, lat: -27.1502 },
      { lon: -48.4998, lat: -27.1498 },
    ] },
    region_profiles: { coastal: { version: 1 } },
    floorplanner_north_rotation_deg: 17,
    floorplanner_vertical_offset_m: 1.5,
    floorplanner_constraints: { max_floors: 6 },
    reference_media: ["a".repeat(64)],
  };
  const request = buildModelingContextRequest(state);
  assert.equal(request.selection.kind, "lote");
  assert.equal(request.selection.source.kind, "user_drawn_recorte");
  assert.equal(request.selection.parcel_polygon_wgs84.length, 3);
  assert.deepEqual(request.selection.bbox_wgs84, [-48.5002, -27.1502, -48.4998, -27.1498]);
  assert.equal(request.north_rotation_deg, 17);
  assert.deepEqual(request.reference_media, ["a".repeat(64)]);
});

test("ModelingContextRequest recusa abertura sem Região Ativa", () => {
  assert.throws(
    () => buildModelingContextRequest({}),
    error => error?.code === "ACTIVE_REGION_REQUIRED",
  );
});

test("pedido fotorreal referencia revisão real e deduplica mídias", () => {
  const request = buildPhotorealRequest({
    project: { id: "fp", current_revision: 3 },
    prompt: "cinematic",
    negativePrompt: "deformed",
    references: ["b".repeat(64), "b".repeat(64)],
    width: 7680,
    height: 3291,
    mode: "full_photoreal",
    seed: 11,
  });
  assert.equal(request.floorplanner_project_id, "fp");
  assert.equal(request.revision, 3);
  assert.deepEqual(request.resolution, { width: 7680, height: 3291 });
  assert.deepEqual(request.reference_media, ["b".repeat(64)]);
  assert.ok(request.passes.includes("depth"));
  assert.ok(request.passes.includes("object_ids"));
  assert.equal(request.enhancement.geometry_guard_px, 2);
});

test("PhotorealClient usa somente rotas locais e expõe cancelamento", async () => {
  const calls = [];
  const api = { json: async (route, options = {}) => { calls.push([route, options]); return { id: "job", status: "QUEUED" }; } };
  const client = new PhotorealClient({ api });
  await client.preflight({ a: 1 });
  await client.submit({ b: 2 });
  await client.getJob("job a");
  await client.cancelJob("job a", "test");
  assert.deepEqual(calls.map(value => value[0]), [
    "/api/v2/photoreal/preflight",
    "/api/v2/photoreal/jobs",
    "/api/v2/render/jobs/job%20a",
    "/api/v2/render/jobs/job%20a/cancel",
  ]);
});

test("FusionSharedState valida hashes e propaga prompt para render", () => {
  const shared = new FusionSharedState();
  let refs = null;
  let prompt = null;
  shared.addEventListener("references", event => { refs = event.detail; });
  shared.addEventListener("prompt", event => { prompt = event.detail; });
  shared.setReferences(["c".repeat(64), "c".repeat(64), "invalid"]);
  shared.setPrompt({ positive: "golden hour", negative: "AI artifacts" });
  assert.deepEqual(refs, ["c".repeat(64)]);
  assert.equal(prompt.positive, "golden hour");
  assert.equal(prompt.negative, "AI artifacts");
});

test("LocalApiClient rejeita egress e preserva erro estruturado", async () => {
  const client = new LocalApiClient({
    fetchImpl: async () => new Response(JSON.stringify({
      error: { code: "MODEL_NOT_INSTALLED", message: "Modelo ausente", retryable: false, details: { task: "chat.global" }, trace_id: "trace" },
    }), { status: 503, headers: { "content-type": "application/json" } }),
  });
  await assert.rejects(
    () => client.json("/api/v2/ai/tools"),
    error => error instanceof ArczApiError && error.code === "MODEL_NOT_INSTALLED" && error.traceId === "trace",
  );
  await assert.rejects(
    () => client.json("https://example.invalid/api"),
    /Rota local inválida/,
  );
  assert.throws(
    () => new LocalApiClient({ fetchImpl: async () => new Response("{}"), baseUrl: "https://example.invalid" }),
    /Base URL deve ser HTTP loopback/,
  );
});

test("StreetSequence navega e localiza frame sem provider", () => {
  const sequence = new StreetSequence({
    schema_version: 1,
    frames: [
      { id: "a", image: "a.png", sha256: "d".repeat(64), lat: -27.15, lon: -48.5, next: ["b"] },
      { id: "b", image: "b.png", sha256: "e".repeat(64), lat: -27.1501, lon: -48.5001, next: [] },
    ],
  });
  assert.equal(sequence.next("a")[0].id, "b");
  assert.equal(sequence.nearest({ lat: -27.15, lon: -48.5 }, 20).frame.id, "a");
  assert.equal(sequence.nearest({ lat: 0, lon: 0 }, 20), null);
});

test("estado inicial V6 contém shell, Floorplanner e abertura cinematográfica válidos", () => {
  const state = estadoInicial();
  assert.equal(state.workspace_mode, "globo");
  assert.equal(state.active_floorplanner_project_id, null);
  assert.deepEqual(state.floorplanner_projects, []);
  assert.equal(state.earth_presentation.schema_version, 1);
  assert.equal(state.earth_presentation.atmosphere, true);
  assert.equal(state.earth_presentation.clouds, true);
  assert.equal(state.earth_presentation.skip_on_reduced_motion, true);
});

test("rota nativa Aedifex usa broker ARCZ local e não cliente OpenAI", async () => {
  const route = await readFile(path.join(ROOT, "integrations/aedifex/overlay/apps/arcz-floorplanner/app/api/ai/chat/route.ts"), "utf8");
  assert.match(route, /\/api\/v2\/ai\/tools/);
  assert.match(route, /chat\.global/);
  assert.match(route, /normalizeLoopbackApi/);
  assert.doesNotMatch(route, /createAIClient|api\.openai\.com|AI_API_KEY/);
});

test("bootstrap Aedifex registra plugins locais sem descoberta remota", async () => {
  const bootstrap = await readFile(path.join(ROOT, "integrations/aedifex/overlay/apps/arcz-floorplanner/lib/bootstrap.ts"), "utf8");
  assert.match(bootstrap, /builtinPlugin/);
  assert.match(bootstrap, /treesPlugin/);
  assert.doesNotMatch(bootstrap, /import\s*\{[^}]*discoverPlugins/);
  assert.doesNotMatch(bootstrap, /\bawait\s+discoverPlugins\s*\(/);
});

test("Floorplanner usa um único agente global com ferramentas Aedifex, sem duplicar editor ou histórico", async () => {
  const page = await readFile(path.join(ROOT, "integrations/aedifex/overlay/apps/arcz-floorplanner/app/page.tsx"), "utf8");
  const combined = await readFile(path.join(ROOT, "integrations/aedifex/overlay/apps/arcz-floorplanner/app/ui/combined-ai-panel.tsx"), "utf8");
  const tools = await readFile(path.join(ROOT, "integrations/aedifex/overlay/packages/arcz-aedifex-tools/src/index.ts"), "utf8");
  assert.match(page, /<Editor/);
  assert.match(page, /CombinedAiPanel/);
  assert.match(combined, /<ArczChatPanel/);
  assert.doesNotMatch(combined, /<AIChatPanel/);
  assert.match(tools, /createAedifexMcpServer/);
  assert.match(tools, /listAedifexTools/);
  assert.match(tools, /dryRun/);
  assert.match(tools, /approvalId/);
  assert.equal((page.match(/<Editor/g) || []).length, 1);
});
