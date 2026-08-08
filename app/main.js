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
      "position:fixed;bottom:40px;left:10px;right:10px;z-index:99999;background:rgba(220,38,38,.92);" +
      "color:#fff;padding:10px 14px;border-radius:6px;font:12px sans-serif;box-shadow:0 4px 12px rgba(0,0,0,.5);cursor:pointer";
    banner.addEventListener("click", () => banner.remove());
    document.body.appendChild(banner);
  }
  banner.textContent = msg;
  console.error(msg);
}

document.addEventListener("DOMContentLoaded", async () => {
  try {
    Cesium.Ion.defaultAccessToken = undefined;

    // requestRenderMode: só desenha quando algo muda (câmera parada = 0% de GPU).
    // A qualidade em si vem do perfil escolhido em qualidade.js, não daqui.
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
      // Sem isto o Cesium tenta a imagem padrão do Ion e leva 401 a cada carga
      // (token zerado logo acima). Quem monta as camadas é o ambiente.js.
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

    // Configuração do globo sem buracos: cor base oceânica escura e cache de tiles
    viewer.scene.globe.baseColor = Cesium.Color.fromCssColorString('#020917');
    viewer.scene.globe.showWaterEffect = true;
    viewer.scene.globe.tileCacheSize = 250;
    viewer.scene.globe.loadingDescendantLimit = 10;
    viewer.scene.globe.depthTestAgainstTerrain = true;

    // Perfil de imagem + detecção de GPU (avisa quando o navegador está sem aceleração).
    const gpu = qualidadeApp.inicializar(viewer);
    if (gpu.software) {
      console.warn(`ARCZ: navegador sem aceleração de vídeo (${gpu.nome}). Rode ABRIR.cmd para abrir com GPU.`);
    }

    // LOCAL-FIRST: o boot usa elipsoide. DEM só é ligado depois de carregar o
    // projeto e apenas quando o usuário/projeto pediu explicitamente `dem`.
    // Tile ausente gera erro estruturado; nunca vira terreno plano silencioso.

    // requestRenderMode economiza GPU, mas exige pedir quadro a cada mudança de
    // estado — senão mover uma peça não aparece até o usuário mexer na câmera.
    if (viewer.scene.requestRenderMode) {
      estadoApp.inscrever(() => viewer.scene.requestRender());
    }

    ambienteApp.inicializar(viewer);
    cameraApp.inicializar(viewer);
    cenaApp.inicializar(viewer);
    feedbackApp.inicializar(viewer);
    // O gizmo lê o bloqueio de seleção do posicionador: este vem antes.
    posicionadorApp.inicializar(viewer);
    gizmoApp.inicializar(viewer);
    bibliotecaApp.inicializar(viewer);
    entornoApp.inicializar(viewer);
    corteApp.inicializar(viewer);
    recorteApp.inicializar(viewer);
    uiApp.inicializar(viewer);

    // Estado salvo (migra o formato antigo automaticamente).
    await estadoApp.carregarDoServidor();

    // V2 é aditiva. Uma falha de plugin/contrato não impede o globo base de
    // abrir; o erro fica visível em console/diagnóstico para implementação.
    let runtimeV2 = null;
    try { runtimeV2 = initializeV2Runtime({ viewer }); }
    catch (runtimeError) { console.error("ARCZ V2 indisponível; núcleo legado preservado:", runtimeError); }

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

    // Corte salvo no projeto volta ligado (com a tampa) junto com a cena.
    if (st.corte?.ativo) corteApp.aplicar();
    if ((st.recorte?.perimetro || []).length >= 3) recorteApp.desenharEntidades();

    cameraApp.definirCamera(
      st.camera && st.camera.lat
        ? st.camera
        : { lat: pos.lat, lon: pos.lon, alt: 250, pitch: -30 }
    );

    // Fusion V6 is additive and lives outside ui.js. If its shell fails, the
    // legacy globe remains fully operational and the failure stays visible.
    try { await initializeFusionShell({ viewer, estadoApp, runtime: runtimeV2 }); }
    catch (fusionError) { console.error("ARCZ Fusion V10 indisponível; globo legado preservado:", fusionError); }

    console.log(`ARCZ pronto — ${(st.pecas || []).length} peças, ${(st.takes || []).length} takes`);
  } catch (e) {
    exibirErro(`Erro ao iniciar aplicativo: ${e.message}`);
  }
});
