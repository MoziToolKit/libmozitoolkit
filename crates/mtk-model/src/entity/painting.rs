//! Procedural geometric model generator and variant registry for Minecraft 1.21+ Paintings.
//!
//! Mirrors `PaintingRenderer`, `PaintingVariant`, and `PaintingVariants` from vanilla Minecraft.
//! Generates exact 1/16 block depth procedural painting frames with segmented quad topology,
//! texture coordinate unrolling, and wooden backing borders.

use std::collections::HashMap;

use glam::{Vec2, Vec3};
use mtk_core::Direction;
use serde::{Deserialize, Serialize};

use crate::baker::baked_model::model::{BakedElement, BakedFace, BakedModel};

/// Total physical thickness of a painting frame in blocks (1/16th block, or 1 pixel).
pub const PAINTING_DEPTH: f32 = 0.0625;

/// Half-depth offset from the origin (1/32th block).
pub const PAINTING_HALF_DEPTH: f32 = 0.03125;

/// Texture coordinate width/height slice for wooden rim edge on `back.png`.
pub const PAINTING_RIM_UV_THICKNESS: f32 = 0.0625;

/// Default texture identifier for the wooden back plate and frame borders.
pub const PAINTING_BACK_TEXTURE: &str = "minecraft:painting/back";

/// Definition of a canonical Minecraft Painting Variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaintingVariantDef {
    /// Canonical variant slug name (e.g. "kebab", "alban").
    pub name: &'static str,
    /// Width in integer blocks (1..=4).
    pub width: u8,
    /// Height in integer blocks (1..=4).
    pub height: u8,
    /// Primary texture resource location (e.g. "minecraft:painting/kebab").
    pub asset_id: &'static str,
    /// English display title.
    pub title: &'static str,
    /// Author / Artist name.
    pub author: Option<&'static str>,
}

