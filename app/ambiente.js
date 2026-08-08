// ARCZ · Ambiente: sol real, clima, sombra, atmosfera, nuvens e fonte de mapa.
//
// Só aplica o que mudou. A versão anterior reagia a qualquer notificação de
// estado (inclusive a da câmera, que escreve a cada quadro) e chamava
// `imageryLayers.removeAll()` + `atualizarNuvens()` 60 vezes por segundo.
//
// A hora agora move a CENA INTEIRA: cor e intensidade da luz direta, o
// espalhamento da atmosfera (céu e ground atmosphere), a névoa, a exposição da
// imagem, o brilho do sol, a lua, as estrelas e a dureza da sombra. Antes só o
// relógio andava — `scene.light` ficava travado em (branco, 2.0) e
// `scene.atmosphere` em zero, então apenas a sombra do modelo girava e a
// impressão era de que o sol afetava o objeto e não o mundo.
import { estadoApp } from "./estado.js";
import { normalizarPosicao, cenaApp } from "./cena.js";
import { qualidadeApp } from "./qualidade.js";
import { entornoApp } from "./entorno.js";
import { NETWORK_MODES } from "./core/network-mode.js";
import {
  solLocal, instanteUtc, horaLocalDe, eventosSolares, solNaFachada,
  fusoDaLongitude, dataDeHoje
} from "./sol.js";
import {
  modeloDoCeu, perfilOrbital, misturarPerfis, criarEstagioPrecipitacao, CONDICOES
} from "./clima.js";

/** Origens de estado que realmente mexem no ambiente. Tudo fora daqui
 *  (gizmo, peças, seleção, câmera, takes) não toca no céu nem no mapa. */
const ORIGENS_DE_AMBIENTE = new Set(["ambiente_ui", "carregamento_inicial", "takes"]);

/** Campos que, quando mudam, exigem recalcular o céu inteiro. */
const CAMPOS_CEU = [
  "data", "hora", "fuso", "fuso_auto", "luz_sol", "condicao", "precipitacao",
  "sol_intensidade", "brilho_ambiente", "neblina", "aerossol",
  "nuvens_cobertura", "fog", "bloom", "estrelas", "lua"
];
const CAMPOS_SOMBRA = ["sombra", "sombra_relevo", "sombra_alcance", "sombra_suave"];
const CAMPOS_NUVEM = ["nuvens", "nuvens_cobertura", "nuvens_altura", "condicao"];

/**
 * Fontes que abrem sockets para terceiros diretamente pelo Cesium. Elas são
 * conectores de visualização temporários, nunca dependências do projeto. Só
 * podem ser escolhidas em `import_assisted`; o padrão é Natural Earth local.
 */
const FONTES_REMOTAS = new Set([
  "satelite", "nuvens_reais", "black_marble", "dia_noite", "mapbox",
  "google3d", "wms_sc", "mapa"
]);
const FONTE_LOCAL_PADRAO = "naturalearth_local";

function modoDeRedeAtual() {
  return estadoApp.obter().network_mode || NETWORK_MODES.OFFLINE_STRICT;
}

function fontePermitida(fonte) {
  return !FONTES_REMOTAS.has(fonte) || modoDeRedeAtual() === NETWORK_MODES.IMPORT_ASSISTED;
}


export class AmbienteManager {
  constructor() {
    this.viewer = null;
    this.cloudCollection = null;
    this.googleTileset = null;
    this.ultimo = {};
    this.luzSol = null;
    this.estagioBrilho = null;
    this.estagioPrecipitacao = null;
    this.tipoPrecipitacao = "nenhuma";
    this.perfil = null;          // último perfil de céu calculado
    this.aoAtualizarSol = null;  // callback da UI (leituras ao vivo)
    this.animando = false;
    this.nuvens = [];
  }

