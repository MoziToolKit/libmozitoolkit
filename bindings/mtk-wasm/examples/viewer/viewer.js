import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { defaultHeightRangeForLevel, VIEWER_CONFIG } from './config.js';
import { createMtkPbrMaterial, loadAtlasTexture } from './material.js';
import {
  collectFromDirectoryHandle,
  collectFromWebkitEntry,
  fetchRemoteSaveEntries,
  normalizeSaveFileList,
} from './save_fs.js';
import { resolveSkinsInBackground } from './skins.js';
import init, {
  init_panic_hook,
  version,
  WasmAssetCache,
  WasmMesherConfig,
  WasmSaveWorld,
  WasmScenePackage,
} from './pkg/mtk_wasm.js';

const canvas = document.getElementById('gl-canvas');
const dropZone = document.getElementById('drop-zone');
const emptyOverlay = document.getElementById('empty-overlay');
const sceneFileInput = document.getElementById('file-scene');
const cacheFileInput = document.getElementById('file-cache');
const saveDirBtn = document.getElementById('btn-pick-save');
const saveDirInput = document.getElementById('dir-save');
const statusBadge = document.getElementById('status-badge');
const resetCamBtn = document.getElementById('btn-reset-cam');

const saveDetails = document.getElementById('save-details');
const saveForm = document.getElementById('save-form');
const saveDimSelect = document.getElementById('save-dim');
const spawnBoundsBtn = document.getElementById('btn-spawn-bounds');
const loadSaveBtn = document.getElementById('btn-load-save');
const boundsError = document.getElementById('bounds-error');
const boundInputs = {
  minX: document.getElementById('bound-min-x'),
  minY: document.getElementById('bound-min-y'),
  minZ: document.getElementById('bound-min-z'),
  maxX: document.getElementById('bound-max-x'),
  maxY: document.getElementById('bound-max-y'),
  maxZ: document.getElementById('bound-max-z'),
};

const statCache = document.getElementById('stat-cache');
const statSave = document.getElementById('stat-save');
const statSkins = document.getElementById('stat-skins');

const optAo = document.getElementById('opt-ao');
const optFluids = document.getElementById('opt-fluids');
const optCtm = document.getElementById('opt-ctm');
const optAnim = document.getElementById('opt-anim');
const optSkinsNet = document.getElementById('opt-skins-net');
const optLeavesCull = document.getElementById('opt-leaves-cull');
const optSun = document.getElementById('opt-sun');
const sunVal = document.getElementById('sun-val');

const statVoxels = document.getElementById('stat-voxels');
const statBounds = document.getElementById('stat-bounds');
const statBatches = document.getElementById('stat-batches');
const statTris = document.getElementById('stat-tris');
const statOpenMs = document.getElementById('stat-open-ms');
const statMeshMs = document.getElementById('stat-mesh-ms');

// ---------------------------------------------------------------------------
// Three.js Renderer, Scene & Lighting
// ---------------------------------------------------------------------------

const renderer = new THREE.WebGLRenderer({
  canvas,
  antialias: true,
  powerPreference: 'high-performance',
});
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
renderer.outputColorSpace = THREE.SRGBColorSpace;
renderer.toneMapping = THREE.ACESFilmicToneMapping;
renderer.toneMappingExposure = VIEWER_CONFIG.scene.exposure;

const scene = new THREE.Scene();
scene.background = new THREE.Color(VIEWER_CONFIG.scene.backgroundColor);
scene.fog = new THREE.FogExp2(VIEWER_CONFIG.scene.backgroundColor, VIEWER_CONFIG.scene.fogDensity);

const camera = new THREE.PerspectiveCamera(50, 1, 0.1, 2000);
camera.position.set(48, 42, 64);

const controls = new OrbitControls(camera, canvas);
controls.enableDamping = true;
controls.dampingFactor = 0.08;

scene.add(new THREE.HemisphereLight(0xdff0ff, 0x3b4438, 1.05));

