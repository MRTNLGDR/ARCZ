import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');

test('Windows launcher refuses partial runtime and starts offline only', () => {
  const source = read('ABRIR_ARCZ.cmd');
  assert.match(source, /ARCZ_NETWORK_MODE=offline_strict/);
  assert.match(source, /ARCZ_BANCO=%CD%\\resources\\assets/);
  assert.match(source, /ARCZ_SEM_NAVEGADOR=1/);
  assert.match(source, /runtime_preflight\.py --profile interactive/);
  assert.match(source, /\/api\/v2\/health/);
  assert.match(source, /PREPARAR_ARCZ\.cmd/);
  assert.doesNotMatch(source, /import_assisted/);
  assert.doesNotMatch(source, /https:\/\/|http:\/\/(?!127\.0\.0\.1)/i);
});

test('Windows preparation launcher is the explicit import-assisted boundary', () => {
  const source = read('PREPARAR_ARCZ.cmd');
  assert.match(source, /ARCZ_NETWORK_MODE=import_assisted/);
  assert.match(source, /ARCZ_BANCO=%CD%\\resources\\assets/);
  assert.match(source, /prepare_local_runtime\.py --interactive/);
  assert.match(source, /ARCZ_NETWORK_MODE=offline_strict/);
  assert.doesNotMatch(source, /curl|powershell.*Invoke-WebRequest/i);
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
