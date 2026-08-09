// ARCZ · Coalescência de eventos de alta frequência.
// Mantém apenas o valor mais recente por frame e oferece flush síncrono para
// mouse/pointer up, garantindo que a posição final nunca seja perdida.

function defaultSchedule(callback) {
  if (typeof globalThis.requestAnimationFrame === "function") {
    return { kind: "raf", id: globalThis.requestAnimationFrame(callback) };
  }
  return { kind: "timeout", id: globalThis.setTimeout(() => callback(Date.now()), 16) };
}

function defaultCancel(handle) {
  if (!handle) return;
  if (handle.kind === "raf" && typeof globalThis.cancelAnimationFrame === "function") {
    globalThis.cancelAnimationFrame(handle.id);
    return;
  }
  globalThis.clearTimeout(handle.id);
}

export class LatestFrameQueue {
  constructor({ schedule = defaultSchedule, cancel = defaultCancel } = {}) {
    this.schedule = schedule;
    this.cancel = cancel;
    this.handle = null;
    this.pending = undefined;
    this.consumer = null;
  }

  get scheduled() {
    return this.handle !== null;
  }

  push(value, consumer) {
    if (typeof consumer !== "function") throw new TypeError("consumer precisa ser função");
    this.pending = value;
    this.consumer = consumer;
    if (this.handle !== null) return;
    this.handle = this.schedule(() => {
      this.handle = null;
      const latest = this.pending;
      const callback = this.consumer;
      this.pending = undefined;
      this.consumer = null;
      if (callback && latest !== undefined) callback(latest);
    });
  }

  flush(consumer = this.consumer) {
    if (this.handle !== null) {
      this.cancel(this.handle);
      this.handle = null;
    }
    const latest = this.pending;
    const callback = consumer;
    this.pending = undefined;
    this.consumer = null;
    if (callback && latest !== undefined) callback(latest);
  }

  clear() {
    if (this.handle !== null) this.cancel(this.handle);
    this.handle = null;
    this.pending = undefined;
    this.consumer = null;
  }
}
