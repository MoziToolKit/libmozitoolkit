#![forbid(unsafe_code)]
//! Host-agnostic entity render plan contract.
//!
//! A render plan is the boundary between the domain model and a DCC host. It
//! describes *what objects should exist*, while the host remains responsible for
//! creating its native objects, parenting them, and binding host-specific
//! materials.

use std::collections::{BTreeMap, HashMap};

use glam::{EulerRot, Mat4, Quat, Vec3};
use mtk_core::{
    mc_rotation_to_z_up, mc_scale_to_z_up, mc_world_to_z_up, CoordinateSystem, MeshData,
};
use serde::{Deserialize, Serialize};

use crate::entity::baker::EntityBaker;
use crate::entity::instance::{entity_family, EntityInstance, EntityKindData};

/// Version of the entity render plan schema.
pub const ENTITY_RENDER_SCHEMA_VERSION: u32 = 1;

/// Configuration for building an entity render plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRenderConfig {
    /// Source revision/generation used by the host to discard stale async plans.
    pub revision: u64,
    /// When true, the host should delete existing entity objects whose keys are
    /// not present in this plan. When false, the plan is additive only.
    pub authoritative: bool,
    /// Optional scene bounds used by the host for origin-centering calculations:
    /// `[min_x, min_y, min_z, size_x, size_y, size_z]`.
    pub bounds: Option<[i32; 6]>,
    /// Whether to center origin at bottom-center of the scene bounds (matches mesher origin_centered).
    pub origin_centered: bool,
    /// Target coordinate system convention (defaults to Z-up right-handed for Blender/DCC).
    pub coordinate_system: CoordinateSystem,
}

impl Default for EntityRenderConfig {
    fn default() -> Self {
        Self {
            revision: 0,
            authoritative: true,
            bounds: None,
            origin_centered: true,
            coordinate_system: CoordinateSystem::ZUpRightHanded,
        }
    }
}

/// Host-space rigid transform for an entity node.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EntityRenderTransform {
    /// Host position [x, y, z] (e.g. Blender Z-Up coords, taking origin centering into account).
    pub location: [f32; 3],
    /// Host local rotation Euler XYZ in radians [pitch, yaw, roll].
    pub rotation_euler: [f32; 3],
    /// Host scale factor [scale_x, scale_y, scale_z].
    pub scale: [f32; 3],
    /// Column-major 4x4 affine transform matrix for direct GPU / DCC scene graph consumption.
    pub matrix: [f32; 16],
}

impl EntityRenderTransform {
    /// Computes the host-space transform for an entity instance given the render configuration.
    pub fn from_entity(entity: &EntityInstance, config: &EntityRenderConfig) -> Self {
        let centered_pos = if config.origin_centered {
            if let Some(bounds) = config.bounds {
                let cx = bounds[0] as f64 + bounds[3] as f64 / 2.0;
                let by = bounds[1] as f64;
                let cz = bounds[2] as f64 + bounds[5] as f64 / 2.0;
                glam::DVec3::new(
                    entity.position.x - cx,
                    entity.position.y - by,
                    entity.position.z - cz,
                )
            } else {
                entity.position
            }
        } else {
            entity.position
        };

        let loc_vec = match config.coordinate_system {
            CoordinateSystem::Minecraft | CoordinateSystem::YUpRightHanded => Vec3::new(
                centered_pos.x as f32,
                centered_pos.y as f32,
                centered_pos.z as f32,
            ),
            CoordinateSystem::ZUpRightHanded => mc_world_to_z_up(
                centered_pos.x as f32,
                centered_pos.y as f32,
                centered_pos.z as f32,
            ),
        };

        let rot_vec = match config.coordinate_system {
            CoordinateSystem::Minecraft | CoordinateSystem::YUpRightHanded => entity.rotation,
            CoordinateSystem::ZUpRightHanded => mc_rotation_to_z_up(entity.rotation),
        };

        let scale_vec = match config.coordinate_system {
            CoordinateSystem::Minecraft | CoordinateSystem::YUpRightHanded => entity.scale,
            CoordinateSystem::ZUpRightHanded => mc_scale_to_z_up(entity.scale),
        };

        let quat = Quat::from_euler(EulerRot::XYZ, rot_vec.x, rot_vec.y, rot_vec.z);
        let mat = Mat4::from_scale_rotation_translation(scale_vec, quat, loc_vec);

        Self {
            location: [loc_vec.x, loc_vec.y, loc_vec.z],
            rotation_euler: [rot_vec.x, rot_vec.y, rot_vec.z],
            scale: [scale_vec.x, scale_vec.y, scale_vec.z],
            matrix: mat.to_cols_array(),
        }
    }
}

