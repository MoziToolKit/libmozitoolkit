//! # `mtk-wasm` (WebAssembly Bindings for Web `.mtkscene` Viewer)
//!
//! Provides WebAssembly bindings for loading `.mtkscene` interchange packages
//! in the browser and emitting Three.js / WebGPU-ready `PerVertex` geometry
//! batches with zero-copy buffer pointers and raw PNG atlas chunks.

mod render;
mod scene;

use mtk_core::threading;
use wasm_bindgen::prelude::*;

pub use render::{WasmRenderBatch, WasmRenderScene};
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

    fn build_test_mtkscene() -> Vec<u8> {
        let mut voxels = SparseVoxelPayload::new();
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
                BakedFace {
                    direction: dir,
                    texture: tex_key.to_string(),
                    uv_rot: 0.0,
                    uv_bounds: [0.0, 0.0, 1.0, 1.0],
                    tint_index,
                    cullface: Some(dir),
                    vertices: [
                        Vec3::new(0.0, 0.0, 0.0),
                        Vec3::new(1.0, 0.0, 0.0),
                        Vec3::new(1.0, 1.0, 0.0),
                        Vec3::new(0.0, 1.0, 0.0),
                    ],
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
    fn test_wasm_concurrency_and_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
        set_hardware_concurrency(8);
        assert_eq!(get_hardware_concurrency(), 8);
        set_hardware_concurrency(0);
    }

    #[test]
    fn test_scene_package_viewer_roundtrip_and_batches() {
        let bytes = build_test_mtkscene();
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
}
