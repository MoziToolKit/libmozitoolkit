import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const pkgDir = process.argv[2];
if (!pkgDir) {
  console.error('Usage: node tools/wasm_smoke.mjs <pkg-dir>');
  process.exit(2);
}

const jsUrl = pathToFileURL(path.resolve(pkgDir, 'mtk_wasm.js')).href;
const wasmBytes = fs.readFileSync(path.resolve(pkgDir, 'mtk_wasm_bg.wasm'));

const mod = await import(jsUrl);
const {
  default: init,
  get_hardware_concurrency,
  init_panic_hook,
  set_hardware_concurrency,
  version,
  WasmMesherConfig,
  WasmSaveWorld,
  WasmScenePackage,
} = mod;

await init({ module_or_path: wasmBytes });
init_panic_hook();

const ver = version();
assert.ok(typeof ver === 'string' && ver.length > 0, `invalid version: ${ver}`);

set_hardware_concurrency(4);
assert.equal(get_hardware_concurrency(), 4);
set_hardware_concurrency(0);

const cfg = new WasmMesherConfig();
assert.equal(cfg.enable_ao, true);
assert.equal(cfg.coordinate_system, 'y_up');
assert.equal(cfg.leaves_cull_mode, 'fancy');
cfg.leaves_cull_mode = 'fast';
assert.equal(cfg.leaves_cull_mode, 'fast');
assert.throws(() => {
  cfg.coordinate_system = 'invalid_coord';
});
cfg.free();

const plan = JSON.parse(
  WasmSaveWorld.plan_region_files(
    'overworld',
    -16,
    -64,
    -16,
    15,
    127,
    15,
    ['region/r.0.0.mca', 'region/r.-1.-1.mca', 'region/r.9.9.mca'],
  ),
);
assert.equal(plan.regions.length, 2);

assert.throws(() => {
  WasmScenePackage.from_bytes(new Uint8Array([1, 2, 3, 4]));
});

console.log(`[wasm_smoke] PASSED mtk-wasm v${ver}`);
