const DEFAULT_PRESENTATION = Object.freeze({
  schema_version: 1,
  enabled: true,
  duration_ms: 6500,
  start_altitude_m: 24000000,
  orbit_altitude_m: 1500000,
  end_altitude_m: 1500000,
  atmosphere: true,
  clouds: true,
  stars: true,
  sun: true,
  moon: true,
  fog: true,
  fog_density: 0.00018,
  hue_shift: 0,
  saturation_shift: -0.05,
  brightness_shift: -0.03,
  orbit_heading_delta_deg: 14,
  skip_on_reduced_motion: true,
  cloud_count: 28,
  cloud_radius_m: 85000,
  cloud_altitude_m: 5200,
  cloud_brightness: 0.92,
  cancel_on_interaction: true,
  show_progress: true,
  persistent_procedural_clouds: true,
});

export const EARTH_INTRO_STATES = Object.freeze({
  IDLE: "IDLE",
  PREPARING: "PREPARING",
  SPACE: "SPACE",
  ORBIT: "ORBIT",
  SITE: "SITE",
  COMPLETE: "COMPLETE",
  CANCELLED: "CANCELLED",
  FAILED: "FAILED",
});

function reducedMotion() {
  return globalThis.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches === true;
}

function finite(value, fallback) {
  const number = Number(value);
  return Number.isFinite(number) ? number : fallback;
}

function clamp(value, min, max, fallback) {
  return Math.min(max, Math.max(min, finite(value, fallback)));
}

function radians(value) {
  return globalThis.Cesium.Math.toRadians(finite(value, 0));
}

export function normalizeEarthPresentation(value = {}) {
  const start = clamp(value.start_altitude_m, 1000000, 80000000, DEFAULT_PRESENTATION.start_altitude_m);
  const orbit = clamp(
    value.orbit_altitude_m ?? value.end_altitude_m,
    500, start,
    DEFAULT_PRESENTATION.orbit_altitude_m,
  );
  return {
    ...DEFAULT_PRESENTATION,
    ...value,
    schema_version: 1,
    enabled: value.enabled !== false,
    duration_ms: Math.round(clamp(value.duration_ms, 400, 30000, DEFAULT_PRESENTATION.duration_ms)),
    start_altitude_m: start,
    orbit_altitude_m: orbit,
    end_altitude_m: clamp(value.end_altitude_m, 20, orbit, orbit),
    atmosphere: value.atmosphere !== false,
    clouds: value.clouds !== false,
    stars: value.stars !== false,
    sun: value.sun !== false,
    moon: value.moon !== false,
    fog: value.fog !== false,
    fog_density: clamp(value.fog_density, 0, 0.01, DEFAULT_PRESENTATION.fog_density),
    hue_shift: clamp(value.hue_shift, -1, 1, DEFAULT_PRESENTATION.hue_shift),
    saturation_shift: clamp(value.saturation_shift, -1, 1, DEFAULT_PRESENTATION.saturation_shift),
    brightness_shift: clamp(value.brightness_shift, -1, 1, DEFAULT_PRESENTATION.brightness_shift),
    orbit_heading_delta_deg: clamp(
      value.orbit_heading_delta_deg,
      -90,
      90,
      DEFAULT_PRESENTATION.orbit_heading_delta_deg,
    ),
    skip_on_reduced_motion: value.skip_on_reduced_motion !== false,
    cloud_count: Math.round(clamp(value.cloud_count, 0, 128, DEFAULT_PRESENTATION.cloud_count)),
    cloud_radius_m: clamp(value.cloud_radius_m, 1000, 500000, DEFAULT_PRESENTATION.cloud_radius_m),
    cloud_altitude_m: clamp(value.cloud_altitude_m, 500, 20000, DEFAULT_PRESENTATION.cloud_altitude_m),
    cloud_brightness: clamp(value.cloud_brightness, 0, 2, DEFAULT_PRESENTATION.cloud_brightness),
    cancel_on_interaction: value.cancel_on_interaction !== false,
    show_progress: value.show_progress !== false,
    persistent_procedural_clouds: value.persistent_procedural_clouds !== false,
  };
}