const sunLight = new THREE.DirectionalLight(0xfff6e5, Number(optSun.value));
sunLight.position.set(80, 140, 60);
scene.add(sunLight);

const fillLight = new THREE.DirectionalLight(0x9bc4ff, 0.45);
fillLight.position.set(-70, 40, -60);
scene.add(fillLight);

const worldGroup = new THREE.Group();
scene.add(worldGroup);

// ---------------------------------------------------------------------------
// Viewer State (S1 Scene Package OR S2 Asset Cache + Save Directory)
// ---------------------------------------------------------------------------

let wasmModule = null;
let activeScenePkg = null;
let activeCache = null;
let activeSaveWorld = null;
let activeMode = null; // 'scene' | 'save' | null
let indexedSaveSource = null;

let activeAnimationUniforms = [];
let sceneRadius = 50;
let sceneCenter = new THREE.Vector3(0, 16, 0);
let rafId = 0;
let isPagePaused = false;
let skinFetchGeneration = 0;
const clock = new THREE.Clock();
const fetchedSkinCache = new Map();

function resizeViewport() {
  const rect = dropZone.getBoundingClientRect();
  const w = Math.max(1, Math.floor(rect.width));
  const h = Math.max(1, Math.floor(rect.height));
  renderer.setSize(w, h, false);
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
}

window.addEventListener('resize', resizeViewport);
resizeViewport();

function tick() {
  if (isPagePaused) {
    rafId = 0;
    return;
  }
  const elapsed = clock.getElapsedTime();
  for (const u of activeAnimationUniforms) {
    u.value = optAnim.checked ? elapsed : 0.0;
  }
  controls.update();
  renderer.render(scene, camera);
  rafId = requestAnimationFrame(tick);
}

function startLoop() {
  if (!rafId && !isPagePaused) {
    rafId = requestAnimationFrame(tick);
  }
}

document.addEventListener('visibilitychange', () => {
  isPagePaused = document.hidden;
  if (!isPagePaused) {
    clock.getDelta();
    startLoop();
  }
});

function activeTextureReader() {
  if (activeMode === 'save' && activeSaveWorld) return activeSaveWorld;
  if (activeMode === 'scene' && activeScenePkg) return activeScenePkg;
  return null;
}

// ---------------------------------------------------------------------------
// Scene Disposal & Rebuild
// ---------------------------------------------------------------------------

function clearWorld() {
  activeAnimationUniforms = [];
  while (worldGroup.children.length > 0) {
    const child = worldGroup.children[0];
    worldGroup.remove(child);
    if (child.geometry) child.geometry.dispose();
    if (child.material) {
      const mats = Array.isArray(child.material) ? child.material : [child.material];
      for (const m of mats) {
        for (const tex of m.userData?.mtkTextures || []) {
          tex.dispose();
        }
        m.dispose();
      }
    }
  }
}

function fitCameraToWorld() {
  const box = new THREE.Box3().setFromObject(worldGroup);
  if (box.isEmpty()) return;
  const sphere = new THREE.Sphere();
  box.getBoundingSphere(sphere);
  sceneCenter.copy(sphere.center);
  sceneRadius = Math.max(sphere.radius, 8);
  controls.target.copy(sceneCenter);
  camera.position.set(
    sceneCenter.x + sceneRadius * 1.15,
    sceneCenter.y + sceneRadius * 0.72,
    sceneCenter.z + sceneRadius * 1.15,
  );
  camera.near = Math.max(0.1, sceneRadius * 0.002);
  camera.far = Math.max(1000, sceneRadius * 12);
  camera.updateProjectionMatrix();
  controls.update();
}

function buildMesherConfigFromUi() {
  const cfg = new WasmMesherConfig();
  cfg.enable_ao = optAo.checked;
  cfg.mesh_fluids = optFluids.checked;
  cfg.enable_ctm = optCtm.checked;
  cfg.enable_animated_textures = optAnim.checked;
  cfg.leaves_cull_mode = optLeavesCull.value;
  return cfg;
}