/// Atlas/material routing for one texture used by an entity mesh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityMaterialRoute {
    /// Canonical source texture key (for example `minecraft:painting/kebab`).
    pub texture_key: String,
    /// Material slot used by the baked `MeshData` face buffer.
    pub material_slot: u16,
    /// Target atlas chunk id when the texture was resolved.
    pub atlas_chunk_id: Option<u16>,
    /// Target atlas texture id when the texture was resolved.
    pub atlas_texture_id: Option<u32>,
}

/// One entity object that the host should create or update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityRenderNode {
    /// Stable identity used by the host for create/update/delete diffing.
    pub key: String,
    /// Suggested display name; hosts may sanitize or uniquify it.
    pub display_name: String,
    /// Canonical entity data, including continuous world position and model attributes.
    pub entity: EntityInstance,
    /// Host-space rigid transform (location, rotation, scale, matrix).
    pub transform: EntityRenderTransform,
    /// Baked geometry in local model space (centered at the entity origin).
    pub local_mesh: MeshData,
    /// Texture/material routes for the baked mesh.
    pub material_routes: Vec<EntityMaterialRoute>,
    /// Host-agnostic metadata. Hosts map these keys into their own property namespace.
    pub metadata: BTreeMap<String, String>,
}

/// Non-fatal issue encountered while building a render plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRenderWarning {
    /// Key of the entity that produced the warning.
    pub key: String,
    /// Stable machine-readable warning code.
    pub code: String,
    /// Human-readable diagnostic text.
    pub message: String,
}

/// Desired set of entity render nodes for a scene revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityRenderPlan {
    /// Schema version consumed by hosts and generated mirrors.
    pub schema_version: u32,
    /// Source scene revision used to discard stale plans.
    pub revision: u64,
    /// Whether absent entity keys should be deleted by the host.
    pub authoritative: bool,
    /// Optional scene bounds for host origin-centering calculations.
    pub bounds: Option<[i32; 6]>,
    /// Desired entity nodes.
    pub nodes: Vec<EntityRenderNode>,
    /// Non-fatal issues (unsupported/invalid entities, failed bakes).
    pub warnings: Vec<EntityRenderWarning>,
}