export function resolveEarthIntroTarget(value = {}) {
  const lon = finite(value.lon ?? value.longitude, Number.NaN);
  const lat = finite(value.lat ?? value.latitude, Number.NaN);
  if (!Number.isFinite(lon) || !Number.isFinite(lat) || lon < -180 || lon > 180 || lat < -90 || lat > 90) {
    return null;
  }
  return {
    lon,
    lat,
    alt: Math.max(20, finite(value.alt ?? value.altitude, 250)),
    heading: finite(value.heading ?? value.rumo, 0),
    pitch: clamp(value.pitch, -90, 10, -30),
    roll: clamp(value.roll, -180, 180, 0),
  };
}

function setCloudVisibility(scene, visible) {
  const primitives = scene?.primitives;
  const CloudCollection = globalThis.Cesium?.CloudCollection;
  if (!primitives || !CloudCollection || typeof primitives.get !== "function") return 0;
  let changed = 0;
  for (let index = 0; index < Number(primitives.length || 0); index += 1) {
    const primitive = primitives.get(index);
    if (primitive instanceof CloudCollection) {
      primitive.show = visible;
      changed += 1;
    }
  }
  return changed;
}

function deterministicUnit(seed) {
  let value = seed >>> 0;
  return () => {
    value = (Math.imul(value, 1664525) + 1013904223) >>> 0;
    return value / 0x100000000;
  };
}