async function rebuildActiveScene({ refitCamera = false } = {}) {
  const source = activeTextureReader();
  if (!source || !wasmModule) return;
  statusBadge.textContent = '正在构建网格…';

  const t0 = performance.now();
  const renderScene = source.build_render_scene(buildMesherConfigFromUi());
  const meshMs = performance.now() - t0;

  clearWorld();

  const batchCount = renderScene.batch_count;
  statBatches.textContent = String(batchCount);
  statTris.textContent = renderScene.total_triangles.toLocaleString();
  statMeshMs.textContent = `${meshMs.toFixed(1)} ms`;

  const texCache = new Map();
  const getTex = async (chunkId, opts) => {
    if (!chunkId) return null;
    if (!texCache.has(chunkId)) {
      texCache.set(chunkId, await loadAtlasTexture(source, chunkId, opts));
    }
    return texCache.get(chunkId);
  };

  for (let i = 0; i < batchCount; i++) {
    const batch = renderScene.take_batch(i);
    const geom = new THREE.BufferGeometry();
    const mem = wasmModule.memory.buffer;

    // Slice once out of WASM linear memory so `batch.free()` and future memory growth stay safe.
    geom.setAttribute(
      'position',
      new THREE.BufferAttribute(new Float32Array(mem, batch.positions_ptr(), batch.positions_len()).slice(), 3),
    );
    geom.setAttribute(
      'normal',
      new THREE.BufferAttribute(new Float32Array(mem, batch.normals_ptr(), batch.normals_len()).slice(), 3),
    );
    geom.setAttribute(
      'uv',
      new THREE.BufferAttribute(new Float32Array(mem, batch.uvs_ptr(), batch.uvs_len()).slice(), 2),
    );
    geom.setAttribute(
      'mtkColor',
      new THREE.BufferAttribute(new Float32Array(mem, batch.colors_ptr(), batch.colors_len()).slice(), 4),
    );
    geom.setAttribute(
      'mtkTintWeights',
      new THREE.BufferAttribute(new Float32Array(mem, batch.tint_weights_ptr(), batch.tint_weights_len()).slice(), 2),
    );
    geom.setIndex(
      new THREE.BufferAttribute(new Uint32Array(mem, batch.indices_ptr(), batch.indices_len()).slice(), 1),
    );

    const hasAnimAttrs = batch.has_animation;
    if (hasAnimAttrs) {
      geom.setAttribute(
        'mtkAnimTiming',
        new THREE.BufferAttribute(new Float32Array(mem, batch.anim_timings_ptr(), batch.anim_timings_len()).slice(), 3),
      );
      geom.setAttribute(
        'mtkAnimFrameSize',
        new THREE.BufferAttribute(new Float32Array(mem, batch.anim_frame_sizes_ptr(), batch.anim_frame_sizes_len()).slice(), 3),
      );
    }

    const isAnimated = batch.is_animated;
    const [albedoTex, overlayTex, normalTex, ormTex, auxTex] = await Promise.all([
      getTex(batch.albedo_chunk_id, { srgb: true, isAnimated }),
      getTex(batch.overlay_chunk_id, { srgb: true, isAnimated }),
      getTex(batch.normal_chunk_id, { srgb: false, isAnimated }),
      getTex(batch.orm_chunk_id, { srgb: false, isAnimated }),
      getTex(batch.aux_chunk_id, { srgb: false, isAnimated }),
    ]);

    const mat = createMtkPbrMaterial({
      albedoTex,
      overlayTex,
      normalTex,
      ormTex,
      auxTex,
      isAnimated,
      hasAnimAttrs,
      materialConfig: VIEWER_CONFIG.material,
      animationUniforms: activeAnimationUniforms,
    });

    const mesh = new THREE.Mesh(geom, mat);
    mesh.renderOrder = isAnimated ? 1 : 0;
    worldGroup.add(mesh);
    batch.free();
  }

  renderScene.free();
  if (refitCamera) fitCameraToWorld();
  resetCamBtn.disabled = batchCount === 0;
  emptyOverlay.classList.add('hidden');
  statusBadge.textContent = '渲染就绪';
}

