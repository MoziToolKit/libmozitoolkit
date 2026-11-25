import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import init, {
  init_panic_hook,
  version,
  WasmAssetCache,
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

const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, powerPreference: 'high-performance' });
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
renderer.outputColorSpace = THREE.SRGBColorSpace;
renderer.toneMapping = THREE.ACESFilmicToneMapping;
renderer.toneMappingExposure = 1.05;

const scene = new THREE.Scene();
scene.background = new THREE.Color(0x9ec5e8);
scene.fog = new THREE.FogExp2(0x9ec5e8, 0.0022);

const camera = new THREE.PerspectiveCamera(50, 1, 0.1, 2000);
camera.position.set(48, 42, 64);

const controls = new OrbitControls(camera, canvas);
controls.enableDamping = true;
controls.dampingFactor = 0.08;

const hemiLight = new THREE.HemisphereLight(0xdff0ff, 0x3b4438, 1.05);
scene.add(hemiLight);

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
let indexedSaveSource = null; // { rootName, relPaths, levelInfo, readFile(relPath): Promise<Uint8Array> }

let activeAnimationUniforms = [];
let sceneRadius = 50;
let sceneCenter = new THREE.Vector3(0, 16, 0);
let rafId = 0;
let isPagePaused = false;
let skinFetchGeneration = 0;
const clock = new THREE.Clock();
const fetchedSkinCache = new Map(); // key -> Uint8Array | null

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
// Texture Decoding via Browser createImageBitmap (No WASM Pixel Copy)
// ---------------------------------------------------------------------------

async function loadAtlasTexture(reader, chunkId, { srgb, isAnimated }) {
  if (!chunkId) return null;
  const pngBytes = reader.read_texture_png(chunkId);
  const blob = new Blob([pngBytes], { type: 'image/png' });
  const bitmap = await createImageBitmap(blob, {
    imageOrientation: 'flipY',
    colorSpaceConversion: 'none',
    premultiplyAlpha: 'none',
  });

  const tex = new THREE.Texture(bitmap);
  tex.flipY = false;
  tex.colorSpace = srgb ? THREE.SRGBColorSpace : THREE.NoColorSpace;
  tex.wrapS = THREE.ClampToEdgeWrapping;
  tex.wrapT = THREE.ClampToEdgeWrapping;
  tex.magFilter = THREE.NearestFilter;
  if (isAnimated) {
    // Strip atlases pack frames vertically; mipmaps would bleed adjacent frames.
    tex.minFilter = THREE.NearestFilter;
    tex.generateMipmaps = false;
  } else {
    tex.minFilter = THREE.NearestMipmapLinearFilter;
    tex.generateMipmaps = true;
  }
  tex.needsUpdate = true;
  return tex;
}

// ---------------------------------------------------------------------------
// Blender MtkPackedV1 + MC_Biome_Tint + MC_Animated_UV_Mapping Shader Material
// ---------------------------------------------------------------------------

