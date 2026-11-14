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
use thiserror::Error;

/// Canonical entity identifier for hanging paintings.
pub const ENTITY_ID_PAINTING: &str = "minecraft:painting";

/// Canonical entity identifier for regular item frames.
pub const ENTITY_ID_ITEM_FRAME: &str = "minecraft:item_frame";

/// Canonical entity identifier for glowing item frames.
pub const ENTITY_ID_GLOW_ITEM_FRAME: &str = "minecraft:glow_item_frame";

/// Canonical entity identifier for armor stands.
pub const ENTITY_ID_ARMOR_STAND: &str = "minecraft:armor_stand";

/// Build and baking capability level for an entity type in `mtk-model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityBuildCapability {
    /// Full procedural 3D mesh baking and UV remapping supported into standalone local-space `MeshData`.
    ProceduralMesh,
    /// Spatial coordinates, metadata, and rigid transform supported; mesh baking handled externally or pending.
    TransformAndMetadataOnly,
}

/// Canonical descriptor for a continuous visual entity type recognized by `mtk-model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SupportedVisualEntity {
    /// Full namespaced entity resource ID (e.g. "minecraft:painting").
    pub id: &'static str,
    /// Lowercase family slug (e.g. "painting").
    pub family: &'static str,
    /// Current procedural build capability in mtk-model.
    pub build_capability: EntityBuildCapability,
}

impl SupportedVisualEntity {
    /// Whether this entity type currently has procedural local-mesh baking support in mtk-model.
    #[inline]
    pub const fn supports_mesh_baking(&self) -> bool {
        matches!(self.build_capability, EntityBuildCapability::ProceduralMesh)
    }
}

/// Table of all visual entity types officially supported and declared by `mtk-model`.
pub const SUPPORTED_VISUAL_ENTITIES: &[SupportedVisualEntity] = &[
    SupportedVisualEntity {
        id: ENTITY_ID_PAINTING,
        family: "painting",
        build_capability: EntityBuildCapability::ProceduralMesh,
    },
    SupportedVisualEntity {
        id: ENTITY_ID_ITEM_FRAME,
        family: "item_frame",
        build_capability: EntityBuildCapability::ProceduralMesh,
    },
    SupportedVisualEntity {
        id: ENTITY_ID_GLOW_ITEM_FRAME,
        family: "glow_item_frame",
        build_capability: EntityBuildCapability::ProceduralMesh,
    },
    SupportedVisualEntity {
        id: ENTITY_ID_ARMOR_STAND,
        family: "armor_stand",
        build_capability: EntityBuildCapability::TransformAndMetadataOnly,
    },
];

/// Returns a static slice of all canonical visual entity type identifiers supported by `mtk-model`.
pub fn supported_visual_entity_ids() -> &'static [&'static str] {
    static IDS: &[&str] = &[
        ENTITY_ID_PAINTING,
        ENTITY_ID_ITEM_FRAME,
        ENTITY_ID_GLOW_ITEM_FRAME,
        ENTITY_ID_ARMOR_STAND,
    ];
    IDS
}

/// Finds the supported visual entity descriptor matching the given entity ID or family.
pub fn find_supported_visual_entity(id: &str) -> Option<&'static SupportedVisualEntity> {
    let fam = entity_family(id);
    SUPPORTED_VISUAL_ENTITIES
        .iter()
        .find(|e| e.id == id || e.family == fam)
}

/// Checks whether an entity type identifier is recognized as a supported visual entity.
pub fn is_supported_visual_entity(id: &str) -> bool {
    find_supported_visual_entity(id).is_some()
}

/// Validation failures for a canonical `EntityInstance`.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum EntityValidationError {
    #[error("Entity position contains non-finite values")]
    NonFinitePosition,
    #[error("Entity rotation contains non-finite values")]
    NonFiniteRotation,
    #[error("Entity scale must be finite and non-zero")]
    InvalidScale,
    #[error("Entity UUID must not be empty when present")]
    EmptyUuid,
    #[error("Entity id '{id}' does not match payload kind '{kind}'")]
    IdKindMismatch { id: String, kind: &'static str },
    #[error("Painting variant must not be empty")]
    EmptyPaintingVariant,
    #[error("Painting dimensions must be at least 1x1, got {width}x{height}")]
    InvalidPaintingDimensions { width: u8, height: u8 },
    #[error("Painting facing must be horizontal, got {0:?}")]
    NonHorizontalPaintingFacing(Direction),
    #[error("Item frame rotation step {0} is outside 0..=7")]
    InvalidItemFrameRotation(u8),
    #[error("Generic entity custom_id must not be empty")]
    EmptyGenericId,
}