// ---------------------------------------------------------------------------
// S1: .mtkscene Loading
// ---------------------------------------------------------------------------

async function loadSceneBytes(bytes, label) {
  skinFetchGeneration += 1;
  statusBadge.textContent = `解析 ${label}…`;
  if (activeScenePkg) {
    activeScenePkg.free();
    activeScenePkg = null;
  }

  const t0 = performance.now();
  activeScenePkg = WasmScenePackage.from_bytes(bytes);
  const openMs = performance.now() - t0;
  activeMode = 'scene';

  const meta = JSON.parse(activeScenePkg.metadata_json());
  const mf = meta.manifest;
  statVoxels.textContent = (mf.voxel_count ?? 0).toLocaleString();
  statBounds.textContent = mf.bounds ? `${mf.bounds[3]}×${mf.bounds[4]}×${mf.bounds[5]}` : '—';
  statOpenMs.textContent = `${openMs.toFixed(1)} ms`;
  statSkins.textContent = mf.capabilities?.has_dynamic_entities ? '已内嵌' : '0';

  await rebuildActiveScene({ refitCamera: true });
}

// ---------------------------------------------------------------------------
// S2: .mtkcache + Local Minecraft Save Directory + CORS Skins
// ---------------------------------------------------------------------------

function syncSaveControls() {
  const ready = Boolean(activeCache && indexedSaveSource);
  saveDimSelect.disabled = !indexedSaveSource;
  spawnBoundsBtn.disabled = !indexedSaveSource;
  loadSaveBtn.disabled = !ready;
}

function populateDimensions(dimensions) {
  saveDimSelect.replaceChildren();
  for (const dim of dimensions) {
    const opt = document.createElement('option');
    opt.value = dim;
    opt.textContent = dim;
    saveDimSelect.appendChild(opt);
  }
}

function applySpawnBounds(level) {
  const [sx, , sz] = level?.spawn || [0, 64, 0];
  const r = VIEWER_CONFIG.selection.defaultRadius;
  const { minY, maxY } = defaultHeightRangeForLevel(level);
  boundInputs.minX.value = String(sx - r);
  boundInputs.minY.value = String(minY);
  boundInputs.minZ.value = String(sz - r);
  boundInputs.maxX.value = String(sx + r);
  boundInputs.maxY.value = String(maxY);
  boundInputs.maxZ.value = String(sz + r);
  clearBoundsErrors();
}

function clearBoundsErrors() {
  boundsError.textContent = '';
  for (const input of Object.values(boundInputs)) {
    input.removeAttribute('aria-invalid');
    input.setCustomValidity('');
  }
}

function readValidatedBounds() {
  const parsed = {};
  for (const [key, input] of Object.entries(boundInputs)) {
    const trimmed = input.value.trim();
    if (!/^-?\d+$/.test(trimmed)) {
      input.setCustomValidity('请输入整数坐标');
      input.setAttribute('aria-invalid', 'true');
      boundsError.textContent = `坐标字段 ${key} 必须是整数`;
      input.focus();
      return null;
    }
    input.setCustomValidity('');
    input.removeAttribute('aria-invalid');
    parsed[key] = Number.parseInt(trimmed, 10);
  }
  boundsError.textContent = '';
  return {
    minX: Math.min(parsed.minX, parsed.maxX),
    minY: Math.min(parsed.minY, parsed.maxY),
    minZ: Math.min(parsed.minZ, parsed.maxZ),
    maxX: Math.max(parsed.minX, parsed.maxX),
    maxY: Math.max(parsed.minY, parsed.maxY),
    maxZ: Math.max(parsed.minZ, parsed.maxZ),
  };
}