  inicializar(viewer) {
    this.viewer = viewer;

    try { viewer.scene.msaaSamples = 2; } catch (e) { console.debug("MSAA indisponível neste renderer:", e); }
    try { viewer.scene.postProcessStages.fxaa.enabled = true; } catch (e) { console.debug("FXAA indisponível neste renderer:", e); }

    viewer.scene.fog.enabled = true;
    if (viewer.scene.skyAtmosphere) viewer.scene.skyAtmosphere.show = true;
    viewer.scene.globe.showGroundAtmosphere = true;
    viewer.scene.globe.enableLighting = true;
    viewer.scene.globe.dynamicAtmosphereLighting = true;
    viewer.scene.globe.dynamicAtmosphereLightingFromSun = true;
    viewer.scene.globe.showWaterEffect = true;
    viewer.scene.globe.depthTestAgainstTerrain = true;
    viewer.scene.globe.baseColor = Cesium.Color.fromCssColorString('#020917');
    if (viewer.scene.skyBox) viewer.scene.skyBox.show = true;
    if (viewer.scene.sun) viewer.scene.sun.show = true;
    if (viewer.scene.moon) viewer.scene.moon.show = true;

    // A atmosfera do Cesium 1.111+ é global (céu + ground atmosphere na mesma
    // fonte). É por ela que o horário chega ao globo, e não só ao modelo.
    if (viewer.scene.atmosphere && Cesium.DynamicAtmosphereLightingType) {
      viewer.scene.atmosphere.dynamicLighting = Cesium.DynamicAtmosphereLightingType.SUNLIGHT;
    }

    // Uma única luz, mutada a cada quadro: recriar SunLight por quadro alocava
    // objeto à toa durante o time-lapse.
    try {
      this.luzSol = new Cesium.SunLight({ color: Cesium.Color.WHITE, intensity: 2.6 });
      viewer.scene.light = this.luzSol;
    } catch (e) {
      console.warn("SunLight indisponível nesta build do Cesium:", e);
    }

    // Exposição da imagem final (a "abertura" da câmera): vale para tudo.
    try {
      this.estagioBrilho = viewer.scene.postProcessStages.add(
        Cesium.PostProcessStageLibrary.createBrightnessStage()
      );
      this.estagioBrilho.uniforms.brightness = 1.0;
    } catch (e) {
      console.warn("Estágio de brilho indisponível:", e);
    }

    this.cloudCollection = new Cesium.CloudCollection();
    viewer.scene.primitives.add(this.cloudCollection);

    viewer.clock.clockRange = Cesium.ClockRange.UNBOUNDED;
    viewer.clock.shouldAnimate = false;
    viewer.clock.onTick.addEventListener(() => this.aoAndarRelogio());

    // Subir para a órbita muda o que a cena precisa mostrar. Sem reagir à
    // altitude, a Terra vista do espaço continuava usando o céu do sítio.
    //
    // `camera.changed` e `camera.moveEnd` são disparados DE DENTRO de
    // `Scene.render`. Aplicar o perfil ali mexia na coleção de pós-processamento
    // no meio do quadro — o estágio entrava sem shader compilado e o Cesium
    // abortava com "An error occurred while rendering. Rendering has stopped.",
    // matando o viewer inteiro. Por isso o trabalho é adiado para fora do render.
    viewer.camera.percentageChanged = 0.2;
    const talvezReaplicar = () => {
      const k = this.fatorOrbital();
      if (Math.abs(k - (this.orbital ?? 0)) <= 0.02) return;
      if (this.reaplicarAgendado) return;
      this.reaplicarAgendado = true;
      setTimeout(() => {
        this.reaplicarAgendado = false;
        if (this.viewer) this.aplicarPerfil();
      }, 0);
    };
    viewer.camera.changed.addEventListener(talvezReaplicar);
    viewer.camera.moveEnd.addEventListener(talvezReaplicar);

    // Trocar de perfil de qualidade não pode zerar o ambiente: qualidade.js
    // reaplica MSAA/HDR/sombra e devolve o controle da luz e da névoa para cá.
    qualidadeApp.aoTrocarPerfil = () => this.aplicarEstado(estadoApp.obter().ambiente, true);

    // Lista branca em vez de lista negra: arrastar o gizmo notifica o estado a
    // cada quadro e a versão anterior reagia a tudo que não fosse "camera".
    // Qualquer falha assíncrona no meio disso remontava as camadas de imagem
    // em pleno arraste — o globo piscava preto ao mover ou girar.
    estadoApp.inscrever((st, origem) => {
      if (!ORIGENS_DE_AMBIENTE.has(origem)) return;
      this.aplicarEstado(st.ambiente);
    });

    this.aplicarEstado(estadoApp.obter().ambiente, true);
  }

  aplicarEstado(amb, forcar = false) {
    if (!this.viewer || !amb) return;
    const ant = this.ultimo;
    const mudou = campos => forcar || campos.some(c => amb[c] !== ant[c]);

    // Trocar de condição de tempo repõe os controles finos do preset; depois
    // disso cada slider pode ser ajustado sem a condição sobrescrever de volta.
    if (!forcar && amb.condicao !== ant.condicao && ant.condicao !== undefined) {
      const c = CONDICOES[amb.condicao] || CONDICOES.limpo;
      estadoApp.atualizar({
        ambiente: {
          nuvens_cobertura: c.cobertura,
          neblina: c.neblina,
          aerossol: c.aerossol,
          nuvens: c.cobertura > 0
        }
      }, "ambiente_interno");
      amb = estadoApp.obter().ambiente;
    }

    if (mudou(CAMPOS_CEU)) this.aplicarCeu(amb);
    if (mudou(CAMPOS_SOMBRA)) this.aplicarSombras(amb);
    if (mudou(CAMPOS_NUVEM)) this.atualizarNuvens(amb);
    if (forcar || amb.animar_sol !== ant.animar_sol || amb.velocidade_sol !== ant.velocidade_sol) {
      this.definirAnimacao(!!amb.animar_sol, amb.velocidade_sol);
    }
    // A camada de nuvens reais é datada: mudar o dia do projeto tem de refazê-la.
    if (forcar || amb.imagery !== ant.imagery || amb.token_mapbox !== ant.token_mapbox ||
        (amb.imagery === "nuvens_reais" && amb.data !== ant.data)) {
      this.pedirImagery(amb.imagery || FONTE_LOCAL_PADRAO);
    }
    if (forcar || amb.entorno_osm !== ant.entorno_osm || amb.entorno_osm_adensar !== ant.entorno_osm_adensar) {
      entornoApp.alternarEntornoOsm(amb.entorno_osm, amb.entorno_osm_adensar);
    }

    this.ultimo = { ...amb };
    this.viewer.scene.requestRender?.();
  }

