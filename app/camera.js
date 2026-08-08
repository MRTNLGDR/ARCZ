// ARCZ · Câmera em tempo real, lente e takes.
//
// A leitura roda a cada quadro, mas só escreve no estado quando o valor muda
// além de um limiar — senão o app inteiro (ambiente, UI, autosave) reagia 60
// vezes por segundo com a câmera parada.
import { estadoApp } from "./estado.js";

/** Distância focal equivalente em 35 mm para um FOV horizontal em graus. */
export function fovParaMm(fovGraus) {
  const rad = (fovGraus * Math.PI) / 180;
  return Math.round(36.0 / (2.0 * Math.tan(rad / 2.0)));
}

/** FOV em graus para uma distância focal equivalente em 35 mm. */
export function mmParaFov(mm) {
  return (2.0 * Math.atan(36.0 / (2.0 * mm)) * 180) / Math.PI;
}

export function mudouCamera(anterior, novo) {
  if (!anterior) return true;
  const limites = {
    lat: 1e-6, lon: 1e-6, alt: 0.05,
    heading: 0.05, pitch: 0.05, roll: 0.05,
    fov: 0.05, distanciaAlvo: 0.5
  };
  return Object.keys(limites).some(
    k => Math.abs((novo[k] ?? 0) - (anterior[k] ?? 0)) > limites[k]
  );
}

export class CameraManager {
  constructor() {
    this.viewer = null;
    this.desativarPostRender = false;
    this.cbPostRender = null;
    this.ultimoPayload = null;
  }

  inicializar(viewer) {
    this.viewer = viewer;
    this.cbPostRender = () => {
      if (this.desativarPostRender || !this.viewer) return;
      this.atualizarMetricasDoViewer();
    };
    viewer.scene.postRender.addEventListener(this.cbPostRender);
  }

  atualizarMetricasDoViewer() {
    const cam = this.viewer.camera;
    const cart = cam.positionCartographic;
    const fovGraus = Cesium.Math.toDegrees(cam.frustum.fov ?? 1.047);

    const agora = performance.now();
    if (!this.ultimaPickTime || (agora - this.ultimaPickTime) > 150) {
      this.ultimaPickTime = agora;
      const raio = cam.getPickRay(
        new Cesium.Cartesian2(this.viewer.canvas.clientWidth / 2, this.viewer.canvas.clientHeight / 2)
      );
      if (raio) {
        const posChao = this.viewer.scene.globe.pick(raio, this.viewer.scene);
        if (posChao) this.ultimaDistanciaAlvo = Math.round(Cesium.Cartesian3.distance(cam.position, posChao));
      }
    }
    const distanciaAlvo = this.ultimaDistanciaAlvo || 0;

    const payload = {
      lat: Number(Cesium.Math.toDegrees(cart.latitude).toFixed(6)),
      lon: Number(Cesium.Math.toDegrees(cart.longitude).toFixed(6)),
      alt: Number(cart.height.toFixed(2)),
      heading: Number(Cesium.Math.toDegrees(cam.heading).toFixed(1)),
      pitch: Number(Cesium.Math.toDegrees(cam.pitch).toFixed(1)),
      roll: Number(Cesium.Math.toDegrees(cam.roll).toFixed(1)),
      fov: Number(fovGraus.toFixed(1)),
      fovMm: fovParaMm(fovGraus),
      distanciaAlvo
    };

    if (!mudouCamera(this.ultimoPayload, payload)) return;
    this.ultimoPayload = payload;
    estadoApp.atualizar({ camera: payload }, "camera");
  }

  definirCamera(camParams) {
    if (!this.viewer) return;
    this.desativarPostRender = true;

    const lat = camParams.lat ?? -27.1545;
    const lon = camParams.lon ?? -48.5022;
    const alt = camParams.alt ?? 150;

    this.viewer.camera.setView({
      destination: Cesium.Cartesian3.fromDegrees(lon, lat, alt),
      orientation: {
        heading: Cesium.Math.toRadians(camParams.heading ?? 0),
        pitch: Cesium.Math.toRadians(camParams.pitch ?? -30),
        roll: Cesium.Math.toRadians(camParams.roll ?? 0)
      }
    });

    if (camParams.fov && this.viewer.camera.frustum.fov !== undefined) {
      this.viewer.camera.frustum.fov = Cesium.Math.toRadians(camParams.fov);
    }

    setTimeout(() => { this.desativarPostRender = false; }, 100);
  }

  /** Aponta a câmera para um ponto, mantendo distância e ângulo atuais. */
  olharPara(lon, lat, alt = 0, distancia = 200) {
    if (!this.viewer) return;
    const centro = Cesium.Cartesian3.fromDegrees(lon, lat, alt);
    this.viewer.camera.flyToBoundingSphere(new Cesium.BoundingSphere(centro, distancia), {
      duration: 1.0
    });
  }

  definirLenteMm(mm) {
    if (!this.viewer) return;
    const fov = mmParaFov(mm);
    this.viewer.camera.frustum.fov = Cesium.Math.toRadians(fov);
    estadoApp.atualizar({ camera: { fov: Number(fov.toFixed(1)), fovMm: mm } }, "camera_lente");
  }

  gravarTake(nome, planoIndex = null) {
    const cam = estadoApp.obter().camera;
    const takes = [...estadoApp.obter().takes];
    const novoTake = {
      id: "take_" + Date.now(),
      nome: nome || `Take ${takes.length + 1}`,
      planoIndex,
      lat: cam.lat, lon: cam.lon, alt: cam.alt,
      heading: cam.heading, pitch: cam.pitch, roll: cam.roll,
      fov: cam.fov,
      criado_em: new Date().toISOString()
    };
    takes.push(novoTake);
    estadoApp.atualizar({ takes }, "takes");
    return novoTake;
  }

  irParaTake(id) {
    const take = estadoApp.obter().takes.find(t => t.id === id);
    if (!take) return false;
    this.definirCamera(take);
    return true;
  }

  excluirTake(id) {
    estadoApp.atualizar({ takes: estadoApp.obter().takes.filter(t => t.id !== id) }, "takes");
  }

  renomearTake(id, novoNome) {
    const takes = estadoApp.obter().takes.map(t => (t.id === id ? { ...t, nome: novoNome } : t));
    estadoApp.atualizar({ takes }, "takes");
  }

  duplicarTake(id) {
    const take = estadoApp.obter().takes.find(t => t.id === id);
    if (!take) return;
    const takes = [...estadoApp.obter().takes, {
      ...take,
      id: "take_" + Date.now(),
      nome: `${take.nome} (copia)`,
      criado_em: new Date().toISOString()
    }];
    estadoApp.atualizar({ takes }, "takes");
  }

  /** Salva o quadro atual como PNG no servidor (pasta takes/). */
  async fotografar(nome) {
    if (!this.viewer) return null;
    this.viewer.render();
    const png = this.viewer.canvas.toDataURL("image/png");
    try {
      const res = await fetch("/api/foto", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ png, nome, pasta: "takes" })
      });
      const data = await res.json();
      return data.ok ? data : null;
    } catch (e) {
      console.error("Falha ao salvar foto:", e);
      return null;
    }
  }
}

export const cameraApp = new CameraManager();
