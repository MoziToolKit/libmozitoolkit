use std::collections::HashMap;
use std::io::Cursor;

use glam::{Vec2, Vec3};
use image::{ImageBuffer, Rgba};
use libmtk::{PackageManifest, SCENE_PACKAGE_FORMAT_VERSION};
use mtk_core::Direction;
use mtk_model::{BakedElement, BakedFace, BakedModel, BakedModelDatabase};
use mtk_package::{ChunkWriteOptions, MtkPackageWriter, PackageProfile};
use mtk_resource::ResourceLocation;
use mtk_texture::{AtlasAddressMap, AtlasChunkMeta, AtlasSpriteLocation, AtlasTextureSlot};
use mtk_voxel::SparseVoxelPayload;

use super::*;

fn encode_tiny_png(rgba: [u8; 4]) -> Vec<u8> {
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_pixel(4, 4, Rgba(rgba));
    let mut cursor = Cursor::new(Vec::new());
    img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
    cursor.into_inner()
}

fn build_test_mtkscene(stepped: bool) -> Vec<u8> {
    let mut voxels = SparseVoxelPayload::new();
    if stepped {
        voxels.bounds = [0, 0, 0, 2, 2, 2];
        voxels.push(0, 0, 0, "minecraft:stone", "minecraft:plains", None, None);
        voxels.push(1, 0, 0, "minecraft:stone", "minecraft:plains", None, None);
        voxels.push(0, 1, 0, "minecraft:stone", "minecraft:plains", None, None);
        voxels.push(
            1,
            0,
            1,
            "minecraft:oak_leaves[persistent=true]",
            "minecraft:plains",
            None,
            None,
        );
    } else {
        voxels.bounds = [0, 0, 0, 2, 1, 1];
        voxels.push(0, 0, 0, "minecraft:stone", "minecraft:plains", None, None);
        voxels.push(
            1,
            0,
            0,
            "minecraft:oak_leaves[persistent=true]",
            "minecraft:plains",
            None,
            None,
        );
    }

    let stone_loc = ResourceLocation::parse_texture_path("minecraft:block/stone").unwrap();
    let leaves_loc = ResourceLocation::parse_texture_path("minecraft:block/oak_leaves").unwrap();

    let mut atlas_map = AtlasAddressMap::new();
    let chunk_meta = AtlasChunkMeta {
        chunk_id: 0,
        category: "blocks".to_string(),
        is_animated: false,
        category_chunk_index: 1,
        width: 4,
        height: 4,
        has_normal: false,
        has_specular: false,
        has_overlay: false,
        has_orm: false,
        has_aux: false,
        has_tint: true,
        pbr: None,
        textures: vec![AtlasTextureSlot {
            role: "albedo".to_string(),
            file_name: "blocks_chunk_001.png".to_string(),
            colorspace: "sRGB".to_string(),
        }],
    };
    atlas_map.chunks.push(chunk_meta);
    atlas_map.sprites.insert(
        stone_loc,
        AtlasSpriteLocation {
            chunk_id: 0,
            uv_bounds: [0.0, 0.5, 0.5, 1.0],
            frame_0_uv_bounds: [0.0, 0.5, 0.5, 1.0],
            pixel_rect: [0, 0, 2, 2],
            strip_pixel_rect: [0, 0, 2, 2],
            frame_size: [2, 2],
            frame_count: 1,
            ..Default::default()
        },
    );
    atlas_map.sprites.insert(
        leaves_loc,
        AtlasSpriteLocation {
            chunk_id: 0,
            uv_bounds: [0.5, 0.5, 1.0, 1.0],
            frame_0_uv_bounds: [0.5, 0.5, 1.0, 1.0],
            pixel_rect: [2, 0, 2, 2],
            strip_pixel_rect: [2, 0, 2, 2],
            frame_size: [2, 2],
            frame_count: 1,
            is_opaque: false,
            ..Default::default()
        },
    );

    let cube_model = |state: &str, tex_key: &str, tint_index: i16| {
        let dirs = [
            Direction::Down,
            Direction::Up,
            Direction::North,
            Direction::South,
            Direction::West,
            Direction::East,
        ];
        let faces: [BakedFace; 6] = std::array::from_fn(|i| {
            let dir = dirs[i];
            let vertices = match dir {
                Direction::Down => [
                    Vec3::new(0.0, 0.0, 1.0),
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 1.0),
                ],
                Direction::Up => [
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.0, 1.0, 1.0),
                    Vec3::new(1.0, 1.0, 1.0),
                    Vec3::new(1.0, 1.0, 0.0),
                ],
                Direction::North => [
                    Vec3::new(1.0, 1.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ],
                Direction::South => [
                    Vec3::new(0.0, 1.0, 1.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    Vec3::new(1.0, 0.0, 1.0),
                    Vec3::new(1.0, 1.0, 1.0),
                ],
                Direction::West => [
                    Vec3::new(0.0, 1.0, 0.0),
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.0, 0.0, 1.0),
                    Vec3::new(0.0, 1.0, 1.0),
                ],
                Direction::East => [
                    Vec3::new(1.0, 1.0, 1.0),
                    Vec3::new(1.0, 0.0, 1.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(1.0, 1.0, 0.0),
                ],
            };
            BakedFace {
                direction: dir,
                texture: tex_key.to_string(),
                uv_rot: 0.0,
                uv_bounds: [0.0, 0.0, 1.0, 1.0],
                tint_index,
                cullface: Some(dir),
                vertices,
                uvs: [
                    Vec2::new(0.0, 0.0),
                    Vec2::new(1.0, 0.0),
                    Vec2::new(1.0, 1.0),
                    Vec2::new(0.0, 1.0),
                ],
                normal: dir.normal(),
                atlas_uvs: Some([
                    Vec2::new(0.0, 0.5),
                    Vec2::new(0.5, 0.5),
                    Vec2::new(0.5, 1.0),
                    Vec2::new(0.0, 1.0),
                ]),
                atlas_chunk_id: Some(0),
                atlas_texture_id: Some(0),
            }
        });
        let elem_faces: HashMap<Direction, BakedFace> =
            faces.iter().map(|f| (f.direction, f.clone())).collect();
        BakedModel {
            block_state: state.to_string(),
            elements: vec![BakedElement {
                from_pos: [0.0, 0.0, 0.0],
                to_pos: [16.0, 16.0, 16.0],
                faces: elem_faces,
            }],
            faces,
            is_cube: true,
            is_opaque: tint_index < 0,
            ..Default::default()
        }
    };

    let mut db = BakedModelDatabase::new();
    db.insert(
        "minecraft:stone".to_string(),
        cube_model("minecraft:stone", "minecraft:block/stone", -1),
    );
    db.insert(
        "minecraft:oak_leaves[persistent=true]".to_string(),
        cube_model(
            "minecraft:oak_leaves[persistent=true]",
            "minecraft:block/oak_leaves",
            0,
        ),
    );

    let mut manifest = PackageManifest::new_scene(voxels.bounds, voxels.len());
    manifest.format_version = SCENE_PACKAGE_FORMAT_VERSION;
    manifest.unique_states_count = 2;
    manifest.unique_biomes_count = 1;
    manifest.atlas_resolution = [4, 4];

    let png_bytes = encode_tiny_png([128, 128, 128, 255]);

    let mut out = Cursor::new(Vec::new());
    let mut writer =
        MtkPackageWriter::new(&mut out, PackageProfile::SceneInterchange, [0u8; 16]).unwrap();
    writer
        .add_json_chunk(
            *b"META",
            "manifest",
            &manifest,
            &ChunkWriteOptions::zstd_fast(),
        )
        .unwrap();
    let voxl_bytes = postcard::to_allocvec(&voxels).unwrap();
    writer
        .add_chunk(
            *b"VOXL",
            "voxels/sparse",
            &voxl_bytes,
            &ChunkWriteOptions::zstd_fast(),
        )
        .unwrap();
    let db_bytes = db.to_binary().unwrap();
    writer
        .add_chunk(
            *b"MODL",
            "models/database",
            &db_bytes,
            &ChunkWriteOptions::zstd_fast(),
        )
        .unwrap();
    let map_json = atlas_map.to_json().unwrap();
    writer
        .add_str_chunk(
            *b"ATLS",
            "atlas/mapping",
            &map_json,
            &ChunkWriteOptions::zstd_fast(),
        )
        .unwrap();
    writer
        .add_chunk(
            *b"ATLS",
            "atlas/textures/blocks_chunk_001.png",
            &png_bytes,
            &ChunkWriteOptions::raw(),
        )
        .unwrap();
    writer.finish().unwrap();

    out.into_inner()
}