/// Static registry of all 51 Painting Variants present in vanilla Minecraft 1.21+.
pub static PAINTING_VARIANTS: [PaintingVariantDef; 51] = [
    // 1x1 (8 variants)
    PaintingVariantDef {
        name: "kebab",
        width: 1,
        height: 1,
        asset_id: "minecraft:painting/kebab",
        title: "Kebab med tre pepperoni",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "aztec",
        width: 1,
        height: 1,
        asset_id: "minecraft:painting/aztec",
        title: "de_aztec",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "alban",
        width: 1,
        height: 1,
        asset_id: "minecraft:painting/alban",
        title: "Albanian",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "aztec2",
        width: 1,
        height: 1,
        asset_id: "minecraft:painting/aztec2",
        title: "de_aztec",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "bomb",
        width: 1,
        height: 1,
        asset_id: "minecraft:painting/bomb",
        title: "Target Successfully Bombed",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "plant",
        width: 1,
        height: 1,
        asset_id: "minecraft:painting/plant",
        title: "Paradisträd",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "wasteland",
        width: 1,
        height: 1,
        asset_id: "minecraft:painting/wasteland",
        title: "Wasteland",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "meditative",
        width: 1,
        height: 1,
        asset_id: "minecraft:painting/meditative",
        title: "Meditative",
        author: Some("Sarah Boeving"),
    },
    // 2x1 (5 variants)
    PaintingVariantDef {
        name: "pool",
        width: 2,
        height: 1,
        asset_id: "minecraft:painting/pool",
        title: "The Pool",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "courbet",
        width: 2,
        height: 1,
        asset_id: "minecraft:painting/courbet",
        title: "Bonjour Monsieur Courbet",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "sea",
        width: 2,
        height: 1,
        asset_id: "minecraft:painting/sea",
        title: "Seaside",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "sunset",
        width: 2,
        height: 1,
        asset_id: "minecraft:painting/sunset",
        title: "sunset_dense",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "creebet",
        width: 2,
        height: 1,
        asset_id: "minecraft:painting/creebet",
        title: "Creebet",
        author: Some("Kristoffer Zetterstrand"),
    },
    // 1x2 (3 variants)
    PaintingVariantDef {
        name: "wanderer",
        width: 1,
        height: 2,
        asset_id: "minecraft:painting/wanderer",
        title: "Wanderer",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "graham",
        width: 1,
        height: 2,
        asset_id: "minecraft:painting/graham",
        title: "Graham",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "prairie_ride",
        width: 1,
        height: 2,
        asset_id: "minecraft:painting/prairie_ride",
        title: "Prairie Ride",
        author: Some("Sarah Boeving"),
    },
    // 2x2 (12 variants)
    PaintingVariantDef {
        name: "match",
        width: 2,
        height: 2,
        asset_id: "minecraft:painting/match",
        title: "Match",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "bust",
        width: 2,
        height: 2,
        asset_id: "minecraft:painting/bust",
        title: "Bust",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "stage",
        width: 2,
        height: 2,
        asset_id: "minecraft:painting/stage",
        title: "The Stage Is Set",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "void",
        width: 2,
        height: 2,
        asset_id: "minecraft:painting/void",
        title: "The void",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "skull_and_roses",
        width: 2,
        height: 2,
        asset_id: "minecraft:painting/skull_and_roses",
        title: "Skull and Roses",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "wither",
        width: 2,
        height: 2,
        asset_id: "minecraft:painting/wither",
        title: "Wither",
        author: Some("Mojang"),
    },
    PaintingVariantDef {
        name: "earth",
        width: 2,
        height: 2,
        asset_id: "minecraft:painting/earth",
        title: "Earth",
        author: Some("Mojang"),
    },
    PaintingVariantDef {
        name: "wind",
        width: 2,
        height: 2,
        asset_id: "minecraft:painting/wind",
        title: "Wind",
        author: Some("Mojang"),
    },
    PaintingVariantDef {
        name: "water",
        width: 2,
        height: 2,
        asset_id: "minecraft:painting/water",
        title: "Water",
        author: Some("Mojang"),
    },
    PaintingVariantDef {
        name: "fire",
        width: 2,
        height: 2,
        asset_id: "minecraft:painting/fire",
        title: "Fire",
        author: Some("Mojang"),
    },
    PaintingVariantDef {
        name: "baroque",
        width: 2,
        height: 2,
        asset_id: "minecraft:painting/baroque",
        title: "Baroque",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "humble",
        width: 2,
        height: 2,
        asset_id: "minecraft:painting/humble",
        title: "Humble",
        author: Some("Sarah Boeving"),
    },
    // 3x3 (9 variants)
    PaintingVariantDef {
        name: "bouquet",
        width: 3,
        height: 3,
        asset_id: "minecraft:painting/bouquet",
        title: "Bouquet",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "cavebird",
        width: 3,
        height: 3,
        asset_id: "minecraft:painting/cavebird",
        title: "Cavebird",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "cotan",
        width: 3,
        height: 3,
        asset_id: "minecraft:painting/cotan",
        title: "Cotán",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "endboss",
        width: 3,
        height: 3,
        asset_id: "minecraft:painting/endboss",
        title: "Endboss",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "fern",
        width: 3,
        height: 3,
        asset_id: "minecraft:painting/fern",
        title: "Fern",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "owlemons",
        width: 3,
        height: 3,
        asset_id: "minecraft:painting/owlemons",
        title: "Owlemons",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "sunflowers",
        width: 3,
        height: 3,
        asset_id: "minecraft:painting/sunflowers",
        title: "Sunflowers",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "tides",
        width: 3,
        height: 3,
        asset_id: "minecraft:painting/tides",
        title: "Tides",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "dennis",
        width: 3,
        height: 3,
        asset_id: "minecraft:painting/dennis",
        title: "Dennis",
        author: Some("Sarah Boeving"),
    },
    // 3x4 (2 variants)
    PaintingVariantDef {
        name: "backyard",
        width: 3,
        height: 4,
        asset_id: "minecraft:painting/backyard",
        title: "Backyard",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "pond",
        width: 3,
        height: 4,
        asset_id: "minecraft:painting/pond",
        title: "Pond",
        author: Some("Sarah Boeving"),
    },
    // 4x2 (5 variants)
    PaintingVariantDef {
        name: "fighters",
        width: 4,
        height: 2,
        asset_id: "minecraft:painting/fighters",
        title: "Fighters",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "changing",
        width: 4,
        height: 2,
        asset_id: "minecraft:painting/changing",
        title: "Changing",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "finding",
        width: 4,
        height: 2,
        asset_id: "minecraft:painting/finding",
        title: "Finding",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "lowmist",
        width: 4,
        height: 2,
        asset_id: "minecraft:painting/lowmist",
        title: "Lowmist",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "passage",
        width: 4,
        height: 2,
        asset_id: "minecraft:painting/passage",
        title: "Passage",
        author: Some("Sarah Boeving"),
    },
    // 4x3 (2 variants)
    PaintingVariantDef {
        name: "skeleton",
        width: 4,
        height: 3,
        asset_id: "minecraft:painting/skeleton",
        title: "Mortal Coil",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "donkey_kong",
        width: 4,
        height: 3,
        asset_id: "minecraft:painting/donkey_kong",
        title: "Kong",
        author: Some("Kristoffer Zetterstrand"),
    },
    // 4x4 (5 variants)
    PaintingVariantDef {
        name: "pointer",
        width: 4,
        height: 4,
        asset_id: "minecraft:painting/pointer",
        title: "Pointer",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "pigscene",
        width: 4,
        height: 4,
        asset_id: "minecraft:painting/pigscene",
        title: "Pigscene",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "burning_skull",
        width: 4,
        height: 4,
        asset_id: "minecraft:painting/burning_skull",
        title: "Skull on Fire",
        author: Some("Kristoffer Zetterstrand"),
    },
    PaintingVariantDef {
        name: "unpacked",
        width: 4,
        height: 4,
        asset_id: "minecraft:painting/unpacked",
        title: "Unpacked",
        author: Some("Sarah Boeving"),
    },
    PaintingVariantDef {
        name: "orb",
        width: 4,
        height: 4,
        asset_id: "minecraft:painting/orb",
        title: "Orb",
        author: Some("Sarah Boeving"),
    },
];

