use std::collections::HashMap;

use glam::{Vec2, Vec3};
use mtk_core::Direction;

use super::emissive::is_block_emissive;
use super::planar::should_skip_zero_thickness_face;
use super::rules::{is_known_non_cube, is_opaque_block};
use crate::baker::baked_model::{BakedElement, BakedFace, BakedModel, BakedVariantGroup};
use crate::baker::math::{bake_face_exact, rotate_direction};
use crate::builtin::BuiltinModelRegistry;
use crate::error::ModelError;
use crate::parser::blockstate::{
    BlockState, BlockStateDefinition, BlockStateResolver, VariantMatch,
};
use crate::parser::model_json::{BlockModelJson, ResolvedBlockModel};

/// Universal, headless Minecraft Model Baker.
///
/// Bakes arbitrary BlockStates and models (both native Minecraft JSON models and Mod Wavefront OBJ meshes)
/// into fully realized geometry completely independent of Blender or any external 3D runtime.
#[derive(Default, Clone)]
pub struct ModelBaker {
    /// In-memory cache for baked models keyed by canonical BlockState string.
    bake_cache: HashMap<String, BakedModel>,
    /// Thread-local cache for dynamically resolved model hierarchies.
    resolved_cache: HashMap<String, std::sync::Arc<ResolvedBlockModel>>,
    /// Shared pre-resolved model database (zero-copy read across threads).
    shared_resolved: Option<std::sync::Arc<HashMap<String, std::sync::Arc<ResolvedBlockModel>>>>,
}

impl ModelBaker {
    /// Creates a new `ModelBaker` instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new `ModelBaker` sharing a pre-resolved model database.
    pub fn with_shared_resolved(
        shared: std::sync::Arc<HashMap<String, std::sync::Arc<ResolvedBlockModel>>>,
    ) -> Self {
        Self {
            bake_cache: HashMap::new(),
            resolved_cache: HashMap::new(),
            shared_resolved: Some(shared),
        }
    }

    /// Clears the internal bake cache and resolved model cache.
    pub fn clear_cache(&mut self) {
        self.bake_cache.clear();
        self.resolved_cache.clear();
    }

    /// Bakes a BlockState into a `BakedModel` using supplied JSON definitions and model loaders.
    ///
    /// - `state_str`: E.g. `"minecraft:oak_stairs[facing=east,half=bottom,shape=straight]"`
    /// - `blockstate_def`: Optional `BlockStateDefinition` (variants or multipart). If None, uses heuristic.
    /// - `model_loader`: Callback retrieving raw `BlockModelJson` given a model ID path.
    pub fn bake_blockstate<F>(
        &mut self,
        state_str: &str,
        blockstate_def: Option<&BlockStateDefinition>,
        model_loader: F,
    ) -> Result<BakedModel, ModelError>
    where
        F: FnMut(&str) -> Option<BlockModelJson>,
    {
        self.bake_blockstate_with_entity(state_str, None, blockstate_def, model_loader)
    }

    /// Bakes a BlockState with optional `BlockEntityData` into a `BakedModel`.
    pub fn bake_blockstate_with_entity<F>(
        &mut self,
        state_str: &str,
        entity: Option<&crate::entity::BlockEntityData>,
        blockstate_def: Option<&BlockStateDefinition>,
        mut model_loader: F,
    ) -> Result<BakedModel, ModelError>
    where
        F: FnMut(&str) -> Option<BlockModelJson>,
    {
        let blockstate = BlockState::parse(state_str)?;
        let canonical_str = blockstate.to_canonical_string();

        if let Some(baked) =
            BuiltinModelRegistry::get_builtin_baked_model_with_entity(&blockstate, entity)
        {
            return Ok(baked);
        }
        let cache_key = match entity {
            Some(e) => format!("{}:{:?}", canonical_str, e),
            None => canonical_str.clone(),
        };

        if let Some(cached) = self.bake_cache.get(&cache_key) {
            return Ok(cached.clone());
        }

        let variant_matches = if let Some(def) = blockstate_def {
            BlockStateResolver::resolve(def, &blockstate)
        } else if let Some(builtin_def) =
            BuiltinModelRegistry::get_builtin_blockstate_def(&blockstate.name)
        {
            BlockStateResolver::resolve(&builtin_def, &blockstate)
        } else {
            vec![BlockStateResolver::heuristic_match(&blockstate)]
        };

        let baked_model = self.bake_matches_with_entity(
            &canonical_str,
            &blockstate,
            entity,
            &variant_matches,
            &mut model_loader,
        )?;
        self.bake_cache.insert(cache_key, baked_model.clone());
        Ok(baked_model)
    }

