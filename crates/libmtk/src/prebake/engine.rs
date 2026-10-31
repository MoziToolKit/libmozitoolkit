//! End-to-end unified precompilation engine producing `.mtkcache` binary packages.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use mtk_core::progress::{ProgressCallback, ProgressReport, ProgressThrottler};
use mtk_material::BiomeResolver;
use mtk_package::{ChunkWriteOptions, MtkPackageFileWriter, PackageProfile};
use mtk_resource::{AtlasCategory, ResourcePackStack};
use mtk_texture::{AtlasBuilder, AtlasBuilderConfig};

use crate::prebake::models::prebake_all_models_with_progress;
use crate::prebake::reader::{
    fingerprint_str_to_bytes16, CacheManifest, ASSET_CACHE_FORMAT_VERSION,
};
use crate::MtkError;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Configuration for unified asset precompilation.
#[derive(Debug, Clone)]
pub struct PrecompileConfig {
    pub max_atlas_width: u32,
    pub max_atlas_height: u32,
    pub atlas_category: String,
    pub compile_atlas: bool,
    pub compile_models: bool,
    pub num_threads: Option<usize>,
    /// Optional external material/biome compatibility overrides baked into the cache.
    pub resolver_overrides: Option<mtk_material::resolver::overrides::ResolverOverrides>,
}

impl Default for PrecompileConfig {
    fn default() -> Self {
        Self {
            max_atlas_width: 4096,
            max_atlas_height: 4096,
            atlas_category: "all".to_string(),
            compile_atlas: true,
            compile_models: true,
            num_threads: None,
            resolver_overrides: None,
        }
    }
}

/// Statistics and summary of a precompilation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecompileResult {
    pub success: bool,
    pub pack_count: usize,
    pub atlas_chunks: usize,
    pub baked_models: usize,
    pub fingerprint: String,
    pub package_path: String,
    pub cache_dir: String,
    pub profile: Option<mtk_core::profile::ExecutionProfile>,
}

/// Executes unified end-to-end asset precompilation directly to `.mtkcache` with progress reporting.
pub fn precompile_all_assets_with_progress(
    stack: &ResourcePackStack,
    cache_dir_or_file: impl AsRef<Path>,
    config: &PrecompileConfig,
    progress_callback: Option<ProgressCallback>,
) -> Result<PrecompileResult, MtkError> {
    let base_path = cache_dir_or_file.as_ref();
    mtk_core::constants::concurrency::execute_parallel(config.num_threads, || {
        precompile_all_assets_inner(stack, base_path, config, progress_callback)
    })
    .map_err(MtkError::ThreadPool)?
}

/// Executes unified end-to-end asset precompilation directly to `.mtkcache`.
pub fn precompile_all_assets(
    stack: &ResourcePackStack,
    cache_dir_or_file: impl AsRef<Path>,
    config: &PrecompileConfig,
) -> Result<PrecompileResult, MtkError> {
    precompile_all_assets_with_progress(stack, cache_dir_or_file, config, None)
}