/// Returns the lower-case path portion of an entity identifier.
pub(crate) fn entity_family(id: &str) -> String {
    id.rsplit(':').next().unwrap_or(id).to_ascii_lowercase()
}

/// High-level entity instance living in continuous 3D space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityInstance {
    /// Optional canonical UUID string of the entity (e.g. "e53d5a41-b841-4770-985d-857c3e5a5961").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    /// Canonical entity identifier, e.g. "minecraft:painting", "minecraft:item_frame", "minecraft:armor_stand".
    pub id: String,
    /// Optional custom display name (from game CustomName tag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_name: Option<String>,
    /// Continuous world-space position [x, y, z].
    ///
    /// For paintings this is the world-space **center** of the painting, not the
    /// support block. `PaintingData::anchor_block` retains the original support block.
    pub position: DVec3,
    /// Outward/orientation direction metadata (e.g. hanging entity wall facing).
    ///
    /// Hosts must not apply this as an extra rotation; `rotation` is authoritative.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facing: Option<Direction>,
    /// Local-model-space Euler rotation `(pitch, yaw, roll)` in radians.
    ///
    /// Canonical convention: yaw `0.0` points North, matching the entity model
    /// front being local `-Z`. This is intentionally not Minecraft's raw yaw
    /// convention (where `0.0` points South); parsers are responsible for converting.
    #[serde(default)]
    pub rotation: Vec3,
    /// Scale factor `[scale_x, scale_y, scale_z]` in Minecraft local axes.
    #[serde(default = "default_scale")]
    pub scale: Vec3,
    /// Strongly-typed entity payload data.
    pub kind: EntityKindData,
}

impl EntityInstance {
    /// Creates a new hanging Painting entity instance.
    pub fn new_painting(
        uuid: Option<String>,
        position: DVec3,
        facing: Direction,
        rotation: Vec3,
        variant: impl Into<String>,
        width: u8,
        height: u8,
        anchor_block: Option<[i32; 3]>,
    ) -> Self {
        let v = variant.into();
        Self {
            uuid,
            id: ENTITY_ID_PAINTING.to_string(),
            custom_name: None,
            position,
            facing: Some(facing),
            rotation,
            scale: Vec3::ONE,
            kind: EntityKindData::Painting(PaintingData {
                variant: v,
                width: width.max(1),
                height: height.max(1),
                anchor_block,
            }),
        }
    }

    /// Creates a new hanging Item Frame entity instance.
    pub fn new_item_frame(
        uuid: Option<String>,
        position: DVec3,
        facing: Direction,
        rotation_euler: Vec3,
        item_id: impl Into<String>,
        count: u8,
        item_rotation: u8,
        is_glowing: bool,
        is_invisible: bool,
        map_id: Option<i32>,
    ) -> Self {
        Self {
            uuid,
            id: if is_glowing {
                ENTITY_ID_GLOW_ITEM_FRAME.to_string()
            } else {
                ENTITY_ID_ITEM_FRAME.to_string()
            },
            custom_name: None,
            position,
            facing: Some(facing),
            rotation: rotation_euler,
            scale: Vec3::ONE,
            kind: EntityKindData::ItemFrame(ItemFrameData {
                item_id: item_id.into(),
                count: if count == 0 { 1 } else { count },
                rotation: item_rotation & 7,
                is_glowing,
                is_invisible,
                map_id,
            }),
        }
    }

