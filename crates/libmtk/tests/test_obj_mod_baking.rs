use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use libmtk::{precompile_all_assets, AssetCacheReader, PrecompileConfig};
use mtk_cull::FaceCuller;
use mtk_resource::{DirectoryPack, MemoryPack, ResourcePackStack, ZipPack};
use mtk_voxel::{MesherConfig, SectionMesher, VoxelStorage};

fn make_solid_png(rgba: [u8; 4]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(16, 16, image::Rgba(rgba));
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
    bytes
}

#[test]
fn test_precompile_and_mesh_synthetic_obj_and_composite_mod() {
    let mut stack = ResourcePackStack::new();
    let mut vanilla = MemoryPack::new("TestVanilla");
    vanilla.insert(
        "assets/minecraft/textures/block/stone.png",
        make_solid_png([128, 128, 128, 255]),
    );
    vanilla.insert(
        "assets/minecraft/models/block/stone.json",
        br##"{
            "textures": { "all": "minecraft:block/stone" },
            "elements": [{
                "from": [0, 0, 0], "to": [16, 16, 16],
                "faces": {
                    "up":    { "texture": "#all", "cullface": "up" },
                    "down":  { "texture": "#all", "cullface": "down" },
                    "north": { "texture": "#all", "cullface": "north" },
                    "south": { "texture": "#all", "cullface": "south" },
                    "west":  { "texture": "#all", "cullface": "west" },
                    "east":  { "texture": "#all", "cullface": "east" }
                }
            }]
        }"##
        .to_vec(),
    );
    vanilla.insert(
        "assets/minecraft/blockstates/stone.json",
        br#"{ "variants": { "": { "model": "minecraft:block/stone" } } }"#.to_vec(),
    );
    stack.push_pack(Box::new(vanilla));

    let mut mod_pack = MemoryPack::new("TestObjMod");
    // Extra non-block/ texture used by .mtl (must be collected into Atlas!)
    mod_pack.insert(
        "assets/testmod/textures/entity/widget.png",
        make_solid_png([255, 160, 0, 255]),
    );
    mod_pack.insert(
        "assets/testmod/textures/block/core.png",
        make_solid_png([0, 200, 255, 255]),
    );
    mod_pack.insert(
        "assets/testmod/models/block/widget.obj.ie",
        br#"
mtllib widget.mtl
usemtl body
v 0.1 0.0 0.1
v 0.9 0.0 0.1
v 0.5 0.9 0.5
vt 0.0 0.0
vt 1.0 0.0
vt 0.5 1.0
f 1/1 2/2 3/3
"#
        .to_vec(),
    );
    mod_pack.insert(
        "assets/testmod/models/block/widget.mtl",
        br#"
newmtl body
map_Kd testmod:entity/widget
"#
        .to_vec(),
    );
    mod_pack.insert(
        "assets/testmod/models/block/widget_obj.json",
        br##"{
            "loader": "neoforge:obj",
            "flip_v": true,
            "model": "testmod:models/block/widget.obj.ie"
        }"##
        .to_vec(),
    );
    mod_pack.insert(
        "assets/testmod/models/block/composite_device.json",
        br##"{
            "loader": "neoforge:composite",
            "children": {
                "obj_part": {
                    "parent": "testmod:block/widget_obj"
                },
                "core_part": {
                    "transform": { "origin": "center", "rotation": [0, 45, 0] },
                    "textures": { "core": "testmod:block/core" },
                    "elements": [{
                        "from": [4, 4, 4],
                        "to": [12, 12, 12],
                        "faces": {
                            "up": { "texture": "#core" }
                        }
                    }]
                }
            }
        }"##
        .to_vec(),
    );
    mod_pack.insert(
        "assets/testmod/blockstates/composite_device.json",
        br#"{ "variants": { "": { "model": "testmod:block/composite_device" } } }"#.to_vec(),
    );
    stack.push_pack(Box::new(mod_pack));

    let temp_cache_dir = std::env::temp_dir().join(format!(
        "mtk_obj_mod_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&temp_cache_dir);

    let config = PrecompileConfig {
        max_atlas_width: 1024,
        max_atlas_height: 1024,
        atlas_category: "blocks".to_string(),
        compile_atlas: true,
        compile_models: true,
        num_threads: None,
        resolver_overrides: None,
    };

    let result = precompile_all_assets(&stack, &temp_cache_dir, &config).unwrap();
    assert!(result.success);

    let reader = AssetCacheReader::open(Path::new(&result.package_path)).unwrap();
    let atlas_map = Arc::new(reader.load_atlas_mapping().unwrap());
    assert!(
        atlas_map
            .resolve_texture_key("testmod:entity/widget")
            .is_some(),
        "Extra entity/ texture referenced by .mtl must be stitched into Atlas"
    );

    let model_db = reader.load_models().unwrap();
    let device_model = model_db
        .get("testmod:composite_device")
        .expect("composite_device must be baked into BakedModelDatabase");
    assert_eq!(device_model.elements.len(), 1);
    assert_eq!(device_model.obj_faces.len(), 1);
    assert!(
        device_model.obj_faces[0].atlas_uvs.is_some(),
        "BakedObjFace must carry pre-mapped Atlas UVs"
    );

    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);
    world.set_block(1, 1, 1, "minecraft:stone", None);
    world.set_block(3, 1, 1, "testmod:composite_device", None);

    let coords = world.get_all_non_empty_sections();
    assert_eq!(coords.len(), 1);
    let padded = world.get_section_padded_array(coords[0]);
    let culler = FaceCuller::default();
    let mesher_cfg = MesherConfig {
        atlas_address_map: Some(atlas_map),
        weld_vertices: false,
        ..Default::default()
    };
    let mesh =
        SectionMesher::mesh_section(&padded, &culler, |st| model_db.get(st).into(), &mesher_cfg);

    // 6 stone quads (12 tris) + 1 core element face (2 tris) + 1 OBJ tri (1 tri) = 15 triangles
    assert_eq!(mesh.triangle_count(), 15);
    assert_eq!(mesh.face_materials.len(), 15);
    assert!(mesh.quad_indices.is_none());

    let _ = fs::remove_dir_all(&temp_cache_dir);
}