async function loadCacheBytes(bytes, label) {
  skinFetchGeneration += 1;
  statusBadge.textContent = `加载缓存 ${label}…`;
  if (activeSaveWorld) {
    activeSaveWorld.free();
    activeSaveWorld = null;
  }
  if (activeCache) {
    activeCache.free();
    activeCache = null;
  }

  const t0 = performance.now();
  activeCache = WasmAssetCache.from_bytes(bytes);
  const openMs = performance.now() - t0;

  const meta = JSON.parse(activeCache.metadata_json());
  const mf = meta.manifest;
  statCache.textContent = `${mf.fingerprint ? mf.fingerprint.slice(0, 14) : label} (v${mf.format_version})`;
  statOpenMs.textContent = `${openMs.toFixed(1)} ms`;
  saveDetails.open = true;
  syncSaveControls();

  if (indexedSaveSource) {
    await executeSaveSelection({ refitCamera: true });
  } else {
    statusBadge.textContent = '缓存已就绪，请选择存档目录';
  }
}

async function indexSaveEntries(entries, folderLabel) {
  skinFetchGeneration += 1;
  statusBadge.textContent = `索引存档 ${folderLabel}…`;
  const normalized = normalizeSaveFileList(entries);
  const levelBytes = await normalized.readFile('level.dat');
  const inspect = JSON.parse(WasmSaveWorld.inspect_level(levelBytes, normalized.relPaths));

  indexedSaveSource = {
    rootName: folderLabel || inspect.level.level_name,
    relPaths: normalized.relPaths,
    readFile: normalized.readFile,
    inspect,
  };

  statSave.textContent = `${inspect.level.level_name} (${inspect.level.version_name})`;
  populateDimensions(inspect.dimensions);
  applySpawnBounds(inspect.level);
  saveDetails.open = true;
  syncSaveControls();

  if (activeCache) {
    await executeSaveSelection({ refitCamera: true });
  } else {
    statusBadge.textContent = `存档已索引 (${inspect.level.level_name})，请加载 .mtkcache`;
  }
}

async function executeSaveSelection({ refitCamera = true } = {}) {
  if (!activeCache || !indexedSaveSource) return;
  const bounds = readValidatedBounds();
  if (!bounds) return;

  const gen = ++skinFetchGeneration;
  const dimension = saveDimSelect.value || 'overworld';
  statusBadge.textContent = '规划并读取区块…';

  const t0 = performance.now();
  if (!activeSaveWorld) {
    activeSaveWorld = new WasmSaveWorld(activeCache);
  }

  const plan = JSON.parse(
    WasmSaveWorld.plan_region_files(
      dimension,
      bounds.minX,
      bounds.minY,
      bounds.minZ,
      bounds.maxX,
      bounds.maxY,
      bounds.maxZ,
      indexedSaveSource.relPaths,
    ),
  );

  const regionBlobs = await Promise.all(
    plan.regions.map(async (r) => ({
      rx: r.rx,
      rz: r.rz,
      data: await indexedSaveSource.readFile(r.path),
    })),
  );
  const extBlobs = await Promise.all(
    plan.external_chunks.map(async (c) => ({
      cx: c.cx,
      cz: c.cz,
      data: await indexedSaveSource.readFile(c.path),
    })),
  );

  for (const [key, bytes] of fetchedSkinCache.entries()) {
    if (bytes) {
      activeSaveWorld.register_skin_png(key, bytes);
    }
  }

  const summary = JSON.parse(
    activeSaveWorld.load_regions(
      regionBlobs,
      extBlobs,
      bounds.minX,
      bounds.minY,
      bounds.minZ,
      bounds.maxX,
      bounds.maxY,
      bounds.maxZ,
    ),
  );
  const openMs = performance.now() - t0;

  activeMode = 'save';
  statVoxels.textContent = (summary.voxel_count ?? 0).toLocaleString();
  statBounds.textContent = summary.bounds
    ? `${summary.bounds[3]}×${summary.bounds[4]}×${summary.bounds[5]}`
    : '—';
  statOpenMs.textContent = `${openMs.toFixed(1)} ms`;

  await rebuildActiveScene({ refitCamera });

  const pending = JSON.parse(activeSaveWorld.pending_skin_requests_json());
  if (pending.length === 0) {
    statSkins.textContent = '0 (无需拉取)';
  } else if (!optSkinsNet.checked) {
    statSkins.textContent = `0/${pending.length} (已禁用联网)`;
  } else {
    void resolveSkinsInBackground({
      saveWorld: activeSaveWorld,
      pending,
      skinCache: fetchedSkinCache,
      skinConfig: VIEWER_CONFIG.skins,
      isCurrent: () => gen === skinFetchGeneration && activeMode === 'save' && Boolean(activeSaveWorld),
      onProgress: ({ resolved, completed, total, done }) => {
        statSkins.textContent = done
          ? `${resolved}/${total} 已加载`
          : `${resolved}/${completed} (共 ${total})`;
      },
    }).then((resolved) => {
      if (resolved > 0 && gen === skinFetchGeneration && activeMode === 'save') {
        return rebuildActiveScene({ refitCamera: false });
      }
      return undefined;
    });
  }
}

