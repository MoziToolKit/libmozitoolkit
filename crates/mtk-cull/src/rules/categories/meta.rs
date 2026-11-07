//! # Block Cull Metadata Computation
//!
//! Provides state string parsing and authoritative BlockCullMeta calculation.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use mtk_core::constants::geometry::EPS;
use mtk_core::Aabb2d;
use mtk_core::Vec3;
use mtk_core::{DirMask, Direction};

use super::catalog::{
    is_non_full_or_partial_block, is_non_occluding_block, AIR_NAMES, FLUID_NAMES, GLASS_NAMES,
    LEAVES_NAMES,
};
use super::parametric::derive_parametric_face_shapes;
use crate::geometry::rect_ops::extract_quad_face_occlusion_rect;
use crate::types::{is_full_rect, BlockCullMeta, CullCategory, FULL_FACE_RECT};

/// Canonical vanilla Minecraft blocks that canonically only exist submerged in water and
/// do not carry an explicit `waterlogged` property in their blockstate string.
#[inline]
pub fn is_inherently_submerged_block(name: &str) -> bool {
    let clean = name.strip_prefix("minecraft:").unwrap_or(name);
    let lower = clean.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "seagrass" | "tall_seagrass" | "kelp" | "kelp_plant" | "bubble_column"
    )
}

/// Fast extraction of raw block name and properties from state string.
///
/// Automatically populates `waterlogged="true"` for inherently submerged blocks
/// (such as `seagrass` and `kelp`) unless explicitly configured otherwise (e.g. `waterlogged=false`).
pub fn parse_block_name_and_props(state_str: &str) -> (String, BTreeMap<String, String>) {
    let (name, mut props) = parse_raw_block_name_and_props(state_str);
    if is_inherently_submerged_block(&name) && !props.contains_key("waterlogged") {
        props.insert("waterlogged".to_string(), "true".to_string());
    }
    (name, props)
}