function createMtkPbrMaterial({ albedoTex, overlayTex, normalTex, ormTex, auxTex, isAnimated, hasAnimAttrs }) {
  const mat = new THREE.MeshStandardMaterial({
    map: albedoTex,
    normalMap: normalTex || null,
    roughness: ormTex ? 1.0 : 0.72,
    metalness: ormTex ? 1.0 : 0.0,
    alphaTest: isAnimated ? 0.05 : 0.45,
    transparent: isAnimated,
    side: isAnimated ? THREE.DoubleSide : THREE.FrontSide,
    depthWrite: true,
  });

  mat.userData.mtkTextures = [albedoTex, overlayTex, normalTex, ormTex, auxTex].filter(Boolean);

  const timeUniform = { value: 0.0 };
  if (hasAnimAttrs) {
    activeAnimationUniforms.push(timeUniform);
  }

  const atlasW = albedoTex?.image?.width ?? 512;
  const atlasH = albedoTex?.image?.height ?? 512;

  mat.customProgramCacheKey = () => (hasAnimAttrs ? 'mtk-pbr-anim' : 'mtk-pbr-static');

  mat.onBeforeCompile = (shader) => {
    shader.uniforms.uTime = timeUniform;
    shader.uniforms.uAtlasSize = { value: new THREE.Vector2(atlasW, atlasH) };
    shader.uniforms.uOverlayMap = { value: overlayTex };
    shader.uniforms.uOrmMap = { value: ormTex };
    shader.uniforms.uAuxMap = { value: auxTex };
    shader.uniforms.uHasOverlay = { value: overlayTex ? 1.0 : 0.0 };
    shader.uniforms.uHasOrm = { value: ormTex ? 1.0 : 0.0 };
    shader.uniforms.uHasAux = { value: auxTex ? 1.0 : 0.0 };
    shader.uniforms.uEmissionStrength = { value: 3.5 };

    const animAttrDecl = hasAnimAttrs
      ? `attribute vec3 mtkAnimTiming;
        attribute vec3 mtkAnimFrameSize;
        varying vec3 vAnimTiming;
        varying vec3 vAnimFrameSize;`
      : '';
    const animAttrAssign = hasAnimAttrs
      ? `vAnimTiming = mtkAnimTiming;
        vAnimFrameSize = mtkAnimFrameSize;`
      : '';
    const animFragHeader = hasAnimAttrs
      ? `varying vec3 vAnimTiming;
        varying vec3 vAnimFrameSize;`
      : '';
    const animFragBody = hasAnimAttrs
      ? `if (vAnimTiming.x <= 1.0) {
            currUv = baseUv;
            nextUv = baseUv;
            blendFac = 0.0;
            return;
          }
          float totalFrames = max(vAnimTiming.x, 1.0);
          float frametime = max(vAnimTiming.y, 1.0);
          float interp = vAnimTiming.z;
          float phase = (uTime * 20.0) / frametime;
          float currFrame = mod(floor(phase), totalFrames);
          float nextFrame = mod(currFrame + 1.0, totalFrames);
          blendFac = fract(phase) * interp;
          float stepV = max(vAnimFrameSize.y, 1.0) / max(uAtlasSize.y, 1.0);
          currUv = vec2(baseUv.x, baseUv.y - currFrame * stepV);
          nextUv = vec2(baseUv.x, baseUv.y - nextFrame * stepV);`
      : `currUv = baseUv;
          nextUv = baseUv;
          blendFac = 0.0;`;

    shader.vertexShader = shader.vertexShader
      .replace(
        '#include <common>',
        `#include <common>
        attribute vec4 mtkColor;
        attribute vec2 mtkTintWeights;
        varying vec4 vMtkColor;
        varying vec2 vTintWeights;
        varying vec2 vMtkUv;
        ${animAttrDecl}`,
      )
      .replace(
        '#include <begin_vertex>',
        `#include <begin_vertex>
        vMtkColor = mtkColor;
        vTintWeights = mtkTintWeights;
        vMtkUv = uv;
        ${animAttrAssign}`,
      );

    shader.fragmentShader = shader.fragmentShader
      .replace(
        '#include <common>',
        `#include <common>
        uniform float uTime;
        uniform vec2 uAtlasSize;
        uniform sampler2D uOverlayMap;
        uniform sampler2D uOrmMap;
        uniform sampler2D uAuxMap;
        uniform float uHasOverlay;
        uniform float uHasOrm;
        uniform float uHasAux;
        uniform float uEmissionStrength;
        varying vec4 vMtkColor;
        varying vec2 vTintWeights;
        varying vec2 vMtkUv;
        ${animFragHeader}

        void computeMtkAnimUvs(in vec2 baseUv, out vec2 currUv, out vec2 nextUv, out float blendFac) {
          ${animFragBody}
        }`,
      )
      .replace(
        '#include <map_fragment>',
        `vec2 mtkCurrUv;
        vec2 mtkNextUv;
        float mtkAnimBlend;
        computeMtkAnimUvs(vMtkUv, mtkCurrUv, mtkNextUv, mtkAnimBlend);

        #ifdef USE_MAP
          vec4 baseSample = texture2D(map, mtkCurrUv);
          if (mtkAnimBlend > 0.001) {
            vec4 nextSample = texture2D(map, mtkNextUv);
            baseSample = mix(baseSample, nextSample, mtkAnimBlend);
          }
        #else
          vec4 baseSample = vec4(1.0);
        #endif

        vec4 overlaySample = vec4(0.0);
        if (uHasOverlay > 0.5) {
          overlaySample = texture2D(uOverlayMap, mtkCurrUv);
          if (mtkAnimBlend > 0.001) {
            vec4 nextOv = texture2D(uOverlayMap, mtkNextUv);
            overlaySample = mix(overlaySample, nextOv, mtkAnimBlend);
          }
        }

        // MC_Biome_Tint: apply tint independently to base & overlay, then composite
        vec3 baseTint = mix(vec3(1.0), vMtkColor.rgb, vTintWeights.x);
        vec3 overlayTint = mix(vec3(1.0), vMtkColor.rgb, vTintWeights.y);
        vec3 tintedBase = baseSample.rgb * baseTint;
        vec3 tintedOverlay = overlaySample.rgb * overlayTint;
        vec3 compositedAlbedo = mix(tintedBase, tintedOverlay, overlaySample.a);
        float compositedAlpha = max(baseSample.a, overlaySample.a);

        float texAo = 1.0;
        float ormRough = 1.0;
        float ormMetal = 0.0;
        float ormEmit = 0.0;
        if (uHasOrm > 0.5) {
          vec4 ormSample = texture2D(uOrmMap, mtkCurrUv);
          if (mtkAnimBlend > 0.001) {
            vec4 nextOrm = texture2D(uOrmMap, mtkNextUv);
            ormSample = mix(ormSample, nextOrm, mtkAnimBlend);
          }
          texAo = ormSample.r;
          ormRough = ormSample.g;
          ormMetal = ormSample.b;
          ormEmit = ormSample.a;
        }

        // MtkPackedV1: BaseColor = albedo * orm.R * vertex_ao; Emission uses clean tinted albedo
        diffuseColor.rgb *= compositedAlbedo * texAo * vMtkColor.a;
        diffuseColor.a *= compositedAlpha;`,
      )
      .replace(
        '#include <roughnessmap_fragment>',
        `float roughnessFactor = roughness * ormRough;`,
      )
      .replace(
        '#include <metalnessmap_fragment>',
        `float metalnessFactor = metalness * ormMetal;`,
      )
      .replace(
        '#include <emissivemap_fragment>',
        `totalEmissiveRadiance += compositedAlbedo * ormEmit * uEmissionStrength;`,
      );
  };

  return mat;
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

async function rebuildActiveScene({ refitCamera = false } = {}) {
  const source = activeTextureReader();
  if (!source || !wasmModule) return;
  statusBadge.textContent = '正在构建网格…';

  const t0 = performance.now();
  const renderScene = source.build_render_scene(
    optAo.checked,
    optFluids.checked,
    optCtm.checked,
    optAnim.checked,
  );
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
    });

    const mesh = new THREE.Mesh(geom, mat);
    mesh.renderOrder = isAnimated ? 1 : 0;
    worldGroup.add(mesh);
    batch.free();
  }

  renderScene.free();
  if (refitCamera) {
    fitCameraToWorld();
  }
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

