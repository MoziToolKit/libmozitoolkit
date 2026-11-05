//! # Parametric Face Occlusion Derivation
//!
//! Computes canonical 2D face occlusion shapes for known partial/non-full blocks
//! (slabs, snow layers, carpets, iron bars, fences, stairs).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use mtk_core::Aabb2d;
use mtk_core::Direction;

use crate::types::FULL_FACE_RECT;

/// Derive canonical 2D face occlusion shapes for known partial / non-full blocks
/// based on block state properties when explicit detailed model elements are absent.
pub fn derive_parametric_face_shapes(
    name_low: &str,
    props: &BTreeMap<String, String>,
) -> [Vec<Aabb2d>; 6] {
    let mut shapes: [Vec<Aabb2d>; 6] = Default::default();

    // 1. Slabs
    if name_low.ends_with("_slab") {
        let slab_type = props.get("type").map(|s| s.as_str()).unwrap_or("bottom");
        if slab_type == "double" {
            for dir in Direction::ALL {
                shapes[dir.to_index()] = alloc::vec![FULL_FACE_RECT];
            }
            return shapes;
        } else if slab_type == "top" {
            shapes[Direction::Up.to_index()] = alloc::vec![FULL_FACE_RECT];
            let side_rect = Aabb2d::from_min_max(0.0, 0.5, 1.0, 1.0);
            shapes[Direction::North.to_index()] = alloc::vec![side_rect];
            shapes[Direction::South.to_index()] = alloc::vec![side_rect];
            shapes[Direction::East.to_index()] = alloc::vec![side_rect];
            shapes[Direction::West.to_index()] = alloc::vec![side_rect];
            return shapes;
        } else {
            // bottom
            shapes[Direction::Down.to_index()] = alloc::vec![FULL_FACE_RECT];
            let side_rect = Aabb2d::from_min_max(0.0, 0.0, 1.0, 0.5);
            shapes[Direction::North.to_index()] = alloc::vec![side_rect];
            shapes[Direction::South.to_index()] = alloc::vec![side_rect];
            shapes[Direction::East.to_index()] = alloc::vec![side_rect];
            shapes[Direction::West.to_index()] = alloc::vec![side_rect];
            return shapes;
        }
    }

    // 2. Snow layers (minecraft:snow)
    if name_low == "snow" || name_low.ends_with(":snow") {
        let layers: u32 = props
            .get("layers")
            .and_then(|l| l.parse().ok())
            .unwrap_or(1)
            .clamp(1, 8);
        let h = layers as f32 / 8.0;
        let side_rect = Aabb2d::from_min_max(0.0, 0.0, 1.0, h);
        shapes[Direction::Down.to_index()] = alloc::vec![FULL_FACE_RECT];
        if layers == 8 {
            shapes[Direction::Up.to_index()] = alloc::vec![FULL_FACE_RECT];
        }
        shapes[Direction::North.to_index()] = alloc::vec![side_rect];
        shapes[Direction::South.to_index()] = alloc::vec![side_rect];
        shapes[Direction::East.to_index()] = alloc::vec![side_rect];
        shapes[Direction::West.to_index()] = alloc::vec![side_rect];
        return shapes;
    }

    // 3. Carpets
    if name_low.ends_with("_carpet") || name_low == "carpet" {
        let h = 1.0 / 16.0;
        let side_rect = Aabb2d::from_min_max(0.0, 0.0, 1.0, h);
        shapes[Direction::Down.to_index()] = alloc::vec![FULL_FACE_RECT];
        shapes[Direction::North.to_index()] = alloc::vec![side_rect];
        shapes[Direction::South.to_index()] = alloc::vec![side_rect];
        shapes[Direction::East.to_index()] = alloc::vec![side_rect];
        shapes[Direction::West.to_index()] = alloc::vec![side_rect];
        return shapes;
    }

    // 4. Iron bars & Glass panes
    if name_low.ends_with("_bars")
        || name_low.ends_with("_pane")
        || name_low == "iron_bars"
        || name_low == "glass_pane"
    {
        let w0 = 7.0 / 16.0;
        let w1 = 9.0 / 16.0;
        let post_cap = Aabb2d::from_min_max(w0, w0, w1, w1);
        let side_cap = Aabb2d::from_min_max(w0, 0.0, w1, 1.0);
        shapes[Direction::Down.to_index()] = alloc::vec![post_cap];
        shapes[Direction::Up.to_index()] = alloc::vec![post_cap];
        if props.get("east").map(|s| s.as_str()) == Some("true") {
            shapes[Direction::East.to_index()] = alloc::vec![side_cap];
        }
        if props.get("west").map(|s| s.as_str()) == Some("true") {
            shapes[Direction::West.to_index()] = alloc::vec![side_cap];
        }
        if props.get("north").map(|s| s.as_str()) == Some("true") {
            shapes[Direction::North.to_index()] = alloc::vec![side_cap];
        }
        if props.get("south").map(|s| s.as_str()) == Some("true") {
            shapes[Direction::South.to_index()] = alloc::vec![side_cap];
        }
        return shapes;
    }

    // 5. Wooden & Nether Brick Fences
    if name_low.ends_with("_fence") || name_low == "fence" {
        let w0 = 7.0 / 16.0;
        let w1 = 9.0 / 16.0;
        let post_cap = Aabb2d::from_min_max(6.0 / 16.0, 6.0 / 16.0, 10.0 / 16.0, 10.0 / 16.0);
        let top_bar = Aabb2d::from_min_max(w0, 12.0 / 16.0, w1, 15.0 / 16.0);
        let bot_bar = Aabb2d::from_min_max(w0, 6.0 / 16.0, w1, 9.0 / 16.0);
        shapes[Direction::Down.to_index()] = alloc::vec![post_cap];
        shapes[Direction::Up.to_index()] = alloc::vec![post_cap];
        if props.get("east").map(|s| s.as_str()) == Some("true") {
            shapes[Direction::East.to_index()] = alloc::vec![bot_bar, top_bar];
        }
        if props.get("west").map(|s| s.as_str()) == Some("true") {
            shapes[Direction::West.to_index()] = alloc::vec![bot_bar, top_bar];
        }
        if props.get("north").map(|s| s.as_str()) == Some("true") {
            shapes[Direction::North.to_index()] = alloc::vec![bot_bar, top_bar];
        }
        if props.get("south").map(|s| s.as_str()) == Some("true") {
            shapes[Direction::South.to_index()] = alloc::vec![bot_bar, top_bar];
        }
        return shapes;
    }

    // 6. Stairs
    if name_low.ends_with("_stairs") {
        let half = props.get("half").map(|s| s.as_str()).unwrap_or("bottom");
        if half == "top" {
            shapes[Direction::Up.to_index()] = alloc::vec![FULL_FACE_RECT];
        } else {
            shapes[Direction::Down.to_index()] = alloc::vec![FULL_FACE_RECT];
        }
        return shapes;
    }

    shapes
}
