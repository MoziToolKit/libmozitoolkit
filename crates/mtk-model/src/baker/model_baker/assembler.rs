use std::collections::HashMap;

use glam::{Vec2, Vec3};
use mtk_core::Direction;

use super::super::emissive::is_block_emissive;
use super::super::planar::should_skip_zero_thickness_face;
use super::super::rules::{is_known_non_cube, is_opaque_block};
use super::ModelBaker;
use crate::baker::baked_model::{BakedElement, BakedFace, BakedModel};
use crate::baker::math::{bake_face_exact, rotate_direction};
use crate::error::ModelError;
use crate::parser::blockstate::{BlockState, VariantMatch};
use crate::parser::model_json::BlockModelJson;

impl ModelBaker {
    pub(crate) fn bake_matches_with_entity<F>(
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
}
