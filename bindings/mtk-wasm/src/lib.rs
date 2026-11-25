//! # `mtk-wasm` (WebAssembly Bindings for Web `.mtkscene` & Save Viewer)
//!
//! Provides WebAssembly bindings for loading `.mtkscene` interchange packages
//! or `.mtkcache` + Minecraft save regions in the browser and emitting
//! Three.js / WebGPU-ready `PerVertex` geometry batches with zero-copy buffer
//! pointers and raw PNG atlas chunks.

mod cache;
mod render;
mod save;
mod scene;

use mtk_core::threading;
use wasm_bindgen::prelude::*;

pub use cache::WasmAssetCache;
pub use render::{WasmRenderBatch, WasmRenderScene};
pub use save::WasmSaveWorld;
pub use scene::WasmScenePackage;

#[cfg(feature = "parallel")]
pub use wasm_bindgen_rayon::init_thread_pool;

/// Sets panic hook for readable console error stacks in browser devtools.
#[wasm_bindgen]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

/// Returns `mtk-wasm` version string.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Configures the hardware concurrency count (e.g. from `navigator.hardwareConcurrency`).
#[wasm_bindgen]
pub fn set_hardware_concurrency(count: usize) {
    threading::set_hardware_concurrency(count);
}

/// Returns the detected or configured hardware concurrency count.
#[wasm_bindgen]
pub fn get_hardware_concurrency() -> usize {
    threading::get_safe_hardware_concurrency()
}

#[cfg(test)]
mod tests {
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
        let leaves_loc =
            ResourceLocation::parse_texture_path("minecraft:block/oak_leaves").unwrap();

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
        let mut render_scene = pkg
            .build_render_scene(Some(true), Some(true), Some(true), Some(true))
            .expect("build render scene");
        assert_eq!(render_scene.batch_count(), 1);
        let batch = render_scene.take_batch(0).expect("take batch 0");

        let pos =
            unsafe { std::slice::from_raw_parts(batch.positions_ptr(), batch.positions_len()) };
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