/// Normalizes a painting variant string (stripping "minecraft:", "painting/", etc.).
pub fn normalize_painting_name(name: &str) -> &str {
    let clean = name.strip_prefix("minecraft:").unwrap_or(name);
    clean.strip_prefix("painting/").unwrap_or(clean)
}

/// Resolves a `PaintingVariantDef` by variant name or asset ID.
pub fn get_painting_variant(name: &str) -> Option<&'static PaintingVariantDef> {
    let norm = normalize_painting_name(name);
    PAINTING_VARIANTS
        .iter()
        .find(|v| v.name.eq_ignore_ascii_case(norm))
}

/// Returns the `(width, height)` in integer blocks for a painting variant, defaulting to `(1, 1)`.
pub fn painting_dimension(name: &str) -> (u8, u8) {
    get_painting_variant(name)
        .map(|v| (v.width, v.height))
        .unwrap_or((1, 1))
}

/// Resolves the canonical texture identifier for a painting variant (e.g. "minecraft:painting/kebab").
pub fn painting_texture_id(name: &str) -> String {
    let norm = normalize_painting_name(name);
    format!("minecraft:painting/{}", norm)
}

/// Returns horizontal counter-clockwise direction around +Y (North -> West -> South -> East).
#[inline]
pub fn counter_clockwise(facing: Direction) -> Direction {
    match facing {
        Direction::North => Direction::West,
        Direction::South => Direction::East,
        Direction::West => Direction::South,
        Direction::East => Direction::North,
        _ => facing,
    }
}

/// Calculates the exact world center position for a painting attached to a support block,
/// mirroring `Painting.calculateBoundingBox` in vanilla Minecraft.
///
/// * `support_pos`: The block coordinates of the support/anchor block.
/// * `facing`: Horizontal direction pointing outwards from the wall into open air.
/// * `width`: Painting width in blocks.
/// * `height`: Painting height in blocks.
pub fn calculate_painting_world_position(
    support_pos: [i32; 3],
    facing: Direction,
    width: u8,
    height: u8,
) -> Vec3 {
    let center = Vec3::new(
        support_pos[0] as f32 + 0.5,
        support_pos[1] as f32 + 0.5,
        support_pos[2] as f32 + 0.5,
    );

    // Wall attachment shift: -0.46875 blocks relative to facing (places back on wall surface at 0.5)
    let attached_to_wall = center - facing.normal() * 0.46875;

    // Even dimensions shift by 0.5 to align with block boundaries
    let horizontal_offset = if width % 2 == 0 { 0.5 } else { 0.0 };
    let vertical_offset = if height % 2 == 0 { 0.5 } else { 0.0 };

    let left = counter_clockwise(facing);
    attached_to_wall + left.normal() * horizontal_offset + Vec3::Y * vertical_offset
}