impl EntityRenderPlan {
    /// Builds a plan from canonical entities using an atlas lookup closure.
    ///
    /// Entities that fail validation or do not yet have a procedural baker are
    /// reported through `warnings` instead of silently disappearing.
    pub fn build<F>(
        entities: &[EntityInstance],
        config: EntityRenderConfig,
        mut atlas_lookup: F,
    ) -> Self
    where
        F: FnMut(&str) -> Option<([f32; 4], u16, u32)>,
    {
        let mut nodes = Vec::new();
        let mut warnings = Vec::new();

        for (index, entity) in entities.iter().enumerate() {
            let key = entity_render_key(entity, index);

            if let Err(error) = entity.validate() {
                warnings.push(EntityRenderWarning {
                    key,
                    code: "invalid_entity".to_string(),
                    message: error.to_string(),
                });
                continue;
            }

            if !matches!(&entity.kind, EntityKindData::Painting(_)) {
                warnings.push(EntityRenderWarning {
                    key,
                    code: "unsupported_entity".to_string(),
                    message: format!(
                        "Entity '{}' does not have a procedural local-mesh baker yet",
                        entity.id
                    ),
                });
                continue;
            }

            let mut resolved_routes: HashMap<String, (u16, u32)> = HashMap::new();
            let baked = EntityBaker::bake_entity_mesh(entity, |texture_key| {
                let resolved = atlas_lookup(texture_key);
                if let Some((_, chunk_id, texture_id)) = resolved {
                    resolved_routes.insert(texture_key.to_string(), (chunk_id, texture_id));
                }
                resolved
            });

            let Some((local_mesh, textures)) = baked else {
                warnings.push(EntityRenderWarning {
                    key,
                    code: "bake_failed".to_string(),
                    message: format!("Failed to bake local mesh for entity '{}'", entity.id),
                });
                continue;
            };

            let material_routes = textures
                .iter()
                .enumerate()
                .map(|(texture_slot, texture_key)| {
                    let resolved = resolved_routes.get(texture_key).copied();
                    EntityMaterialRoute {
                        texture_key: texture_key.clone(),
                        material_slot: resolved
                            .map(|(chunk_id, _)| chunk_id)
                            .unwrap_or(texture_slot as u16),
                        atlas_chunk_id: resolved.map(|(chunk_id, _)| chunk_id),
                        atlas_texture_id: resolved.map(|(_, texture_id)| texture_id),
                    }
                })
                .collect();

            let transform = EntityRenderTransform::from_entity(entity, &config);

            let mut local_mesh = local_mesh;
            local_mesh.transform_coordinate_system(config.coordinate_system);

            nodes.push(EntityRenderNode {
                display_name: entity_display_name(entity, index),
                key,
                metadata: entity_metadata(entity),
                entity: entity.clone(),
                transform,
                local_mesh,
                material_routes,
            });
        }

        Self {
            schema_version: ENTITY_RENDER_SCHEMA_VERSION,
            revision: config.revision,
            authoritative: config.authoritative,
            bounds: config.bounds,
            nodes,
            warnings,
        }
    }
}

/// Builds an entity render plan. This is the canonical entry point for hosts and bindings.
pub fn build_entity_render_plan<F>(
    entities: &[EntityInstance],
    config: EntityRenderConfig,
    atlas_lookup: F,
) -> EntityRenderPlan
where
    F: FnMut(&str) -> Option<([f32; 4], u16, u32)>,
{
    EntityRenderPlan::build(entities, config, atlas_lookup)
}

/// Returns a stable identity key for host-side create/update/delete diffing.
pub fn entity_render_key(entity: &EntityInstance, index: usize) -> String {
    if let Some(uuid) = &entity.uuid {
        return format!("uuid:{uuid}");
    }

    let kind_key = match &entity.kind {
        EntityKindData::Painting(p) => format!("painting:{}", p.variant),
        EntityKindData::ItemFrame(f) => format!("item_frame:{}", f.item_id),
        EntityKindData::ArmorStand(_) => "armor_stand".to_string(),
        EntityKindData::Generic { custom_id, .. } => format!("generic:{custom_id}"),
    };

    format!(
        "synth:{}:{}:{:.3},{:.3},{:.3}:{:03}",
        entity.id, kind_key, entity.position.x, entity.position.y, entity.position.z, index
    )
}

fn entity_display_name(entity: &EntityInstance, index: usize) -> String {
    let slug = entity_family(&entity.id);
    if let Some(custom_name) = &entity.custom_name {
        return format!("{custom_name}_{slug}");
    }
    if let Some(uuid) = &entity.uuid {
        let short: String = uuid.chars().filter(|c| *c != '-').take(8).collect();
        return format!("{slug}_{short}");
    }

    let suffix = match &entity.kind {
        EntityKindData::Painting(p) => p.variant.clone(),
        EntityKindData::ItemFrame(f) if !f.item_id.is_empty() => f.item_id.clone(),
        _ => format!("{:03}", index + 1),
    };
    format!("{slug}_{suffix}_{:03}", index + 1)
}

