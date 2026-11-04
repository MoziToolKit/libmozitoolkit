use crate::identifier::{ResourceLocation, DEFAULT_NAMESPACE};
use glam::IVec3;
use mtk_core::direction::{DirMask, Direction};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

/// Symmetry modes for random CTM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CtmSymmetry {
    #[default]
    None,
    Opposite,
    All,
}

impl CtmSymmetry {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "opposite" => CtmSymmetry::Opposite,
            "all" => CtmSymmetry::All,
            _ => CtmSymmetry::None,
        }
    }
}

/// The CTM connection algorithm method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CtmMethod {
    /// 47-tile full connected textures
    Full { inner_seams: bool },
    /// 5-tile compact connected textures
    Compact { inner_seams: bool },
    /// 4-tile horizontal connection (e.g. bookshelves, logs)
    Horizontal,
    /// 4-tile vertical connection (e.g. pillars, pillars)
    Vertical,
    /// Grid horizontal + vertical connection
    HorizontalVertical,
    /// Grid vertical + horizontal connection
    VerticalHorizontal,
    /// Top/bottom capping connection
    Top,
    /// NxM large texture repeat pattern
    Repeat { width: u32, height: u32 },
    /// Weighted pseudo-random variant selection
    Random {
        weights: Vec<f32>,
        total_weight: f32,
        symmetry: CtmSymmetry,
        linked: bool,
    },
    /// Direct fixed texture replacement
    Fixed,
    /// Overlay alpha-transition edge blending
    Overlay,
}

/// Connection logic determining which neighboring blocks connect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConnectLogic {
    SameBlock,
    SameTile,
    SameState,
    BlockNames(Vec<String>),
    Textures(Vec<ResourceLocation>),
}

/// A block pattern match with optional BlockState property constraints (e.g. `grass_block:snowy=false`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockMatch {
    pub block: String,
    pub properties: HashMap<String, Vec<String>>,
}

impl BlockMatch {
    /// Parse a block match token, preserving context namespace if not explicitly prefixed.
    pub fn parse(token: &str) -> Self {
        Self::parse_with_namespace(token, DEFAULT_NAMESPACE)
    }

    /// Parse a block match token with explicit fallback context namespace.
    pub fn parse_with_namespace(token: &str, default_ns: &str) -> Self {
        let parts: Vec<&str> = token.split(':').collect();
        let mut block_ns = default_ns;
        let mut block_name = parts[0];
        let mut prop_start = 1;

        if parts.len() > 1 && !parts[1].contains('=') {
            block_ns = parts[0];
            block_name = parts[1];
            prop_start = 2;
        }

        let canonical_name = format!("{}:{}", block_ns, block_name);
        let mut properties = HashMap::new();

        for &prop_token in &parts[prop_start..] {
            if let Some((k, v)) = prop_token.split_once('=') {
                let values: Vec<String> = v.split(',').map(|s| s.trim().to_string()).collect();
                properties.insert(k.trim().to_string(), values);
            }
        }

        Self {
            block: canonical_name,
            properties,
        }
    }