  /** Fuso do sítio: automático pela longitude, ou o que o projeto fixou. */
  fusoDe(amb = estadoApp.obter().ambiente) {
    if (amb.fuso_auto === false && amb.fuso !== undefined && amb.fuso !== null) {
      return Number(amb.fuso);
    }
    return fusoDaLongitude(normalizarPosicao(estadoApp.obter().posicao).lon);
  }

  instanteAtual(amb = estadoApp.obter().ambiente) {
    return instanteUtc(amb.data || dataDeHoje(), amb.hora ?? 15, this.fusoDe(amb));
  }

  /** Sol real (elevação/azimute) no sítio do projeto, no instante do relógio. */
  solAgora() {
    const p = normalizarPosicao(estadoApp.obter().posicao);
    const tempo = this.viewer ? this.viewer.clock.currentTime : this.instanteAtual();
    return solLocal(tempo, p.lon, p.lat, p.alt || 0);
  }

  // -------------------------------------------------------------------- céu
  /** Põe o relógio no instante certo e propaga o clima para a cena inteira. */
  aplicarCeu(amb = estadoApp.obter().ambiente) {
    if (!this.viewer) return null;
    this.viewer.clock.currentTime = this.instanteAtual(amb);
    return this.aplicarPerfil(amb);
  }

  /**
   * Quanto a cena é "de órbita" e não "de sítio": 0 no chão, 1 no espaço.
   * A transição vai de 25 km (ainda dá para ver o terreno) a 400 km (órbita
   * baixa), que é onde o tempo local deixa de fazer qualquer sentido.
   */
  fatorOrbital() {
    if (!this.viewer) return 0;
    const alt = this.viewer.camera.positionCartographic?.height ?? 0;
    const t = (alt - 25000) / (400000 - 25000);
    const k = Math.min(1, Math.max(0, t));
    return k * k * (3 - 2 * k);
  }

  /** Recalcula e aplica o perfil de céu a partir do relógio atual. */
  aplicarPerfil(amb = estadoApp.obter().ambiente) {
    if (!this.viewer) return null;
    const cena = this.viewer.scene;
    const sol = this.solAgora();

    // O modelo de clima descreve o céu SOBRE O SÍTIO. Da órbita quem manda é o
    // Sol de verdade: sem esta mistura, sítio à noite deixava o planeta INTEIRO
    // escuro e sem limbo azul, inclusive o hemisfério em pleno dia.
    const k = this.fatorOrbital();
    this.orbital = k;
    const perfil = k > 0.001
      ? misturarPerfis(modeloDoCeu(sol.elevacao, amb), perfilOrbital(), k)
      : modeloDoCeu(sol.elevacao, amb);
    perfil.azimute = sol.azimute;
    this.perfil = perfil;

    const iluminar = amb.luz_sol !== false;

    // 1. Luz direta — chega ao globo, ao terreno, ao OSM e aos modelos.
    if (this.luzSol) {
      const c = perfil.luz.cor;
      this.luzSol.color = new Cesium.Color(c.r, c.g, c.b, 1.0);
      this.luzSol.intensity = iluminar ? perfil.luz.intensidade : 2.0;
    }
    cena.globe.enableLighting = iluminar;
    cena.globe.dynamicAtmosphereLighting = iluminar;
    cena.globe.dynamicAtmosphereLightingFromSun = iluminar;

    // 2. Atmosfera global (céu + ground atmosphere).
    const atm = cena.atmosphere;
    const a = perfil.atmosfera;
    if (atm) {
      atm.hueShift = a.hueShift;
      atm.saturationShift = a.saturationShift;
      atm.brightnessShift = a.brightnessShift;
      atm.lightIntensity = a.lightIntensity;
      if (atm.rayleighCoefficient) {
        atm.rayleighCoefficient.x = a.rayleighCoefficient.x;
        atm.rayleighCoefficient.y = a.rayleighCoefficient.y;
        atm.rayleighCoefficient.z = a.rayleighCoefficient.z;
      }
      if (atm.mieCoefficient) {
        atm.mieCoefficient.x = a.mieCoefficient.x;
        atm.mieCoefficient.y = a.mieCoefficient.y;
        atm.mieCoefficient.z = a.mieCoefficient.z;
      }
      atm.mieAnisotropy = a.mieAnisotropy;
      atm.rayleighScaleHeight = a.rayleighScaleHeight;
      atm.mieScaleHeight = a.mieScaleHeight;
    }
    if (cena.skyAtmosphere) {
      cena.skyAtmosphere.show = true;
      cena.skyAtmosphere.hueShift = a.hueShift;
      cena.skyAtmosphere.saturationShift = a.saturationShift;
      cena.skyAtmosphere.brightnessShift = a.brightnessShift;
      cena.skyAtmosphere.perFragmentAtmosphere = true;
    }

    // 3. Névoa: densidade pela umidade/aerossol, brilho pela hora.
    //    Da órbita a névoa é desligada — ela é fenômeno de superfície e só
    //    lavava o planeta de branco quando a câmera subia.
    cena.fog.enabled = amb.fog !== false && k < 0.6;
    cena.fog.density = perfil.nevoa.density;
    cena.fog.minimumBrightness = perfil.nevoa.minimumBrightness;

    // 4. Corpos celestes. No espaço o Sol, a Lua e as estrelas aparecem
    //    sempre: é o que dá escala e faz a Terra parecer um planeta.
    if (cena.sun) {
      cena.sun.show = perfil.ceu.mostrarSol;
      cena.sun.glowFactor = perfil.ceu.glow;
    }
    if (cena.moon) cena.moon.show = amb.lua !== false && perfil.ceu.mostrarLua;
    if (cena.skyBox) cena.skyBox.show = amb.estrelas !== false;

    // Nuvens locais somem ao subir: 10 km de puffs viram sujeira sub-pixel.
    if (this.cloudCollection) this.cloudCollection.show = k < 0.25 && amb.nuvens !== false;

    // 5. Exposição e bloom da imagem inteira.
    if (this.estagioBrilho) this.estagioBrilho.uniforms.brightness = perfil.exposicao;
    const bloom = cena.postProcessStages?.bloom;
    if (bloom) {
      bloom.enabled = !!amb.bloom;
      if (bloom.enabled) {
        bloom.uniforms.contrast = perfil.bloom.contrast;
        bloom.uniforms.brightness = perfil.bloom.brightness;
        bloom.uniforms.delta = perfil.bloom.delta;
        bloom.uniforms.sigma = perfil.bloom.sigma;
      }
    }

    // 6. Luz do céu nos modelos: sem isso eles ficam pretos quando o sol se põe.
    cenaApp.aplicarIluminacaoModelos?.(perfil.ibl);

    // 7. Sombra acompanha o clima (macia e fraca sob nuvem).
    this.aplicarSombras(amb, perfil);

    // 8. Precipitação em tela cheia.
    this.aplicarPrecipitacao(perfil);

    cena.requestRender?.();
    this.aoAtualizarSol?.(this.leitura());
    return perfil;
  }

