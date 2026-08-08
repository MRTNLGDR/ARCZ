import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const { collectModelingContextLayers } = await import("../app/floorplanner/modeling-context.js");
const { createFloorplannerBridgeChannel } = await import("../app/floorplanner/floorplanner-host.js");
const { FloorplannerClient } = await import("../app/floorplanner/floorplanner-client.js");
const { estadoInicial } = await import("../app/estado.js");

const HASH_A = "a".repeat(64);
const HASH_B = "b".repeat(64);

test("camadas de contexto são locais, imutáveis, deduplicadas e determinísticas", () => {
  const state = {
    procedural_layers: [{
      id: "roads",
      owner: "generator:roads",
      manifest: {
        job_id: "job-1",
        generator: "arcz.roads@2",
        inputs_hash: HASH_B,
        source_versions: { osm: "fixture" },
        source_packages: [HASH_A],
        outputs: [
          { kind: "geojson", path: "data/context/roads.geojson", sha256: HASH_A },
          { kind: "png", path: "data/context/preview.png", sha256: HASH_B },
        ],
      },
    }],
    floorplanner_context_layers: [
      {
        id: "manual:terrain",
        role: "terrain",
        format: "glb",
        asset_path: "/data/context/terrain.glb",
        sha256: HASH_B,
        coordinate_space: "ENU_LOCAL",
        transform: { position_m: [0, 0, 0], rotation_euler_rad: [0, 0, 0], scale: [1, 1, 1] },
      },
      // exact duplicate must not be mounted twice
      { id: "manual:terrain:duplicate", format: "glb", path: "/data/context/terrain.glb", sha256: HASH_B },
    ],
  };
  const result = collectModelingContextLayers(state);
  assert.equal(result.length, 2);
  assert.deepEqual(result.map(item => item.role), ["roads", "terrain"]);
  assert.ok(result.every(item => item.readonly === true));
  assert.ok(result.every(item => item.asset_path.startsWith("/")));
  assert.equal(result[0].format, "geojson");
  assert.equal(result[1].coordinate_space, "ENU_LOCAL");
});

test("camada de contexto recusa provider remoto e hash ausente", () => {
  assert.throws(
    () => collectModelingContextLayers({ floorplanner_context_layers: [{ format: "glb", path: "https://provider.invalid/model.glb", sha256: HASH_A }] }),
    error => error?.code === "CONTEXT_LAYER_PATH_INVALID",
  );
  assert.throws(
    () => collectModelingContextLayers({ floorplanner_context_layers: [{ format: "glb", path: "/data/model.glb" }] }),
    error => error?.code === "CONTEXT_LAYER_HASH_REQUIRED",
  );
});

test("canal postMessage usa entropia criptográfica e o sidecar o exige", () => {
  const first = createFloorplannerBridgeChannel();
  const second = createFloorplannerBridgeChannel();
  assert.match(first, /^[a-f0-9-]{32,64}$/i);
  assert.notEqual(first, second);
  const client = new FloorplannerClient();
  const status = { runtime: { url: "http://127.0.0.1:8124" } };
  assert.throws(() => client.sidecarUrl("project", status), error => error?.code === "AEDIFEX_BRIDGE_CHANNEL_INVALID");
  const url = new URL(client.sidecarUrl("project", status, { channel: first }));
  assert.equal(url.origin, "http://127.0.0.1:8124");
  assert.equal(url.searchParams.get("project"), "project");
  assert.equal(url.searchParams.get("channel"), first);
});

test("estado persistente V10 inclui camadas de contexto sem segredo ou URL externa", () => {
  const state = estadoInicial();
  assert.deepEqual(state.floorplanner_context_layers, []);
  assert.deepEqual(state.floorplanner_derivatives, []);
  assert.equal(state.network_mode, "offline_strict");
});

test("overlay possui um único fluxo IFC transacional e copia WASM local", async () => {
  const page = await readFile(path.join(ROOT, "integrations/aedifex/overlay/apps/arcz-floorplanner/app/page.tsx"), "utf8");
  const panel = await readFile(path.join(ROOT, "integrations/aedifex/overlay/apps/arcz-floorplanner/app/ui/arcz-import-export-panel.tsx"), "utf8");
  const pkg = JSON.parse(await readFile(path.join(ROOT, "integrations/aedifex/overlay/apps/arcz-floorplanner/package.json"), "utf8"));
  assert.equal((page.match(/ArczImportExportPanel/g) || []).length >= 2, true); // import + mount
  assert.doesNotMatch(page, /ArczIfcPanel/);
  assert.match(panel, /convertIfcToAedifex/);
  assert.match(panel, /applySceneGraphToEditor\(before/);
  assert.match(panel, /expected_revision|onCommitImportedScene/);
  assert.equal(pkg.dependencies["@aedifex/ifc-converter"], "workspace:*");
  assert.match(pkg.scripts.prebuild, /copy-web-ifc-wasm/);
  assert.match(pkg.scripts.predev, /copy-web-ifc-wasm/);
});

test("contexto Aedifex 3D é readonly, hash-verificado e excluído do export", async () => {
  const source = await readFile(path.join(ROOT, "integrations/aedifex/overlay/apps/arcz-floorplanner/app/ui/arcz-context-layers.tsx"), "utf8");
  const bridge = await readFile(path.join(ROOT, "integrations/aedifex/overlay/packages/arcz-bridge/src/index.ts"), "utf8");
  assert.match(source, /arczExportExclude: true/);
  assert.match(source, /nonEditable: true/);
  assert.match(source, /object\.raycast = \(\) => null/);
  assert.match(source, /contextLayerMatrix/);
  assert.match(bridge, /CONTEXT_LAYER_HASH_MISMATCH/);
  assert.match(bridge, /crypto\.subtle\.digest\('SHA-256'/);
});

test("host e sidecar exigem o mesmo canal em ambas as direções", async () => {
  const host = await readFile(path.join(ROOT, "app/floorplanner/floorplanner-host.js"), "utf8");
  const page = await readFile(path.join(ROOT, "integrations/aedifex/overlay/apps/arcz-floorplanner/app/page.tsx"), "utf8");
  assert.match(host, /data\.channel !== this\.channel/);
  assert.match(host, /channel: this\.channel/);
  assert.match(page, /data\.channel !== config\.channel/);
  assert.match(page, /\{ \.\.\.payload, channel: config\.channel \}/);
});