    /// Bakes all discrete variants of a BlockState into a `BakedVariantGroup`.
    pub fn bake_blockstate_variants<F>(
        &mut self,
        state_str: &str,
        blockstate_def: Option<&BlockStateDefinition>,
        mut model_loader: F,
    ) -> Result<BakedVariantGroup, ModelError>
    where
        F: FnMut(&str) -> Option<BlockModelJson>,
    {
        let blockstate = BlockState::parse(state_str)?;
        let canonical_str = blockstate.to_canonical_string();

        if let Some(baked) =
            BuiltinModelRegistry::get_builtin_baked_model_with_entity(&blockstate, None)
        {
            return Ok(BakedVariantGroup::new(canonical_str, vec![baked], vec![1]));
        }

        let variant_specs = if let Some(def) = blockstate_def {
            BlockStateResolver::resolve_variants(def, &blockstate)
        } else if let Some(builtin_def) =
            BuiltinModelRegistry::get_builtin_blockstate_def(&blockstate.name)
        {
            BlockStateResolver::resolve_variants(&builtin_def, &blockstate)
        } else {
            vec![(vec![BlockStateResolver::heuristic_match(&blockstate)], 1)]
        };

        let mut models = Vec::with_capacity(variant_specs.len());
        let mut weights = Vec::with_capacity(variant_specs.len());

        for (matches, weight) in variant_specs {
            let model =
                self.bake_matches(&canonical_str, &blockstate, &matches, &mut model_loader)?;
            models.push(model);
            weights.push(weight);
        }

        Ok(BakedVariantGroup::new(canonical_str, models, weights))
    }

    fn bake_matches<F>(
        &mut self,
        canonical_str: &str,
        blockstate: &BlockState,
        variant_matches: &[VariantMatch],
        model_loader: &mut F,
    ) -> Result<BakedModel, ModelError>
    where
        F: FnMut(&str) -> Option<BlockModelJson>,
    {
        self.bake_matches_with_entity(
            canonical_str,
            blockstate,
            None,
            variant_matches,
            model_loader,
        )
    }