fn precompile_all_assets_inner(
    stack: &ResourcePackStack,
    base_path: &Path,
    config: &PrecompileConfig,
    progress_callback: Option<ProgressCallback>,
) -> Result<PrecompileResult, MtkError> {
    if stack.is_empty() || !stack.has_vanilla_client_base() {
        return Err(MtkError::Resource(mtk_resource::error::ResourceError::MissingVanillaBasePack(
            "Cannot precompile: resource pack stack must contain a valid Minecraft Vanilla Client base pack".into(),
        )));
    }

    let fingerprint = stack.compute_stack_fingerprint();

    // Apply external compatibility rules for atlas/model baking in this run.
    match config.resolver_overrides.as_ref() {
        Some(overrides) => mtk_material::resolver::set_material_overrides(overrides.clone()),
        None => mtk_material::resolver::clear_material_overrides(),
    }

    // Determine target .mtkcache file path
    let package_path = if base_path.extension().is_some_and(|ext| ext == "mtkcache") {
        if let Some(parent) = base_path.parent() {
            fs::create_dir_all(parent)?;
        }
        base_path.to_path_buf()
    } else {
        fs::create_dir_all(base_path)?;
        base_path.join(format!("{}.mtkcache", fingerprint))
    };

    let fp_bytes = fingerprint_str_to_bytes16(&fingerprint);
    let mut writer =
        MtkPackageFileWriter::create(&package_path, PackageProfile::AssetCache, fp_bytes)?;

    let mut profiler = mtk_core::ExecutionProfiler::start();

    // 0. Biome Tinting & Colormaps
    if let Some(cb) = progress_callback {
        cb(ProgressReport::new(
            "prebake_biome",
            0,
            1,
            "Extracting biome mappings and colormaps...",
        ));
    }

    let biome_resolver =
        profiler.measure("prebake_biome", || -> Result<BiomeResolver, MtkError> {
            let mut biome_resolver = BiomeResolver::new();
            biome_resolver.load_from_pack_stack(stack);
            if let Some(overrides) = config.resolver_overrides.as_ref() {
                biome_resolver.apply_overrides(overrides);
                let overrides_json = serde_json::to_string(overrides)?;
                writer.add_str_chunk(
                    *b"COMP",
                    "resolver/overrides",
                    &overrides_json,
                    &ChunkWriteOptions::zstd_fast(),
                )?;
            }
            let biome_json = biome_resolver.to_json()?;
            writer.add_str_chunk(
                *b"BIOM",
                "biome/mapping",
                &biome_json,
                &ChunkWriteOptions::zstd_fast(),
            )?;

            for cm_name in &["grass", "foliage", "dry_foliage"] {
                let asset_path = format!("assets/minecraft/textures/colormap/{}.png", cm_name);
                if let Some(bytes) = stack.open_asset_raw(&asset_path) {
                    writer.add_chunk(
                        *b"BIOM",
                        format!("biome/colormap/{}", cm_name),
                        &bytes,
                        &ChunkWriteOptions::raw(),
                    )?;
                }
            }
            Ok(biome_resolver)
        })?;

    if let Some(cb) = progress_callback {
        cb(ProgressReport::new(
            "prebake_biome",
            1,
            1,
            "Extracted biome mappings and colormaps",
        ));
    }

    let mut chunk_count = 0;
    let mut baked_count = 0;
    let mut baked_atlas_opt = None;

    // 1. Atlas Baking & Packaging (Raw PNG streams, Zstd for address map)
    if config.compile_atlas {
        if let Some(cb) = progress_callback {
            cb(ProgressReport::new(
                "prebake_atlas",
                0,
                1,
                "Packing sprite textures into atlases...",
            ));
        }

        let baked_atlas = profiler.measure("prebake_atlas", || -> Result<_, MtkError> {
            let mut def_list: Vec<(String, mtk_resource::AtlasDefinition)> = Vec::new();
            let mut seen_cats = std::collections::HashSet::new();

            if config.atlas_category != "all"
                && config.atlas_category != "blocks"
                && !config.atlas_category.is_empty()
            {
                let cat = AtlasCategory::parse(&config.atlas_category);
                let def = stack.load_atlas_category(&cat);
                def_list.push((cat.as_str().to_string(), def));
            } else {
                for loc in stack.list_all_atlas_locations() {
                    if let Ok(mut def) = stack.load_atlas_definition(&loc) {
                        let cat_name = loc.path.clone();
                        // Banner patterns are synthesized dynamically into the Dynamic Entity Atlas;
                        // for the static atlas, we only bake the basic banner stand/pole pattern ("banner_base").
                        if cat_name == "banner_patterns" {
                            def = mtk_resource::AtlasDefinition {
                                sources: vec![mtk_resource::AtlasSource::Single {
                                    resource: mtk_resource::ResourceLocation::new(
                                        "minecraft",
                                        "entity/banner/banner_base",
                                    ),
                                    sprite: Some(mtk_resource::ResourceLocation::new(
                                        "minecraft",
                                        "entity/banner/banner_base",
                                    )),
                                }],
                            };
                        }
                        if seen_cats.insert(cat_name.clone()) {
                            def_list.push((cat_name, def));
                        }
                    }
                }
                for standard_cat in &AtlasCategory::ALL_STANDARD {
                    let cat_name = standard_cat.as_str().to_string();
                    if seen_cats.insert(cat_name.clone()) {
                        let def = stack.load_atlas_category(standard_cat);
                        def_list.push((cat_name, def));
                    }
                }
            }

            let atlas_cfg = AtlasBuilderConfig {
                max_width: config.max_atlas_width,
                max_height: config.max_atlas_height,
                mip_level: 0,
                padding: 0,
            };
            let atlas_builder = AtlasBuilder::new(atlas_cfg);
            let mut baked_atlas = atlas_builder.build_categories(stack, &def_list)?;

            // Decode raw LabPBR companions into canonical MtkPackedV1 channels before the
            // address map is serialized, so has_orm/has_aux/pbr metadata is authoritative.
            mtk_material::pbr::decode_baked_atlas(&mut baked_atlas)?;

            // Determine authoritative has_tint for each chunk
            for chunk_meta in &mut baked_atlas.address_map.chunks {
                if chunk_meta.category != "blocks" {
                    chunk_meta.has_tint = false;
                    continue;
                }
                let mut chunk_has_tint = false;
                for (res, loc) in &baked_atlas.address_map.sprites {
                    if loc.chunk_id == chunk_meta.chunk_id {
                        let tint_info = biome_resolver.get_tint_info(&res.path, None, None);
                        if tint_info.tint_type != mtk_material::biome::TINT_TYPE_NONE {
                            chunk_has_tint = true;
                            break;
                        }
                    }
                }
                if !chunk_has_tint {
                    for (res, loc) in &baked_atlas.address_map.anim_sprites {
                        if loc.chunk_id == chunk_meta.chunk_id {
                            let tint_info = biome_resolver.get_tint_info(&res.path, None, None);
                            if tint_info.tint_type != mtk_material::biome::TINT_TYPE_NONE {
                                chunk_has_tint = true;
                                break;
                            }
                        }
                    }
                }
                chunk_meta.has_tint = chunk_has_tint;
            }

            let total_chunks = baked_atlas.chunks.len();

            let mapping_json = baked_atlas.address_map.to_json()?;
            writer.add_str_chunk(
                *b"ATLS",
                "atlas/mapping",
                &mapping_json,
                &ChunkWriteOptions::zstd_fast(),
            )?;

            let atlas_throttler =
                ProgressThrottler::new("prebake_atlas", total_chunks.max(1), progress_callback)
                    .with_step(1)
                    .with_prefix("Packaging atlas chunk textures");

            let mut texture_tasks = Vec::new();
            for chunk in &baked_atlas.chunks {
                let stem = chunk.file_stem();
                texture_tasks.push((format!("atlas/textures/{}.png", stem), &chunk.albedo));
                if let Some(ref normal) = chunk.normal_gl {
                    texture_tasks.push((format!("atlas/textures/{}_n.png", stem), normal));
                }
                if let Some(ref orm) = chunk.orm {
                    texture_tasks.push((format!("atlas/textures/{}_orm.png", stem), orm));
                }
                if let Some(ref aux) = chunk.aux {
                    texture_tasks.push((format!("atlas/textures/{}_aux.png", stem), aux));
                }
                if let Some(ref overlay) = chunk.overlay {
                    texture_tasks.push((format!("atlas/textures/{}_overlay.png", stem), overlay));
                }
            }

            #[cfg(feature = "parallel")]
            let encoded_results: Result<Vec<(String, Vec<u8>)>, MtkError> = texture_tasks
                .into_par_iter()
                .map(|(path, buf)| {
                    let bytes = buf.to_png_bytes().map_err(MtkError::Texture)?;
                    Ok((path, bytes))
                })
                .collect();

            #[cfg(not(feature = "parallel"))]
            let encoded_results: Result<Vec<(String, Vec<u8>)>, MtkError> = texture_tasks
                .into_iter()
                .map(|(path, buf)| {
                    let bytes = buf.to_png_bytes().map_err(MtkError::Texture)?;
                    Ok((path, bytes))
                })
                .collect();

            let encoded_textures = encoded_results?;
            for (path, bytes) in encoded_textures {
                writer.add_chunk(*b"ATLS", path, &bytes, &ChunkWriteOptions::raw())?;
                atlas_throttler.inc();
            }

            Ok(baked_atlas)
        })?;

        chunk_count = baked_atlas.chunks.len();
        baked_atlas_opt = Some(baked_atlas);
    }

    // 2. Full-Scale Model Baking & Postcard Packaging (Zstd compressed)
    if config.compile_models {
        let atlas_map = baked_atlas_opt.as_ref().map(|a| &a.address_map);
        let model_db = profiler.measure("prebake_models", || {
            prebake_all_models_with_progress(stack, atlas_map, progress_callback)
        })?;
        baked_count = model_db.len();
        let bin_bytes = model_db.to_binary()?;
        writer.add_chunk(
            *b"MODL",
            "models/database",
            &bin_bytes,
            &ChunkWriteOptions::raw(),
        )?;
    }

    // 3. Package Banner Patterns and Base Cloth into BANR chunks for runtime Dynamic Entity Atlas synthesis
    let mut banner_baker = mtk_texture::BannerBaker::default();
    let _ = banner_baker.load_from_pack_stack(stack);
    if let Ok(base_bytes) = banner_baker.base_cloth_png_bytes() {
        writer.add_chunk(
            *b"BANR",
            "banner/base",
            &base_bytes,
            &ChunkWriteOptions::raw(),
        )?;
    }
    let pattern_map = banner_baker.all_pattern_png_bytes();
    if !pattern_map.is_empty() {
        if let Ok(pattern_bytes) = postcard::to_allocvec(&pattern_map) {
            writer.add_chunk(
                *b"BANR",
                "banner/patterns",
                &pattern_bytes,
                &ChunkWriteOptions::zstd_fast(),
            )?;
        }
    }

    // 4. Save Cache Manifest inside package
    let epoch_now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let manifest = CacheManifest {
        format_version: ASSET_CACHE_FORMAT_VERSION,
        fingerprint: fingerprint.clone(),
        pack_count: stack.len(),
        atlas_chunks: chunk_count,
        baked_models: baked_count,
        created_at_epoch_secs: epoch_now,
    };
    writer.add_json_chunk(
        *b"META",
        "manifest",
        &manifest,
        &ChunkWriteOptions::zstd_fast(),
    )?;

    // 4. Finalize container
    profiler.measure("finalize_package", || writer.finish())?;

    if let Some(cb) = progress_callback {
        cb(ProgressReport::new(
            "prebake_manifest",
            1,
            1,
            "Saved .mtkcache binary package and completed precompilation",
        ));
    }

    let cache_dir_str = if base_path.is_file() {
        base_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string())
    } else {
        base_path.to_string_lossy().to_string()
    };

    let profile = profiler.finish();

    Ok(PrecompileResult {
        success: true,
        pack_count: stack.len(),
        atlas_chunks: chunk_count,
        baked_models: baked_count,
        fingerprint,
        package_path: package_path.to_string_lossy().to_string(),
        cache_dir: cache_dir_str,
        profile: Some(profile),
    })
}
