import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');

test('all Windows entrypoints converge on the single ARCZ.bat launcher', () => {
  const root = read('ARCZ.bat');
  const open = read('ABRIR_ARCZ.cmd');
  const prepare = read('PREPARAR_ARCZ.cmd');
  const photoreal = read('PREPARAR_FOTORREAL.cmd');
  const ps = read('tools/windows/arcz-launch.ps1');

  assert.match(root, /tools\\windows\\arcz-launch\.ps1/i);
  assert.match(open, /call\s+"%~dp0ARCZ\.bat"/i);
  assert.match(prepare, /call\s+"%~dp0ARCZ\.bat"\s+-ForceSetup/i);
  assert.match(photoreal, /call\s+"%~dp0ARCZ\.bat"\s+-ForceSetup/i);
  assert.match(ps, /tools\\windows\\arcz_launch\.py/i);
});

test('canonical Windows controller refuses partial runtime and opens offline only', () => {
  const source = read('tools/windows/arcz_launch.py');
  assert.match(source, /ARCZ_NETWORK_MODE["']\]\s*=\s*["']offline_strict/);
  assert.match(source, /resources["']\s*\/\s*["']assets/);
  assert.match(source, /runtime_preflight\.py/);
  assert.match(source, /--profile["']?,?\s*["']interactive/);
  assert.match(source, /\/api\/v2\/health/);
  assert.match(source, /arcz_local\.py/);
  assert.match(source, /127\.0\.0\.1:8123/);
  assert.doesNotMatch(source, /https:\/\//i);
});

test('canonical Windows controller owns the explicit import-assisted setup boundary', () => {
  const source = read('tools/windows/arcz_launch.py');
  assert.match(source, /ARCZ_NETWORK_MODE["']\]\s*=\s*["']import_assisted/);
  assert.match(source, /prepare_local_runtime\.py/);
  assert.match(source, /--interactive/);
  assert.match(source, /offline_strict/);
  assert.match(source, /PREPARED_HEAD/);
});

test('runtime preparation materializes only pinned repo-local vendors', () => {
  const source = read('tools/prepare_local_runtime.py');
  assert.match(source, /materialize_upstreams\.py/);
  assert.match(source, /--only", "cesiumjs/);
  assert.match(source, /vendor_cesium\.py/);
  assert.match(source, /--only", "aedifex/);
  assert.match(source, /vendor_aedifex_controlled\.py/);
  assert.match(source, /build_aedifex_sidecar_controlled\.py/);
  assert.match(source, /smoke_aedifex_sidecar\.py/);
  assert.match(source, /offline_strict/);
});

test('Aedifex standalone packaging refuses dangling or external Bun links', () => {
  const source = read('tools/build_aedifex_sidecar_controlled.py');
  assert.match(source, /materialize_dangling_links/);
  assert.match(source, /symlink sem alvo materializável/);
  assert.match(source, /standalone ainda possui symlink dangling\/externo/);
  assert.doesNotMatch(source, /ignore_dangling_symlinks\s*=\s*True/);
});