    fn bake_matches_with_entity<F>(
        &mut self,
        canonical_str: &str,
        blockstate: &BlockState,
        entity: Option<&crate::entity::BlockEntityData>,
        variant_matches: &[VariantMatch],
        model_loader: &mut F,
    ) -> Result<BakedModel, ModelError>
    where
        F: FnMut(&str) -> Option<BlockModelJson>,
    {
        let mut baked_elements = Vec::new();
        let mut six_faces: [Option<BakedFace>; 6] = [None, None, None, None, None, None];

        let allow_blockstate_builtin = variant_matches.len() == 1;

        for variant in variant_matches {
            let resolved = self.resolve_variant_model_with_entity(
                &variant.model_id,
                blockstate,
                entity,
                allow_blockstate_builtin,
                model_loader,
            )?;

            for elem in &resolved.elements {
                let from_pos = elem.from;
                let to_pos = elem.to;
                let elem_rot = elem.rotation.as_ref();

                // DCC-First zero-thickness planar element face deduplication and single-sided baking:
                // Minecraft elements with zero thickness (plants like cross.json, crops, ladders, rails, decals)
                // define duplicate back-to-back faces with opposite normals purely to counteract OpenGL backface culling.
                // In modern DCC pipelines (Blender), double-sided rendering is handled via shaders, and overlapping
                // coplanar geometry causes Z-fighting and gets mistakenly culled as occluded interior contact faces.
                let is_full_cuboid = from_pos == [0.0, 0.0, 0.0]
                    && to_pos == [16.0, 16.0, 16.0]
                    && elem_rot.is_none();

                let mut elem_faces = HashMap::new();

                const CANONICAL_DIRS: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
                for orig_dir_str in CANONICAL_DIRS {
                    let face_data = match elem.faces.get(orig_dir_str) {
                        Some(f) => f,
                        None => continue,
                    };
                    let orig_dir = match Direction::parse_loose(orig_dir_str) {
                        Some(d) => d,
                        None => continue,
                    };

                    if should_skip_zero_thickness_face(from_pos, to_pos, orig_dir, &elem.faces) {
                        continue;
                    }

                    let cullface_dir = face_data
                        .cullface
                        .as_deref()
                        .and_then(Direction::parse_loose)
                        .or(if is_full_cuboid { Some(orig_dir) } else { None });

                    let uv_base = if let Some(uv) = face_data.uv {
                        if uv.iter().any(|&c| c > 16.0) {
                            64.0
                        } else {
                            16.0
                        }
                    } else {
                        16.0
                    };

                    let (eff_rot_x, eff_rot_y) = if elem.transform.is_some() {
                        (0.0, 0.0)
                    } else {
                        (variant.rot_x, variant.rot_y)
                    };

                    let baked_geom = bake_face_exact(
                        orig_dir,
                        from_pos,
                        to_pos,
                        face_data.uv,
                        face_data.rotation as f32,
                        eff_rot_x,
                        eff_rot_y,
                        elem_rot,
                        elem.transform.as_ref(),
                        variant.uvlock,
                        uv_base,
                    );

                    let rotated_cullface = cullface_dir.map(|cd| {
                        let mut d = rotate_direction(cd, eff_rot_x, eff_rot_y);
                        if let Some(bt) = elem.transform.as_ref() {
                            d = rotate_direction(d, bt.rotate[0], bt.rotate[1]);
                        }
                        d
                    });

                    let v1 = baked_geom.positions[1] - baked_geom.positions[0];
                    let v2 = baked_geom.positions[2] - baked_geom.positions[0];
                    let normal = v1.cross(v2).normalize_or_zero();

                    let baked_face = BakedFace {
                        direction: baked_geom.direction,
                        texture: face_data.texture.clone(),
                        uv_rot: baked_geom.detected_rotation,
                        uv_bounds: baked_geom.uv_bounds,
                        tint_index: face_data.tintindex,
                        cullface: rotated_cullface,
                        vertices: baked_geom.positions,
                        uvs: baked_geom.uvs,
                        normal,
                        ..Default::default()
                    };

                    let idx = baked_geom.direction.to_index();
                    if six_faces[idx].is_none() {
                        six_faces[idx] = Some(baked_face.clone());
                    }

                    elem_faces.insert(orig_dir, baked_face);
                }

                baked_elements.push(BakedElement {
                    from_pos,
                    to_pos,
                    faces: elem_faces,
                });
            }
        }

        // Fill missing 6-face summary entries
        let mut fallback_tex = "minecraft:block/dirt";
        'find_fallback: for el in &baked_elements {
            const CANONICAL_DIRS: [Direction; 6] = [
                Direction::Down,
                Direction::Up,
                Direction::North,
                Direction::South,
                Direction::West,
                Direction::East,
            ];
            for dir in CANONICAL_DIRS {
                if let Some(f) = el.faces.get(&dir) {
                    if !f.texture.is_empty() {
                        fallback_tex = f.texture.as_str();
                        break 'find_fallback;
                    }
                }
            }
        }

        let mut final_six_faces = [
            BakedFace::default(),
            BakedFace::default(),
            BakedFace::default(),
            BakedFace::default(),
            BakedFace::default(),
            BakedFace::default(),
        ];

        for dir in Direction::ALL {
            let idx = dir.to_index();
            if let Some(ref bf) = six_faces[idx] {
                final_six_faces[idx] = bf.clone();
            } else {
                final_six_faces[idx] = BakedFace {
                    direction: dir,
                    texture: fallback_tex.to_string(),
                    uv_rot: 0.0,
                    uv_bounds: [0.0, 0.0, 1.0, 1.0],
                    tint_index: -1,
                    cullface: None,
                    vertices: [Vec3::ZERO; 4],
                    uvs: [Vec2::ZERO; 4],
                    normal: dir.normal(),
                    ..Default::default()
                };
            }
        }

        let short_name = blockstate.name.as_str();
        let is_double_slab = short_name.ends_with("_slab")
            && blockstate.properties.get("type").map(|s| s.as_str()) == Some("double");

        let is_known_non_cube = is_known_non_cube(short_name, is_double_slab);
        let is_cube = !is_known_non_cube
            && !baked_elements.is_empty()
            && baked_elements
                .iter()
                .all(|el| el.from_pos == [0.0, 0.0, 0.0] && el.to_pos == [16.0, 16.0, 16.0]);

        let is_opaque = is_opaque_block(short_name);

        let emissive = is_block_emissive(blockstate);