/// Raw parser extracting unnormalized block name and property map without default injection.
fn parse_raw_block_name_and_props(state_str: &str) -> (String, BTreeMap<String, String>) {
    let trimmed = state_str.trim();
    if trimmed.is_empty() {
        return ("air".to_string(), BTreeMap::new());
    }

    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        #[cfg(feature = "serde")]
        {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                let raw_state = val
                    .get("state")
                    .or_else(|| val.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("air");

                let mut props = BTreeMap::new();
                if let Some(opaque_val) = val.get("opaque").and_then(|v| v.as_i64()) {
                    props.insert("__opaque".to_string(), opaque_val.to_string());
                }
                if let Some(type_val) = val.get("type").and_then(|v| v.as_i64()) {
                    props.insert("__type".to_string(), type_val.to_string());
                }

                if let Some(open) = raw_state.find('[') {
                    if let Some(close) = raw_state.rfind(']') {
                        let base = &raw_state[..open];
                        let name = base.split(':').next_back().unwrap_or(base).to_string();
                        for item in raw_state[open + 1..close].split(',') {
                            if let Some((k, v)) = item.split_once('=') {
                                props.insert(k.trim().to_string(), v.trim().to_string());
                            }
                        }
                        return (name, props);
                    }
                }
                let name = raw_state
                    .split(':')
                    .next_back()
                    .unwrap_or(raw_state)
                    .to_string();
                return (name, props);
            }
        }
    }

    if let Some(open) = trimmed.find('[') {
        if let Some(close) = trimmed.rfind(']') {
            let base = &trimmed[..open];
            let name = base.split(':').next_back().unwrap_or(base).to_string();
            let mut props = BTreeMap::new();
            for item in trimmed[open + 1..close].split(',') {
                if let Some((k, v)) = item.split_once('=') {
                    props.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
            return (name, props);
        }
    }

    let name = trimmed
        .split(':')
        .next_back()
        .unwrap_or(trimmed)
        .to_string();
    (name, BTreeMap::new())
}

/// Computes a BlockCullMeta directly for a state string without checking cache.
pub fn compute_block_cull_meta(
    state_str: &str,
    element_quads: Option<&[([Vec3; 4], Direction)]>,
    is_opaque_hint: Option<bool>,
) -> BlockCullMeta {
    let (name, props) = parse_block_name_and_props(state_str);
    let name_low = name.to_ascii_lowercase();

    let json_opaque = props
        .get("__opaque")
        .and_then(|v| v.parse::<i64>().ok())
        .map(|v| v != 0);
    let effective_opaque_hint = is_opaque_hint.or(json_opaque);
    let json_type = props.get("__type").and_then(|v| v.parse::<i64>().ok());

    let is_waterlogged = props.get("waterlogged").map(|s| s.as_str()) == Some("true");
    let clean_name = name_low.strip_prefix("minecraft:").unwrap_or(&name_low);
    let is_air = state_str.is_empty()
        || AIR_NAMES.iter().any(|&n| name_low == n || clean_name == n)
        || clean_name == "air"
        || clean_name.ends_with("_air");
    let is_fluid = !is_air
        && (FLUID_NAMES
            .iter()
            .any(|&n| n.strip_prefix("minecraft:").unwrap_or(n) == clean_name));
    let is_leaves = !is_air
        && (LEAVES_NAMES
            .iter()
            .any(|&n| name_low == n || clean_name == n)
            || clean_name == "leaves"
            || clean_name.ends_with("_leaves")
            || clean_name == "mangrove_roots");

    let is_pane = clean_name.ends_with("_pane")
        || clean_name.ends_with("_bars")
        || clean_name == "glass_pane"
        || clean_name == "iron_bars";
    let is_ice = clean_name == "ice"
        || clean_name.ends_with("_ice")
        || clean_name == "frosted_ice"
        || clean_name == "blue_ice"
        || clean_name == "packed_ice";
    let is_glass = !is_air
        && !is_pane
        && (GLASS_NAMES
            .iter()
            .any(|&n| name_low == n || clean_name == n)
            || (clean_name.contains("stained_glass") && !is_pane)
            || clean_name == "glass"
            || clean_name.ends_with("_glass")
            || is_ice);
    let is_non_occluding = !is_air
        && (json_type == Some(1) || json_type == Some(4) || is_non_occluding_block(clean_name));
    let is_double_slab =
        clean_name.ends_with("_slab") && props.get("type").map(|s| s.as_str()) == Some("double");
    let is_non_full = !is_air
        && !is_double_slab
        && (is_pane
            || clean_name.ends_with("_slab")
            || clean_name.ends_with("_stairs")
            || (clean_name.contains("piston")
                && (props.get("extended").map(|s| s.as_str()) == Some("true")
                    || clean_name.contains("head")))
            || is_non_full_or_partial_block(clean_name));

    let (
        category,
        is_full_cube,
        is_opaque,
        cull_group,
        face_shapes,
        full_face_mask,
        empty_face_mask,
    ) = if is_air {
        (
            CullCategory::Air,
            false,
            false,
            "air".to_string(),
            <[Vec<Aabb2d>; 6]>::default(),
            DirMask::empty(),
            DirMask::ALL,
        )
    } else if is_fluid {
        (
            CullCategory::Fluid,
            false,
            false,
            if name_low.contains("water") {
                "water".to_string()
            } else {
                "lava".to_string()
            },
            <[Vec<Aabb2d>; 6]>::default(),
            DirMask::empty(),
            DirMask::ALL,
        )
    } else if is_glass {
        (
            CullCategory::GlassTranslucent,
            true,
            false,
            if name_low.contains("glass") {
                "glass".to_string()
            } else {
                name_low.clone()
            },
            <[Vec<Aabb2d>; 6]>::default(),
            DirMask::empty(),
            DirMask::ALL,
        )
    } else if is_leaves {
        (
            CullCategory::CutoutLeaves,
            true,
            false,
            "leaves".to_string(),
            <[Vec<Aabb2d>; 6]>::default(),
            DirMask::empty(),
            DirMask::ALL,
        )
    } else if is_non_occluding {
        (
            CullCategory::NonOccluding,
            false,
            false,
            "non_occluding".to_string(),
            <[Vec<Aabb2d>; 6]>::default(),
            DirMask::empty(),
            DirMask::ALL,
        )
    } else if let Some(quads) = element_quads {
        // Detailed shape analysis from model baked quads
        let is_full_cube = Direction::ALL.iter().all(|dir| {
            quads.iter().any(|(verts, d)| {
                if d == dir {
                    if let Some(rect) = extract_quad_face_occlusion_rect(verts, *dir) {
                        return is_full_rect(&rect, EPS);
                    }
                }
                false
            })
        });

        let is_opaque = effective_opaque_hint.unwrap_or(is_full_cube);

        if is_full_cube {
            if is_opaque {
                let mut face_shapes: [Vec<Aabb2d>; 6] = Default::default();
                for dir in Direction::ALL {
                    face_shapes[dir.to_index()] = alloc::vec![FULL_FACE_RECT];
                }
                (
                    CullCategory::SolidOpaque,
                    true,
                    true,
                    "solid".to_string(),
                    face_shapes,
                    DirMask::ALL,
                    DirMask::empty(),
                )
            } else {
                (
                    CullCategory::GlassTranslucent,
                    true,
                    false,
                    name_low.clone(),
                    <[Vec<Aabb2d>; 6]>::default(),
                    DirMask::empty(),
                    DirMask::ALL,
                )
            }
        } else {
            let mut face_shapes: [Vec<Aabb2d>; 6] = Default::default();
            for (verts, dir) in quads {
                if let Some(rect) = extract_quad_face_occlusion_rect(verts, *dir) {
                    face_shapes[dir.to_index()].push(rect);
                }
            }

            let mut full_face_mask = DirMask::empty();
            let mut empty_face_mask = DirMask::empty();
            for dir in Direction::ALL {
                let dir_shapes = &face_shapes[dir.to_index()];
                if dir_shapes.iter().any(|s| is_full_rect(s, EPS)) {
                    if is_opaque {
                        full_face_mask |= dir.mask();
                    } else {
                        empty_face_mask |= dir.mask();
                    }
                } else if dir_shapes.is_empty() {
                    empty_face_mask |= dir.mask();
                }
            }
            (
                CullCategory::PartialShape,
                false,
                is_opaque,
                "partial".to_string(),
                face_shapes,
                full_face_mask,
                empty_face_mask,
            )
        }
    } else if is_non_full {
        let face_shapes = derive_parametric_face_shapes(&name_low, &props);
        let mut full_face_mask = DirMask::empty();
        let mut empty_face_mask = DirMask::empty();
        for dir in Direction::ALL {
            let dir_shapes = &face_shapes[dir.to_index()];
            if dir_shapes.iter().any(|s| is_full_rect(s, EPS)) {
                full_face_mask |= dir.mask();
            } else if dir_shapes.is_empty() {
                empty_face_mask |= dir.mask();
            }
        }
        (
            CullCategory::PartialShape,
            false,
            false,
            "partial".to_string(),
            face_shapes,
            full_face_mask,
            empty_face_mask,
        )
    } else {
        let is_opaque = effective_opaque_hint.unwrap_or(true);
        if is_opaque {
            let mut face_shapes: [Vec<Aabb2d>; 6] = Default::default();
            for dir in Direction::ALL {
                face_shapes[dir.to_index()] = alloc::vec![FULL_FACE_RECT];
            }
            (
                CullCategory::SolidOpaque,
                true,
                true,
                "solid".to_string(),
                face_shapes,
                DirMask::ALL,
                DirMask::empty(),
            )
        } else {
            (
                CullCategory::GlassTranslucent,
                true,
                false,
                name_low.clone(),
                <[Vec<Aabb2d>; 6]>::default(),
                DirMask::empty(),
                DirMask::ALL,
            )
        }
    };

    BlockCullMeta {
        state_str: state_str.to_string(),
        block_name: name,
        category,
        is_full_cube,
        is_opaque,
        is_air,
        is_fluid,
        cull_group,
        face_shapes,
        full_face_mask,
        empty_face_mask,
        props,
        is_waterlogged,
        has_baked_model: element_quads.is_some(),
    }
}

/// Computes standard non-occluding cull metadata for non-solid, decorative, or complex entity blocks.
pub fn compute_non_occluding_meta(state_str: &str) -> BlockCullMeta {
    let (name, props) = parse_block_name_and_props(state_str);
    let is_waterlogged = props.get("waterlogged").map(|s| s.as_str()) == Some("true");
    BlockCullMeta {
        state_str: state_str.to_string(),
        block_name: name,
        category: CullCategory::NonOccluding,
        is_full_cube: false,
        is_opaque: false,
        is_air: false,
        is_fluid: false,
        cull_group: "non_occluding".to_string(),
        face_shapes: Default::default(),
        full_face_mask: DirMask::empty(),
        empty_face_mask: DirMask::ALL,
        props,
        is_waterlogged,
        has_baked_model: false,
    }
}