  aplicarSombras(amb = estadoApp.obter().ambiente, perfil = this.perfil) {
    const v = this.viewer;
    if (!v) return;
    const ligada = amb.sombra !== false;
    v.shadows = ligada;

    if (v.shadowMap) {
      v.shadowMap.enabled = ligada;
      v.shadowMap.darkness = perfil ? perfil.sombra.darkness : 0.3;
      v.shadowMap.softShadows = amb.sombra_suave !== false && (perfil ? perfil.sombra.suave : true);
      v.shadowMap.maximumDistance = Math.max(200, Number(amb.sombra_alcance) || 4000);
      v.shadowMap.normalOffset = true;
    }

    // O ponto do "a cena toda": com RECEIVE_ONLY o relevo nunca projetava
    // sombra — morro nenhum escurecia o terreno nem o prédio ao entardecer.
    v.scene.globe.shadows = !ligada
      ? Cesium.ShadowMode.DISABLED
      : (amb.sombra_relevo !== false ? Cesium.ShadowMode.ENABLED : Cesium.ShadowMode.RECEIVE_ONLY);

    entornoApp.aplicarSombra?.(ligada);
    v.scene.requestRender?.();
  }

  aplicarPrecipitacao(perfil) {
    const cena = this.viewer?.scene;
    if (!cena) return;
    const tipo = perfil?.precipitacao?.tipo || "nenhuma";

    // Trocar estágio de pós-processamento é mutação de coleção: nunca dentro do
    // quadro. Adiar um quadro é invisível e não derruba o render.
    if (tipo !== this.tipoPrecipitacao && !this.trocaPrecipAgendada) {
      this.trocaPrecipAgendada = true;
      setTimeout(() => {
        this.trocaPrecipAgendada = false;
        if (!this.viewer) return;
        const alvo = this.perfil?.precipitacao?.tipo || "nenhuma";
        if (alvo === this.tipoPrecipitacao) return;

        if (this.estagioPrecipitacao) {
          cena.postProcessStages.remove(this.estagioPrecipitacao);
          this.estagioPrecipitacao = null;
        }
        if (alvo === "chuva" || alvo === "neve") {
          try {
            this.estagioPrecipitacao = cena.postProcessStages.add(criarEstagioPrecipitacao(alvo));
          } catch (e) {
            console.warn("Estágio de precipitação indisponível:", e);
          }
        }
        this.tipoPrecipitacao = this.estagioPrecipitacao ? alvo : "nenhuma";
        this.ajustarModoDeRender();
        cena.requestRender?.();
      }, 0);
    }
    if (this.estagioPrecipitacao) {
      const u = this.estagioPrecipitacao.uniforms;
      u.intensidade = perfil.precipitacao.intensidade;
      if ("inclinacao" in u) {
        const vento = Number(estadoApp.obter().ambiente.vento_kmh) || 0;
        u.inclinacao = Math.max(-0.9, Math.min(0.9, vento / 60));
      }
    }
  }

