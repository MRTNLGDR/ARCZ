import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const {
  normalizeSiteAuthoringLayout,
  bboxFromRegionState,
  regionSummary,
} = await import("../app/floorplanner/site-authoring-layout.js");
const {
  clampPanelWidth,
  nextPanelTabIndex,
  panelHoverIsAvailable,
} = await import("../app/shell/paineis/collapsible-panel-dock.js");
const {
  slugifyPrompt,
  parsePromptTags,
  extractInferenceText,
} = await import("../app/prompts/prompt-library-model.js");
const {
  normalizeReferenceRoles,
  previewKind,
} = await import("../app/media/reference-media-model.js");
const {
  parseVector3,
  normalizeRenderPasses,
  buildPhotorealRequest,
} = await import("../app/render/photoreal-client.js");

function installCesiumStub() {
  globalThis.Cesium = {
    Math: { toRadians: value => value * Math.PI / 180 },
    Cartesian2: class Cartesian2 { constructor(x, y) { this.x = x; this.y = y; } },
    Cartesian3: class Cartesian3 {
      constructor(x, y, z) { this.x = x; this.y = y; this.z = z; }
      static fromDegrees(lon, lat, height) { return { lon, lat, height }; }
    },
    CloudCollection: class CloudCollection {
      constructor() { this.items = []; this.show = true; }
      add(value) { this.items.push(value); return value; }
      removeAll() { this.items.length = 0; }
      isDestroyed() { return false; }
    },
    DynamicAtmosphereLightingType: { SUNLIGHT: "sunlight" },
    Color: { fromCssColorString: value => ({ value }) },
    EasingFunction: { QUINTIC_IN_OUT: "quint", CUBIC_OUT: "cubic" },
  };
}
installCesiumStub();
const {
  normalizeEarthPresentation,
  resolveEarthIntroTarget,
  ensureProceduralClouds,
  applyCinematicEarthBaseline,
  CinematicGlobeIntro,
  flyToCamera,
} = await import("../app/earth/cinematic-globe.js");



test("dock usa navegação circular, hover apenas em ponteiro fino e largura segura", () => {
  assert.equal(nextPanelTabIndex(0, "ArrowUp", 4), 3);
  assert.equal(nextPanelTabIndex(3, "ArrowDown", 4), 0);
  assert.equal(nextPanelTabIndex(2, "Home", 4), 0);
  assert.equal(nextPanelTabIndex(1, "End", 4), 3);
  assert.equal(panelHoverIsAvailable(() => ({ matches: true })), false);
  assert.equal(panelHoverIsAvailable(() => ({ matches: false })), true);
});

test("baseline cinematográfico e nuvens locais não dependem de provider", () => {
  const primitives = {
    values: [],
    add(value) { this.values.push(value); return value; },
  };
  const scene = {
    primitives, requestRender() {}, highDynamicRange: false,
    postProcessStages: { fxaa: { enabled: false } },
    skyAtmosphere: {}, atmosphere: {}, sun: {}, moon: {}, skyBox: {}, fog: {},
    globe: {},
  };
  const result = applyCinematicEarthBaseline({ scene }, { clouds: true, fog: true });
  assert.equal(result.applied, true);
  assert.equal(scene.highDynamicRange, true);
  assert.equal(scene.globe.enableLighting, true);
  const first = ensureProceduralClouds(scene, { lon: -48, lat: -27 }, { cloud_count: 4 });
  assert.equal(first.created, true);
  assert.equal(first.count, 4);
  assert.equal(first.collection.items.length, 4);
  const second = ensureProceduralClouds(scene, { lon: -48, lat: -27 }, { cloud_count: 2 });
  assert.equal(second.collection, first.collection);
  assert.equal(second.collection.items.length, 2, "regeneração deve reutilizar e limpar a coleção local");
});

test("estado da abertura funciona em runtimes sem CustomEvent", () => {
  const previousEvent = globalThis.CustomEvent;
  const previousDispatch = globalThis.dispatchEvent;
  try {
    delete globalThis.CustomEvent;
    delete globalThis.dispatchEvent;
    let observed = null;
    const intro = new CinematicGlobeIntro({ onStateChange: value => { observed = value; } });
    assert.doesNotThrow(() => intro._setState("PREPARING", { progress: 0.25 }));
    assert.deepEqual(observed, { state: "PREPARING", progress: 0.25 });
  } finally {
    if (previousEvent !== undefined) globalThis.CustomEvent = previousEvent;
    if (previousDispatch !== undefined) globalThis.dispatchEvent = previousDispatch;
  }
});

test("layout de autoria mantém globo visível e limita dimensões persistíveis", () => {
  assert.deepEqual(normalizeSiteAuthoringLayout({ split_ratio: 0.01, auto_publish_delay_ms: 9 }), {
    schema_version: 1,
    show_globe: true,
    split_ratio: 0.2,
    auto_publish: true,
    auto_publish_delay_ms: 400,
  });
  assert.equal(normalizeSiteAuthoringLayout({ show_globe: false, auto_publish: false }).show_globe, false);
  assert.equal(clampPanelWidth(12), 240);
  assert.equal(clampPanelWidth(999), 720);
});