        let mut quads: Vec<([glam::Vec3; 4], Direction)> = Vec::new();
        if !baked_elements.is_empty() {
            for elem in &baked_elements {
                for (&dir, face) in &elem.faces {
                    quads.push((face.vertices, dir));
                }
            }
        } else if is_cube {
            for dir in Direction::ALL {
                quads.push((final_six_faces[dir.to_index()].vertices, dir));
            }
        }
        let quads_slice = if quads.is_empty() {
            None
        } else {
            Some(quads.as_slice())
        };
        let cull_meta =
            mtk_cull::compute_block_cull_meta(canonical_str, quads_slice, Some(is_opaque));

        let mut baked_model = BakedModel {
            block_state: canonical_str.to_string(),
            elements: baked_elements,
            obj_faces: Vec::new(),
            faces: final_six_faces,
            is_cube,
            is_opaque,
            is_emissive: emissive,
            cull_meta: Some(cull_meta),
            culled_faces: Default::default(),
            unculled_faces: Default::default(),
        };
        baked_model.rebuild_face_buckets();

        Ok(baked_model)
    }

    fn resolve_variant_model_with_entity<F>(
        &mut self,
        model_id: &str,
        blockstate: &BlockState,
        entity: Option<&crate::entity::BlockEntityData>,
        allow_blockstate_builtin: bool,
        model_loader: &mut F,
    ) -> Result<std::sync::Arc<ResolvedBlockModel>, ModelError>
    where
        F: FnMut(&str) -> Option<BlockModelJson>,
    {
        let clean_block = blockstate
            .name
            .strip_prefix("minecraft:")
            .unwrap_or(&blockstate.name);
        let is_bell = clean_block == "bell";
        let has_entity = entity.is_some();

        // Fast path: check shared pre-resolved cache and local cache (only when no entity data)
        if !is_bell && !has_entity {
            if let Some(ref shared) = self.shared_resolved {
                if let Some(r) = shared.get(model_id) {
                    if !r.elements.is_empty() {
                        return Ok(std::sync::Arc::clone(r));
                    }
                }
            }
            if let Some(r) = self.resolved_cache.get(model_id) {
                if !r.elements.is_empty() {
                    return Ok(std::sync::Arc::clone(r));
                }
            }
        }

        // Full resolution path
        let mut can_cache = false;
        let r = if let Some(external) = model_loader(model_id) {
            let res = external.resolve_hierarchy(model_id, |id| {
                model_loader(id).or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
            })?;
            let is_missing_bell_body = is_bell && !res.textures.contains_key("bell_body");
            if res.elements.is_empty() || is_missing_bell_body {
                if allow_blockstate_builtin {
                    if let Some(builtin) =
                        BuiltinModelRegistry::get_builtin_model_with_entity(blockstate, entity)
                    {
                        builtin.resolve_hierarchy(model_id, |id| {
                            model_loader(id)
                                .or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
                        })?
                    } else if let Some(builtin) =
                        BuiltinModelRegistry::get_builtin_model_by_id(model_id)
                    {
                        builtin.resolve_hierarchy(model_id, |id| {
                            model_loader(id)
                                .or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
                        })?
                    } else {
                        res
                    }
                } else if let Some(builtin) =
                    BuiltinModelRegistry::get_builtin_model_by_id(model_id)
                {
                    builtin.resolve_hierarchy(model_id, |id| {
                        model_loader(id)
                            .or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
                    })?
                } else {
                    res
                }
            } else {
                can_cache = true;
                res
            }
        } else if allow_blockstate_builtin {
            if let Some(builtin) =
                BuiltinModelRegistry::get_builtin_model_with_entity(blockstate, entity)
            {
                builtin.resolve_hierarchy(model_id, |id| {
                    model_loader(id).or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
                })?
            } else if let Some(builtin) = BuiltinModelRegistry::get_builtin_model_by_id(model_id) {
                builtin.resolve_hierarchy(model_id, |id| {
                    model_loader(id).or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
                })?
            } else {
                ResolvedBlockModel::default()
            }
        } else if let Some(builtin) = BuiltinModelRegistry::get_builtin_model_by_id(model_id) {
            builtin.resolve_hierarchy(model_id, |id| {
                model_loader(id).or_else(|| BuiltinModelRegistry::get_builtin_model_by_id(id))
            })?
        } else {
            ResolvedBlockModel::default()
        };

        let arc_r = std::sync::Arc::new(r);
        if can_cache && !is_bell && !arc_r.elements.is_empty() {
            self.resolved_cache
                .insert(model_id.to_string(), std::sync::Arc::clone(&arc_r));
        }
        Ok(arc_r)
    }

    /// Bakes a Wavefront OBJ mesh into a `BakedModel`.
    #[allow(clippy::too_many_arguments)]
    pub fn bake_obj_model(
        &mut self,
        block_state: &str,
        obj_text: &str,
        texture_mappings: &HashMap<String, String>,
        fallback_texture: &str,
        rot_x: f32,
        rot_y: f32,
        rot_z: f32,
        offset: Vec3,
        object_filter: Option<&[&str]>,
    ) -> Result<BakedModel, ModelError> {
        super::obj_baker::bake_obj_model_impl(
            block_state,
            obj_text,
            texture_mappings,
            fallback_texture,
            rot_x,
            rot_y,
            rot_z,
            offset,
            object_filter,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bake_cube_blockstate() {
        let mut baker = ModelBaker::new();

        let model_json = r##"{
            "textures": {
                "all": "minecraft:block/stone"
            },
            "elements": [
                {
                    "from": [0, 0, 0],
                    "to": [16, 16, 16],
                    "faces": {
                        "down":  { "texture": "#all", "cullface": "down" },
                        "up":    { "texture": "#all", "cullface": "up" },
                        "north": { "texture": "#all", "cullface": "north" },
                        "south": { "texture": "#all", "cullface": "south" },
                        "west":  { "texture": "#all", "cullface": "west" },
                        "east":  { "texture": "#all", "cullface": "east" }
                    }
                }
            ]
        }"##;

        let parsed_model: BlockModelJson = serde_json::from_str(model_json).unwrap();

        let baked = baker
            .bake_blockstate("minecraft:stone", None, |_id| Some(parsed_model.clone()))
            .unwrap();

        assert!(baked.is_cube);
        assert_eq!(baked.elements.len(), 1);
        assert_eq!(
            baked.get_face(Direction::Up).texture,
            "minecraft:block/stone"
        );

        let mesh = baked.to_mesh(false);
        assert_eq!(mesh.face_count(), 6);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.vertex_count(), 24);
    }

    #[test]
    fn test_bake_obj_model() {
        let mut baker = ModelBaker::new();
        let obj_text = r#"
v 0 0 0
v 16 0 0
v 16 16 0
vt 0 0
vt 1 0
vt 1 1
f 1/1 2/2 3/3
"#;
        let baked = baker
            .bake_obj_model(
                "mod:custom_gear",
                obj_text,
                &HashMap::new(),
                "mod:block/gear",
                0.0,
                0.0,
                0.0,
                Vec3::ZERO,
                None,
            )
            .unwrap();

        assert!(!baked.is_cube);
        assert_eq!(baked.obj_faces.len(), 1);
        let mesh = baked.to_mesh(false);
        assert_eq!(mesh.triangle_count(), 1);
        assert_eq!(mesh.vertex_count(), 3);
    }

    #[test]
    fn test_bake_blockstate_variants() {
        let mut baker = ModelBaker::new();

        let model_json = r##"{
            "textures": { "all": "minecraft:block/dirt" },
            "elements": [{
                "from": [0, 0, 0],
                "to": [16, 16, 16],
                "faces": {
                    "up": { "texture": "#all", "cullface": "up" }
                }
            }]
        }"##;
        let parsed_model: BlockModelJson = serde_json::from_str(model_json).unwrap();

        let bs_def_json = r##"{
            "variants": {
                "": [
                    { "model": "minecraft:block/dirt" },
                    { "model": "minecraft:block/dirt", "y": 90 },
                    { "model": "minecraft:block/dirt", "y": 180 },
                    { "model": "minecraft:block/dirt", "y": 270 }
                ]
            }
        }"##;
        let bs_def: BlockStateDefinition = serde_json::from_str(bs_def_json).unwrap();

        let group = baker
            .bake_blockstate_variants("minecraft:dirt", Some(&bs_def), |_id| {
                Some(parsed_model.clone())
            })
            .unwrap();

        assert_eq!(group.len(), 4);
        assert_eq!(group.weights, vec![1, 1, 1, 1]);
        assert_eq!(group.total_weight, 4);

        assert_eq!(group.models[0].get_face(Direction::Up).uv_rot, 0.0);
        assert_eq!(group.models[1].get_face(Direction::Up).uv_rot, 90.0);
        assert_eq!(group.models[2].get_face(Direction::Up).uv_rot, 180.0);
        assert_eq!(group.models[3].get_face(Direction::Up).uv_rot, 270.0);
    }
}