  /** Chuva e time-lapse precisam de quadro contínuo; o resto economiza GPU. */
  ajustarModoDeRender() {
    if (!this.viewer) return;
    const continuo = this.animando || this.tipoPrecipitacao !== "nenhuma";
    this.viewer.scene.requestRenderMode = !continuo;
    this.viewer.scene.requestRender?.();
  }

  // ------------------------------------------------------------- time-lapse
  definirAnimacao(ligar, velocidade) {
    if (!this.viewer) return;
    const relogio = this.viewer.clock;
    relogio.multiplier = Math.max(1, Number(velocidade) || 300);
    if (ligar && !this.animando) relogio.currentTime = this.instanteAtual();
    relogio.shouldAnimate = !!ligar;
    const parou = this.animando && !ligar;
    this.animando = !!ligar;
    // Semeado, e não zerado: o PRIMEIRO tique depois de ligar é justamente o
    // que carrega todo o tempo real acumulado com o relógio parado. Sem a
    // referência aqui, esse quadro escapava do limite e o sol saltava horas.
    this.instanteAnterior = ligar ? Cesium.JulianDate.clone(relogio.currentTime) : null;
    this.ajustarModoDeRender();
    if (parou) this.comprometerHoraDoRelogio();
  }

  /**
   * O Cesium avança o relógio pelo TEMPO REAL decorrido desde o quadro anterior
   * multiplicado pela velocidade. Enquanto a janela está em segundo plano o
   * navegador congela o `requestAnimationFrame`: ao voltar, um único quadro
   * carrega minutos de tempo real e o sol salta horas — às vezes dias — de uma
   * vez. Aqui o passo é limitado ao equivalente a 250 ms de tempo real, que já
   * é um quadro péssimo. O time-lapse continua contínuo em vez de teleportar.
   */
  limitarSaltoDoRelogio() {
    const relogio = this.viewer.clock;
    const anterior = this.instanteAnterior;
    this.instanteAnterior = Cesium.JulianDate.clone(relogio.currentTime);
    if (!anterior) return;

    const passou = Cesium.JulianDate.secondsDifference(relogio.currentTime, anterior);
    const limite = Math.abs(relogio.multiplier) * 0.25;
    if (Math.abs(passou) > limite) {
      relogio.currentTime = Cesium.JulianDate.addSeconds(
        anterior, Math.sign(passou) * limite, new Cesium.JulianDate()
      );
      this.instanteAnterior = Cesium.JulianDate.clone(relogio.currentTime);
    }
  }

  aoAndarRelogio() {
    if (!this.animando) return;
    this.limitarSaltoDoRelogio();      // a cada quadro, sem estrangulamento

    const agora = performance.now();
    // Recalcular o céu 12×/s já é mais fino do que o olho percebe num time-lapse.
    if (this.ultimoTick && agora - this.ultimoTick < 80) return;
    this.ultimoTick = agora;
    this.aplicarPerfil(estadoApp.obter().ambiente);
    this.moverNuvens();
  }

  /**
   * Grava no estado a hora a que o time-lapse chegou.
   * Durante a animação nada é escrito no estado de propósito: a persistência
   * tem debounce de 600 ms e gravaria o projeto no disco duas vezes por segundo.
   */
  comprometerHoraDoRelogio() {
    if (!this.viewer) return;
    const amb = estadoApp.obter().ambiente;
    const base = amb.data || dataDeHoje();
    const hora = horaLocalDe(this.viewer.clock.currentTime, base, this.fusoDe(amb));
    const dias = Math.floor(hora / 24);
    let data = base;
    if (dias !== 0) {
      const d = new Date(`${base}T12:00:00Z`);
      d.setUTCDate(d.getUTCDate() + dias);
      data = d.toISOString().slice(0, 10);
    }
    estadoApp.atualizar({ ambiente: { hora: ((hora % 24) + 24) % 24, data } }, "ambiente_ui");
  }

  // --------------------------------------------------------------- leituras
  /** Tudo que a UI mostra sobre o sol, da mesma fonte usada na renderização. */
  leitura() {
    const amb = estadoApp.obter().ambiente;
    const p = normalizarPosicao(estadoApp.obter().posicao);
    const fuso = this.fusoDe(amb);
    const sol = this.solAgora();
    const hora = this.viewer
      ? horaLocalDe(this.viewer.clock.currentTime, amb.data || dataDeHoje(), fuso)
      : (amb.hora ?? 15);
    return {
      hora: ((hora % 24) + 24) % 24,
      data: amb.data || dataDeHoje(),
      fuso,
      elevacao: sol.elevacao,
      azimute: sol.azimute,
      fase: this.perfil?.fase || "—",
      eventos: this.eventos(),
      lat: p.lat,
      lon: p.lon
    };
  }

  eventos() {
    const amb = estadoApp.obter().ambiente;
    const p = normalizarPosicao(estadoApp.obter().posicao);
    const data = amb.data || dataDeHoje();
    const fuso = this.fusoDe(amb);
    const chave = `${data}|${fuso}|${p.lat.toFixed(4)}|${p.lon.toFixed(4)}`;
    if (this.cacheEventos?.chave === chave) return this.cacheEventos.valor;
    const valor = eventosSolares(data, fuso, p.lon, p.lat, p.alt || 0);
    this.cacheEventos = { chave, valor };
    return valor;
  }