function applySpawnBounds(spawn) {
  const [sx, , sz] = spawn || [0, 64, 0];
  boundInputs.minX.value = String(sx - 64);
  boundInputs.minY.value = '-64';
  boundInputs.minZ.value = String(sz - 64);
  boundInputs.maxX.value = String(sx + 64);
  boundInputs.maxY.value = '320';
  boundInputs.maxZ.value = String(sz + 64);
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

function normalizeSaveFileList(entries) {
  // Normalize slashes and locate the folder prefix that owns `level.dat`.
  const normalized = entries.map(([rawPath, readerFn]) => [
    rawPath.replace(/\\/g, '/').replace(/^\.\//, '').replace(/^\/+/, ''),
    readerFn,
  ]);
  const levelEntry = normalized.find(([p]) => p === 'level.dat' || p.endsWith('/level.dat'));
  if (!levelEntry) {
    throw new Error('所选目录中未找到 level.dat');
  }
  const levelPath = levelEntry[0];
  const prefix = levelPath === 'level.dat' ? '' : levelPath.slice(0, levelPath.length - 'level.dat'.length);

  const fileMap = new Map();
  const relPaths = [];
  for (const [fullPath, readerFn] of normalized) {
    if (prefix && !fullPath.startsWith(prefix)) continue;
    const rel = prefix ? fullPath.slice(prefix.length) : fullPath;
    if (!rel) continue;
    fileMap.set(rel, readerFn);
    relPaths.push(rel);
  }
  return {
    relPaths,
    readFile: (relPath) => {
      const fn = fileMap.get(relPath);
      if (!fn) throw new Error(`Missing save file: ${relPath}`);
      return fn();
    },
  };
}

async function indexSaveEntries(entries, folderLabel) {
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
  applySpawnBounds(inspect.level.spawn);
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

  // Re-apply session-cached skins before loading so first mesh already includes known skins.
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
  } else {
    void resolveSkinsInBackground(pending, gen);
  }
}

function buildCandidateSkinUrls(req) {
  if (req.skin_url) {
    return [req.skin_url];
  }
  const cleanUuid = req.uuid ? req.uuid.replace(/^(uuid_|uuid:)/i, '').replace(/-/g, '').toLowerCase() : '';
  const cleanName = req.name ? req.name.replace(/^(player_|player:)/i, '').trim() : '';
  const isV4Uuid = cleanUuid.length === 32 && cleanUuid[12] === '4';
  const ids = isV4Uuid
    ? [cleanUuid, cleanName].filter(Boolean)
    : [cleanName, cleanUuid].filter(Boolean);

  const urls = [];
  for (const id of ids) {
    const enc = encodeURIComponent(id);
    urls.push(`https://crafthead.net/skin/${enc}`, `https://minotar.net/skin/${enc}`);
  }
  return urls;
}

async function fetchFirstValidSkin(saveWorld, req) {
  if (fetchedSkinCache.has(req.key)) {
    const cached = fetchedSkinCache.get(req.key);
    return cached ? saveWorld.register_skin_png(req.key, cached) : false;
  }

  for (const url of buildCandidateSkinUrls(req)) {
    try {
      const resp = await fetch(url, { mode: 'cors' });
      if (!resp.ok) continue;
      const bytes = new Uint8Array(await resp.arrayBuffer());
      if (saveWorld.register_skin_png(req.key, bytes)) {
        fetchedSkinCache.set(req.key, bytes);
        return true;
      }
    } catch {
      // Try next mirror silently; failed/blocked mirrors are tolerated.
    }
  }

  fetchedSkinCache.set(req.key, null);
  return false;
}

async function resolveSkinsInBackground(pending, gen) {
  const total = pending.length;
  let completed = 0;
  let resolved = 0;
  statSkins.textContent = `拉取中 0/${total}`;

  let cursor = 0;
  const workerCount = Math.min(4, total);
  const workers = Array.from({ length: workerCount }, async () => {
    while (cursor < pending.length) {
      if (gen !== skinFetchGeneration || activeMode !== 'save' || !activeSaveWorld) return;
      const req = pending[cursor++];
      const ok = await fetchFirstValidSkin(activeSaveWorld, req);
      completed += 1;
      if (ok) resolved += 1;
      if (gen === skinFetchGeneration) {
        statSkins.textContent = `${resolved}/${completed} (共 ${total})`;
      }
    }
  });

  await Promise.all(workers);

  if (gen !== skinFetchGeneration || activeMode !== 'save' || !activeSaveWorld) return;
  statSkins.textContent = `${resolved}/${total} 已加载`;
  if (resolved > 0) {
    await rebuildActiveScene({ refitCamera: false });
  }
}

// ---------------------------------------------------------------------------
// Browser File System Helpers (FSA Directory Picker, webkitdirectory, Drop)
// ---------------------------------------------------------------------------

async function collectFromDirectoryHandle(dirHandle, prefix = '') {
  const entries = [];
  for await (const [name, handle] of dirHandle.entries()) {
    const rel = prefix ? `${prefix}/${name}` : name;
    if (handle.kind === 'directory') {
      entries.push(...(await collectFromDirectoryHandle(handle, rel)));
    } else if (handle.kind === 'file') {
      entries.push([
        rel,
        async () => new Uint8Array(await (await handle.getFile()).arrayBuffer()),
      ]);
    }
  }
  return entries;
}

async function collectFromWebkitEntry(entry, prefix = '') {
  const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
  if (entry.isFile) {
    return [
      [
        rel,
        () =>
          new Promise((resolve, reject) => {
            entry.file(
              async (file) => resolve(new Uint8Array(await file.arrayBuffer())),
              reject,
            );
          }),
      ],
    ];
  }
  if (entry.isDirectory) {
    const reader = entry.createReader();
    const batchRead = () =>
      new Promise((resolve, reject) => reader.readEntries(resolve, reject));
    const entries = [];
    while (true) {
      const chunk = await batchRead();
      if (!chunk || chunk.length === 0) break;
      for (const child of chunk) {
        entries.push(...(await collectFromWebkitEntry(child, rel)));
      }
    }
    return entries;
  }
  return [];
}

// ---------------------------------------------------------------------------
// UI Event Bindings & URL Parameters (?scene= / ?cache=)
// ---------------------------------------------------------------------------

sceneFileInput.addEventListener('change', async (e) => {
  const file = e.target.files?.[0];
  if (!file) return;
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    await loadSceneBytes(bytes, file.name);
  } catch (err) {
    statusBadge.textContent = `加载失败: ${err}`;
    console.error(err);
  }
});

