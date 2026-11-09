//! # Scene Asset Collector
//!
//! Gathers all blockstates, referenced textures, fluid textures (via `FluidType`),
//! and block entity visuals (skulls, decorated pots, banners) needed by a scene.

use std::collections::BTreeSet;

use mtk_model::{BakedModelDatabase, BlockEntityData};
use mtk_voxel::{FluidType, SparseVoxelPayload};

/// Collected asset identifiers needed by the scene.
#[derive(Debug, Default, Clone)]
pub struct NeededSceneAssets {
    /// Texture paths/identifiers needed across all models and entities.
    pub needed_textures: BTreeSet<String>,
    /// Unique blockstates referenced in the sparse voxel grid.
    pub needed_blockstates: BTreeSet<String>,
    /// Relevant tree-shaken CTM rules for this scene.
    pub shaken_ctm_rules: Vec<mtk_resource::CtmRule>,
}

/// Collects all unique blockstate strings and texture paths required to render the voxels.
pub fn collect_needed_assets(
    voxels: &SparseVoxelPayload,
    source_models: &BakedModelDatabase,
    source_ctm_rules: Option<&[mtk_resource::CtmRule]>,
) -> NeededSceneAssets {
    let mut needed_textures = BTreeSet::new();
    let mut needed_blockstates = BTreeSet::new();

    // 1. Inspect blockstates and their model elements / faces
    for state in &voxels.state_palette {
        needed_blockstates.insert(state.clone());
        if let Some(model) = source_models.get(state) {
            for elem in &model.elements {
                for face in elem.faces.values() {
                    if !face.texture.is_empty() {
                        needed_textures.insert(face.texture.clone());
                    }
                }
            }
            for face in &model.faces {
                if !face.texture.is_empty() {
                    needed_textures.insert(face.texture.clone());
                }
            }
            for obj_face in &model.obj_faces {
                if !obj_face.texture.is_empty() {
                    needed_textures.insert(obj_face.texture.clone());
                }
            }
        }

        if let Some(group) = source_models.get_variant_group(state) {
            for model in &group.models {
                for elem in &model.elements {
                    for face in elem.faces.values() {
                        if !face.texture.is_empty() {
                            needed_textures.insert(face.texture.clone());
                        }
                    }
                }
                for face in &model.faces {
                    if !face.texture.is_empty() {
                        needed_textures.insert(face.texture.clone());
                    }
                }
                for obj_face in &model.obj_faces {
                    if !obj_face.texture.is_empty() {
                        needed_textures.insert(obj_face.texture.clone());
                    }
                }
            }
        }
    }

    // 2. Canonical fluid resolution via FluidType
    for state in &voxels.state_palette {
        let fluid_type = FluidType::from_name(state).or_else(|| {
            if state.contains("waterlogged=true")
                || state.contains("seagrass")
                || state.contains("kelp")
            {
                Some(FluidType::Water)
            } else {
                None
            }
        });

        if let Some(fluid) = fluid_type {
            match fluid {
                FluidType::Water => {
                    needed_textures.insert("minecraft:block/water_still".to_string());
                    needed_textures.insert("minecraft:block/water_flow".to_string());
                }
                FluidType::Lava => {
                    needed_textures.insert("minecraft:block/lava_still".to_string());
                    needed_textures.insert("minecraft:block/lava_flow".to_string());
                }
            }
        }
    }

    // 3. Inspect block entity visual NBTs for custom texture identifiers
    for entity_json in &voxels.entity_palette {
        if let Some(be) = BlockEntityData::from_visual_json(entity_json) {
            if let Some(tex) = be.texture_identifier() {
                needed_textures.insert(tex);
            }
            match &be {
                BlockEntityData::DecoratedPot { sherds } => {
                    for sherd in sherds.iter().flatten() {
                        needed_textures.insert(BlockEntityData::sherd_texture(Some(sherd)));
                    }
                }
                BlockEntityData::Banner {
                    base_color,
                    patterns,
                } => {
                    needed_textures.insert("minecraft:entity/banner/banner_base".to_string());
                    needed_textures.insert(mtk_model::banner::banner_texture_identifier(
                        base_color, patterns,
                    ));
                }
                _ => {}
            }
        }
    }

    // 4. Banner states without explicit entity visual JSON
    for state in &voxels.state_palette {
        if let Some(color) = mtk_model::banner::extract_banner_color_from_state(state) {
            needed_textures.insert("minecraft:entity/banner/banner_base".to_string());
            needed_textures.insert(mtk_model::banner::banner_texture_identifier(color, &[]));
        }
    }

    // 5. CTM Tree-shaking: collect rules matching needed blockstates or base textures
    let mut shaken_ctm_rules = Vec::new();
    if let Some(rules) = source_ctm_rules {
        for rule in rules {
            if rule.applies_to_scene(&needed_blockstates, &needed_textures) {
                for tile_loc in rule.tiles.iter().flatten() {
                    needed_textures.insert(tile_loc.to_string());
                }
                shaken_ctm_rules.push(rule.clone());
            }
        }
    }

    NeededSceneAssets {
        needed_textures,
        needed_blockstates,
        shaken_ctm_rules,
    }
}
