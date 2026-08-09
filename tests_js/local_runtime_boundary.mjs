import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const CDN_PATTERNS = [
  /unpkg\.com/i,
  /cdn\.jsdelivr\.net/i,
  /cdnjs\.cloudflare\.com/i,
  /fonts\.googleapis\.com/i,
  /fonts\.gstatic\.com/i,
  /esm\.sh/i,
  /cdn\.skypack\.dev/i,
];

const PRIMARY_RUNTIME = [
  'index.html',
  'app/main.js',
  'app/shell/fusion-shell.js',
  'app/render/photoreal-client.js',
  'app/render/photoreal-workspace.js',
  'app/workflow-clean.css',
];

test('primary browser runtime has no CDN fallback', () => {
  for (const path of PRIMARY_RUNTIME) {
    const source = read(path);
    for (const pattern of CDN_PATTERNS) {
      assert.equal(pattern.test(source), false, `${path} contains forbidden CDN ${pattern}`);
    }
  }
});

test('map runtime is explicitly repo-local and fail-closed', () => {
  const index = read('index.html');
  assert.match(index, /\/vendor\/cesium\/Cesium\/Cesium\.js/);
  assert.match(index, /\/vendor\/cesium\/Cesium\/Widgets\/widgets\.css/);
  assert.match(index, /if \(!globalThis\.Cesium\)/);
  assert.match(index, /vendor_cesium\.py --from-pinned-source --allow-network --force/);
  assert.doesNotMatch(index, /https?:\/\/.*cesium/i);
});

test('photoreal path remains local V2 API only', () => {
  const client = read('app/render/photoreal-client.js');
  assert.match(client, /\/api\/v2\/photoreal\/preflight/);
  assert.match(client, /\/api\/v2\/photoreal\/jobs/);
  assert.doesNotMatch(client, /https?:\/\//i);
});