#[test]
fn test_stepped_scene_preserves_per_face_normals_and_ao() {
    let bytes = build_test_mtkscene(true);
    let pkg = WasmScenePackage::from_bytes(&bytes).expect("open stepped scene");
    let mut render_scene = pkg.build_render_scene(None).expect("build render scene");
    assert_eq!(render_scene.batch_count(), 1);
    let batch = render_scene.take_batch(0).expect("take batch 0");

    let pos = unsafe { std::slice::from_raw_parts(batch.positions_ptr(), batch.positions_len()) };
    let norm = unsafe { std::slice::from_raw_parts(batch.normals_ptr(), batch.normals_len()) };
    let col = unsafe { std::slice::from_raw_parts(batch.colors_ptr(), batch.colors_len()) };
    let idx = unsafe { std::slice::from_raw_parts(batch.indices_ptr(), batch.indices_len()) };

    let mut saw_shaded_ao = false;
    let mut saw_full_ao = false;
    for t in 0..batch.triangle_count() {
        let i0 = idx[t * 3] as usize;
        let i1 = idx[t * 3 + 1] as usize;
        let i2 = idx[t * 3 + 2] as usize;

        let n0 = Vec3::from_slice(&norm[i0 * 3..i0 * 3 + 3]);
        let n1 = Vec3::from_slice(&norm[i1 * 3..i1 * 3 + 3]);
        let n2 = Vec3::from_slice(&norm[i2 * 3..i2 * 3 + 3]);
        assert!(
            (n0 - n1).length() < 1e-5 && (n1 - n2).length() < 1e-5,
            "triangle {t} has mismatched corner normals: {n0:?}, {n1:?}, {n2:?}"
        );

        let p0 = Vec3::from_slice(&pos[i0 * 3..i0 * 3 + 3]);
        let p1 = Vec3::from_slice(&pos[i1 * 3..i1 * 3 + 3]);
        let p2 = Vec3::from_slice(&pos[i2 * 3..i2 * 3 + 3]);
        let geom_n = (p1 - p0).cross(p2 - p0).normalize_or_zero();
        assert!(
            geom_n.dot(n0) > 0.99,
            "triangle {t} normal {n0:?} disagrees with geometric winding {geom_n:?}"
        );

        for vi in [i0, i1, i2] {
            let ao = col[vi * 4 + 3];
            assert!((0.19..=1.01).contains(&ao), "unexpected AO {ao}");
            if ao < 0.95 {
                saw_shaded_ao = true;
            } else {
                saw_full_ao = true;
            }
        }
    }
    assert!(
        saw_shaded_ao && saw_full_ao,
        "expected both shaded and unshaded AO vertices"
    );
}