        let mut render_scene = pkg
            .build_render_scene(Some(true), Some(true), Some(true), Some(true))
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
        // Every Three.js vertex buffer must have aligned PerVertex counts
        let v_count = batch.vertex_count();
        assert_eq!(batch.positions_len(), v_count * 3);
        assert_eq!(batch.normals_len(), v_count * 3);
        assert_eq!(batch.uvs_len(), v_count * 2);
        assert_eq!(batch.colors_len(), v_count * 4);
        assert_eq!(batch.tint_weights_len(), v_count * 2);
        assert_eq!(batch.indices_len(), batch.triangle_count() * 3);
    }

    #[test]
    fn test_save_and_cache_parity_against_untitled_mtkscene() {
        let cache_path = std::path::Path::new(
            "/Users/jaxlocke/Library/Application Support/Blender/5.2/extensions/.user/vscode_development/MoziToolKit/cache/mtk_fp_58d0c7fee81ad3bf.mtkcache",
        );
        let save_dir = std::path::Path::new(
            "/Users/jaxlocke/Library/Application Support/PrismLauncher/instances/26.2/minecraft/saves/新的世界",
        );
        let scene_path = std::path::Path::new("/Users/jaxlocke/Untitled.mtkscene");
        if !cache_path.exists() || !save_dir.exists() || !scene_path.exists() {
            println!("SKIP test_save_and_cache_parity_against_untitled_mtkscene: host test assets missing");
            return;
        }

        let cache_bytes = std::fs::read(cache_path).expect("read mtkcache");
        let cache = WasmAssetCache::from_bytes(&cache_bytes).expect("open WasmAssetCache");

        // Collect normalized save-relative file paths just as browser webkitdirectory/FSA would.
        let mut rel_paths = Vec::new();
        let mut stack = vec![save_dir.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read_dir").flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(rel) = p.strip_prefix(save_dir) {
                    rel_paths.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        let dims = mtk_save::SaveLoader::discover_dimensions_from_paths(
            rel_paths.iter().map(String::as_str),
        );
        assert!(dims.contains(&"overworld".to_string()), "dims={dims:?}");

        let level_bytes = std::fs::read(save_dir.join("level.dat")).expect("read level.dat");
        let level = mtk_save::LevelData::from_bytes(&level_bytes).expect("parse level.dat");
        assert!(!level.level_name.is_empty());

        let min_b = glam::IVec3::new(-64, -64, -64);
        let max_b = glam::IVec3::new(64, 320, 64);
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
            .map(|r| {
                (
                    r.rx,
                    r.rz,
                    std::fs::read(save_dir.join(&r.path)).expect("read mca"),
                )
            })
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
                std::fs::read(save_dir.join(&ext.path)).expect("read mcc"),
            );
        }

        let mut save_world = WasmSaveWorld::from_cache_for_test(&cache).expect("new WasmSaveWorld");
        let summary = save_world
            .load_regions_internal(&region_refs, &ext_map, min_b, max_b)
            .expect("load_regions");
        let pending_skins = save_world.collect_pending_skin_requests();
        let mut save_render = save_world
            .build_render_scene_internal(Some(true), Some(true), Some(true), Some(true))
            .expect("build save render scene");

        // Compare against Untitled.mtkscene exported from the same selection.
        let scene_bytes = std::fs::read(scene_path).expect("read Untitled.mtkscene");
        let scene_pkg = WasmScenePackage::from_bytes(&scene_bytes).expect("open WasmScenePackage");
        let scene_meta_json = scene_pkg.metadata_json().expect("scene meta");
        let scene_meta: serde_json::Value = serde_json::from_str(&scene_meta_json).unwrap();
        let mut scene_render = scene_pkg
            .build_render_scene(Some(true), Some(true), Some(true), Some(true))
            .expect("build scene render");

        println!(
            "PARITY COMPARISON:\n  save summary: sections={}, voxels={}, bounds={:?}, pending_skins={}\n  save render : batches={}, verts={}, tris={}\n  scene meta  : voxels={}, bounds={}\n  scene render: batches={}, verts={}, tris={}",
            summary.loaded_sections,
            summary.voxel_count,
            summary.bounds,
            pending_skins.len(),
            save_render.batch_count(),
            save_render.total_vertices(),
            save_render.total_triangles(),
            scene_meta["manifest"]["voxel_count"],
            scene_meta["manifest"]["bounds"],
            scene_render.batch_count(),
            scene_render.total_vertices(),
            scene_render.total_triangles(),
        );

        let save_batch_summary: Vec<(i32, usize, usize, Option<String>)> = (0..save_render
            .batch_count())
            .map(|i| {
                let b = save_render.take_batch(i).unwrap();
                (
                    b.chunk_id(),
                    b.vertex_count(),
                    b.triangle_count(),
                    b.albedo_chunk_id(),
                )
            })
            .collect();
        let scene_batch_summary: Vec<(i32, usize, usize, Option<String>)> = (0..scene_render
            .batch_count())
            .map(|i| {
                let b = scene_render.take_batch(i).unwrap();
                (
                    b.chunk_id(),
                    b.vertex_count(),
                    b.triangle_count(),
                    b.albedo_chunk_id(),
                )
            })
            .collect();
        println!("  save batches : {save_batch_summary:?}");
        println!("  scene batches: {scene_batch_summary:?}");
        println!("  pending skins: {pending_skins:?}");
        println!("  save textures: {:?}", save_world.texture_names());

        // Check dynamic entity PNG readability and assert zero collision with static entities atlas.
        for (cid, _, _, albedo) in &save_batch_summary {
            if *cid >= mtk_texture::DEFAULT_DYNAMIC_ENTITIES_CHUNK_ID as i32 {
                let tex_id = albedo.as_deref().expect("dynamic albedo id");
                assert!(tex_id.starts_with("dynamic/chunk_"));
                let png = save_world
                    .read_texture_png_bytes_internal(tex_id)
                    .expect("encode dynamic atlas PNG");
                assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
            }
        }
        assert!(
            save_world
                .texture_names()
                .contains(&"dynamic/chunk_65520.png".to_string()),
            "expected dynamic/chunk_65520.png from prewarmed BannerBaker atlas"
        );
        assert!(save_world
            .read_texture_png_bytes_internal("dynamic/chunk_65520.png")
            .expect("read dynamic banner/skin atlas")
            .starts_with(&[0x89, b'P', b'N', b'G']));

        assert_eq!(
            summary.voxel_count as u64,
            scene_meta["manifest"]["voxel_count"].as_u64().unwrap()
        );
        assert_eq!(
            save_render.total_triangles(),
            scene_render.total_triangles()
        );
        assert_eq!(save_render.total_vertices(), scene_render.total_vertices());
    }
}
