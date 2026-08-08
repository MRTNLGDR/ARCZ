export class BudgetClient {
  constructor({ baseUrl = "/api/v2", fetchImpl = globalThis.fetch?.bind(globalThis) } = {}) {
    if (!fetchImpl) throw new Error("fetch indisponível");
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.fetch = fetchImpl;
  }

  async evaluate(request, { signal } = {}) {
    const response = await this.fetch(`${this.baseUrl}/budget`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(request), signal
    });
    const data = await response.json();
    if (!response.ok) throw structuredApiError(data, response.status);
    return data;
  }

  async release(reservationId, { signal, state = "RELEASED" } = {}) {
    const response = await this.fetch(`${this.baseUrl}/budget/release`, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ reservation_id: reservationId, state }), signal
    });
    const data = await response.json();
    if (!response.ok) throw structuredApiError(data, response.status);
    return data;
  }

  async reserve(request, { signal } = {}) {
    const estimate = request?.estimate || request?.requested || request || {};
    const requested = request?.requested || {
      triangles: Math.max(0, Math.trunc(estimate.triangles ?? estimate.triangulos ?? 0)),
      instances: Math.max(0, Math.trunc(estimate.instances ?? estimate.instancias ?? 0)),
      draw_calls: Math.max(0, Math.trunc(estimate.draw_calls ?? estimate.drawCalls ?? 0)),
      geometry_mb: Math.max(0, Number(estimate.geometry_mb ?? estimate.memoriaMB ?? 0)),
      textures_mb: Math.max(0, Number(estimate.textures_mb ?? estimate.texturasMB ?? 0)),
      framebuffer_mb: Math.max(0, Number(estimate.framebuffer_mb ?? 0)),
      materials: Math.max(0, Math.trunc(estimate.materials ?? 0)),
      vegetation_overdraw: Math.max(0, Number(estimate.vegetation_overdraw ?? 0)),
      cpu_ms: Math.max(0, Number(estimate.cpu_ms ?? 0)),
      gpu_upload_ms: Math.max(0, Number(estimate.gpu_upload_ms ?? 0)),
      cache_mb: Math.max(0, Number(estimate.cache_mb ?? 0))
    };
    const decision = await this.evaluate({ requested, profile: estimate.profile || request?.profile || "EQUILIBRADO", reserve: true }, { signal });
    if (decision.decision !== "ACCEPT" || !decision.reservation_id) {
      const error = new Error(`Orçamento recusado: ${decision.decision}`);
      Object.assign(error, { code: "BUDGET_NOT_ACCEPTED", decision });
      throw error;
    }
    let closed = false;
    const close = async state => {
      if (closed) return false;
      closed = true;
      await this.release(decision.reservation_id, { signal, state });
      return true;
    };
    return Object.freeze({
      ...decision,
      release: () => close("RELEASED"),
      commit: () => close("COMMITTED")
    });
  }

  asServices() { return { budgetReserve: this.reserve.bind(this) }; }
}

export function structuredApiError(data, status = 500) {
  const payload = data?.error || { code: "HTTP_ERROR", message: `HTTP ${status}`, retryable: status >= 500, details: {}, trace_id: "unknown" };
  const error = new Error(payload.message);
  Object.assign(error, payload, { status });
  return error;
}