#[test]
fn test_wasm_weld_mode_defaults_to_per_vertex_and_deduplicates_matching_gpu_corners() {
    use mtk_core::attribute_constants::ATTR_AO;
    use mtk_core::{
        AttributeData, AttributeDomain, FaceAttributes, MeshAttribute, MeshData, Quad, WeldMode,
    };

    let mut cfg = WasmMesherConfig::new();
    assert_eq!(cfg.weld_mode(), "per_vertex");
    // Web/Three.js must reject DCC position-only manifold welding because it corrupts normals.
    assert!(crate::config::parse_wasm_weld_mode("manifold").is_err());
    assert!(crate::config::parse_wasm_weld_mode("invalid").is_err());
    assert_eq!(
        crate::config::parse_wasm_weld_mode("none").unwrap(),
        WeldMode::None
    );
    assert_eq!(
        crate::config::parse_wasm_weld_mode("per_vertex").unwrap(),
        WeldMode::PerVertex
    );
    cfg.set_weld_mode("none").expect("allow none");
    assert_eq!(cfg.weld_mode(), "none");
    cfg.set_weld_mode("per_vertex").expect("allow per_vertex");
    assert_eq!(cfg.weld_mode(), "per_vertex");

    // Two adjacent coplanar quads sharing edge x=1 with identical mirrored UVs & AO,
    // plus one perpendicular face sharing edge at (1, 0, 0)-(1, 0, 1).
    let mut mesh = MeshData::new();
    let attrs = FaceAttributes {
        material_slot: 0,
        tint_index: -1,
        ..Default::default()
    };
    mesh.append_quad(
        &Quad {
            vertices: [
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(1.0, 0.0, 0.0),
            ],
            normal: Vec3::Y,
            uvs: [
                Vec2::new(0.0, 0.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(1.0, 0.0),
            ],
        },
        &attrs,
    );
    mesh.append_quad(
        &Quad {
            vertices: [
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(2.0, 0.0, 1.0),
                Vec3::new(2.0, 0.0, 0.0),
            ],
            normal: Vec3::Y,
            uvs: [
                Vec2::new(1.0, 0.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(0.0, 0.0),
            ],
        },
        &attrs,
    );
    mesh.append_quad(
        &Quad {
            vertices: [
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::new(1.0, -1.0, 1.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
            ],
            normal: Vec3::X,
            uvs: [
                Vec2::new(1.0, 1.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 0.0),
                Vec2::new(0.0, 1.0),
            ],
        },
        &attrs,
    );
    mesh.add_custom_attribute(MeshAttribute::new(
        ATTR_AO,
        AttributeDomain::Corner,
        AttributeData::Float(vec![1.0; 12]),
    ));

    let unwelded = crate::render::build_render_scene(&mesh, None, WeldMode::None);
    let mut welded = crate::render::build_render_scene(&mesh, None, WeldMode::PerVertex);
    assert_eq!(unwelded.total_vertices(), 12);
    assert_eq!(
        welded.total_vertices(),
        10,
        "matching shared edge on coplanar quads must weld (12 -> 10), while perpendicular face stays split"
    );

    let batch = welded.take_batch(0).unwrap();
    let norm = unsafe { std::slice::from_raw_parts(batch.normals_ptr(), batch.normals_len()) };
    let idx = unsafe { std::slice::from_raw_parts(batch.indices_ptr(), batch.indices_len()) };
    for t in 0..batch.triangle_count() {
        let i0 = idx[t * 3] as usize;
        let i1 = idx[t * 3 + 1] as usize;
        let i2 = idx[t * 3 + 2] as usize;
        let n0 = Vec3::from_slice(&norm[i0 * 3..i0 * 3 + 3]);
        let n1 = Vec3::from_slice(&norm[i1 * 3..i1 * 3 + 3]);
        let n2 = Vec3::from_slice(&norm[i2 * 3..i2 * 3 + 3]);
        assert!((n0 - n1).length() < 1e-5 && (n1 - n2).length() < 1e-5);
    }
}

#[test]
fn test_wasm_concurrency_and_version() {
    assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    set_hardware_concurrency(8);
    assert_eq!(get_hardware_concurrency(), 8);
    set_hardware_concurrency(0);
}

#[test]
fn test_scene_package_viewer_roundtrip_and_batches() {
    let bytes = build_test_mtkscene(false);
    let pkg = WasmScenePackage::from_bytes(&bytes).expect("open scene bytes");

    let meta_json = pkg.metadata_json().expect("metadata json");
    let parsed: serde_json::Value = serde_json::from_str(&meta_json).unwrap();
    assert_eq!(parsed["manifest"]["voxel_count"], 2);
    assert_eq!(parsed["atlas_chunks"][0]["chunk_id"], 0);

    let names = pkg.texture_names();
    assert_eq!(names, vec!["atlas/textures/blocks_chunk_001.png"]);

    let raw_png = pkg
        .read_texture_png_bytes("blocks_chunk_001.png")
        .expect("read png");
    assert!(raw_png.starts_with(&[0x89, b'P', b'N', b'G']));

    let mut cfg = WasmMesherConfig::new();
    cfg.set_enable_ao(true);
    let mut render_scene = pkg
        .build_render_scene(Some(cfg))
        .expect("build render scene");
    assert_eq!(render_scene.batch_count(), 1);
    assert!(render_scene.total_vertices() > 0);
    assert!(render_scene.total_triangles() > 0);

    let batch = render_scene.take_batch(0).expect("take batch 0");
    assert_eq!(batch.chunk_id(), 0);
    assert_eq!(
        batch.albedo_chunk_id().as_deref(),
        Some("atlas/textures/blocks_chunk_001.png")
    );
    let v_count = batch.vertex_count();
    assert_eq!(batch.positions_len(), v_count * 3);
    assert_eq!(batch.normals_len(), v_count * 3);
    assert_eq!(batch.uvs_len(), v_count * 2);
    assert_eq!(batch.colors_len(), v_count * 4);
    assert_eq!(batch.tint_weights_len(), v_count * 2);
    assert_eq!(batch.indices_len(), batch.triangle_count() * 3);
}

fn push_nbt_tag_header(buf: &mut Vec<u8>, tag_type: u8, name: &str) {
    buf.push(tag_type);
    let bytes = name.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(bytes);
}

fn push_nbt_string_tag(buf: &mut Vec<u8>, name: &str, value: &str) {
    push_nbt_tag_header(buf, 8, name);
    let bytes = value.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(bytes);
}

fn push_nbt_int_tag(buf: &mut Vec<u8>, name: &str, value: i32) {
    push_nbt_tag_header(buf, 3, name);
    buf.extend_from_slice(&value.to_be_bytes());
}

fn push_nbt_long_array_tag(buf: &mut Vec<u8>, name: &str, values: &[u64]) {
    push_nbt_tag_header(buf, 12, name);
    buf.extend_from_slice(&(values.len() as i32).to_be_bytes());
    for v in values {
        buf.extend_from_slice(&v.to_be_bytes());
    }
}

fn build_synthetic_skull_and_banner_mca() -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut nbt = Vec::new();
    push_nbt_tag_header(&mut nbt, 10, "");
    push_nbt_int_tag(&mut nbt, "DataVersion", 4903);
    push_nbt_int_tag(&mut nbt, "xPos", 0);
    push_nbt_int_tag(&mut nbt, "zPos", 0);
    push_nbt_int_tag(&mut nbt, "yPos", 0);

    push_nbt_tag_header(&mut nbt, 9, "sections");
    nbt.push(10);
    nbt.extend_from_slice(&1i32.to_be_bytes());
    push_nbt_tag_header(&mut nbt, 1, "Y");
    nbt.push(0);

    push_nbt_tag_header(&mut nbt, 10, "block_states");
    push_nbt_tag_header(&mut nbt, 9, "palette");
    nbt.push(10);
    nbt.extend_from_slice(&4i32.to_be_bytes());
    push_nbt_string_tag(&mut nbt, "Name", "minecraft:air");
    nbt.push(0);
    push_nbt_string_tag(&mut nbt, "Name", "minecraft:stone");
    nbt.push(0);
    push_nbt_string_tag(&mut nbt, "Name", "minecraft:player_head");
    nbt.push(0);
    push_nbt_string_tag(&mut nbt, "Name", "minecraft:red_banner");
    nbt.push(0);

    let mut longs = vec![0u64; 256];
    longs[0] = 1u64 | (2u64 << 4) | (3u64 << 8);
    push_nbt_long_array_tag(&mut nbt, "data", &longs);
    nbt.push(0);
    nbt.push(0);

    push_nbt_tag_header(&mut nbt, 9, "block_entities");
    nbt.push(10);
    nbt.extend_from_slice(&2i32.to_be_bytes());

    push_nbt_string_tag(&mut nbt, "id", "minecraft:skull");
    push_nbt_int_tag(&mut nbt, "x", 1);
    push_nbt_int_tag(&mut nbt, "y", 0);
    push_nbt_int_tag(&mut nbt, "z", 0);
    push_nbt_tag_header(&mut nbt, 10, "profile");
    push_nbt_string_tag(&mut nbt, "name", "Notch");
    nbt.push(0);
    nbt.push(0);

    push_nbt_string_tag(&mut nbt, "id", "minecraft:banner");
    push_nbt_int_tag(&mut nbt, "x", 2);
    push_nbt_int_tag(&mut nbt, "y", 0);
    push_nbt_int_tag(&mut nbt, "z", 0);
    nbt.push(0);

    nbt.push(0);

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&nbt).unwrap();
    let compressed = encoder.finish().unwrap();

    let payload_len = 1 + compressed.len();
    let total_chunk_bytes = 4 + payload_len;
    let sectors = total_chunk_bytes.div_ceil(4096).max(1);
    let mut mca = vec![0u8; 4096 * (2 + sectors)];
    mca[0..3].copy_from_slice(&[0, 0, 2]);
    mca[3] = sectors as u8;
    let off = 4096 * 2;
    mca[off..off + 4].copy_from_slice(&(payload_len as u32).to_be_bytes());
    mca[off + 4] = 2;
    mca[off + 5..off + 5 + compressed.len()].copy_from_slice(&compressed);
    mca
}

fn build_minimal_asset_cache_bytes() -> Vec<u8> {
    let mut pack = mtk_resource::MemoryPack::new("WasmSaveUnitPack");
    let stone_png = encode_tiny_png([128, 128, 128, 255]);
    pack.insert("assets/minecraft/textures/block/stone.png", stone_png);
    pack.insert(
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
    pack.insert(
        "assets/minecraft/blockstates/stone.json",
        br##"{ "variants": { "": { "model": "minecraft:block/stone" } } }"##.to_vec(),
    );

    let mut stack = mtk_resource::ResourcePackStack::new();
    stack.push_pack(Box::new(pack));

    let temp_dir = std::env::temp_dir().join(format!(
        "mtk_wasm_cache_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    let cfg = libmtk::PrecompileConfig {
        max_atlas_width: 256,
        max_atlas_height: 256,
        atlas_category: "blocks".to_string(),
        compile_atlas: true,
        compile_models: true,
        num_threads: Some(1),
        resolver_overrides: None,
    };
    let res = libmtk::precompile_all_assets(&stack, &temp_dir, &cfg).unwrap();
    let bytes = std::fs::read(&res.package_path).unwrap();
    let _ = std::fs::remove_dir_all(&temp_dir);
    bytes
}

#[test]
fn test_save_world_skin_pre_register_multi_region_and_corrupt_mca() {
    let cache_bytes = build_minimal_asset_cache_bytes();
    let cache = WasmAssetCache::from_bytes(&cache_bytes).expect("open minimal cache");
    let mca = build_synthetic_skull_and_banner_mca();
    let regions = [mtk_save::RegionBytes {
        rx: 0,
        rz: 0,
        data: &mca,
    }];
    let empty_ext = HashMap::new();
    let min_b = glam::IVec3::new(0, 0, 0);
    let max_b = glam::IVec3::new(15, 15, 15);

    let mut world = WasmSaveWorld::from_cache_for_test(&cache).expect("new WasmSaveWorld");
    let summary = world
        .load_regions_internal(&regions, &empty_ext, min_b, max_b)
        .expect("load synthetic region");
    assert_eq!(summary.voxel_count, 3);

    let pending = world.collect_pending_skin_requests();
    assert_eq!(pending.len(), 1);
    assert!(
        pending[0]
            .candidate_urls
            .iter()
            .any(|u| u.contains("crafthead.net/skin/notch")),
        "expected core-derived candidate URLs, got {:?}",
        pending[0].candidate_urls
    );
    let skin_key = pending[0].key.clone();

    let skin_img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(64, 64, Rgba([220, 180, 140, 255]));
    let mut skin_png = Vec::new();
    skin_img
        .write_to(&mut Cursor::new(&mut skin_png), image::ImageFormat::Png)
        .unwrap();
    assert!(world.register_skin_png_internal(&skin_key, &skin_png));
    assert!(world.collect_pending_skin_requests().is_empty());

    let scene_1 = world
        .build_render_scene_internal(None)
        .expect("first render scene");
    assert!(scene_1.total_triangles() > 0);
    assert!(world
        .texture_names()
        .contains(&"dynamic/chunk_65520.png".to_string()));

    world
        .load_regions_internal(&regions, &empty_ext, min_b, max_b)
        .expect("reload synthetic region");
    assert!(
        world.collect_pending_skin_requests().is_empty(),
        "resolved skins should persist across load_regions on the same WasmSaveWorld"
    );
    let mut scene_2 = world
        .build_render_scene_internal(None)
        .expect("second render scene after reload");
    let mut saw_dynamic_batch = false;
    for i in 0..scene_2.batch_count() {
        let b = scene_2.take_batch(i).unwrap();
        if b.chunk_id() >= mtk_texture::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID as i32 {
            saw_dynamic_batch = true;
            assert_eq!(
                b.albedo_chunk_id().as_deref(),
                Some("dynamic/chunk_65520.png")
            );
        }
    }
    assert!(
        saw_dynamic_batch,
        "reloaded WasmSaveWorld must keep dynamic banner/skin atlas registration"
    );

    let corrupt = [0xDEu8, 0xAD, 0xBE, 0xEF];
    let corrupt_regions = [mtk_save::RegionBytes {
        rx: 0,
        rz: 0,
        data: &corrupt,
    }];
    assert!(
        world
            .load_regions_internal(&corrupt_regions, &empty_ext, min_b, max_b)
            .is_err(),
        "non-empty corrupted .mca must fail instead of returning empty sections"
    );
}

#[test]
fn test_save_and_cache_parity_with_testkit() {
    let Some(jar) = mtk_testkit::fabric_jar() else {
        mtk_testkit::skip("no Minecraft jar (set MTK_TEST_JAR or place ../26.2-Fabric.jar)");
        return;
    };
    let Some(save_dir) = mtk_testkit::save_world() else {
        mtk_testkit::skip("no Minecraft save world available (set MTK_TEST_SAVE)");
        return;
    };

    let temp_dir = std::env::temp_dir().join(format!(
        "mtk_wasm_parity_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let mut stack = mtk_resource::ResourcePackStack::new();
    stack.push_pack(Box::new(
        mtk_resource::ZipPack::from_file("vanilla", &jar).expect("open jar"),
    ));
    if let Some(assets) = mtk_testkit::real_assets_root() {
        stack.push_pack(Box::new(mtk_resource::DirectoryPack::new(
            "mojang_assets",
            &assets,
        )));
    }

    let cache_dir = temp_dir.join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let precompile = libmtk::precompile_all_assets(
        &stack,
        &cache_dir,
        &libmtk::PrecompileConfig {
            max_atlas_width: 4096,
            max_atlas_height: 4096,
            atlas_category: "blocks".to_string(),
            compile_atlas: true,
            compile_models: true,
            num_threads: None,
            resolver_overrides: None,
        },
    )
    .expect("precompile testkit assets");

    let cache_bytes = std::fs::read(&precompile.package_path).expect("read mtkcache");
    let cache = WasmAssetCache::from_bytes(&cache_bytes).expect("open WasmAssetCache");

    let min_b = glam::IVec3::new(-16, -64, -16);
    let max_b = glam::IVec3::new(15, 127, 15);

    let mut rel_paths = Vec::new();
    let mut stack_dirs = vec![save_dir.clone()];
    while let Some(dir) = stack_dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir").flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack_dirs.push(p);
            } else if let Ok(rel) = p.strip_prefix(&save_dir) {
                rel_paths.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let plan = mtk_save::SaveLoader::plan_dimension_region_files(
        "overworld",
        min_b,
        max_b,
        rel_paths.iter().map(String::as_str),
    );
    assert!(!plan.regions.is_empty());

    let loaded_mca: Vec<(i32, i32, Vec<u8>)> = plan
        .regions
        .iter()
        .map(|r| (r.rx, r.rz, std::fs::read(save_dir.join(&r.path)).unwrap()))
        .collect();
    let region_refs: Vec<mtk_save::RegionBytes<'_>> = loaded_mca
        .iter()
        .map(|(rx, rz, data)| mtk_save::RegionBytes {
            rx: *rx,
            rz: *rz,
            data,
        })
        .collect();
    let mut ext_map = HashMap::new();
    for ext in &plan.external_chunks {
        ext_map.insert(
            (ext.cx, ext.cz),
            std::fs::read(save_dir.join(&ext.path)).unwrap(),
        );
    }

    let mut save_world = WasmSaveWorld::from_cache_for_test(&cache).expect("new WasmSaveWorld");
    let summary = save_world
        .load_regions_internal(&region_refs, &ext_map, min_b, max_b)
        .expect("load_regions");

    let skin_img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(64, 64, Rgba([200, 160, 120, 255]));
    let mut offline_skin_png = Vec::new();
    skin_img
        .write_to(
            &mut Cursor::new(&mut offline_skin_png),
            image::ImageFormat::Png,
        )
        .unwrap();
    let pending_skins = save_world.collect_pending_skin_requests();
    for req in &pending_skins {
        assert!(save_world.register_skin_png_internal(&req.key, &offline_skin_png));
    }

    let mut storage = mtk_voxel::VoxelStorage::new();
    mtk_save::SaveLoader::load_box_into_storage(&save_dir, "overworld", min_b, max_b, &mut storage)
        .expect("load_box_into_storage");

    let cache_reader = libmtk::AssetCacheReader::open(&precompile.package_path).unwrap();
    let models = cache_reader.load_models().unwrap();
    let atlas_map = cache_reader.load_atlas_mapping().unwrap();
    let sparse = SparseVoxelPayload::from_storage(
        &storage,
        &mtk_voxel::MesherConfig {
            coordinate_system: mtk_voxel::CoordinateSystem::Minecraft,
            origin_centered: false,
            ..Default::default()
        },
    )
    .expect("sparse payload");

    let mut world_for_entities = mtk_voxel::VoxelWorld::from_storage(
        storage,
        Some(mtk_voxel::MesherConfig {
            atlas_address_map: Some(std::sync::Arc::new(atlas_map.clone())),
            ..Default::default()
        }),
        None,
        None,
    );
    let mut textures = mtk_block_entity_textures::BlockEntityTextures::default();
    textures.set_allow_network(false);
    textures.set_banner_baker(cache_reader.load_banner_baker().unwrap());
    world_for_entities.set_block_entity_textures(Some(Box::new(textures)));
    for req in &pending_skins {
        world_for_entities.register_memory_skin(&req.key, offline_skin_png.clone());
    }
    world_for_entities.prepare_block_entity_textures(
        None,
        mtk_voxel::TextureResolveMode::Cached,
        None,
    );
    let dyn_atlases = world_for_entities.dynamic_entity_atlases_owned();
    let dyn_slots = world_for_entities.dynamic_entity_slots();

    let scene_path = temp_dir.join("parity.mtkscene");
    let mut cache_source = cache_reader;
    libmtk::export_scene_package(
        &scene_path,
        libmtk::SceneExportRequest {
            voxels: &sparse,
            source_models: &models,
            source_atlas_map: &atlas_map,
            atlas_source: &mut cache_source,
            config: &libmtk::SceneExporterConfig::default(),
            dynamic_entity_atlases: (!dyn_atlases.is_empty()).then_some(&dyn_atlases),
            dynamic_entity_slots: (!dyn_slots.is_empty()).then_some(&dyn_slots),
            ctm_mode: libmtk::CtmMode::Auto,
        },
    )
    .expect("export parity.mtkscene");

    let save_render = save_world
        .build_render_scene_internal(None)
        .expect("build save render scene");

    let scene_bytes = std::fs::read(&scene_path).expect("read parity.mtkscene");
    let scene_pkg = WasmScenePackage::from_bytes(&scene_bytes).expect("open WasmScenePackage");
    let scene_meta: serde_json::Value =
        serde_json::from_str(&scene_pkg.metadata_json().unwrap()).unwrap();
    let scene_render = scene_pkg
        .build_render_scene(None)
        .expect("build scene render");

    assert_eq!(
        summary.voxel_count as u64,
        scene_meta["manifest"]["voxel_count"].as_u64().unwrap()
    );
    assert_eq!(
        save_render.total_triangles(),
        scene_render.total_triangles()
    );
    assert_eq!(save_render.total_vertices(), scene_render.total_vertices());

    let _ = std::fs::remove_dir_all(&temp_dir);
}