function cloudSeed(target) {
  const text = `${target.lon.toFixed(5)}:${target.lat.toFixed(5)}`;
  let hash = 2166136261;
  for (const character of text) {
    hash ^= character.charCodeAt(0);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

/** Creates lightweight procedural Cesium clouds around the active site only. */
export function ensureProceduralClouds(scene, target, rawConfig = {}) {
  const Cesium = globalThis.Cesium;
  const config = normalizeEarthPresentation(rawConfig);
  if (!scene?.primitives || !Cesium?.CloudCollection || !target || !config.clouds || config.cloud_count <= 0) {
    return { created: false, count: 0, reason: "CLOUDS_UNAVAILABLE_OR_DISABLED" };
  }
  let collection = scene.__arczProceduralClouds;
  if (!collection || collection.isDestroyed?.()) {
    collection = scene.primitives.add(new Cesium.CloudCollection());
    collection.__arczOwned = true;
    scene.__arczProceduralClouds = collection;
  } else if (typeof collection.removeAll === "function") {
    collection.removeAll();
  }
  collection.show = true;
  const random = deterministicUnit(cloudSeed(target));
  const metresPerLat = 111320;
  const metresPerLon = Math.max(1000, metresPerLat * Math.cos(Cesium.Math.toRadians(target.lat)));
  for (let index = 0; index < config.cloud_count; index += 1) {
    const angle = random() * Math.PI * 2;
    const radius = Math.sqrt(random()) * config.cloud_radius_m;
    const east = Math.cos(angle) * radius;
    const north = Math.sin(angle) * radius;
    const altitude = config.cloud_altitude_m * (0.72 + random() * 0.72);
    const width = 700 + random() * 2400;
    const depth = 450 + random() * 1500;
    const height = 240 + random() * 850;
    collection.add({
      position: Cesium.Cartesian3.fromDegrees(
        target.lon + east / metresPerLon,
        target.lat + north / metresPerLat,
        altitude,
      ),
      scale: Cesium.Cartesian2 ? new Cesium.Cartesian2(width, depth) : undefined,
      maximumSize: Cesium.Cartesian3 ? new Cesium.Cartesian3(width, depth, height) : undefined,
      slice: 0.28 + random() * 0.5,
      brightness: config.cloud_brightness * (0.82 + random() * 0.28),
    });
  }
  scene.requestRender?.();
  return { created: true, count: config.cloud_count, collection };
}

/**
 * Applies only reversible/presentation properties. It never changes active
 * region, terrain provider, imagery source, camera controls or project data.
 */
export function applyCinematicEarthBaseline(viewer, rawConfig = {}) {
  const Cesium = globalThis.Cesium;
  if (!viewer?.scene || !Cesium) return { applied: false, reason: "CESIUM_UNAVAILABLE" };
  const config = normalizeEarthPresentation(rawConfig);
  const scene = viewer.scene;
  const warnings = [];
  try {
    if ("highDynamicRange" in scene) scene.highDynamicRange = true;
    if (scene.postProcessStages?.fxaa) scene.postProcessStages.fxaa.enabled = true;
    if (scene.skyAtmosphere) {
      scene.skyAtmosphere.show = config.atmosphere;
      scene.skyAtmosphere.hueShift = config.hue_shift;
      scene.skyAtmosphere.saturationShift = config.saturation_shift;
      scene.skyAtmosphere.brightnessShift = config.brightness_shift;
      if ("perFragmentAtmosphere" in scene.skyAtmosphere) scene.skyAtmosphere.perFragmentAtmosphere = true;
      if ("dynamicLighting" in scene.skyAtmosphere && Cesium.DynamicAtmosphereLightingType) {
        scene.skyAtmosphere.dynamicLighting = Cesium.DynamicAtmosphereLightingType.SUNLIGHT;
      }
    }
    if (scene.atmosphere && Cesium.DynamicAtmosphereLightingType && "dynamicLighting" in scene.atmosphere) {
      scene.atmosphere.dynamicLighting = Cesium.DynamicAtmosphereLightingType.SUNLIGHT;
    }
    if (scene.sun) scene.sun.show = config.sun;
    if (scene.moon) scene.moon.show = config.moon;
    if (scene.skyBox) scene.skyBox.show = config.stars;
    if (scene.fog) {
      scene.fog.enabled = config.fog;
      scene.fog.density = config.fog_density;
      if ("visualDensityScalar" in scene.fog) scene.fog.visualDensityScalar = 0.35;
    }
    if (scene.globe) {
      scene.globe.show = true;
      scene.globe.showGroundAtmosphere = config.atmosphere;
      scene.globe.enableLighting = true;
      scene.globe.dynamicAtmosphereLighting = true;
      scene.globe.dynamicAtmosphereLightingFromSun = true;
      scene.globe.showWaterEffect = true;
      scene.globe.depthTestAgainstTerrain = true;
      if (Cesium.Color) {
        scene.globe.baseColor = Cesium.Color.fromCssColorString?.("#111923") || scene.globe.baseColor;
        scene.globe.undergroundColor = Cesium.Color.fromCssColorString?.("#070a0e") || scene.globe.undergroundColor;
      }
    }
    setCloudVisibility(scene, config.clouds);
    scene.requestRender?.();
    return { applied: true, config, warnings };
  } catch (error) {
    warnings.push({ code: "EARTH_BASELINE_PARTIAL", message: error?.message || String(error) });
    console.warn("Baseline cinematográfico parcial:", error);
    scene.requestRender?.();
    return { applied: true, config, warnings };
  }
}

/** Promise wrapper for Cesium's callback-based Camera.flyTo API. */
export function flyToCamera(camera, options, { signal } = {}) {
  if (!camera?.flyTo) return Promise.reject(new Error("Câmera Cesium indisponível"));
  if (signal?.aborted) return Promise.resolve(false);
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = value => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener?.("abort", onAbort);
      resolve(value);
    };
    const onAbort = () => {
      try { camera.cancelFlight?.(); } catch (error) { console.debug("Cancelamento de voo não suportado:", error); }
      finish(false);
    };
    signal?.addEventListener?.("abort", onAbort, { once: true });
    try {
      camera.flyTo({
        ...options,
        complete: () => finish(true),
        cancel: () => finish(false),
      });
    } catch (error) {
      signal?.removeEventListener?.("abort", onAbort);
      reject(error);
    }
  });
}

function snapshotController(controller) {
  if (!controller) return null;
  const keys = [
    "enableRotate", "enableTranslate", "enableZoom", "enableTilt", "enableLook",
    "enableCollisionDetection", "inertiaSpin", "inertiaTranslate", "inertiaZoom",
    "maximumMovementRatio", "minimumZoomDistance", "maximumZoomDistance",
  ];
  return Object.fromEntries(keys.filter(key => key in controller).map(key => [key, controller[key]]));
}

function disableController(controller) {
  if (!controller) return;
  for (const key of ["enableRotate", "enableTranslate", "enableZoom", "enableTilt", "enableLook"]) {
    if (key in controller) controller[key] = false;
  }
}