test("lote desenhado domina bbox e resumo da Região Ativa", () => {
  const state = {
    active_region: { request: { region_id: "region", scale: "bairro", bbox_wgs84: [0, 0, 9, 9] } },
    recorte: { perimetro: [
      { lon: -48.5, lat: -27.15 },
      { lon: -48.49, lat: -27.15 },
      { lon: -48.49, lat: -27.14 },
    ] },
  };
  assert.deepEqual(bboxFromRegionState(state), [-48.5, -27.15, -48.49, -27.14]);
  assert.equal(regionSummary(state).scale, "lote desenhado");
  assert.equal(regionSummary(state).source, "recorte manual bloqueável");
});

test("biblioteca de prompts normaliza slug/tags e exige saída textual real", () => {
  assert.equal(slugifyPrompt("  Fachada Úmida 8K  "), "fachada-umida-8k");
  assert.deepEqual(parsePromptTags("exterior, 8k, exterior,cinema"), ["exterior", "8k", "cinema"]);
  assert.equal(extractInferenceText({ result: { translation: "texto traduzido" } }), "texto traduzido");
  assert.throws(() => extractInferenceText({ result: {} }), /contrato textual inválido/);
});

test("mídias preservam papéis válidos e escolhem preview sem fingir suporte", () => {
  assert.deepEqual(normalizeReferenceRoles(["style", "style", " camera "]), ["style", "camera"]);
  assert.deepEqual(normalizeReferenceRoles([]), ["reference"]);
  assert.equal(previewKind({ category: "image", mime: "image/png" }), "image");
  assert.equal(previewKind({ category: "image", mime: "image/x-exr" }), "metadata");
  assert.equal(previewKind({ category: "document", mime: "application/pdf" }), "pdf");
});

test("request fotorreal valida câmera, passes e saída 8K sem provider remoto", () => {
  assert.deepEqual(parseVector3("1, 2; 3", [0, 0, 0]), [1, 2, 3]);
  assert.deepEqual(parseVector3("x", [4, 5, 6]), [4, 5, 6]);
  assert.deepEqual(normalizeRenderPasses(["beauty", "depth", "depth", "unknown"]), ["beauty", "depth"]);
  const request = buildPhotorealRequest({
    project: { id: "project", current_revision: 9 },
    width: 8192,
    height: 4320,
    format: "exr",
    outputName: "take 01/hero",
    references: ["a".repeat(64), "a".repeat(64)],
    camera: { position: "12 8 12", target: "0 2 0", focal_length_mm: 50 },
  });
  assert.deepEqual(request.resolution, { width: 8192, height: 4320 });
  assert.equal(request.format, "exr");
  assert.equal(request.output_name, "take-01-hero");
  assert.equal(request.camera.focal_length_mm, 50);
  assert.deepEqual(request.reference_media, ["a".repeat(64)]);
});

test("configuração cinematográfica é limitada e destino inválido não move câmera", () => {
  const value = normalizeEarthPresentation({
    duration_ms: 99,
    start_altitude_m: 999999999,
    orbit_altitude_m: 300,
    end_altitude_m: -10,
    orbit_heading_delta_deg: 200,
  });
  assert.equal(value.duration_ms, 400);
  assert.equal(value.start_altitude_m, 80000000);
  assert.equal(value.orbit_altitude_m, 500);
  assert.equal(value.end_altitude_m, 20);
  assert.equal(value.orbit_heading_delta_deg, 90);
  assert.equal(resolveEarthIntroTarget({ lat: 92, lon: 0 }), null);
  assert.deepEqual(resolveEarthIntroTarget({ lat: -27, lon: -48, pitch: -200 }), {
    lon: -48, lat: -27, alt: 250, heading: 0, pitch: -90, roll: 0,
  });
});

test("flyToCamera só resolve quando callback real do Cesium termina", async () => {
  let options;
  const camera = {
    flyTo(value) { options = value; },
    cancelFlight() { options?.cancel?.(); },
  };
  let settled = false;
  const promise = flyToCamera(camera, { duration: 1 }).then(value => { settled = true; return value; });
  await Promise.resolve();
  assert.equal(settled, false, "o wrapper não pode fingir conclusão síncrona");
  options.complete();
  assert.equal(await promise, true);

  const controller = new AbortController();
  const aborted = flyToCamera(camera, { duration: 1 }, { signal: controller.signal });
  controller.abort();
  assert.equal(await aborted, false);
});

test("host Floorplanner conserva Cesium, publicação por revisão e autoridade única", async () => {
  const source = await readFile(path.join(ROOT, "app/floorplanner/floorplanner-host.js"), "utf8");
  assert.match(source, /attachGlobe\(/);
  assert.match(source, /publishNow\("revision_saved_auto"\)/);
  assert.match(source, /readonly/);
  assert.match(source, /scene_hash/);
  assert.match(source, /generation_epoch/);
  assert.doesNotMatch(source, /Google|Mapbox|api\.openai/);
});
