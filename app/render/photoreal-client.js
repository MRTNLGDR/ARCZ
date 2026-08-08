import { LocalApiClient } from "../core/api-client.js";

const ALL_PASSES = ["beauty", "depth", "normals", "object_ids", "semantic_masks", "material_masks", "sky_mask"];

export class PhotorealClient {
  constructor({ api = new LocalApiClient() } = {}) { this.api = api; }
  preflight(request) { return this.api.json("/api/v2/photoreal/preflight", { method: "POST", body: request }); }
  submit(request) { return this.api.json("/api/v2/photoreal/jobs", { method: "POST", body: request }); }
  listJobs(limit = 20) { return this.api.json(`/api/v2/render/jobs?limit=${encodeURIComponent(limit)}`); }
  getJob(id) { return this.api.json(`/api/v2/render/jobs/${encodeURIComponent(id)}`); }
  cancelJob(id, reason = "cancelled_by_user") {
    return this.api.json(`/api/v2/render/jobs/${encodeURIComponent(id)}/cancel`, {
      method: "POST", body: { reason },
    });
  }
  async waitJob(id, { signal, pollMs = 800, onUpdate = null } = {}) {
    for (;;) {
      if (signal?.aborted) throw signal.reason || new DOMException("Aborted", "AbortError");
      const job = await this.getJob(id);
      onUpdate?.(job);
      if (["COMPLETED", "CANCELLED", "FAILED_RETRYABLE", "FAILED_PERMANENT"].includes(job.status)) return job;
      await new Promise((resolve, reject) => {
        const timer = setTimeout(resolve, Math.max(250, Number(pollMs) || 800));
        signal?.addEventListener("abort", () => {
          clearTimeout(timer);
          reject(signal.reason || new DOMException("Aborted", "AbortError"));
        }, { once: true });
      });
    }
  }
}

export function parseVector3(value, fallback) {
  const result = Array.isArray(value)
    ? value.map(Number)
    : String(value || "").split(/[;,\s]+/).filter(Boolean).map(Number);
  if (result.length !== 3 || !result.every(Number.isFinite)) return [...fallback];
  return result;
}

export function normalizeRenderPasses(value) {
  const source = Array.isArray(value) ? value : [];
  const result = [...new Set(source.filter(item => ALL_PASSES.includes(item)))];
  return result.length ? result : ["beauty"];
}

export function buildPhotorealRequest({
  project,
  prompt = "",
  negativePrompt = "",
  references = [],
  width = 7680,
  height = 4320,
  mode = "full_photoreal",
  outputName = "arcz-render",
  seed = 1,
  camera = {},
  format = "png",
  passes = ALL_PASSES,
  modelId = null,
  geometryGuardPx = 2,
  generationEpoch = 0,
  sceneExportId = null,
  quality = "balanced",
  engine = "cycles",
  renderSettings = {},
  environment = {},
} = {}) {
  if (!project?.id || Number(project.current_revision) < 1) {
    throw new Error("Projeto Floorplanner precisa ter uma revisão salva");
  }
  return {
    schema_version: 1,
    floorplanner_project_id: project.id,
    revision: Number(project.current_revision),
    scene_export_id: sceneExportId || null,
    quality: ["draft", "preview", "balanced", "high", "ultra"].includes(quality) ? quality : "balanced",
    engine: engine === "eevee" ? "eevee" : "cycles",
    camera: {
      position: parseVector3(camera.position, [12, 8, 12]),
      target: parseVector3(camera.target, [0, 2, 0]),
      focal_length_mm: Number(camera.focal_length_mm ?? 35),
      aperture: Number(camera.aperture ?? 5.6),
      focus_distance_m: Number(camera.focus_distance_m ?? 15),
      sensor_width_mm: Number(camera.sensor_width_mm ?? 36),
      shift_x: Number(camera.shift_x ?? 0),
      shift_y: Number(camera.shift_y ?? 0),
      clip_start_m: Number(camera.clip_start_m ?? 0.05),
      clip_end_m: Number(camera.clip_end_m ?? 100000),
      vertical_correction: camera.vertical_correction !== false,
    },
    resolution: { width: Number(width), height: Number(height) },
    format: ["png", "jpg", "exr"].includes(format) ? format : "png",
    passes: normalizeRenderPasses(passes),
    reference_media: [...new Set((references || []).map(String))],
    enhancement: {
      mode,
      model_id: modelId || null,
      prompt,
      negative_prompt: negativePrompt,
      seed: Math.max(0, Math.trunc(Number(seed) || 0)),
      geometry_guard_px: Number(geometryGuardPx),
    },
    render_settings: {
      samples: renderSettings.samples == null ? null : Math.max(1, Math.trunc(Number(renderSettings.samples) || 1)),
      denoise: renderSettings.denoise !== false,
      device: ["auto", "cpu", "gpu"].includes(renderSettings.device) ? renderSettings.device : "auto",
      tile_size: Math.min(2048, Math.max(32, Math.trunc(Number(renderSettings.tile_size) || 256))),
      transparent_background: Boolean(renderSettings.transparent_background),
      color_management: ["AgX", "Filmic", "Standard"].includes(renderSettings.color_management)
        ? renderSettings.color_management : "AgX",
      look: String(renderSettings.look || "AgX - Medium High Contrast"),
      motion_blur: Boolean(renderSettings.motion_blur),
      motion_blur_shutter: Math.min(2, Math.max(0, Number(renderSettings.motion_blur_shutter ?? 0.5))),
      max_memory_mb: renderSettings.max_memory_mb == null ? null : Math.max(512, Math.trunc(Number(renderSettings.max_memory_mb) || 512)),
    },
    environment: {
      world_mode: ["nishita", "studio", "transparent"].includes(environment.world_mode)
        ? environment.world_mode : "nishita",
      strength: Math.max(0, Number(environment.strength ?? 0.8)),
      sun_elevation_deg: Number(environment.sun_elevation_deg ?? 25),
      sun_rotation_deg: Number(environment.sun_rotation_deg ?? -35),
      sun_energy: Math.max(0, Number(environment.sun_energy ?? 3)),
      haze: Math.max(0, Number(environment.haze ?? 1)),
      cloud_density: Math.min(1, Math.max(0, Number(environment.cloud_density ?? 0))),
      ground_color: parseVector3(environment.ground_color, [0.16, 0.18, 0.14]).map(value => Math.min(1, Math.max(0, value))),
    },
    output_name: String(outputName || "arcz-render").replace(/[^A-Za-z0-9._-]/g, "-") || "arcz-render",
    generation_epoch: Math.max(0, Math.trunc(Number(generationEpoch) || 0)),
  };
}
