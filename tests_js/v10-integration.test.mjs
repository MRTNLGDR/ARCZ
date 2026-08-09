import test from "node:test";
import assert from "node:assert/strict";
import { readFile, access } from "node:fs/promises";
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

test("dock usa navegação circular, hover apenas em ponteiro fino e largura segura", () => {
  assert.equal(nextPanelTabIndex(0, "ArrowUp", 4), 3);
  assert.equal(nextPanelTabIndex(3, "ArrowDown", 4), 0);
  assert.equal(nextPanelTabIndex(2, "Home", 4), 0);
  assert.equal(nextPanelTabIndex(1, "End", 4), 3);
  assert.equal(panelHoverIsAvailable(() => ({ matches: true })), false);
  assert.equal(panelHoverIsAvailable(() => ({ matches: false })), true);
  assert.equal(clampPanelWidth(12), 240);
  assert.equal(clampPanelWidth(999), 720);
});

test("startup V10 não possui mais abertura cinematográfica e entra no fluxo de autoria", async () => {
  const shell = await readFile(path.join(ROOT, "app/shell/fusion-shell.js"), "utf8");
  assert.match(shell, /await this\.activate\("globo", \{ persist: false \}\)/);
  assert.match(shell, /1 · Localizar/);
  assert.match(shell, /2 · Modelar/);
  assert.match(shell, /3 · Fotorreal/);
  assert.match(shell, /4 · Rua/);
  assert.doesNotMatch(shell, /CinematicGlobeIntro|\.play\(/);
  await assert.rejects(access(path.join(ROOT, "app/earth/cinematic-globe.js")));
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

test("host Floorplanner conserva Cesium, publicação por revisão e autoridade única", async () => {
  const source = await readFile(path.join(ROOT, "app/floorplanner/floorplanner-host.js"), "utf8");
  assert.match(source, /attachGlobe\(/);
  assert.match(source, /publishNow\("revision_saved_auto"\)/);
  assert.match(source, /readonly/);
  assert.match(source, /scene_hash/);
  assert.match(source, /generation_epoch/);
  assert.doesNotMatch(source, /Google|Mapbox|api\.openai/);
});