/// Calculates the exact world position and Euler rotation for a painting attached to a support block.
///
/// * `support_pos`: The block coordinates of the support/anchor block.
/// * `facing`: Horizontal direction pointing outwards from the wall into open air.
/// * `width`: Painting width in blocks.
/// * `height`: Painting height in blocks.
///
/// Returns `(position: glam::DVec3, rotation: glam::Vec3)`.
pub fn calculate_painting_world_transform(
    support_pos: [i32; 3],
    facing: Direction,
    width: u8,
    height: u8,
) -> (glam::DVec3, glam::Vec3) {
    let center = glam::DVec3::new(
        support_pos[0] as f64 + 0.5,
        support_pos[1] as f64 + 0.5,
        support_pos[2] as f64 + 0.5,
    );
    let facing_norm = glam::DVec3::new(
        facing.normal().x as f64,
        facing.normal().y as f64,
        facing.normal().z as f64,
    );
    let attached_to_wall = center - facing_norm * 0.46875;
    let horizontal_offset = if width % 2 == 0 { 0.5 } else { 0.0 };
    let vertical_offset = if height % 2 == 0 { 0.5 } else { 0.0 };

    let left = counter_clockwise(facing);
    let left_norm = glam::DVec3::new(
        left.normal().x as f64,
        left.normal().y as f64,
        left.normal().z as f64,
    );
    let pos = attached_to_wall + left_norm * horizontal_offset + glam::DVec3::Y * vertical_offset;
    let y_rot = facing_to_y_rot(facing);
    (pos, glam::Vec3::new(0.0, y_rot, 0.0))
}

/// Rotation angle around Y axis for horizontal facing (North, East, South, West).
#[inline]
pub fn facing_to_y_rot(facing: Direction) -> f32 {
    match facing {
        Direction::North => 0.0,
        Direction::East => -std::f32::consts::FRAC_PI_2,
        Direction::South => std::f32::consts::PI,
        Direction::West => std::f32::consts::FRAC_PI_2,
        _ => 0.0,
    }
}

