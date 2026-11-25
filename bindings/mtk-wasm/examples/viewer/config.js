/**
 * Frontend-owned rendering, selection, and network defaults for the Web 3D viewer.
 * Keeping these parameters explicit prevents hardcoding viewer policy in Rust bindings.
 */
export const VIEWER_CONFIG = {
  scene: {
    backgroundColor: 0x9ec5e8,
    fogDensity: 0.0022,
    sunIntensity: 2.4,
    exposure: 1.05,
  },
  material: {
    defaultRoughness: 0.72,
    staticAlphaTest: 0.45,
    animatedAlphaTest: 0.05,
    emissionStrength: 3.5,
    ticksPerSecond: 20.0,
  },
  selection: {
    defaultRadius: 64,
    modernMinY: -64,
    modernMaxY: 319,
    legacyMinY: 0,
    legacyMaxY: 255,
    modernWorldDataVersion: 2844, // 1.18+ (21w37a+) expanded vertical height range
  },
  skins: {
    allowNetwork: true,
    concurrency: 4,
    requestTimeoutMs: 8000,
  },
};

/**
 * Derives default vertical bounds from parsed `level.dat` metadata.
 */
export function defaultHeightRangeForLevel(level) {
  const { modernMinY, modernMaxY, legacyMinY, legacyMaxY, modernWorldDataVersion } =
    VIEWER_CONFIG.selection;
  const isPre118 =
    Boolean(level?.is_pre_flattening) ||
    (Number.isFinite(level?.data_version) && level.data_version > 0 && level.data_version < modernWorldDataVersion);
  return isPre118
    ? { minY: legacyMinY, maxY: legacyMaxY }
    : { minY: modernMinY, maxY: modernMaxY };
}