fn entity_metadata(entity: &EntityInstance) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert("entity_id".to_string(), entity.id.clone());
    if let Some(uuid) = &entity.uuid {
        metadata.insert("uuid".to_string(), uuid.clone());
    }
    if let Some(custom_name) = &entity.custom_name {
        metadata.insert("custom_name".to_string(), custom_name.clone());
    }
    if let Some(facing) = entity.facing {
        metadata.insert("facing".to_string(), facing.as_str().to_string());
    }

    match &entity.kind {
        EntityKindData::Painting(p) => {
            metadata.insert("variant".to_string(), p.variant.clone());
            metadata.insert("width".to_string(), p.width.to_string());
            metadata.insert("height".to_string(), p.height.to_string());
            if let Some(anchor) = p.anchor_block {
                metadata.insert(
                    "anchor_block".to_string(),
                    format!("{},{},{}", anchor[0], anchor[1], anchor[2]),
                );
            }
        }
        EntityKindData::ItemFrame(f) => {
            metadata.insert("item_id".to_string(), f.item_id.clone());
            metadata.insert("item_rotation".to_string(), f.rotation.to_string());
            metadata.insert("is_glowing".to_string(), f.is_glowing.to_string());
            metadata.insert("is_invisible".to_string(), f.is_invisible.to_string());
            if let Some(map_id) = f.map_id {
                metadata.insert("map_id".to_string(), map_id.to_string());
            }
        }
        EntityKindData::ArmorStand(a) => {
            metadata.insert("show_arms".to_string(), a.show_arms.to_string());
            metadata.insert("no_base_plate".to_string(), a.no_base_plate.to_string());
            metadata.insert("is_small".to_string(), a.is_small.to_string());
        }
        EntityKindData::Generic { custom_id, .. } => {
            metadata.insert("custom_id".to_string(), custom_id.clone());
        }
    }

    metadata
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{DVec3, Vec3};
    use mtk_core::Direction;

    fn painting(uuid: Option<&str>, variant: &str, width: u8, height: u8) -> EntityInstance {
        EntityInstance::new_painting(
            uuid.map(str::to_string),
            DVec3::new(10.5, 64.0, -20.5),
            Direction::South,
            Vec3::new(0.0, std::f32::consts::PI, 0.0),
            variant,
            width,
            height,
            Some([10, 64, -20]),
        )
    }

    #[test]
    fn test_build_entity_render_plan_painting() {
        let entities = vec![painting(
            Some("12345678-1234-1234-1234-123456789abc"),
            "kebab",
            1,
            1,
        )];
        let config = EntityRenderConfig {
            revision: 42,
            authoritative: true,
            bounds: Some([0, 0, 0, 16, 16, 16]),
            origin_centered: true,
            coordinate_system: CoordinateSystem::ZUpRightHanded,
        };
        let plan = EntityRenderPlan::build(&entities, config, |_texture| {
            Some(([0.0, 0.0, 0.5, 0.5], 7, 9))
        });

        assert_eq!(plan.schema_version, ENTITY_RENDER_SCHEMA_VERSION);
        assert_eq!(plan.revision, 42);
        assert!(plan.authoritative);
        assert_eq!(plan.bounds, Some([0, 0, 0, 16, 16, 16]));
        assert!(plan.warnings.is_empty());
        assert_eq!(plan.nodes.len(), 1);

        let node = &plan.nodes[0];
        assert_eq!(node.key, "uuid:12345678-1234-1234-1234-123456789abc");
        assert_eq!(node.metadata.get("variant"), Some(&"kebab".to_string()));
        assert_eq!(node.metadata.get("width"), Some(&"1".to_string()));
        assert!(node.local_mesh.face_count() > 0);
        // Validate local mesh coordinate transformation (Z-up basis: X width, Y thickness, Z height)
        let (mut min_x, mut max_x) = (f32::INFINITY, f32::NEG_INFINITY);
        let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
        let (mut min_z, mut max_z) = (f32::INFINITY, f32::NEG_INFINITY);
        for p in &node.local_mesh.positions {
            min_x = min_x.min(p[0]);
            max_x = max_x.max(p[0]);
            min_y = min_y.min(p[1]);
            max_y = max_y.max(p[1]);
            min_z = min_z.min(p[2]);
            max_z = max_z.max(p[2]);
        }
        assert!((min_x - (-0.5)).abs() < 1e-4);
        assert!((max_x - 0.5).abs() < 1e-4);
        assert!((min_y - (-0.03125)).abs() < 1e-4);
        assert!((max_y - 0.03125).abs() < 1e-4);
        assert!((min_z - (-0.5)).abs() < 1e-4);
        assert!((max_z - 0.5).abs() < 1e-4);
        assert_eq!(node.material_routes.len(), 2);
        for route in &node.material_routes {
            assert_eq!(route.atlas_chunk_id, Some(7));
            assert_eq!(route.atlas_texture_id, Some(9));
            assert_eq!(route.material_slot, 7);
        }

        // Validate computed Z-Up transform
        assert_eq!(node.transform.location, [2.5, 28.5, 64.0]);
        assert!((node.transform.rotation_euler[2].abs() - std::f32::consts::PI).abs() < 1e-4);
        assert_eq!(node.transform.scale, [1.0, 1.0, 1.0]);
        assert_eq!(node.transform.matrix.len(), 16);
    }

    #[test]
    fn test_entity_render_transform_minecraft_and_no_centering() {
        let ent = painting(None, "kebab", 1, 1);

        // 1. Without origin centering (Minecraft coordinates)
        let config_raw = EntityRenderConfig {
            origin_centered: false,
            coordinate_system: CoordinateSystem::Minecraft,
            ..Default::default()
        };
        let t_raw = EntityRenderTransform::from_entity(&ent, &config_raw);
        assert_eq!(t_raw.location, [10.5, 64.0, -20.5]);
        assert_eq!(t_raw.rotation_euler, [0.0, std::f32::consts::PI, 0.0]);
        assert_eq!(t_raw.scale, [1.0, 1.0, 1.0]);

        // 2. Without origin centering (Z-Up coordinates)
        let config_z_up = EntityRenderConfig {
            origin_centered: false,
            coordinate_system: CoordinateSystem::ZUpRightHanded,
            ..Default::default()
        };
        let t_z_up = EntityRenderTransform::from_entity(&ent, &config_z_up);
        assert_eq!(t_z_up.location, [10.5, 20.5, 64.0]);
        assert!((t_z_up.rotation_euler[2].abs() - std::f32::consts::PI).abs() < 1e-4);
        assert_eq!(t_z_up.scale, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn test_build_plan_reports_unsupported_and_invalid_entities() {
        let mut invalid = painting(None, "kebab", 1, 1);
        invalid.id = "minecraft:item_frame".to_string();

        let frame = EntityInstance::new_item_frame(
            None,
            DVec3::new(5.0, 65.0, 3.0),
            Direction::West,
            Vec3::ZERO,
            "minecraft:diamond_sword",
            1,
            0,
            false,
            false,
            None,
        );

        let plan = EntityRenderPlan::build(
            &[invalid, frame],
            EntityRenderConfig::default(),
            |_texture| None,
        );

        assert!(plan.nodes.is_empty());
        assert_eq!(plan.warnings.len(), 2);
        assert_eq!(plan.warnings[0].code, "invalid_entity");
        assert_eq!(plan.warnings[1].code, "unsupported_entity");
    }

    #[test]
    fn test_entity_render_key_is_stable_for_uuid() {
        let entity = painting(Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"), "kebab", 1, 1);
        assert_eq!(
            entity_render_key(&entity, 3),
            "uuid:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
        );
    }

    #[test]
    fn test_entity_render_plan_serialization_roundtrip() {
        let entities = vec![painting(None, "kebab", 1, 1)];
        let plan = EntityRenderPlan::build(&entities, EntityRenderConfig::default(), |_texture| {
            Some(([0.0, 0.0, 0.5, 0.5], 1, 2))
        });

        let json = serde_json::to_string(&plan).expect("serialize render plan");
        let parsed: EntityRenderPlan =
            serde_json::from_str(&json).expect("deserialize render plan");
        assert_eq!(plan, parsed);
    }
}