/// Creates a procedural `BakedModel` for a painting in local model space (centered at (0,0,0)).
pub fn create_painting_baked_model(variant_name: &str, custom_w: u32, custom_h: u32) -> BakedModel {
    let (width, height) = if custom_w > 0 && custom_h > 0 {
        (custom_w as u8, custom_h as u8)
    } else {
        painting_dimension(variant_name)
    };
    let width = width.max(1);
    let height = height.max(1);
    let front_texture = painting_texture_id(variant_name);
    let back_texture = PAINTING_BACK_TEXTURE;

    let w = width as f32;
    let h = height as f32;

    let mut elements = Vec::with_capacity((width as usize) * (height as usize));

    for sy in 0..height {
        for sx in 0..width {
            let x0 = -w / 2.0 + (sx + 1) as f32;
            let x1 = -w / 2.0 + sx as f32;
            let y0 = -h / 2.0 + (sy + 1) as f32;
            let y1 = -h / 2.0 + sy as f32;

            let z_front = -PAINTING_HALF_DEPTH;
            let z_back = PAINTING_HALF_DEPTH;

            let mut faces = HashMap::new();

            // 1. Front face (pointing towards -Z / North in local space)
            {
                let verts = [
                    Vec3::new(x0, y1, z_front),
                    Vec3::new(x1, y1, z_front),
                    Vec3::new(x1, y0, z_front),
                    Vec3::new(x0, y0, z_front),
                ];

                let u0 = (width - sx) as f32 / w;
                let u1 = (width - (sx + 1)) as f32 / w;
                let v0 = (height - sy) as f32 / h;
                let v1 = (height - (sy + 1)) as f32 / h;

                let uvs = [
                    Vec2::new(u1, v0),
                    Vec2::new(u0, v0),
                    Vec2::new(u0, v1),
                    Vec2::new(u1, v1),
                ];

                faces.insert(
                    Direction::North,
                    BakedFace {
                        direction: Direction::North,
                        texture: front_texture.clone(),
                        uv_rot: 0.0,
                        uv_bounds: [u1, v1, u0, v0],
                        tint_index: -1,
                        cullface: None,
                        vertices: verts,
                        uvs,
                        normal: Vec3::new(0.0, 0.0, -1.0),
                        atlas_uvs: None,
                        atlas_chunk_id: None,
                        atlas_texture_id: None,
                    },
                );
            }

            // 2. Back face (pointing towards +Z / South in local space)
            {
                let verts = [
                    Vec3::new(x0, y0, z_back),
                    Vec3::new(x1, y0, z_back),
                    Vec3::new(x1, y1, z_back),
                    Vec3::new(x0, y1, z_back),
                ];

                let uvs = [
                    Vec2::new(1.0, 0.0),
                    Vec2::new(0.0, 0.0),
                    Vec2::new(0.0, 1.0),
                    Vec2::new(1.0, 1.0),
                ];

                faces.insert(
                    Direction::South,
                    BakedFace {
                        direction: Direction::South,
                        texture: back_texture.to_string(),
                        uv_rot: 0.0,
                        uv_bounds: [0.0, 0.0, 1.0, 1.0],
                        tint_index: -1,
                        cullface: None,
                        vertices: verts,
                        uvs,
                        normal: Vec3::new(0.0, 0.0, 1.0),
                        atlas_uvs: None,
                        atlas_chunk_id: None,
                        atlas_texture_id: None,
                    },
                );
            }

            // 3. Top rim border (outer perimeter only, +Y / Up)
            if sy == height - 1 {
                let verts = [
                    Vec3::new(x0, y0, z_front),
                    Vec3::new(x1, y0, z_front),
                    Vec3::new(x1, y0, z_back),
                    Vec3::new(x0, y0, z_back),
                ];

                let uvs = [
                    Vec2::new(0.0, 0.0),
                    Vec2::new(1.0, 0.0),
                    Vec2::new(1.0, PAINTING_RIM_UV_THICKNESS),
                    Vec2::new(0.0, PAINTING_RIM_UV_THICKNESS),
                ];

                faces.insert(
                    Direction::Up,
                    BakedFace {
                        direction: Direction::Up,
                        texture: back_texture.to_string(),
                        uv_rot: 0.0,
                        uv_bounds: [0.0, 0.0, 1.0, PAINTING_RIM_UV_THICKNESS],
                        tint_index: -1,
                        cullface: None,
                        vertices: verts,
                        uvs,
                        normal: Vec3::Y,
                        atlas_uvs: None,
                        atlas_chunk_id: None,
                        atlas_texture_id: None,
                    },
                );
            }

            // 4. Bottom rim border (outer perimeter only, -Y / Down)
            if sy == 0 {
                let verts = [
                    Vec3::new(x0, y1, z_back),
                    Vec3::new(x1, y1, z_back),
                    Vec3::new(x1, y1, z_front),
                    Vec3::new(x0, y1, z_front),
                ];

                let uvs = [
                    Vec2::new(0.0, 0.0),
                    Vec2::new(1.0, 0.0),
                    Vec2::new(1.0, PAINTING_RIM_UV_THICKNESS),
                    Vec2::new(0.0, PAINTING_RIM_UV_THICKNESS),
                ];

                faces.insert(
                    Direction::Down,
                    BakedFace {
                        direction: Direction::Down,
                        texture: back_texture.to_string(),
                        uv_rot: 0.0,
                        uv_bounds: [0.0, 0.0, 1.0, PAINTING_RIM_UV_THICKNESS],
                        tint_index: -1,
                        cullface: None,
                        vertices: verts,
                        uvs,
                        normal: Vec3::NEG_Y,
                        atlas_uvs: None,
                        atlas_chunk_id: None,
                        atlas_texture_id: None,
                    },
                );
            }

            // 5. Right rim border (outer perimeter only, x0 side, +X / East)
            if sx == width - 1 {
                let verts = [
                    Vec3::new(x0, y0, z_back),
                    Vec3::new(x0, y1, z_back),
                    Vec3::new(x0, y1, z_front),
                    Vec3::new(x0, y0, z_front),
                ];

                let uvs = [
                    Vec2::new(PAINTING_RIM_UV_THICKNESS, 0.0),
                    Vec2::new(PAINTING_RIM_UV_THICKNESS, 1.0),
                    Vec2::new(0.0, 1.0),
                    Vec2::new(0.0, 0.0),
                ];

                faces.insert(
                    Direction::East,
                    BakedFace {
                        direction: Direction::East,
                        texture: back_texture.to_string(),
                        uv_rot: 0.0,
                        uv_bounds: [0.0, 0.0, PAINTING_RIM_UV_THICKNESS, 1.0],
                        tint_index: -1,
                        cullface: None,
                        vertices: verts,
                        uvs,
                        normal: Vec3::X,
                        atlas_uvs: None,
                        atlas_chunk_id: None,
                        atlas_texture_id: None,
                    },
                );
            }

            // 6. Left rim border (outer perimeter only, x1 side, -X / West)
            if sx == 0 {
                let verts = [
                    Vec3::new(x1, y0, z_front),
                    Vec3::new(x1, y1, z_front),
                    Vec3::new(x1, y1, z_back),
                    Vec3::new(x1, y0, z_back),
                ];

                let uvs = [
                    Vec2::new(PAINTING_RIM_UV_THICKNESS, 0.0),
                    Vec2::new(PAINTING_RIM_UV_THICKNESS, 1.0),
                    Vec2::new(0.0, 1.0),
                    Vec2::new(0.0, 0.0),
                ];

                faces.insert(
                    Direction::West,
                    BakedFace {
                        direction: Direction::West,
                        texture: back_texture.to_string(),
                        uv_rot: 0.0,
                        uv_bounds: [0.0, 0.0, PAINTING_RIM_UV_THICKNESS, 1.0],
                        tint_index: -1,
                        cullface: None,
                        vertices: verts,
                        uvs,
                        normal: Vec3::NEG_X,
                        atlas_uvs: None,
                        atlas_chunk_id: None,
                        atlas_texture_id: None,
                    },
                );
            }

            elements.push(BakedElement {
                from_pos: [x1, y1, z_front],
                to_pos: [x0, y0, z_back],
                faces,
            });
        }
    }

    let mut model = BakedModel {
        block_state: format!("minecraft:painting[variant={}]", variant_name),
        elements,
        obj_faces: Vec::new(),
        faces: Default::default(),
        is_cube: false,
        is_opaque: false,
        is_emissive: false,
        cull_meta: None,
        culled_faces: Default::default(),
        unculled_faces: Vec::new(),
    };
    model.rebuild_face_buckets();
    model
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_51_variants_registered() {
        assert_eq!(PAINTING_VARIANTS.len(), 51);

        // Verify key canonical sizes
        assert_eq!(painting_dimension("kebab"), (1, 1));
        assert_eq!(painting_dimension("pool"), (2, 1));
        assert_eq!(painting_dimension("wanderer"), (1, 2));
        assert_eq!(painting_dimension("match"), (2, 2));
        assert_eq!(painting_dimension("bouquet"), (3, 3));
        assert_eq!(painting_dimension("fighters"), (4, 2));
        assert_eq!(painting_dimension("skeleton"), (4, 3));
        assert_eq!(painting_dimension("pointer"), (4, 4));
        assert_eq!(painting_dimension("backyard"), (3, 4));
    }

    #[test]
    fn test_normalization_and_texture_id() {
        assert_eq!(normalize_painting_name("minecraft:kebab"), "kebab");
        assert_eq!(normalize_painting_name("painting/kebab"), "kebab");
        assert_eq!(normalize_painting_name("minecraft:painting/kebab"), "kebab");
        assert_eq!(normalize_painting_name("kebab"), "kebab");

        assert_eq!(painting_texture_id("kebab"), "minecraft:painting/kebab");
        assert_eq!(
            painting_texture_id("minecraft:painting/kebab"),
            "minecraft:painting/kebab"
        );
    }

    #[test]
    fn test_1x1_kebab_baked_model() {
        let model = create_painting_baked_model("kebab", 0, 0);
        assert_eq!(model.elements.len(), 1);
        let el = &model.elements[0];
        // 1x1 has front, back, top, bottom, left, right (6 faces)
        assert_eq!(el.faces.len(), 6);

        let front = el.faces.get(&Direction::North).expect("Front face");
        assert_eq!(front.texture, "minecraft:painting/kebab");

        let back = el.faces.get(&Direction::South).expect("Back face");
        assert_eq!(back.texture, "minecraft:painting/back");
    }

    #[test]
    fn test_4x4_pointer_segmentation() {
        let model = create_painting_baked_model("pointer", 0, 0);
        // 4x4 has 16 segments
        assert_eq!(model.elements.len(), 16);

        // Corner segment (sx=0, sy=0) has front, back, bottom, left (4 faces)
        let corner = &model.elements[0];
        assert_eq!(corner.faces.len(), 4);

        // Center segment (sx=1, sy=1, index=5) has only front and back (2 faces, no borders)
        let center = &model.elements[5];
        assert_eq!(center.faces.len(), 2);
    }

    #[test]
    fn test_painting_uv_alignment_with_vanilla() {
        // 1x1 painting
        let model = create_painting_baked_model("kebab", 0, 0);
        let el = &model.elements[0];
        let front = el.faces.get(&Direction::North).expect("Front face");
        // pt0 (x0, y1) bottom-left: u=0.0, v=1.0
        // pt1 (x1, y1) bottom-right: u=1.0, v=1.0
        // pt2 (x1, y0) top-right: u=1.0, v=0.0
        // pt3 (x0, y0) top-left: u=0.0, v=0.0
        assert_eq!(front.uvs[0], Vec2::new(0.0, 1.0));
        assert_eq!(front.uvs[1], Vec2::new(1.0, 1.0));
        assert_eq!(front.uvs[2], Vec2::new(1.0, 0.0));
        assert_eq!(front.uvs[3], Vec2::new(0.0, 0.0));

        // Back face:
        let back = el.faces.get(&Direction::South).expect("Back face");
        assert_eq!(back.uvs[0], Vec2::new(1.0, 0.0));
        assert_eq!(back.uvs[1], Vec2::new(0.0, 0.0));
        assert_eq!(back.uvs[2], Vec2::new(0.0, 1.0));
        assert_eq!(back.uvs[3], Vec2::new(1.0, 1.0));

        // 2x1 painting: sx=0 (right segment of world, left segment of texture is sx=1)
        // For sx=width-1 (sx=1), U ranges from 0.0 to 0.5 (left slice of texture)
        // For sx=0, U ranges from 0.5 to 1.0 (right slice of texture)
        let model_2x1 = create_painting_baked_model("pool", 0, 0);
        // segment 0 is sx=0, sy=0
        let seg_right_world = &model_2x1.elements[0];
        let front_right = seg_right_world.faces.get(&Direction::North).unwrap();
        assert_eq!(front_right.uv_bounds, [0.5, 0.0, 1.0, 1.0]);

        // segment 1 is sx=1, sy=0 (viewer's left)
        let seg_left_world = &model_2x1.elements[1];
        let front_left = seg_left_world.faces.get(&Direction::North).unwrap();
        assert_eq!(front_left.uv_bounds, [0.0, 0.0, 0.5, 1.0]);
    }

    #[test]
    fn test_world_position_calculation() {
        let support = [10, 64, 20];
        // Facing South (+Z), 1x1 painting
        let pos = calculate_painting_world_position(support, Direction::South, 1, 1);
        // Center: x=10.5, y=64.5, z=20.5 - 0.46875*(0,0,1) = 20.03125
        assert!((pos.x - 10.5).abs() < 1e-4);
        assert!((pos.y - 64.5).abs() < 1e-4);
        assert!((pos.z - 20.03125).abs() < 1e-4);

        // Even width (2x1) shifts horizontally by 0.5 towards left (East for South facing)
        let pos_even = calculate_painting_world_position(support, Direction::South, 2, 1);
        assert!((pos_even.x - 11.0).abs() < 1e-4);
    }
}
