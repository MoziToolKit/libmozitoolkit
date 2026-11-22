use std::collections::HashMap;
use std::sync::Arc;

use mtk_core::attribute_constants::*;
use mtk_core::{AttributeData, Direction};
use mtk_model::{
    BlockModelJson, BlockStateDefinition, ModelBaker, ObjAssetStore, ParsedMtlLibrary,
    ParsedObjModel,
};

#[test]
fn test_neoforge_obj_parent_textures_flip_v_visibility_and_atlas_remap() {
    let mut store = ObjAssetStore::new();
    let obj_text = r#"
mtllib wheel.mtl
o VisibleWheel
usemtl m_plates
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 0.5 1.0 0.0
vt 0.0 0.2
vt 1.0 0.2
vt 0.5 0.9
vn 0.0 0.0 1.0
f 1/1/1 2/2/1 3/3/1

o HiddenBrace
usemtl m_hidden
v 0.0 0.0 1.0
v 1.0 0.0 1.0
v 1.0 1.0 1.0
v 0.0 1.0 1.0
vt 0.0 0.0
vt 1.0 0.0
vt 1.0 1.0
vt 0.0 1.0
f 4/4 5/5 6/6 7/7
"#;
    let mtl_text = r#"
newmtl m_plates
map_Kd #plates
forge_TintIndex 2

newmtl m_hidden
map_Kd #insert
"#;
    store.insert_obj(
        "assets/testmod/models/block/wheel.obj",
        ParsedObjModel::parse_str(obj_text),
    );
    store.insert_mtl(
        "assets/testmod/models/block/wheel.mtl",
        ParsedMtlLibrary::parse_str(mtl_text),
    );

    let parent_textures_json: BlockModelJson = serde_json::from_str(
        r##"{
            "textures": {
                "plates": "testmod:block/wheel_plates",
                "insert": "testmod:block/wheel_insert",
                "particle": "testmod:block/wheel_plates"
            }
        }"##,
    )
    .unwrap();

    let obj_model_json: BlockModelJson = serde_json::from_str(
        r##"{
            "parent": "testmod:block/wheel_textures",
            "loader": "neoforge:obj",
            "flip_v": true,
            "model": "testmod:models/block/wheel.obj",
            "visibility": {
                "HiddenBrace": false
            }
        }"##,
    )
    .unwrap();

    let bs_def: BlockStateDefinition = serde_json::from_str(
        r##"{
            "variants": {
                "facing=west": { "model": "testmod:block/wheel", "y": -90 },
                "facing=east": { "model": "testmod:block/wheel", "y": 270 }
            }
        }"##,
    )
    .unwrap();

    let mut models = HashMap::new();
    models.insert("testmod:block/wheel_textures", parent_textures_json);
    models.insert("testmod:block/wheel", obj_model_json);

    let mut baker = ModelBaker::new().with_obj_assets(Arc::new(store));
    let mut baked_neg90 = baker
        .bake_blockstate("testmod:wheel[facing=west]", Some(&bs_def), |id| {
            models.get(id).cloned()
        })
        .unwrap();
    let baked_270 = baker
        .bake_blockstate("testmod:wheel[facing=east]", Some(&bs_def), |id| {
            models.get(id).cloned()
        })
        .unwrap();

    assert!(!baked_neg90.is_cube);
    assert!(!baked_neg90.is_opaque);
    assert_eq!(
        baked_neg90.obj_faces.len(),
        1,
        "HiddenBrace must be filtered out"
    );
    let f = &baked_neg90.obj_faces[0];
    assert_eq!(f.texture, "testmod:block/wheel_plates");
    assert_eq!(f.tint_index, 2);
    assert_eq!(f.vertices.len(), 3);
    // flip_v = true transforms raw OBJ v=0.2 -> 0.8
    assert!((f.uvs[0].y - 0.8).abs() < 1e-5);
    assert!((f.uvs[2].y - 0.1).abs() < 1e-5);

    // Negative rotation y=-90 must produce identical geometry to y=270
    for (a, b) in f
        .vertices
        .iter()
        .zip(baked_270.obj_faces[0].vertices.iter())
    {
        assert!(a.abs_diff_eq(*b, 1e-5), "vertex mismatch: {a:?} vs {b:?}");
    }

    // Atlas UV remapping must populate atlas_uvs, chunk_id, and texture_id
    baked_neg90.remap_to_atlas_with(|tex| {
        if tex == "testmod:block/wheel_plates" {
            Some(([0.25, 0.5, 0.75, 1.0], 3, 42))
        } else {
            None
        }
    });
    let remapped_face = &baked_neg90.obj_faces[0];
    assert_eq!(remapped_face.atlas_chunk_id, Some(3));
    assert_eq!(remapped_face.atlas_texture_id, Some(42));
    let atlas_uvs = remapped_face.atlas_uvs.as_ref().unwrap();
    assert!((atlas_uvs[0].x - 0.25).abs() < 1e-5);
    // Minecraft v=0.8 inside [0.5, 1.0] top-down -> 0.5 + (1.0 - 0.8) * 0.5 = 0.6
    assert!((atlas_uvs[0].y - 0.6).abs() < 1e-5);

    let (mesh, _) = baked_neg90.to_mesh_with_textures(false);
    assert!(!mesh.is_quad_topology());
    assert!(mesh.quad_indices.is_none());
    assert_eq!(mesh.triangle_count(), 1);
    assert_eq!(mesh.face_materials, vec![3]);
    assert_eq!(mesh.face_tint_indices, vec![2]);
    assert_eq!(mesh.uvs.len(), 3);
    assert_eq!(
        mesh.get_custom_attribute(ATTR_BIOME_TINT_COLOR)
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn test_ie_obj_default_flip_v_and_ngon_fan_triangulation() {
    let mut store = ObjAssetStore::new();
    let obj_text = r#"
mtllib pentagon.mtl
usemtl plate
v 0.0 0.0 0.0
v 1.0 0.0 0.0
v 1.0 0.5 0.0
v 0.5 1.0 0.0
v 0.0 0.5 0.0
vt 0.0 0.0
vt 1.0 0.0
vt 1.0 0.5
vt 0.5 1.0
vt 0.0 0.5
f 1/1 2/2 3/3 4/4 5/5
"#;
    let mtl_text = "newmtl plate\nmap_Kd #texture\n";
    store.insert_obj(
        "assets/immersiveengineering/models/block/slope.obj.ie",
        ParsedObjModel::parse_str(obj_text),
    );
    store.insert_mtl(
        "assets/immersiveengineering/models/block/pentagon.mtl",
        ParsedMtlLibrary::parse_str(mtl_text),
    );

    let model_json: BlockModelJson = serde_json::from_str(
        r##"{
            "loader": "immersiveengineering:ie_obj",
            "model": "immersiveengineering:models/block/slope.obj.ie",
            "textures": {
                "texture": "immersiveengineering:block/metal"
            }
        }"##,
    )
    .unwrap();

    let resolved = model_json
        .resolve_hierarchy("immersiveengineering:block/slope", |_| None)
        .unwrap();
    assert_eq!(resolved.obj_specs.len(), 1);
    assert!(
        resolved.obj_specs[0].flip_v,
        "immersiveengineering:ie_obj must default flip_v = true"
    );

    let baker = ModelBaker::new().with_obj_assets(Arc::new(store));
    let baked = baker.bake_resolved_model("immersiveengineering:block/slope", &resolved);
    assert_eq!(baked.obj_faces.len(), 1);
    assert_eq!(baked.obj_faces[0].vertices.len(), 5);

    let mesh = baked.to_mesh(false);
    // 5-gon fan triangulates into 3 triangles
    assert!(!mesh.is_quad_topology());
    assert!(mesh.quad_indices.is_none());
    assert_eq!(mesh.triangle_count(), 3);
    assert_eq!(mesh.face_materials.len(), 3);
    assert_eq!(mesh.uvs.len(), 9);
}

