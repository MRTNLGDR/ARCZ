// ARCZ · Bootstrap do visualizador.
import { estadoApp } from "./estado.js";
import { ambienteApp } from "./ambiente.js";
import { entornoApp } from "./entorno.js";
import { cameraApp } from "./camera.js";
import { gizmoApp } from "./gizmo.js";
import { cenaApp, normalizarPosicao } from "./cena.js";
import { bibliotecaApp } from "./lib.js";
import { feedbackApp } from "./feedback.js";
import { posicionadorApp } from "./posicionar.js";
import { corteApp } from "./corte.js";
import { recorteApp } from "./recorte.js";
import { uiApp } from "./ui.js";
import { criarProvedorDeRelevo } from "./relevo.js";
import { qualidadeApp } from "./qualidade.js";
import { initializeV2Runtime } from "./runtime-v2.js";
import { initializeFusionShell } from "./shell/fusion-shell.js";

window.addEventListener("error", e => exibirErro(`Erro JS: ${e.message} (${e.filename}:${e.lineno})`));
window.addEventListener("unhandledrejection", e => exibirErro(`Promise rejeitada: ${e.reason}`));
window.addEventListener("pagehide", () => { void estadoApp.flushAgora?.(); });

function exibirErro(msg) {
  let banner = document.getElementById("error_banner");
  if (!banner) {
    banner = document.createElement("div");
    banner.id = "error_banner";
    banner.style.cssText =
      "position:fixed;bottom:40px;left:10px;right:10px;z-index:99999;background:#8f1d28;" +
      "color:#fff;padding:10px 14px;border-radius:6px;font:12px sans-serif;box-shadow:none;cursor:pointer";
    banner.addEventListener("click", () => banner.remove());
    document.body.appendChild(banner);
  }
  banner.textContent = msg;
  console.error(msg);
}

function bloquearShell(msg) {
  const workspace = document.getElementById("fusion_workspace");
  if (workspace) workspace.remove();
  const body = document.getElementById("corpo");
  if (!body) return;
  const blocker = document.createElement("div");
  blocker.className = "arcz-primary-shell-error";
  const title = document.createElement("strong");
  title.textContent = "Interface principal indisponível";
  const detail = document.createElement("p");
  detail.textContent = msg;
  const action = document.createElement("p");
  action.textContent = "Abra Diagnóstico/console e corrija o runtime. O ARCZ não troca silenciosamente para uma UI antiga.";
  blocker.append(title, detail, action);
  body.appendChild(blocker);
}

document.addEventListener("DOMContentLoaded", async () => {
  try {
    Cesium.Ion.defaultAccessToken = undefined;

    const viewer = new Cesium.Viewer("cesiumContainer", {
      animation: false,
      baseLayerPicker: false,
      fullscreenButton: false,
      geocoder: false,
      homeButton: false,
      infoBox: false,
      sceneModePicker: false,
      selectionIndicator: false,
      timeline: false,
      navigationHelpButton: false,
      scene3DOnly: true,
      requestRenderMode: true,
      maximumRenderTimeChange: 0.05,
      baseLayer: false,
      contextOptions: {
        webgl: {
          powerPreference: "high-performance",
          failIfMajorPerformanceCaveat: false,
          preferLowPowerToHighPerformance: false,
          alpha: false,
          depth: true,
          stencil: true,
          antialias: true
        },
        allowTextureFilterAnisotropic: true
      },
      terrainProvider: new Cesium.EllipsoidTerrainProvider()
    });
    window.arczViewer = viewer;

    viewer.scene.globe.baseColor = Cesium.Color.fromCssColorString('#020917');
    viewer.scene.globe.showWaterEffect = true;
    viewer.scene.globe.tileCacheSize = 250;
    viewer.scene.globe.loadingDescendantLimit = 10;
    viewer.scene.globe.depthTestAgainstTerrain = true;

    const gpu = qualidadeApp.inicializar(viewer);
    if (gpu.software) {
      console.warn(`ARCZ: navegador sem aceleração de vídeo (${gpu.nome}). Use ABRIR_ARCZ.cmd para validar/iniciar o runtime local.`);
    }

    if (viewer.scene.requestRenderMode) {
      estadoApp.inscrever(() => viewer.scene.requestRender());
    }

    ambienteApp.inicializar(viewer);
    cameraApp.inicializar(viewer);
    cenaApp.inicializar(viewer);
    feedbackApp.inicializar(viewer);
    posicionadorApp.inicializar(viewer);
    gizmoApp.inicializar(viewer);
    bibliotecaApp.inicializar(viewer);
    entornoApp.inicializar(viewer);
    corteApp.inicializar(viewer);
    recorteApp.inicializar(viewer);
    uiApp.inicializar(viewer);

    await estadoApp.carregarDoServidor();

    // Runtime V2 é aditivo ao mapa, mas qualquer falha fica explícita no painel
    // de diagnóstico. A casca principal nunca é substituída por uma UI fake.
    let runtimeV2 = null;
    try {
      runtimeV2 = initializeV2Runtime({ viewer });
    } catch (runtimeError) {
      exibirErro(`ARCZ V2 indisponível: ${runtimeError.message || runtimeError}`);
    }

    const st = estadoApp.obter();
    const pos = normalizarPosicao(st.posicao);

    if (st.ambiente?.relevo === "dem") {
      const relevo = criarProvedorDeRelevo();
      if (relevo) viewer.terrainProvider = relevo;
      else console.warn("ARCZ: DEM local solicitado, mas o provider desta build está indisponível.");
    }

    const principal = st.primary_model;
    if (principal?.enabled && principal?.path) {
      await cenaApp.carregarPredio(principal.path, st.posicao, principal.lod || st.posicao?.lod || "equilibrado");
    } else {
      await cenaApp.carregarPredio(null, st.posicao);
    }
    await cenaApp.sincronizarDerivadoAtivo(st);
    cenaApp.restaurarPecas(st.pecas || []);

    if (st.corte?.ativo) corteApp.aplicar();
    if ((st.recorte?.perimetro || []).length >= 3) recorteApp.desenharEntidades();

    cameraApp.definirCamera(
      st.camera && st.camera.lat
        ? st.camera
        : { lat: pos.lat, lon: pos.lon, alt: 250, pitch: -30 }
    );

    try {
      await initializeFusionShell({ viewer, estadoApp, runtime: runtimeV2 });
    } catch (fusionError) {
      const message = fusionError?.message || String(fusionError);
      exibirErro(`ARCZ Fusion indisponível: ${message}`);
      bloquearShell(message);
      throw fusionError;
    }

    console.log(`ARCZ pronto — ${(st.pecas || []).length} peças, ${(st.takes || []).length} takes`);
  } catch (e) {
    exibirErro(`Erro ao iniciar aplicativo: ${e.message}`);
  }
});
