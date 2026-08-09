import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';

const text = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');

test('authoring shell opens directly into a four-step real workflow', () => {
  const shell = text('app/shell/fusion-shell.js');
  assert.match(shell, /1 · Localizar/);
  assert.match(shell, /2 · Modelar/);
  assert.match(shell, /3 · Fotorreal/);
  assert.match(shell, /4 · Rua/);
  assert.doesNotMatch(shell, /CinematicGlobeIntro/);
  assert.doesNotMatch(shell, /\.play\(/);
  assert.equal(existsSync(new URL('../app/earth/cinematic-globe.js', import.meta.url)), false);
});

test('photoreal UI is wired to the real V2 preflight and job endpoints', () => {
  const client = text('app/render/photoreal-client.js');
  const service = text('arcz_server/photoreal.py');
  const worker = text('resources/workers/render.photoreal.worker.json');
  assert.match(client, /\/api\/v2\/photoreal\/preflight/);
  assert.match(client, /\/api\/v2\/photoreal\/jobs/);
  assert.match(service, /PHOTOREAL_WORKER_NOT_INSTALLED/);
  assert.match(service, /BLENDER_NOT_INSTALLED/);
  assert.match(service, /sha256_file/);
  assert.match(worker, /workers\/blender\/launch_blender\.py/);
  assert.match(worker, /offline_strict/);
});

test('reconstruction and archviz sources contain no former synthetic placeholders', () => {
  const reconstruct = text('crates/arcz-app/src/reconstruct_worker.rs');
  const archviz = text('crates/arcz-app/src/archviz_worker.rs');
  assert.doesNotMatch(reconstruct, /estimated_points_or_vertices:\s*100_000/);
  assert.doesNotMatch(reconstruct, /Bounding volume fictício/);
  assert.doesNotMatch(archviz, /wood_albedo\.png/);
  assert.doesNotMatch(archviz, /mat_pbr_default/);
  assert.match(reconstruct, /sha256_file/);
  assert.match(archviz, /sha256_file/);
});