// ---------------------------------------------------------------------------
// UI Event Bindings & URL Parameters (?scene= / ?cache= / ?save=)
// ---------------------------------------------------------------------------

sceneFileInput.addEventListener('change', async (e) => {
  const file = e.target.files?.[0];
  sceneFileInput.value = '';
  if (!file) return;
  try {
    await loadSceneBytes(new Uint8Array(await file.arrayBuffer()), file.name);
  } catch (err) {
    statusBadge.textContent = `加载失败: ${err}`;
    console.error(err);
  }
});

cacheFileInput.addEventListener('change', async (e) => {
  const file = e.target.files?.[0];
  cacheFileInput.value = '';
  if (!file) return;
  try {
    await loadCacheBytes(new Uint8Array(await file.arrayBuffer()), file.name);
  } catch (err) {
    statusBadge.textContent = `缓存加载失败: ${err}`;
    console.error(err);
  }
});

saveDirBtn.addEventListener('click', async () => {
  if (typeof window.showDirectoryPicker === 'function') {
    try {
      const handle = await window.showDirectoryPicker({ mode: 'read' });
      const entries = await collectFromDirectoryHandle(handle);
      await indexSaveEntries(entries, handle.name);
      return;
    } catch (err) {
      if (err?.name === 'AbortError') return;
      console.warn('showDirectoryPicker fallback to webkitdirectory:', err);
    }
  }
  saveDirInput.click();
});

saveDirInput.addEventListener('change', async (e) => {
  const files = Array.from(e.target.files || []);
  saveDirInput.value = '';
  if (files.length === 0) return;
  try {
    const folderName = (files[0].webkitRelativePath || '').split('/')[0] || '存档目录';
    const entries = files.map((file) => [
      file.webkitRelativePath || file.name,
      async () => new Uint8Array(await file.arrayBuffer()),
    ]);
    await indexSaveEntries(entries, folderName);
  } catch (err) {
    statusBadge.textContent = `存档读取失败: ${err}`;
    console.error(err);
  }
});

for (const input of Object.values(boundInputs)) {
  input.addEventListener('input', () => {
    input.setCustomValidity('');
    input.removeAttribute('aria-invalid');
    boundsError.textContent = '';
  });
}

spawnBoundsBtn.addEventListener('click', () => {
  if (indexedSaveSource?.inspect?.level) {
    applySpawnBounds(indexedSaveSource.inspect.level);
  }
});

saveForm.addEventListener('submit', (e) => {
  e.preventDefault();
  executeSaveSelection({ refitCamera: true }).catch((err) => {
    statusBadge.textContent = `选区载入失败: ${err}`;
    console.error(err);
  });
});

dropZone.addEventListener('dragover', (e) => {
  e.preventDefault();
  dropZone.classList.add('drag-active');
});

dropZone.addEventListener('dragleave', () => {
  dropZone.classList.remove('drag-active');
});