#[test]
fn test_neoforge_composite_with_transform_parent_child_and_mixed_obj() {
    let mut store = ObjAssetStore::new();
    let obj_text = r#"
mtllib part.mtl
usemtl glass
v 0.2 0.2 0.2
v 0.8 0.2 0.2
v 0.5 0.8 0.2
vt 0.0 0.0
vt 1.0 0.0
vt 0.5 1.0
f 1/1 2/2 3/3
"#;
    let mtl_text = "newmtl glass\nmap_Kd testmod:block/glass\n";
    store.insert_obj(
        "assets/testmod/models/block/part.obj",
        ParsedObjModel::parse_str(obj_text),
    );
    store.insert_mtl(
        "assets/testmod/models/block/part.mtl",
        ParsedMtlLibrary::parse_str(mtl_text),
    );

    let sub_part_json: BlockModelJson = serde_json::from_str(
        r##"{
            "elements": [
                {
                    "from": [0, 0, 0],
                    "to": [16, 4, 16],
                    "faces": {
                        "up": { "texture": "#base_tex" }
                    }
                }
            ]
        }"##,
    )
    .unwrap();

    let composite_json: BlockModelJson = serde_json::from_str(
        r##"{
            "loader": "neoforge:composite",
            "textures": {
                "base_tex": "testmod:block/base",
                "core_tex": "testmod:block/core"
            },
            "children": {
                "base": {
                    "parent": "testmod:block/sub_part"
                },
                "core": {
                    "transform": { "origin": "center", "rotation": [0, 45, 0] },
                    "elements": [
                        {
                            "from": [4, 4, 4],
                            "to": [12, 12, 12],
                            "faces": {
                                "north": { "texture": "#core_tex" }
                            }
                        }
                    ]
                },
                "tri_obj": {
                    "loader": "neoforge:obj",
                    "flip_v": true,
                    "model": "testmod:models/block/part.obj"
                }
            }
        }"##,
    )
    .unwrap();

    let mut models = HashMap::new();
    models.insert("testmod:block/sub_part", sub_part_json);
    models.insert("testmod:block/machine", composite_json.clone());

    let resolved = composite_json
        .resolve_hierarchy("testmod:block/machine", |id| models.get(id).cloned())
        .unwrap();
    assert_eq!(resolved.elements.len(), 2);
    assert_eq!(resolved.obj_specs.len(), 1);
    assert!(resolved.elements[0].root_transform.is_none());
    assert!(resolved.elements[1].root_transform.is_some());

    let baker = ModelBaker::new().with_obj_assets(Arc::new(store));
    let baked = baker.bake_resolved_model("testmod:block/machine", &resolved);
    assert_eq!(baked.elements.len(), 2);
    assert_eq!(baked.obj_faces.len(), 1);

    // Core's north face was rotated 45 degrees around [0.5, 0.5, 0.5], so its normal X and Z must be non-zero
    let core_north = baked.elements[1].faces.get(&Direction::North).unwrap();
    assert!(core_north.normal.x.abs() > 0.5);
    assert!(core_north.normal.z.abs() > 0.5);

    // Converting mixed element quads + 1 OBJ triangle to MeshData must produce pure triangles
    let mesh = baked.to_mesh(false);
    assert!(!mesh.is_quad_topology());
    assert!(mesh.quad_indices.is_none());
    // 2 quad element faces (4 tris) + 1 OBJ triangle (1 tri) = 5 triangles
    assert_eq!(mesh.triangle_count(), 5);
    assert_eq!(mesh.face_materials.len(), 5);
    assert_eq!(mesh.uvs.len(), 15);
    assert_eq!(
        mesh.get_custom_attribute(ATTR_SOURCE_TEXTURE_IDX)
            .unwrap()
            .data,
        AttributeData::Int32(vec![0, 0, 1, 1, 2])
    );
}

