import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');

test('photoreal service accepts only a hashed Blender vendor inside the repo', () => {
  const source = read('arcz_server/photoreal.py');
  assert.match(source, /vendor["']?\s*\/\s*["']blender|vendor" \/ "blender/);
  assert.match(source, /executable_sha256/);
  assert.match(source, /runtime_network_required/);
  assert.match(source, /verified_repo_local/);
  assert.match(source, /resolved_blender/);
  assert.doesNotMatch(source, /shutil\.which\(["']blender["']\)/);
});

test('Blender worker executes only the executable frozen by preflight', () => {
  const source = read('workers/blender/launch_blender.py');
  assert.match(source, /resolved_blender/);
  assert.match(source, /BLENDER_NOT_VERIFIED/);
  assert.match(source, /vendor.*blender/);
  assert.match(source, /BLENDER_HASH_MISMATCH/);
  assert.doesNotMatch(source, /shutil\.which\(["']blender["']\)/);
  assert.doesNotMatch(source, /os\.environ\.get\(["']ARCZ_BLENDER["']\)\s+or/);
});

test('full runtime preflight requires the same Blender vendor contract', () => {
  const source = read('tools/runtime_preflight.py');
  assert.match(source, /blender_repo_vendor/);
  assert.match(source, /vendor.*blender/);
  assert.match(source, /executable_sha256/);
  assert.doesNotMatch(source, /shutil\.which\(["']blender["']\)/);
});

test('Blender vendor importer performs no download and records local integrity', () => {
  const source = read('tools/vendor_blender.py');
  assert.match(source, /Blender --version/);
  assert.match(source, /executable_sha256/);
  assert.match(source, /license_sha256/);
  assert.match(source, /runtime_network_required/);
  assert.doesNotMatch(source, /requests\.|urllib\.|curl|wget|https?:\/\//i);
});