  /** Sincroniza data e hora com o relógio real, já no fuso do sítio. */
  sincronizarHoraReal() {
    const amb = estadoApp.obter().ambiente;
    const fuso = this.fusoDe(amb);
    const agoraLocal = new Date(Date.now() + fuso * 3600000);
    const z = n => String(n).padStart(2, "0");
    const data = `${agoraLocal.getUTCFullYear()}-${z(agoraLocal.getUTCMonth() + 1)}-${z(agoraLocal.getUTCDate())}`;
    const hora = agoraLocal.getUTCHours() + agoraLocal.getUTCMinutes() / 60;
    estadoApp.atualizar({ ambiente: { data, hora } }, "ambiente_ui");
    return { data, hora };
  }

  voarParaEspaco() {
    if (!this.viewer) return;
    const pos = normalizarPosicao(estadoApp.obter().posicao);
    this.viewer.camera.flyTo({
      destination: Cesium.Cartesian3.fromDegrees(pos.lon, pos.lat, 18000000),
      orientation: { heading: 0, pitch: Cesium.Math.toRadians(-89), roll: 0 },
      duration: 2.0
    });
  }

  /**
   * Hora em que o sol bate mais de frente na fachada de um dado rumo, na data
   * escolhida. Devolve também a incidência e a elevação, para a UI poder dizer
   * quando a fachada simplesmente não pega sol naquele dia.
   */
  calcularSolNaFachada(rumoFachadaGraus) {
    const amb = estadoApp.obter().ambiente;
    const p = normalizarPosicao(estadoApp.obter().posicao);
    return solNaFachada(
      amb.data || dataDeHoje(), this.fusoDe(amb), p.lon, p.lat, rumoFachadaGraus, p.alt || 0
    );
  }

  // ----------------------------------------------------------------- nuvens
  /** Campo de nuvens ancorado no sítio (não na câmera) e proporcional à cobertura. */
  atualizarNuvens(amb = estadoApp.obter().ambiente) {
    if (!this.cloudCollection || !this.viewer) return;
    this.cloudCollection.removeAll();
    this.nuvens = [];
    if (amb.nuvens === false) return;

    // Sempre o modelo LOCAL, nunca `this.perfil`: em órbita o perfil misturado
    // zera a quantidade de nuvens, e o campo ficaria vazio ao descer de volta.
    const perfil = modeloDoCeu(this.solAgora().elevacao, amb);
    const total = Math.min(220, perfil.nuvens.quantidade);
    if (total <= 0) return;

    const p = normalizarPosicao(estadoApp.obter().posicao);
    const alcance = 0.09;  // ~10 km de lado, centrado no projeto

    for (let i = 0; i < total; i++) {
      const lat = p.lat + (Math.random() - 0.5) * alcance;
      const lon = p.lon + (Math.random() - 0.5) * alcance;
      const alt = perfil.nuvens.altura * (0.85 + Math.random() * 0.4);
      const largura = perfil.nuvens.escala * (0.6 + Math.random() * 0.9);
      const nuvem = this.cloudCollection.add({
        position: Cesium.Cartesian3.fromDegrees(lon, lat, alt),
        scale: new Cesium.Cartesian2(largura, largura * (0.32 + Math.random() * 0.18)),
        maximumSize: new Cesium.Cartesian3(largura * 0.5, largura * 0.2, largura * 0.35),
        slice: 0.3 + Math.random() * 0.4,
        brightness: perfil.nuvens.brilho
      });
      this.nuvens.push({ nuvem, lat, lon, alt });
    }
  }

  /** Deriva com o vento — só roda durante o time-lapse, para não acordar a GPU. */
  moverNuvens() {
    if (!this.nuvens?.length) return;
    const amb = estadoApp.obter().ambiente;
    const vento = Number(amb.vento_kmh) || 0;
    if (vento <= 0) return;
    const rad = Cesium.Math.toRadians(Number(amb.vento_rumo) || 90);
    // km/h → graus por tique de 80 ms (1 grau ≈ 111 km).
    const passo = (vento / 3600) * 0.08 / 111;

    for (const n of this.nuvens) {
      n.lat += Math.cos(rad) * passo;
      n.lon += Math.sin(rad) * passo;
      n.nuvem.position = Cesium.Cartesian3.fromDegrees(n.lon, n.lat, n.alt);
    }
  }

  /** Serializa as trocas de mapa: um pedido em voo, sempre o último ganha. */
  async pedirImagery(fonte, tokenMapbox = "") {
    this.imageryPedida = { fonte, tokenMapbox };
    if (this.imageryEmVoo) return;
    this.imageryEmVoo = true;
    try {
      while (this.imageryPedida) {
        const pedido = this.imageryPedida;
        this.imageryPedida = null;
        await this.atualizarImagery(pedido.fonte, pedido.tokenMapbox);
      }
    } finally {
      this.imageryEmVoo = false;
    }
  }

