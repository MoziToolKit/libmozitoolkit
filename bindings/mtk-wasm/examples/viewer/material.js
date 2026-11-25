import * as THREE from 'three';

/**
 * Decodes a raw PNG chunk from `WasmScenePackage` / `WasmSaveWorld` via browser
 * `createImageBitmap` without pixel-copying inside WASM linear memory.
 */
export async function loadAtlasTexture(reader, chunkId, { srgb, isAnimated }) {
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

/**
 * Creates a Three.js PBR material implementing Blender `MtkPackedV1` +
 * `MC_Biome_Tint` + `MC_Animated_UV_Mapping` with frontend-supplied defaults.
 */
export function createMtkPbrMaterial({
  albedoTex,
  overlayTex,
  normalTex,
  ormTex,
  auxTex,
  isAnimated,
  hasAnimAttrs,
  materialConfig,
  animationUniforms,
}) {
  // Static atlas chunks bundle opaque terrain and cutout foliage together, so they
  // stay opaque with alphaTest cutout; animated chunks (water/portal/etc.) enable blending.
  const mat = new THREE.MeshStandardMaterial({
    map: albedoTex,
    normalMap: normalTex || null,
    roughness: ormTex ? 1.0 : materialConfig.defaultRoughness,
    metalness: ormTex ? 1.0 : 0.0,
    alphaTest: isAnimated ? materialConfig.animatedAlphaTest : materialConfig.staticAlphaTest,
    transparent: isAnimated,
    side: isAnimated ? THREE.DoubleSide : THREE.FrontSide,
    depthWrite: true,
  });

  mat.userData.mtkTextures = [albedoTex, overlayTex, normalTex, ormTex, auxTex].filter(Boolean);

  const timeUniform = { value: 0.0 };
  if (hasAnimAttrs && animationUniforms) {
    animationUniforms.push(timeUniform);
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
    shader.uniforms.uEmissionStrength = { value: materialConfig.emissionStrength };
    shader.uniforms.uTicksPerSecond = { value: materialConfig.ticksPerSecond };

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
          float phase = (uTime * uTicksPerSecond) / frametime;
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
        uniform float uTicksPerSecond;
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