    /// Tests if a block state string (e.g. `minecraft:grass_block[snowy=false]`) satisfies this match condition.
    pub fn matches(&self, state: &str) -> bool {
        let block_name = extract_block_name(state);
        let canonical_name = if block_name.contains(':') {
            block_name.to_string()
        } else {
            format!("{}:{}", DEFAULT_NAMESPACE, block_name)
        };
        if self.block != canonical_name {
            return false;
        }
        if self.properties.is_empty() {
            return true;
        }
        if let Some(prop_start) = state.find('[') {
            let prop_str = state[prop_start + 1..]
                .strip_suffix(']')
                .unwrap_or(&state[prop_start + 1..]);
            for part in prop_str.split(',') {
                if let Some((k, v)) = part.split_once('=') {
                    let k = k.trim();
                    let v = v.trim();
                    if let Some(allowed_vals) = self.properties.get(k) {
                        if !allowed_vals.iter().any(|val| val == v) {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }
}

/// Fully parsed OptiFine / Continuity CTM rule definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CtmRule {
    pub name: String,
    pub source_path: String,
    pub priority: i32,
    pub method: CtmMethod,
    /// Ordered list of referenced sprite tiles. None represents `<skip>` or `<default>`.
    pub tiles: Vec<Option<ResourceLocation>>,
    pub match_blocks: Vec<BlockMatch>,
    pub match_tiles: Vec<ResourceLocation>,
    pub connect_logic: ConnectLogic,
    pub faces: DirMask,
    pub biomes: Option<Vec<String>>,
    pub height_ranges: Option<Vec<(i32, i32)>>,
    pub tint_index: Option<i32>,
    pub tint_block: Option<String>,
}

impl CtmRule {
    /// Checks whether this rule could potentially match any of the given block states or base texture identifiers.
    pub fn applies_to_scene(
        &self,
        blockstates: &BTreeSet<String>,
        textures: &BTreeSet<String>,
    ) -> bool {
        if !self.match_blocks.is_empty() {
            for state in blockstates {
                if self.match_blocks.iter().any(|m| m.matches(state)) {
                    return true;
                }
            }
        }
        if !self.match_tiles.is_empty() {
            for tex in textures {
                let loc = ResourceLocation::parse_texture_path(tex)
                    .or_else(|_| ResourceLocation::parse(tex))
                    .unwrap_or_else(|_| ResourceLocation::new(DEFAULT_NAMESPACE, tex));
                if self.match_tiles.contains(&loc) {
                    return true;
                }
            }
        }
        false
    }

    /// Checks if this rule applies to the given block state, face, world coordinates, and biome.
    pub fn matches_block(
        &self,
        state: &str,
        face: Direction,
        world_pos: IVec3,
        base_tile: Option<&ResourceLocation>,
        biome: Option<&str>,
    ) -> bool {
        if !self.faces.contains_dir(face) {
            return false;
        }
        if let Some(ref heights) = self.height_ranges {
            let y = world_pos.y;
            if !heights.iter().any(|&(min, max)| y >= min && y <= max) {
                return false;
            }
        }
        if let Some(ref biomes) = self.biomes {
            if let Some(b) = biome {
                let canonical_b = if b.contains(':') {
                    b.to_string()
                } else {
                    format!("{}:{}", DEFAULT_NAMESPACE, b)
                };
                if !biomes.contains(&canonical_b) {
                    return false;
                }
            }
        }
        if !self.match_blocks.is_empty() && !self.match_blocks.iter().any(|m| m.matches(state)) {
            return false;
        }
        if !self.match_tiles.is_empty() {
            match base_tile {
                Some(bt) => {
                    if !self.match_tiles.contains(bt) {
                        return false;
                    }
                }
                None => return false,
            }
        }
        true
    }
}

/// Helper to extract the block identifier before any property brackets `[...]`.
#[inline]
pub fn extract_block_name(state: &str) -> &str {
    if let Some(idx) = state.find('[') {
        &state[..idx]
    } else {
        state
    }
}

/// Computes the canonical tangent `(up, left)` directions for a standard cardinal cube face.
/// Re-exports and delegates to `mtk_core::Direction::canonical_face_tangents` as SSOT.
#[inline]
pub const fn get_face_tangents(face: Direction) -> (Direction, Direction) {
    face.canonical_face_tangents()
}

/// Deterministic 3D spatial pseudorandom number generator matching standard Minecraft / OptiFine.
#[inline]
pub fn coordinate_random(x: i32, y: i32, z: i32) -> f32 {
    let mut l = (x as i64).wrapping_mul(3129871) ^ (z as i64).wrapping_mul(116129781) ^ (y as i64);
    l = l
        .wrapping_mul(l)
        .wrapping_mul(42317861)
        .wrapping_add(l.wrapping_mul(11));
    let hash = (l >> 16) as u32;
    (hash & 0x00ff_ffff) as f32 / 16777216.0
}