    /// Validates the canonical entity contract before baking, exporting, or persisting.
    ///
    /// The most important invariant is that `id` and `kind` describe the same
    /// entity family. Derived fields such as painting dimensions and item-frame
    /// rotation steps are validated as well.
    pub fn validate(&self) -> Result<(), EntityValidationError> {
        if !self.position.is_finite() {
            return Err(EntityValidationError::NonFinitePosition);
        }
        if !self.rotation.is_finite() {
            return Err(EntityValidationError::NonFiniteRotation);
        }
        if !self.scale.is_finite()
            || self.scale.x.abs() <= f32::EPSILON
            || self.scale.y.abs() <= f32::EPSILON
            || self.scale.z.abs() <= f32::EPSILON
        {
            return Err(EntityValidationError::InvalidScale);
        }
        if let Some(uuid) = &self.uuid {
            if uuid.trim().is_empty() {
                return Err(EntityValidationError::EmptyUuid);
            }
        }

        let family = entity_family(&self.id);
        match &self.kind {
            EntityKindData::Painting(p) => {
                if family != "painting" {
                    return Err(EntityValidationError::IdKindMismatch {
                        id: self.id.clone(),
                        kind: "painting",
                    });
                }
                if p.variant.trim().is_empty() {
                    return Err(EntityValidationError::EmptyPaintingVariant);
                }
                if p.width == 0 || p.height == 0 {
                    return Err(EntityValidationError::InvalidPaintingDimensions {
                        width: p.width,
                        height: p.height,
                    });
                }
                if let Some(facing) = self.facing {
                    if matches!(facing, Direction::Up | Direction::Down) {
                        return Err(EntityValidationError::NonHorizontalPaintingFacing(facing));
                    }
                }
            }
            EntityKindData::ItemFrame(f) => {
                if family != "item_frame" && family != "glow_item_frame" {
                    return Err(EntityValidationError::IdKindMismatch {
                        id: self.id.clone(),
                        kind: "item_frame",
                    });
                }
                if f.rotation > 7 {
                    return Err(EntityValidationError::InvalidItemFrameRotation(f.rotation));
                }
            }
            EntityKindData::ArmorStand(_) => {
                if family != "armor_stand" {
                    return Err(EntityValidationError::IdKindMismatch {
                        id: self.id.clone(),
                        kind: "armor_stand",
                    });
                }
            }
            EntityKindData::Generic { custom_id, .. } => {
                if custom_id.trim().is_empty() {
                    return Err(EntityValidationError::EmptyGenericId);
                }
            }
        }

        Ok(())
    }

    /// Collects all texture asset locations required to render this entity,
    /// enabling scene package exporters to tree-shake and crop sprites into Mini-Atlas.
    pub fn required_textures(&self) -> Vec<String> {
        match &self.kind {
            EntityKindData::Painting(p) => {
                vec![
                    crate::entity::painting::painting_texture_id(&p.variant),
                    crate::entity::painting::PAINTING_BACK_TEXTURE.to_string(),
                ]
            }
            EntityKindData::ItemFrame(f) => {
                if f.is_invisible {
                    Vec::new()
                } else if f.is_glowing {
                    vec![
                        crate::entity::item_frame::GLOW_ITEM_FRAME_BACK_TEXTURE.to_string(),
                        crate::entity::item_frame::ITEM_FRAME_WOOD_TEXTURE.to_string(),
                    ]
                } else {
                    vec![
                        crate::entity::item_frame::ITEM_FRAME_BACK_TEXTURE.to_string(),
                        crate::entity::item_frame::ITEM_FRAME_WOOD_TEXTURE.to_string(),
                    ]
                }
            }
            EntityKindData::ArmorStand(_) => Vec::new(),
            EntityKindData::Generic { .. } => Vec::new(),
        }
    }