export class CinematicGlobeIntro {
  constructor({ viewer, estadoApp, onStateChange = null } = {}) {
    this.viewer = viewer;
    this.estadoApp = estadoApp;
    this.onStateChange = onStateChange;
    this.abortController = null;
    this.overlay = null;
    this.state = EARTH_INTRO_STATES.IDLE;
    this.runId = 0;
    this.interactionCleanups = [];
  }

  _setState(state, detail = {}) {
    this.state = state;
    this.onStateChange?.({ state, ...detail });
    if (typeof globalThis.dispatchEvent === "function" && typeof globalThis.CustomEvent === "function") {
      globalThis.dispatchEvent(new globalThis.CustomEvent(
        "arcz:earth-intro-state",
        { detail: { state, ...detail } },
      ));
    }
    const status = this.overlay?.querySelector?.("[data-earth-intro-status]");
    if (status && detail.label) status.textContent = detail.label;
    const progress = this.overlay?.querySelector?.("progress");
    if (progress && Number.isFinite(detail.progress)) progress.value = detail.progress;
  }

  createOverlay(config) {
    this.overlay?.remove();
    const overlay = document.createElement("div");
    overlay.className = "arcz-earth-intro";
    overlay.setAttribute("role", "presentation");
    overlay.innerHTML = [
      '<div class="arcz-earth-intro__stars" aria-hidden="true"></div>',
      '<div class="arcz-earth-intro__horizon" aria-hidden="true"></div>',
      '<div class="arcz-earth-intro__vignette" aria-hidden="true"></div>',
      '<div class="arcz-earth-intro__grain" aria-hidden="true"></div>',
      '<div class="arcz-earth-intro__status" role="status" aria-live="polite" aria-atomic="true"><span data-earth-intro-status>Preparando Terra cinematográfica…</span>',
      config.show_progress ? '<progress max="1" value="0" aria-label="Progresso da abertura"></progress>' : "",
      "</div>",
      '<button type="button" class="arcz-earth-intro__skip">Pular abertura</button>',
    ].join("");
    document.body.append(overlay);
    overlay.querySelector("button")?.addEventListener("click", () => this.cancel("skip_button"));
    this.overlay = overlay;
  }