#[test]
fn test_deduplicate_faces_culls_back_to_back_obj_cards_only_when_same_texture() {
    use glam::Vec3;
    use mtk_model::{BakedModel, BakedObjFace};

    let make_obj_quad = |verts: Vec<Vec3>, texture: &str, tint_index: i16| BakedObjFace {
        direction: Direction::South,
        texture: texture.to_string(),
        tint_index,
        vertices: verts,
        uvs: vec![
            glam::Vec2::new(0.0, 0.0),
            glam::Vec2::new(1.0, 0.0),
            glam::Vec2::new(1.0, 1.0),
            glam::Vec2::new(0.0, 1.0),
        ],
        normal: Vec3::Z,
        atlas_uvs: None,
        atlas_chunk_id: None,
        atlas_texture_id: None,
    };

    let front = vec![
        Vec3::new(0.0, 0.0, 0.5),
        Vec3::new(1.0, 0.0, 0.5),
        Vec3::new(1.0, 1.0, 0.5),
        Vec3::new(0.0, 1.0, 0.5),
    ];
    let back: Vec<Vec3> = front.iter().copied().rev().collect();

    let mut baked = BakedModel {
        obj_faces: vec![
            make_obj_quad(front, "testmod:block/razor_wire", -1),
            // Opposite-winding back-to-back card with identical texture & tint -> must be removed
            make_obj_quad(back.clone(), "testmod:block/razor_wire", -1),
            // Opposite-winding or coplanar card with distinct texture -> must be preserved
            make_obj_quad(back, "testmod:block/razor_wire_overlay", -1),
        ],
        ..Default::default()
    };

    let removed = baked.deduplicate_faces();
    assert_eq!(removed, 1);
    assert_eq!(baked.obj_faces.len(), 2);
    assert_eq!(baked.obj_faces[0].texture, "testmod:block/razor_wire");
    assert_eq!(
        baked.obj_faces[1].texture,
        "testmod:block/razor_wire_overlay"
    );
}

#[test]
fn test_obj_only_baked_model_cull_meta_is_partial_and_non_opaque() {
    use glam::Vec3;
    use mtk_core::DirMask;
    use mtk_cull::CullCategory;
    use mtk_model::{BakedModel, BakedObjFace};

    let baked = BakedModel {
        block_state: "testmod:track[waterlogged=true]".to_string(),
        obj_faces: vec![BakedObjFace {
            direction: Direction::Up,
            texture: "testmod:block/track".to_string(),
            tint_index: -1,
            vertices: vec![
                Vec3::new(0.0, 0.1, 0.0),
                Vec3::new(0.0, 0.1, 1.0),
                Vec3::new(1.0, 0.1, 1.0),
            ],
            uvs: vec![
                glam::Vec2::new(0.0, 0.0),
                glam::Vec2::new(0.0, 1.0),
                glam::Vec2::new(1.0, 1.0),
            ],
            normal: Vec3::Y,
            atlas_uvs: None,
            atlas_chunk_id: None,
            atlas_texture_id: None,
        }],
        is_cube: false,
        is_opaque: false,
        ..Default::default()
    };

    let meta = baked.get_or_compute_cull_meta();
    assert_eq!(meta.category, CullCategory::PartialShape);
    assert!(!meta.is_full_cube);
    assert!(!meta.is_opaque);
    assert!(meta.has_baked_model);
    assert!(meta.is_waterlogged);
    assert_eq!(meta.full_face_mask, DirMask::empty());
    assert_eq!(meta.empty_face_mask, DirMask::ALL);
}