    /// Collects all unique blockstate strings required to render this entity.
    pub fn required_blockstates(&self) -> Vec<String> {
        Vec::new()
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
    Generic {
        custom_id: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        payload: Vec<u8>,
    },
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
    /// Optional block coordinate of the support/anchor block (if loaded from save).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_block: Option<[i32; 3]>,
}

/// Data payload for an Item Frame entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemFrameData {
    /// Item identifier held in frame (e.g. "minecraft:diamond_sword").
    #[serde(default)]
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

fn default_scale() -> Vec3 {
    Vec3::ONE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_painting() -> EntityInstance {
        EntityInstance::new_painting(
            Some("12345678-1234-1234-1234-123456789abc".to_string()),
            DVec3::new(10.5, 64.0, -20.5),
            Direction::South,
            Vec3::new(0.0, std::f32::consts::PI, 0.0),
            "kebab",
            1,
            1,
            Some([10, 64, -20]),
        )
    }

    #[test]
    fn test_valid_entities_pass_validation() {
        valid_painting().validate().expect("valid painting");
        EntityInstance::new_item_frame(
            None,
            DVec3::new(5.0, 65.0, 3.0),
            Direction::West,
            Vec3::ZERO,
            "minecraft:diamond_sword",
            1,
            2,
            false,
            false,
            None,
        )
        .validate()
        .expect("valid item frame");
    }

    #[test]
    fn test_validation_rejects_id_kind_mismatch() {
        let mut painting = valid_painting();
        painting.id = "minecraft:item_frame".to_string();
        assert!(matches!(
            painting.validate(),
            Err(EntityValidationError::IdKindMismatch { .. })
        ));
    }

    #[test]
    fn test_validation_rejects_invalid_painting_fields() {
        let mut painting = valid_painting();
        if let EntityKindData::Painting(p) = &mut painting.kind {
            p.width = 0;
        }
        assert!(matches!(
            painting.validate(),
            Err(EntityValidationError::InvalidPaintingDimensions { .. })
        ));

        let mut painting = valid_painting();
        painting.facing = Some(Direction::Up);
        assert!(matches!(
            painting.validate(),
            Err(EntityValidationError::NonHorizontalPaintingFacing(_))
        ));
    }

    #[test]
    fn test_validation_rejects_bad_scale_and_rotation() {
        let mut painting = valid_painting();
        painting.scale = Vec3::ZERO;
        assert!(matches!(
            painting.validate(),
            Err(EntityValidationError::InvalidScale)
        ));

        let mut painting = valid_painting();
        painting.rotation.x = f32::NAN;
        assert!(matches!(
            painting.validate(),
            Err(EntityValidationError::NonFiniteRotation)
        ));
    }

    #[test]
    fn test_validation_rejects_item_frame_rotation_step() {
        let mut frame = EntityInstance::new_item_frame(
            None,
            DVec3::new(5.0, 65.0, 3.0),
            Direction::West,
            Vec3::ZERO,
            "minecraft:diamond_sword",
            1,
            2,
            false,
            false,
            None,
        );
        if let EntityKindData::ItemFrame(f) = &mut frame.kind {
            f.rotation = 8;
        }
        assert!(matches!(
            frame.validate(),
            Err(EntityValidationError::InvalidItemFrameRotation(8))
        ));
    }

    #[test]
    fn test_required_textures_normalizes_painting_variant() {
        let mut painting = valid_painting();
        if let EntityKindData::Painting(p) = &mut painting.kind {
            p.variant = "minecraft:kebab".to_string();
        }
        let textures = painting.required_textures();
        assert!(textures.contains(&"minecraft:painting/kebab".to_string()));
        assert!(!textures
            .iter()
            .any(|t| t.contains("minecraft:painting/minecraft:")));
    }

    #[test]
    fn test_supported_visual_entities_schema_declaration() {
        let ids = supported_visual_entity_ids();
        assert!(ids.contains(&ENTITY_ID_PAINTING));
        assert!(ids.contains(&ENTITY_ID_ITEM_FRAME));
        assert!(ids.contains(&ENTITY_ID_GLOW_ITEM_FRAME));
        assert!(ids.contains(&ENTITY_ID_ARMOR_STAND));

        assert!(is_supported_visual_entity("minecraft:painting"));
        assert!(is_supported_visual_entity("painting"));
        assert!(is_supported_visual_entity("minecraft:armor_stand"));
        assert!(!is_supported_visual_entity("minecraft:creeper"));

        let painting_desc =
            find_supported_visual_entity(ENTITY_ID_PAINTING).expect("painting desc");
        assert_eq!(painting_desc.family, "painting");
        assert!(painting_desc.supports_mesh_baking());

        let frame_desc =
            find_supported_visual_entity(ENTITY_ID_ITEM_FRAME).expect("item frame desc");
        assert_eq!(frame_desc.family, "item_frame");
        assert!(frame_desc.supports_mesh_baking());
    }
}