  async atualizarImagery(fonte, tokenMapbox = "") {
    if (!this.viewer) return;
    const layers = this.viewer.imageryLayers;

    if (!fontePermitida(fonte)) {
      const mode = modoDeRedeAtual();
      console.warn(`ARCZ: fonte remota '${fonte}' bloqueada em ${mode}. Ative import_assisted explicitamente para usar o conector.`);
      this.trocarCamadasDeCima(layers);
      if (estadoApp.obter().ambiente?.imagery !== FONTE_LOCAL_PADRAO) {
        queueMicrotask(() => estadoApp.atualizar(
          { ambiente: { imagery: FONTE_LOCAL_PADRAO } }, "ambiente_ui"
        ));
      }
      return;
    }

    if (this.googleTileset && fonte !== "google3d") {
      this.viewer.scene.primitives.remove(this.googleTileset);
      this.googleTileset = null;
    }

    if (fonte === "google3d") {
      this.trocarCamadasDeCima(layers);
      if (!this.googleTileset) {
        try {
          this.googleTileset = await Cesium.createGooglePhotorealistic3DTileset();
          this.viewer.scene.primitives.add(this.googleTileset);
        } catch (e) {
          console.warn("Google 3D Tiles indisponível; voltando à base local:", e);
          this.googleTileset = null;
          // Grava a queda no estado: o seletor passa a mostrar a verdade e o
          // próprio ciclo de estado monta o Esri. Mexer no `ultimo` daqui era o
          // que fazia o mapa ser remontado a cada arraste do gizmo.
          estadoApp.atualizar({ ambiente: { imagery: FONTE_LOCAL_PADRAO } }, "ambiente_ui");
        }
      }
      return;
    }

    this.trocarCamadasDeCima(layers);

    if (fonte === "black_marble") {
      this.adicionarBlackMarble(layers);
      return;
    }

    if (fonte === "dia_noite") {
      this.adicionarEsri(layers);
      this.adicionarLuzesNoturnasCamada(layers);
      return;
    }

    // Nuvens de verdade. O Esri é um mosaico de céu limpo — por construção
    // nunca tem nuvem, e é por isso que a Terra vista do espaço parecia uma
    // bola de plástico. O MODIS/VIIRS do dia traz o tempo real daquela data.
    if (fonte === "nuvens_reais") {
      this.adicionarEsri(layers);          // base nítida no zoom de projeto
      this.adicionarNuvensReais(layers);   // por cima, só até o nível 9
      return;
    }

    if (fonte === "mapbox") {
      // Segredo efêmero: nunca é salvo em projeto.json.
      const token = (tokenMapbox || "").trim() || sessionStorage.getItem("arcz.connector.mapbox.token") || "";
      if (token) {
        layers.addImageryProvider(
          new Cesium.MapboxStyleImageryProvider({ styleId: "satellite-v9", accessToken: token })
        );
        return;
      }
      console.warn("Mapbox sem token de sessão; voltando à base local.");
      return;
    }

    if (fonte === "mapa") {
      layers.addImageryProvider(
        new Cesium.UrlTemplateImageryProvider({
          url: "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
          maximumLevel: 19,
          credit: "© OpenStreetMap"
        })
      );
      return;
    }

    if (fonte === "wms_sc") {
      this.adicionarWmsSc(layers);
      return;
    }

    // `naturalearth_local` e `nenhuma` mantêm somente a base local. Fontes
    // remotas reconhecidas retornam nos ramos acima; valor desconhecido não
    // ganha fallback de rede.
    if (fonte !== "nenhuma" && fonte !== FONTE_LOCAL_PADRAO) {
      console.warn(`ARCZ: fonte de imagery desconhecida '${fonte}'; usando base local.`);
    }
  }

  adicionarEsri(layers) {
    try {
      const provider = new Cesium.UrlTemplateImageryProvider({
        url: "https://services.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
        // z18 é o teto real da cobertura no litoral de SC: em z19 o Esri devolve
        // o tile branco "Map data not yet available" em cima do terreno.
        maximumLevel: 18,
        credit: "Esri World Imagery"
      });
      this.avisarUmaVez(provider, "Esri World Imagery falhou (rede?); ficou a base offline Natural Earth II.");
      layers.addImageryProvider(provider);
    } catch (e) {
      console.warn("Esri indisponível:", e);
    }
  }

  adicionarWmsSc(layers) {
    try {
      const provider = new Cesium.WebMapServiceImageryProvider({
        url: "https://sigsc.sc.gov.br/sigserver/SIGSC/wms",
        layers: "OrtoRGB-Landsat-2012",
        parameters: { format: "image/png", transparent: false, version: "1.1.1" },
        // Fora de Santa Catarina o GeoServer devolve tile vazio: recortar evita
        // pedir milhares de tiles inúteis quando a câmera está longe.
        rectangle: Cesium.Rectangle.fromDegrees(-54.0992, -29.4785, -48.1126, -25.8239),
        maximumLevel: 18,
        credit: "SIGSC · Governo de Santa Catarina (OrtoRGB 2012)"
      });
      // Nunca deixar o globo em branco se o GeoServer estadual cair.
      this.avisarUmaVez(provider, "SIGSC indisponível; voltando ao Esri World Imagery.", () => {
        if (!this.esriDeSocorro) {
          this.esriDeSocorro = true;
          this.adicionarEsri(layers);
        }
      });
      this.esriDeSocorro = false;
      layers.addImageryProvider(provider);
    } catch (e) {
      console.warn("SIGSC indisponível; usando Esri:", e);
      this.adicionarEsri(layers);
    }
  }