cacheFileInput.addEventListener('change', async (e) => {
  const file = e.target.files?.[0];
  if (!file) return;
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    await loadCacheBytes(bytes, file.name);
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
  if (indexedSaveSource?.inspect?.level?.spawn) {
    applySpawnBounds(indexedSaveSource.inspect.level.spawn);
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

for (const el of [optAo, optFluids, optCtm, optAnim]) {
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
  const absIndexUrl = new URL(indexUrl, window.location.href);
  const resp = await fetch(absIndexUrl);
  if (!resp.ok) throw new Error(`HTTP ${resp.status} for ${indexUrl}`);
  const manifest = await resp.json();
  const baseUrl = new URL('./', absIndexUrl);
  const relFiles = Array.isArray(manifest.files) ? manifest.files : [];
  const entries = relFiles.map((relPath) => [
    relPath,
    async () => {
      const fileUrl = new URL(relPath, baseUrl);
      const r = await fetch(fileUrl);
      if (!r.ok) throw new Error(`HTTP ${r.status} for ${relPath}`);
      return new Uint8Array(await r.arrayBuffer());
    },
  ]);

  // Index level.dat & dims first; if bounds override is provided, temporarily hold off auto-load once.
  const hadCache = activeCache;
  if (boundsParam && hadCache) {
    const parts = boundsParam.split(',').map((s) => Number.parseInt(s.trim(), 10));
    if (parts.length === 6 && parts.every(Number.isFinite)) {
      const prevCache = activeCache;
      activeCache = null;
      await indexSaveEntries(entries, manifest.name || 'remote-save');
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
  await indexSaveEntries(entries, manifest.name || 'remote-save');
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
      const bytes = new Uint8Array(await resp.arrayBuffer());
      await loadCacheBytes(bytes, cacheUrl.split('/').pop() || cacheUrl);
    }

    if (saveUrl) {
      await loadRemoteSaveIndex(saveUrl, boundsParam);
    }

    if (sceneUrl) {
      const resp = await fetch(sceneUrl);
      if (!resp.ok) throw new Error(`HTTP ${resp.status} for ${sceneUrl}`);
      const bytes = new Uint8Array(await resp.arrayBuffer());
      await loadSceneBytes(bytes, sceneUrl);
    }
  } catch (err) {
    statusBadge.textContent = `初始化失败: ${err}`;
    console.error(err);
  }
}

bootstrap();
