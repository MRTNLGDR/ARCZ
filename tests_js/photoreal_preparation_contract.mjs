import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');

test('single launcher materializes a real Blender distribution into the repo', () => {
  const alias = read('PREPARAR_FOTORREAL.cmd');
  const source = read('tools/windows/arcz_launch.py');

  assert.match(alias, /ARCZ\.bat/);
  assert.match(alias, /-ForceSetup/);
  assert.match(source, /vendor_blender\.py/);
  assert.match(source, /_blender_check/);
  assert.match(source, /--source/);
  assert.match(source, /--license-file/);
  assert.match(source, /vendor["']\s*\/\s*["']toolchains/);
  assert.match(source, /offline_strict/);
  assert.doesNotMatch(source, /https:\/\//i);
});

test('base Cycles preflight does not require a diffusion model', () => {
  const source = read('tools/photoreal_preflight.py');
  assert.match(source, /render\.photoreal\.cycles/);
  assert.match(source, /_blender_check/);
  assert.match(source, /render\.photoreal\.worker\.json/);
  assert.match(source, /enhancement_model_required["']:\s*False/);
  assert.doesNotMatch(source, /ModelRegistry|render-diffusion/);
});