  /**
   * Base permanente offline: pirâmide Natural Earth II que já vem no pacote do
   * Cesium (vendor/.../Assets/Textures/NaturalEarthII). Antes daqui o código
   * apontava para "natural_earth_1024.jpg", arquivo que não existe na
   * distribuição — a camada "à prova de branco" era, na prática, um 404.
   */
  garantirBaseOffline(layers) {
    if (this.camadaBase && layers.contains(this.camadaBase)) return this.camadaBase;
    try {
      this.camadaBase = Cesium.ImageryLayer.fromProviderAsync(
        Cesium.TileMapServiceImageryProvider.fromUrl(
          "/vendor/cesium/Cesium/Assets/Textures/NaturalEarthII",
          { credit: "Natural Earth II (offline)" }
        )
      );
      layers.add(this.camadaBase, 0);
    } catch (e) {
      console.warn("Base offline Natural Earth II indisponível:", e);
      this.camadaBase = null;
    }
    return this.camadaBase;
  }

  /** Troca de mapa sem piscar: a base offline fica, só o que está acima sai. */
  trocarCamadasDeCima(layers) {
    this.garantirBaseOffline(layers);
    for (let i = layers.length - 1; i >= 0; i--) {
      const camada = layers.get(i);
      if (camada !== this.camadaBase) layers.remove(camada, true);
    }
    if (this.camadaBase) layers.lowerToBottom(this.camadaBase);
  }

  adicionarBlackMarble(layers) {
    try {
      const provider = this.criarProvedorBlackMarble();
      this.avisarUmaVez(provider, "NASA Black Marble indisponível; ficou a base offline Natural Earth II.");
      layers.addImageryProvider(provider);
    } catch (e) {
      console.warn("NASA Black Marble indisponível:", e);
    }
  }

  /** Luzes noturnas por cima do satélite: só aparecem no lado escuro do globo. */
  adicionarLuzesNoturnasCamada(layers) {
    try {
      const provider = this.criarProvedorBlackMarble();
      this.avisarUmaVez(provider, "Luzes noturnas (NASA GIBS) indisponíveis; fica só o dia.");
      const nightLayer = layers.addImageryProvider(provider);
      nightLayer.dayAlpha = 0.0;
      nightLayer.nightAlpha = 1.0;
    } catch (e) {
      console.warn("Luzes noturnas indisponíveis:", e);
    }
  }

  /**
   * Cobertura de nuvens real do dia (NASA GIBS · MODIS Terra, cor verdadeira).
   *
   * O produto só existe até o nível 9 (~150 m/px) e é publicado com algumas
   * horas de atraso, então a data é recuada até o último dia com passagem
   * garantida. Acima do nível 9 a camada some sozinha e fica o Esri nítido:
   * de longe você vê o tempo do planeta, de perto vê o lote.
   */
  adicionarNuvensReais(layers) {
    try {
      const amb = estadoApp.obter().ambiente;
      const escolhida = new Date(`${amb.data || dataDeHoje()}T12:00:00Z`);
      const limite = new Date(Date.now() - 36 * 3600 * 1000);   // atraso de publicação
      const dia = (escolhida > limite ? limite : escolhida).toISOString().slice(0, 10);

      const provider = new Cesium.UrlTemplateImageryProvider({
        url: "https://gibs.earthdata.nasa.gov/wmts/epsg3857/best/MODIS_Terra_CorrectedReflectance_TrueColor" +
             `/default/${dia}/GoogleMapsCompatible_Level9/{z}/{y}/{x}.jpg`,
        maximumLevel: 9,
        credit: `NASA GIBS · MODIS Terra (${dia})`
      });
      this.avisarUmaVez(provider, "NASA GIBS indisponível; ficou só o mosaico Esri (sem nuvens).");
      const camada = layers.addImageryProvider(provider);
      camada.alpha = 0.92;
      this.camadaNuvensReais = camada;
    } catch (e) {
      console.warn("Nuvens reais (NASA GIBS) indisponíveis:", e);
    }
  }

  criarProvedorBlackMarble() {
    return new Cesium.UrlTemplateImageryProvider({
      url: "https://gibs.earthdata.nasa.gov/wmts/epsg3857/best/VIIRS_Black_Marble/default/2016-01-01/GoogleMapsCompatible_Level8/{z}/{y}/{x}.png",
      maximumLevel: 8,
      credit: "NASA GIBS / Earth at Night (Black Marble)"
    });
  }

  /** Um aviso por provedor, não um por tile que falha. */
  avisarUmaVez(provider, mensagem, aoFalhar = null) {
    let avisado = false;
    provider.errorEvent.addEventListener(() => {
      if (avisado) return;
      avisado = true;
      console.warn("ARCZ mapa:", mensagem);
      if (aoFalhar) aoFalhar();
    });
  }
}

export const ambienteApp = new AmbienteManager();
