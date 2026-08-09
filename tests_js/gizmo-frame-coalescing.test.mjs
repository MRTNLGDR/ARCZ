import assert from "node:assert/strict";
import test from "node:test";

import { LatestFrameQueue } from "../app/core/frame-coalescer.js";
import { installGizmoFrameCoalescing } from "../app/core/gizmo-frame-coalescing.js";

function fakeFrameClock() {
  let nextId = 1;
  const pending = new Map();
  return {
    schedule(callback) {
      const handle = { kind: "test", id: nextId++ };
      pending.set(handle.id, callback);
      return handle;
    },
    cancel(handle) {
      pending.delete(handle?.id);
    },
    runNext() {
      const first = pending.entries().next();
      if (first.done) return false;
      const [id, callback] = first.value;
      pending.delete(id);
      callback(0);
      return true;
    },
    size() { return pending.size; },
  };
}

test("LatestFrameQueue mantém só o valor mais recente por frame", () => {
  const clock = fakeFrameClock();
  const queue = new LatestFrameQueue(clock);
  const seen = [];

  for (let i = 1; i <= 100; i++) queue.push(i, value => seen.push(value));

  assert.equal(clock.size(), 1);
  assert.deepEqual(seen, []);
  assert.equal(clock.runNext(), true);
  assert.deepEqual(seen, [100]);
  assert.equal(clock.size(), 0);
});

test("flush cancela o frame pendente e entrega a posição final uma única vez", () => {
  const clock = fakeFrameClock();
  const queue = new LatestFrameQueue(clock);
  const seen = [];

  queue.push({ x: 10 }, value => seen.push(value.x));
  queue.push({ x: 20 }, value => seen.push(value.x));
  queue.flush();

  assert.deepEqual(seen, [20]);
  assert.equal(clock.size(), 0);
  assert.equal(clock.runNext(), false);
});

test("gizmo processa hover imediatamente mas coalesce drag e faz flush antes de soltar", () => {
  const clock = fakeFrameClock();
  const calls = [];
  const gizmo = {
    arraste: null,
    aoMover(event) { calls.push(["move", event.endPosition.x, this.arraste !== null]); },
    aoSoltar() { calls.push(["release", this.arraste !== null]); this.arraste = null; },
  };

  const controller = installGizmoFrameCoalescing(gizmo, clock);

  gizmo.aoMover({ endPosition: { x: 1, y: 1 } });
  assert.deepEqual(calls, [["move", 1, false]]);

  gizmo.arraste = { eixo: "livre" };
  for (let i = 2; i <= 50; i++) gizmo.aoMover({ endPosition: { x: i, y: i } });
  assert.equal(clock.size(), 1);
  assert.equal(calls.length, 1);

  clock.runNext();
  assert.deepEqual(calls.at(-1), ["move", 50, true]);

  gizmo.aoMover({ endPosition: { x: 60, y: 60 } });
  gizmo.aoMover({ endPosition: { x: 70, y: 70 } });
  gizmo.aoSoltar();

  assert.deepEqual(calls.slice(-2), [["move", 70, true], ["release", true]]);
  assert.equal(clock.size(), 0);
  assert.equal(gizmo.arraste, null);

  controller.dispose();
});
