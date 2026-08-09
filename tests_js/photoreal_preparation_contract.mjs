import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');

test('photoreal preparation takes an explicit local Blender distribution', () => {
  const source = read('PREPARAR_FOTORREAL.cmd');
  assert.match(source, /vendor_blender\.py/);
  assert.match(source, /photoreal_preflight\.py/);
  assert.match(source, /ARCZ_NETWORK_MODE=offline_strict/);
  assert.match(source, /--source/);
  assert.match(source, /--license-file/);
  assert.doesNotMatch(source, /curl|wget|Invoke-WebRequest|https?:\/\//i);
});

test('base Cycles preflight does not require a diffusion model', () => {
  const source = read('tools/photoreal_preflight.py');
  assert.match(source, /render\.photoreal\.cycles/);
  assert.match(source, /_blender_check/);
  assert.match(source, /render\.photoreal\.worker\.json/);
  assert.match(source, /enhancement_model_required["']:\s*False/);
  assert.doesNotMatch(source, /ModelRegistry|render-diffusion/);
});