#[test]
fn test_real_obj_mod_jars_if_present() {
    let Some(mod_samples_dir) = mtk_testkit::mod_samples_dir() else {
        mtk_testkit::skip("no mod-samples directory available (set MTK_TEST_MOD_DIR)");
        return;
    };
    let Some(vanilla_dir) = mtk_testkit::real_assets_root() else {
        mtk_testkit::skip("no full vanilla asset set");
        return;
    };

    let sample_jars = [
        "create-1.21.1-6.0.10.jar",
        "ImmersiveEngineering-1.21.1-12.4.2-194.jar",
        "Steam_Rails-1.7.3+forge-mc1.20.1.jar",
        "Mekanism-1.21.1-10.7.19.85.jar",
        "pneumaticcraft-repressurized-8.2.24+mc1.21.1.jar",
        "electrodynamics-1.21.1-1.1.1.jar",
    ];
    let existing_jars: Vec<_> = sample_jars
        .iter()
        .map(|name| mod_samples_dir.join(name))
        .filter(|p| p.is_file())
        .collect();
    if existing_jars.len() != sample_jars.len() {
        mtk_testkit::skip(&format!(
            "incomplete mod-samples archive ({}/{} jars found in {})",
            existing_jars.len(),
            sample_jars.len(),
            mod_samples_dir.display()
        ));
        return;
    }

    let mut stack = ResourcePackStack::new();
    for jar_path in &existing_jars {
        let name = jar_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("ModJar");
        stack.push_pack(Box::new(ZipPack::from_file(name, jar_path).unwrap()));
    }
    stack.push_pack(Box::new(DirectoryPack::new("Vanilla", vanilla_dir)));

    let temp_cache_dir =
        std::env::temp_dir().join(format!("mtk_real_obj_mods_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_cache_dir);

    let config = PrecompileConfig {
        max_atlas_width: 4096,
        max_atlas_height: 4096,
        atlas_category: "blocks".to_string(),
        compile_atlas: true,
        compile_models: true,
        num_threads: None,
        resolver_overrides: None,
    };

    let res = precompile_all_assets(&stack, &temp_cache_dir, &config).unwrap();
    assert!(res.success);

    let reader = AssetCacheReader::open(Path::new(&res.package_path)).unwrap();
    let model_db = reader.load_models().unwrap();
    let atlas_map = Arc::new(reader.load_atlas_mapping().unwrap());

    // Verify representative OBJ & composite models baked across all 6 real mods
    let showcase_states = [
        "create:lit_blaze_burner[flame_type=regular]",
        "create:crushing_wheel[axis=y]",
        "immersiveengineering:razor_wire[facing=east]",
        "railways:buffer_wide[diagonal=false,facing=east]",
        "mekanism:quantum_entangloporter[facing=down]",
        "pneumaticcraft:etching_tank",
        "electrodynamics:combustionchamber[facing=south]",
    ];
    for state in showcase_states {
        let m = model_db
            .get(state)
            .unwrap_or_else(|| panic!("expected real mod state {state} in BakedModelDatabase"));
        assert!(
            !m.elements.is_empty() || !m.obj_faces.is_empty(),
            "real mod state {state} must bake non-empty geometry"
        );
    }

    let obj_baked_count = model_db
        .models
        .values()
        .filter(|m| !m.obj_faces.is_empty())
        .count();
    assert!(
        obj_baked_count > 50,
        "expected >50 OBJ-backed baked models from real mod samples, got {obj_baked_count}"
    );

    // Invariant datagen `waterlogged=true` keys on Create & Steam 'n' Rails must be folded out
    // of BakedModelDatabase keys while runtime full queries still resolve seamlessly.
    let spurious_waterlogged = model_db
        .models
        .keys()
        .filter(|k| {
            (k.starts_with("create:") || k.starts_with("railways:"))
                && k.contains("waterlogged=true")
        })
        .count();
    assert_eq!(
        spurious_waterlogged, 0,
        "expected invariant datagen waterlogged=true states to fold out of BakedModelDatabase"
    );
    assert!(model_db
        .get("create:track[shape=ae,turn=false,waterlogged=true]")
        .is_some());
    assert!(model_db
        .get("railways:track_warped[shape=cr_d,turn=false,waterlogged=true]")
        .is_some());

    // Verify meshing a section containing real OBJ & composite models across all 6 mods
    // succeeds with zero NaNs and zero duplicate face vertex sets when welded.
    let mut world = VoxelStorage::new();
    world.set_bounds(0, 0, 0, 16, 16, 16);
    for (idx, state) in showcase_states.iter().enumerate() {
        world.set_block((idx as i32) * 2 + 1, 2, 2, state, None);
    }

    let coords = world.get_all_non_empty_sections();
    assert_eq!(coords.len(), 1);
    let padded = world.get_section_padded_array(coords[0]);
    let culler = FaceCuller::default();

    for weld_vertices in [false, true] {
        let mesher_cfg = MesherConfig {
            atlas_address_map: Some(Arc::clone(&atlas_map)),
            weld_vertices,
            ..Default::default()
        };
        let mesh = SectionMesher::mesh_section(
            &padded,
            &culler,
            |st| model_db.get(st).into(),
            &mesher_cfg,
        );
        assert!(mesh.triangle_count() > 500);
        assert!(mesh
            .positions
            .iter()
            .all(|p| p[0].is_finite() && p[1].is_finite() && p[2].is_finite()));

        if weld_vertices {
            let mut seen_tris = HashSet::new();
            for tri in mesh.indices.as_chunks::<3>().0 {
                let mut key = [tri[0], tri[1], tri[2]];
                key.sort_unstable();
                assert!(
                    seen_tris.insert(key),
                    "welded OBJ/composite mesh produced duplicate triangle {key:?}"
                );
            }
        }
    }

    let _ = fs::remove_dir_all(&temp_cache_dir);
}
