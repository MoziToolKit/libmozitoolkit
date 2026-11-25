import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import init, { init_panic_hook, version, WasmScenePackage } from './pkg/mtk_wasm.js';

const canvas = document.getElementById('gl-canvas');
const dropZone = document.getElementById('drop-zone');
const emptyOverlay = document.getElementById('empty-overlay');
const fileInput = document.getElementById('file-input');
const statusBadge = document.getElementById('status-badge');
const resetCamBtn = document.getElementById('btn-reset-cam');

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

let wasmModule = null;
let activePackage = null;
let activeMeta = null;
let activeAnimationUniforms = [];
let sceneRadius = 50;
let sceneCenter = new THREE.Vector3(0, 16, 0);
let rafId = 0;
let isPagePaused = false;
const clock = new THREE.Clock();

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

// ---------------------------------------------------------------------------
// Texture Decoding via Browser createImageBitmap (No WASM Pixel Copy)
// ---------------------------------------------------------------------------

async function loadAtlasTexture(pkg, chunkId, { srgb, isAnimated }) {
  if (!chunkId) return null;
  const pngBytes = pkg.read_texture_png(chunkId);
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
        float ormRough = 0.72;
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
        for (const key of ['map', 'normalMap', 'roughnessMap', 'metalnessMap']) {
          if (m[key]) m[key].dispose();
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
  if (!activePackage || !wasmModule) return;
  statusBadge.textContent = '正在构建网格…';
  clearWorld();

  const t0 = performance.now();
  const renderScene = activePackage.build_render_scene(
    optAo.checked,
    optFluids.checked,
    optCtm.checked,
    optAnim.checked,
  );
  const meshMs = performance.now() - t0;

  const batchCount = renderScene.batch_count;
  statBatches.textContent = String(batchCount);
  statTris.textContent = renderScene.total_triangles.toLocaleString();
  statMeshMs.textContent = `${meshMs.toFixed(1)} ms`;

  const texCache = new Map();
  const getTex = async (chunkId, opts) => {
    if (!chunkId) return null;
    if (!texCache.has(chunkId)) {
      texCache.set(chunkId, await loadAtlasTexture(activePackage, chunkId, opts));
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
  resetCamBtn.disabled = false;
  emptyOverlay.classList.add('hidden');
  statusBadge.textContent = '渲染就绪';
}

async function loadSceneBytes(bytes, label) {
  statusBadge.textContent = `解析 ${label}…`;
  if (activePackage) {
    activePackage.free();
    activePackage = null;
  }

  const t0 = performance.now();
  activePackage = WasmScenePackage.from_bytes(bytes);
  const openMs = performance.now() - t0;

  activeMeta = JSON.parse(activePackage.metadata_json());
  const mf = activeMeta.manifest;
  statVoxels.textContent = (mf.voxel_count ?? 0).toLocaleString();
  statBounds.textContent = mf.bounds ? `${mf.bounds[3]}×${mf.bounds[4]}×${mf.bounds[5]}` : '—';
  statOpenMs.textContent = `${openMs.toFixed(1)} ms`;

  await rebuildActiveScene({ refitCamera: true });
}

// ---------------------------------------------------------------------------
// File Input, Drag-and-Drop & Optional ?scene= Auto-Load
// ---------------------------------------------------------------------------

fileInput.addEventListener('change', async (e) => {
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
  const file = e.dataTransfer?.files?.[0];
  if (!file) return;
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    await loadSceneBytes(bytes, file.name);
  } catch (err) {
    statusBadge.textContent = `加载失败: ${err}`;
    console.error(err);
  }
});

for (const el of [optAo, optFluids, optCtm, optAnim]) {
  el.addEventListener('change', () => {
    rebuildActiveScene({ refitCamera: false }).catch(console.error);
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

async function bootstrap() {
  try {
    wasmModule = await init();
    init_panic_hook();
    statusBadge.textContent = `WASM v${version()} 就绪`;
    startLoop();

    const params = new URLSearchParams(window.location.search);
    const sceneUrl = params.get('scene');
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
