#![forbid(unsafe_code)]
//! High-level decoupled entity instance representations living in continuous 3D space.
//!
//! Unlike discrete voxel block entities that are strictly anchored to a 1x1x1 integer grid
//! (such as Signs, Banners, Decorated Pots, and Skulls), visual entities (such as Paintings,
//! Item Frames, Armor Stands, and Display Entities) have continuous positions, arbitrary
//! dimensions, and rigid transformations.

use glam::{DVec3, Vec3};
use mtk_core::Direction;
use serde::{Deserialize, Serialize};

/// High-level entity instance living in continuous 3D space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityInstance {
    /// Canonical entity identifier, e.g. "minecraft:painting", "minecraft:item_frame", "minecraft:armor_stand".
    pub id: String,
    /// Continuous world position [x, y, z].
    pub position: DVec3,
    /// Outward or orientation direction (if applicable, e.g. hanging entities).
    pub facing: Option<Direction>,
    /// Continuous Euler rotation (pitch, yaw, roll) in radians.
    #[serde(default)]
    pub rotation: Vec3,
    /// Strong-typed entity payload data.
    pub kind: EntityKindData,
}

impl EntityInstance {
    /// Creates a new hanging Painting entity instance.
    pub fn new_painting(
        position: DVec3,
        facing: Direction,
        variant: impl Into<String>,
        width: u8,
        height: u8,
    ) -> Self {
        let v = variant.into();
        Self {
            id: "minecraft:painting".to_string(),
            position,
            facing: Some(facing),
            rotation: Vec3::ZERO,
            kind: EntityKindData::Painting(PaintingData {
                variant: v,
                width: width.max(1),
                height: height.max(1),
            }),
        }
    }

    /// Creates a new hanging Item Frame entity instance.
    pub fn new_item_frame(
        position: DVec3,
        facing: Direction,
        item_id: impl Into<String>,
        rotation: u8,
        is_glowing: bool,
        is_invisible: bool,
        map_id: Option<i32>,
    ) -> Self {
        Self {
            id: if is_glowing {
                "minecraft:glow_item_frame".to_string()
            } else {
                "minecraft:item_frame".to_string()
            },
            position,
            facing: Some(facing),
            rotation: Vec3::ZERO,
            kind: EntityKindData::ItemFrame(ItemFrameData {
                item_id: item_id.into(),
                count: 1,
                rotation: rotation & 7,
                is_glowing,
                is_invisible,
                map_id,
            }),
        }
    }
}

/// Strongly-typed payload for visual entities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EntityKindData {
    /// Hanging Painting.
    #[serde(rename = "minecraft:painting", alias = "painting")]
    Painting(PaintingData),
    /// Item Frame or Glow Item Frame.
    #[serde(rename = "minecraft:item_frame", alias = "item_frame")]
    ItemFrame(ItemFrameData),
    /// Armor Stand.
    #[serde(rename = "minecraft:armor_stand", alias = "armor_stand")]
    ArmorStand(ArmorStandData),
    /// Generic or unsupported entity with raw payload.
    #[serde(rename = "generic")]
    Generic { custom_id: String, payload: Vec<u8> },
}

/// Data payload for a Painting entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaintingData {
    /// Normalized variant identifier (e.g. "kebab", "unpacked", "pointer").
    pub variant: String,
    /// Width in blocks.
    pub width: u8,
    /// Height in blocks.
    pub height: u8,
}

/// Data payload for an Item Frame entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemFrameData {
    /// Item identifier held in frame (e.g. "minecraft:diamond_sword").
    pub item_id: String,
    /// Item stack count.
    #[serde(default = "default_one")]
    pub count: u8,
    /// Rotation step (0..7, in 45-degree increments).
    #[serde(default)]
    pub rotation: u8,
    /// Whether this is a glow item frame.
    #[serde(default)]
    pub is_glowing: bool,
    /// Whether the frame itself is invisible.
    #[serde(default)]
    pub is_invisible: bool,
    /// Optional Filled Map ID if holding a map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_id: Option<i32>,
}

/// Data payload for an Armor Stand entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmorStandData {
    /// Whether the armor stand has arms.
    #[serde(default)]
    pub show_arms: bool,
    /// Whether the stone slab base plate is hidden.
    #[serde(default)]
    pub no_base_plate: bool,
    /// Whether this is a small baby-sized armor stand.
    #[serde(default)]
    pub is_small: bool,
}

fn default_one() -> u8 {
    1
}