  _watchInteraction(config) {
    this._clearInteractionWatchers();
    if (!config.cancel_on_interaction) return;
    const canvas = this.viewer?.scene?.canvas;
    const cancel = event => {
      if (event?.target?.closest?.(".arcz-earth-intro__skip")) return;
      this.cancel(`user_${event?.type || "interaction"}`);
    };
    const registrations = [
      [canvas, "pointerdown", cancel, { capture: true }],
      [canvas, "wheel", cancel, { capture: true, passive: true }],
      [globalThis, "keydown", event => {
        if (["Escape", " ", "Enter", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(event.key)) cancel(event);
      }, { capture: true }],
    ];
    for (const [target, type, listener, options] of registrations) {
      if (!target?.addEventListener) continue;
      target.addEventListener(type, listener, options);
      this.interactionCleanups.push(() => target.removeEventListener(type, listener, options));
    }
  }

  _clearInteractionWatchers() {
    for (const cleanup of this.interactionCleanups.splice(0)) cleanup();
  }

  async play(destination) {
    const config = normalizeEarthPresentation(this.estadoApp?.obter?.()?.earth_presentation || {});
    applyCinematicEarthBaseline(this.viewer, config);
    if (!config.enabled) return false;
    if (config.skip_on_reduced_motion && reducedMotion()) return false;
    const Cesium = globalThis.Cesium;
    if (!Cesium || !this.viewer?.camera || !this.viewer?.scene) return false;

    const appState = this.estadoApp?.obter?.() || {};
    const target = resolveEarthIntroTarget(destination || appState.camera || appState.posicao);
    if (!target) return false;

    this.cancel("superseded");
    const runId = ++this.runId;
    this.abortController = new AbortController();
    const { signal } = this.abortController;
    this.createOverlay(config);
    this._setState(EARTH_INTRO_STATES.PREPARING, { progress: 0, label: "Preparando Terra cinematográfica…" });
    this._watchInteraction(config);

    const controller = this.viewer.scene.screenSpaceCameraController;
    const previous = snapshotController(controller);
    const total = config.duration_ms / 1000;
    const orbitAltitude = Math.max(target.alt * 2, config.orbit_altitude_m);
    const startAltitude = Math.max(orbitAltitude, config.start_altitude_m);
    let completed = false;

    try {
      disableController(controller);
      ensureProceduralClouds(this.viewer.scene, target, config);
      this.viewer.camera.setView({
        destination: Cesium.Cartesian3.fromDegrees(target.lon, target.lat, startAltitude),
        orientation: {
          heading: radians(target.heading - config.orbit_heading_delta_deg),
          pitch: radians(-88),
          roll: 0,
        },
      });
      this._setState(EARTH_INTRO_STATES.SPACE, { progress: 0.08, label: "Vista orbital da Terra" });
      const reachedOrbit = await flyToCamera(this.viewer.camera, {
        destination: Cesium.Cartesian3.fromDegrees(target.lon, target.lat, orbitAltitude),
        orientation: {
          heading: radians(target.heading + config.orbit_heading_delta_deg),
          pitch: radians(-62),
          roll: 0,
        },
        duration: total * 0.58,
        easingFunction: Cesium.EasingFunction.QUINTIC_IN_OUT,
      }, { signal });
      if (!reachedOrbit || signal.aborted) return false;

      this._setState(EARTH_INTRO_STATES.ORBIT, { progress: 0.62, label: "Aproximando a Região Ativa" });
      const reachedSite = await flyToCamera(this.viewer.camera, {
        destination: Cesium.Cartesian3.fromDegrees(target.lon, target.lat, target.alt),
        orientation: {
          heading: radians(target.heading),
          pitch: radians(target.pitch),
          roll: radians(target.roll),
        },
        duration: total * 0.42,
        easingFunction: Cesium.EasingFunction.CUBIC_OUT,
      }, { signal });
      completed = Boolean(reachedSite && !signal.aborted);
      this._setState(
        completed ? EARTH_INTRO_STATES.COMPLETE : EARTH_INTRO_STATES.CANCELLED,
        { progress: completed ? 1 : 0.62, label: completed ? "Região pronta para autoria" : "Abertura cancelada" },
      );
      return completed;
    } catch (error) {
      this._setState(EARTH_INTRO_STATES.FAILED, { label: `Falha na abertura: ${error?.message || error}` });
      throw error;
    } finally {
      if (controller && previous) Object.assign(controller, previous);
      this._clearInteractionWatchers();
      this.viewer.scene.requestRender?.();
      if (!config.persistent_procedural_clouds && this.viewer.scene.__arczProceduralClouds) {
        this.viewer.scene.__arczProceduralClouds.show = false;
      }
      const overlay = this.overlay;
      overlay?.classList.add("is-ending");
      setTimeout(() => overlay?.remove(), reducedMotion() ? 0 : 480);
      if (this.overlay === overlay) this.overlay = null;
      if (this.runId === runId) this.abortController = null;
      if (!completed && this.state !== EARTH_INTRO_STATES.FAILED) {
        this._setState(EARTH_INTRO_STATES.CANCELLED, { label: "Abertura cancelada" });
      }
    }
  }

  cancel(reason = "cancelled") {
    if (this.abortController) {
      this._setState(EARTH_INTRO_STATES.CANCELLED, { reason, label: "Abertura cancelada" });
      this.abortController.abort(reason);
    }
    this.abortController = null;
    this._clearInteractionWatchers();
    try { this.viewer?.camera?.cancelFlight?.(); } catch (error) {
      console.debug("Cancelamento de voo não suportado:", error);
    }
    this.overlay?.remove();
    this.overlay = null;
  }

  dispose() {
    this.cancel("disposed");
    const collection = this.viewer?.scene?.__arczProceduralClouds;
    if (collection?.__arczOwned && this.viewer?.scene?.primitives?.remove) {
      try { this.viewer.scene.primitives.remove(collection); } catch (error) { console.debug(error); }
    }
    if (this.viewer?.scene) this.viewer.scene.__arczProceduralClouds = null;
    this.state = EARTH_INTRO_STATES.IDLE;
  }
}