dropZone.addEventListener('drop', async (e) => {
  e.preventDefault();
  dropZone.classList.remove('drag-active');

  try {
    const items = Array.from(e.dataTransfer?.items || []);
    const dirEntries = [];
    let droppedFile = null;

    for (const item of items) {
      const webkitEntry = item.webkitGetAsEntry?.();
      if (webkitEntry?.isDirectory) {
        dirEntries.push({
          name: webkitEntry.name,
          entries: await collectFromWebkitEntry(webkitEntry, ''),
        });
      } else if (item.kind === 'file') {
        const f = item.getAsFile();
        if (f) droppedFile = f;
      }
    }

    if (dirEntries.length > 0) {
      await indexSaveEntries(dirEntries[0].entries, dirEntries[0].name);
      return;
    }

    const file = droppedFile || e.dataTransfer?.files?.[0];
    if (!file) return;
    const bytes = new Uint8Array(await file.arrayBuffer());
    if (file.name.toLowerCase().endsWith('.mtkcache')) {
      await loadCacheBytes(bytes, file.name);
    } else {
      await loadSceneBytes(bytes, file.name);
    }
  } catch (err) {
    statusBadge.textContent = `拖放加载失败: ${err}`;
    console.error(err);
  }
});

for (const el of [optAo, optFluids, optCtm, optAnim, optLeavesCull]) {
  el.addEventListener('change', () => {
    rebuildActiveScene({ refitCamera: false }).catch((err) => {
      statusBadge.textContent = `重建失败: ${err}`;
      console.error(err);
    });
  });
}

optSun.addEventListener('input', () => {
  const v = Number(optSun.value);
  sunLight.intensity = v;
  sunVal.textContent = v.toFixed(1);
});

resetCamBtn.addEventListener('click', fitCameraToWorld);

// ---------------------------------------------------------------------------
// Bootstrap WASM
// ---------------------------------------------------------------------------

async function loadRemoteSaveIndex(indexUrl, boundsParam) {
  const { name, entries } = await fetchRemoteSaveEntries(indexUrl);
  const hadCache = activeCache;
  if (boundsParam && hadCache) {
    const parts = boundsParam.split(',').map((s) => Number.parseInt(s.trim(), 10));
    if (parts.length === 6 && parts.every(Number.isFinite)) {
      const prevCache = activeCache;
      activeCache = null;
      await indexSaveEntries(entries, name);
      activeCache = prevCache;
      boundInputs.minX.value = String(parts[0]);
      boundInputs.minY.value = String(parts[1]);
      boundInputs.minZ.value = String(parts[2]);
      boundInputs.maxX.value = String(parts[3]);
      boundInputs.maxY.value = String(parts[4]);
      boundInputs.maxZ.value = String(parts[5]);
      syncSaveControls();
      await executeSaveSelection({ refitCamera: true });
      return;
    }
  }
  await indexSaveEntries(entries, name);
}

async function bootstrap() {
  try {
    wasmModule = await init();
    init_panic_hook();
    statusBadge.textContent = `WASM v${version()} 就绪`;
    syncSaveControls();
    startLoop();

    const params = new URLSearchParams(window.location.search);
    const cacheUrl = params.get('cache');
    const saveUrl = params.get('save');
    const boundsParam = params.get('bounds');
    const sceneUrl = params.get('scene');

    if (cacheUrl) {
      const resp = await fetch(cacheUrl);
      if (!resp.ok) throw new Error(`HTTP ${resp.status} for ${cacheUrl}`);
      await loadCacheBytes(new Uint8Array(await resp.arrayBuffer()), cacheUrl.split('/').pop() || cacheUrl);
    }

    if (saveUrl) {
      await loadRemoteSaveIndex(saveUrl, boundsParam);
    }

    if (sceneUrl) {
      const resp = await fetch(sceneUrl);
      if (!resp.ok) throw new Error(`HTTP ${resp.status} for ${sceneUrl}`);
      await loadSceneBytes(new Uint8Array(await resp.arrayBuffer()), sceneUrl);
    }
  } catch (err) {
    statusBadge.textContent = `初始化失败: ${err}`;
    console.error(err);
  }
}

bootstrap();
