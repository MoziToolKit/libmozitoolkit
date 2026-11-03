//! # Scene Interchange Exporter
//!
//! Provides the end-to-end export pipeline that tree-shakes block models, crops only
//! the referenced sprite textures from the active global atlas (including static, overlay,
//! animated strips, and dynamic entity skins/banners), stitches them into a compact,
//! self-contained Scene Mini-Atlas, remaps model UV coordinates, and bundles everything
//! into a self-contained `.mtkscene` package.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use mtk_model::{BakedModelDatabase, BlockEntityData};
use mtk_resource::{AnimationMetadata, ResourceLocation};
use mtk_texture::atlas::{
    AtlasAddressMap, AtlasChunkMeta, AtlasSpriteLocation, DYNAMIC_ATLAS_HEIGHT,
    DYNAMIC_ATLAS_WIDTH, DYNAMIC_CHUNK_START,
};
use mtk_texture::banner::BannerBaker;
use mtk_texture::{RgbaBuffer, Stitcher};
use mtk_voxel::SparseVoxelPayload;

use super::metadata::SceneMetadata;
use super::package::{write_scene_package, ScenePackageWriteOptions};
use crate::MtkError;

/// Abstract source for reading raw or prebaked atlas chunk images.
pub trait SceneAtlasSource {
    /// Loads the Albedo RGBA image for the specified atlas chunk stem (e.g. `"blocks_chunk_001"`).
    fn get_albedo(
        &mut self,
        chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError>;
    /// Loads the Normal RGBA image for the specified atlas chunk stem (if present).
    fn get_normal(
        &mut self,
        chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError>;
    /// Loads the ORM RGBA image for the specified atlas chunk stem (if present).
    fn get_orm(&mut self, chunk_id: u16, chunk_stem: &str) -> Result<Option<RgbaBuffer>, MtkError>;
    /// Loads the AUX RGBA image for the specified atlas chunk stem (if present).
    fn get_aux(&mut self, chunk_id: u16, chunk_stem: &str) -> Result<Option<RgbaBuffer>, MtkError>;
    /// Loads the Overlay RGBA image for the specified atlas chunk stem (if present).
    fn get_overlay(
        &mut self,
        chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError>;
}

impl SceneAtlasSource for crate::prebake::AssetCacheReader {
    fn get_albedo(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let chunk_path = format!("atlas/textures/{}.png", chunk_stem);
        if self.package_reader().has_chunk(&chunk_path) {
            let buf = self.read_texture_rgba(&chunk_path)?;
            Ok(Some(buf))
        } else {
            Ok(None)
        }
    }

    fn get_normal(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let chunk_path = format!("atlas/textures/{}_n.png", chunk_stem);
        if self.package_reader().has_chunk(&chunk_path) {
            let buf = self.read_texture_rgba(&chunk_path)?;
            Ok(Some(buf))
        } else {
            Ok(None)
        }
    }

    fn get_orm(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let chunk_path = format!("atlas/textures/{}_orm.png", chunk_stem);
        if self.package_reader().has_chunk(&chunk_path) {
            let buf = self.read_texture_rgba(&chunk_path)?;
            Ok(Some(buf))
        } else {
            Ok(None)
        }
    }

    fn get_aux(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let chunk_path = format!("atlas/textures/{}_aux.png", chunk_stem);
        if self.package_reader().has_chunk(&chunk_path) {
            let buf = self.read_texture_rgba(&chunk_path)?;
            Ok(Some(buf))
        } else {
            Ok(None)
        }
    }

    fn get_overlay(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let chunk_path = format!("atlas/textures/{}_overlay.png", chunk_stem);
        if self.package_reader().has_chunk(&chunk_path) {
            let buf = self.read_texture_rgba(&chunk_path)?;
            Ok(Some(buf))
        } else {
            Ok(None)
        }
    }
}

impl SceneAtlasSource for HashMap<String, RgbaBuffer> {
    fn get_albedo(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let key = format!("{}.png", chunk_stem);
        Ok(self.get(&key).cloned())
    }

    fn get_normal(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let key = format!("{}_n.png", chunk_stem);
        Ok(self.get(&key).cloned())
    }

    fn get_orm(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let key = format!("{}_orm.png", chunk_stem);
        Ok(self.get(&key).cloned())
    }

    fn get_aux(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let key = format!("{}_aux.png", chunk_stem);
        Ok(self.get(&key).cloned())
    }

    fn get_overlay(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let key = format!("{}_overlay.png", chunk_stem);
        Ok(self.get(&key).cloned())
    }
}

/// Configuration options for building the scene interchange package.
#[derive(Debug, Clone)]
pub struct SceneExporterConfig {
    /// Maximum dimension of the generated scene mini-atlas (default 2048).
    pub max_atlas_size: u32,
    /// Mipmap level alignment (default 0).
    pub mip_level: u32,
}

impl Default for SceneExporterConfig {
    fn default() -> Self {
        Self {
            max_atlas_size: 2048,
            mip_level: 0,
        }
    }
}

/// In-memory artifacts generated by the scene interchange export pipeline.
#[derive(Debug, Clone)]
pub struct SceneExportResult {
    pub metadata: SceneMetadata,
    pub mini_atlas_map: AtlasAddressMap,
    pub mini_atlas_textures: Vec<(String, Vec<u8>)>,
    pub tree_shaken_models: BakedModelDatabase,
    pub sprite_count: usize,
}

struct SpriteCrop {
    key: String,
    width: u32,
    height: u32,
    albedo: RgbaBuffer,
    normal: Option<RgbaBuffer>,
    orm: Option<RgbaBuffer>,
    aux: Option<RgbaBuffer>,
    overlay: Option<RgbaBuffer>,
    source_loc: AtlasSpriteLocation,
}

struct AnimatedSpriteCrop {
    key: String,
    width: u32,
    height: u32,
    frame_count: u32,
    animation: Option<AnimationMetadata>,
    albedo: RgbaBuffer,
    normal: Option<RgbaBuffer>,
    orm: Option<RgbaBuffer>,
    aux: Option<RgbaBuffer>,
    source_loc: AtlasSpriteLocation,
}

/// Builds all components of a Scene Interchange bundle in memory.
pub fn build_scene_interchange(
    voxels: &SparseVoxelPayload,
    source_models: &BakedModelDatabase,
    source_atlas_map: &AtlasAddressMap,
    atlas_source: &mut impl SceneAtlasSource,
    config: &SceneExporterConfig,
    dynamic_entity_atlases: Option<&HashMap<u16, Vec<u8>>>,
) -> Result<SceneExportResult, MtkError> {
    // 1. Collect all unique texture identifiers needed by the scene
    let mut needed_textures = BTreeSet::new();
    let mut needed_blockstates = BTreeSet::new();

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
            }
        }
    }

    // Fluid auto-defense: if water or lava exists in states, inject still & flow textures
    for state in &voxels.state_palette {
        let lower = state.to_ascii_lowercase();
        if lower.contains("water") {
            needed_textures.insert("minecraft:block/water_still".to_string());
            needed_textures.insert("minecraft:block/water_flow".to_string());
        }
        if lower.contains("lava") {
            needed_textures.insert("minecraft:block/lava_still".to_string());
            needed_textures.insert("minecraft:block/lava_flow".to_string());
        }
    }

    // Inspect block entity visual NBTs for custom texture identifiers
    let mut dynamic_entity_specs = Vec::new();

    for entity_json in &voxels.entity_palette {
        if let Some(be) = BlockEntityData::from_visual_json(entity_json) {
            if let Some(tex) = be.texture_identifier() {
                needed_textures.insert(tex.clone());
                dynamic_entity_specs.push((tex, be.clone()));
            }
            match &be {
                BlockEntityData::DecoratedPot { sherds } => {
                    for sherd in sherds.iter().flatten() {
                        needed_textures.insert(BlockEntityData::sherd_texture(Some(sherd)));
                    }
                }
                BlockEntityData::ItemFrame { item_id, .. } => {
                    needed_textures.insert(item_id.clone());
                }
                _ => {}
            }
        }
    }

    // 2. Crop individual sprites from the source atlas sheets
    let mut loaded_albedo: HashMap<u16, Option<RgbaBuffer>> = HashMap::new();
    let mut loaded_normal: HashMap<u16, Option<RgbaBuffer>> = HashMap::new();
    let mut loaded_orm: HashMap<u16, Option<RgbaBuffer>> = HashMap::new();
    let mut loaded_aux: HashMap<u16, Option<RgbaBuffer>> = HashMap::new();
    let mut loaded_overlay: HashMap<u16, Option<RgbaBuffer>> = HashMap::new();

    let mut crops = Vec::new();
    let mut anim_crops = Vec::new();
    let mut processed_anim_keys = BTreeSet::new();

    // Pre-resolve banner baker for standalone banner generation if needed
    let mut banner_baker = BannerBaker::default();

    for tex_key in needed_textures {
        let maybe_loc = source_atlas_map.lookup_str(&tex_key);

        // Case A: Dynamic entity sprite in 0xFFF0..=0xFFFF
        let is_dynamic_chunk = maybe_loc
            .map(|l| l.chunk_id >= DYNAMIC_CHUNK_START)
            .unwrap_or(false);

        if is_dynamic_chunk {
            let sprite_loc = maybe_loc.unwrap();
            let chunk_id = sprite_loc.chunk_id;
            let [x, y, w, h] = sprite_loc.pixel_rect;

            let mut cropped_albedo: Option<RgbaBuffer> = None;
            if let Some(dyn_map) = dynamic_entity_atlases {
                if let Some(raw_bytes) = dyn_map.get(&chunk_id) {
                    if raw_bytes.len() >= (DYNAMIC_ATLAS_WIDTH * DYNAMIC_ATLAS_HEIGHT * 4) as usize
                    {
                        let full_buf = RgbaBuffer {
                            width: DYNAMIC_ATLAS_WIDTH,
                            height: DYNAMIC_ATLAS_HEIGHT,
                            pixels: raw_bytes.clone(),
                        };
                        if x + w <= full_buf.width && y + h <= full_buf.height && w > 0 && h > 0 {
                            cropped_albedo = Some(full_buf.crop(x, y, w, h));
                        }
                    }
                }
            }

            if let Some(albedo) = cropped_albedo {
                crops.push(SpriteCrop {
                    key: tex_key.clone(),
                    width: w,
                    height: h,
                    albedo,
                    normal: None,
                    orm: None,
                    aux: None,
                    overlay: None,
                    source_loc: sprite_loc.clone(),
                });
                continue;
            }
        }

        // Case B: Entity texture fallback when not registered in source atlas map
        if maybe_loc.is_none() {
            // Check if it's a banner
            if tex_key.starts_with("banner:")
                || tex_key.starts_with("minecraft:banner/")
                || tex_key.contains("_banner")
            {
                // Attempt to bake banner cloth using BannerBaker
                let clean = tex_key
                    .strip_prefix("minecraft:banner/")
                    .or_else(|| tex_key.strip_prefix("banner:"))
                    .unwrap_or(&tex_key);
                let (base_color, pat_str) = clean.split_once('+').unwrap_or((clean, ""));
                let mut patterns = Vec::new();
                if !pat_str.is_empty() {
                    for pat_part in pat_str.split('+') {
                        if let Some((p, c)) = pat_part.split_once('#') {
                            patterns.push((p, c));
                        }
                    }
                }
                let baked = banner_baker.bake(base_color, &patterns);
                let (w, h) = (baked.width, baked.height);
                let loc = AtlasSpriteLocation {
                    pixel_rect: [0, 0, w, h],
                    strip_pixel_rect: [0, 0, w, h],
                    frame_size: [w, h],
                    frame_count: 1,
                    ..Default::default()
                };
                crops.push(SpriteCrop {
                    key: tex_key.clone(),
                    width: w,
                    height: h,
                    albedo: (*baked).clone(),
                    normal: None,
                    orm: None,
                    aux: None,
                    overlay: None,
                    source_loc: loc,
                });
                continue;
            }

            // Check if it's a player skin
            if tex_key.starts_with("minecraft:skin/") {
                // If dynamic entity buffer is provided, try taking the first available skin or Steve fallback
                let mut skin_buf = None;
                if let Some(dyn_map) = dynamic_entity_atlases {
                    if let Some(raw_bytes) = dyn_map.get(&DYNAMIC_CHUNK_START) {
                        if raw_bytes.len()
                            >= (DYNAMIC_ATLAS_WIDTH * DYNAMIC_ATLAS_HEIGHT * 4) as usize
                        {
                            let full = RgbaBuffer {
                                width: DYNAMIC_ATLAS_WIDTH,
                                height: DYNAMIC_ATLAS_HEIGHT,
                                pixels: raw_bytes.clone(),
                            };
                            skin_buf = Some(full.crop(0, 0, 64, 64));
                        }
                    }
                }
                let albedo =
                    skin_buf.unwrap_or_else(|| RgbaBuffer::solid(64, 64, 200, 160, 140, 255));
                let loc = AtlasSpriteLocation {
                    pixel_rect: [0, 0, 64, 64],
                    strip_pixel_rect: [0, 0, 64, 64],
                    frame_size: [64, 64],
                    frame_count: 1,
                    ..Default::default()
                };
                crops.push(SpriteCrop {
                    key: tex_key.clone(),
                    width: 64,
                    height: 64,
                    albedo,
                    normal: None,
                    orm: None,
                    aux: None,
                    overlay: None,
                    source_loc: loc,
                });
                continue;
            }

            continue;
        }

        // Case C: Standard static or animated sprite from source atlas
        let sprite_loc = maybe_loc.unwrap();
        let chunk_id = sprite_loc.chunk_id;
        let chunk_meta = source_atlas_map.get_chunk_meta(chunk_id);
        let stem = chunk_meta
            .map(|m| m.file_stem())
            .unwrap_or_else(|| format!("blocks_chunk_{:03}", chunk_id + 1));

        // Load source chunk albedo, normal, orm, aux, overlay if not cached
        if let std::collections::hash_map::Entry::Vacant(e) = loaded_albedo.entry(chunk_id) {
            let alb = atlas_source.get_albedo(chunk_id, &stem)?;
            let norm = atlas_source.get_normal(chunk_id, &stem)?;
            let orm = atlas_source.get_orm(chunk_id, &stem)?;
            let aux = atlas_source.get_aux(chunk_id, &stem)?;
            let over = atlas_source.get_overlay(chunk_id, &stem)?;

            e.insert(alb);
            loaded_normal.insert(chunk_id, norm);
            loaded_orm.insert(chunk_id, orm);
            loaded_aux.insert(chunk_id, aux);
            loaded_overlay.insert(chunk_id, over);
        }

        let albedo_buf = match loaded_albedo.get(&chunk_id).and_then(|opt| opt.as_ref()) {
            Some(b) => b,
            None => continue,
        };

        let [x, y, w, h] = sprite_loc.pixel_rect;
        if w == 0 || h == 0 || x + w > albedo_buf.width || y + h > albedo_buf.height {
            continue;
        }

        let albedo_crop = albedo_buf.crop(x, y, w, h);
        let normal_crop = loaded_normal
            .get(&chunk_id)
            .and_then(|opt| opt.as_ref())
            .and_then(|b| {
                if x + w <= b.width && y + h <= b.height {
                    Some(b.crop(x, y, w, h))
                } else {
                    None
                }
            });
        let orm_crop = loaded_orm
            .get(&chunk_id)
            .and_then(|opt| opt.as_ref())
            .and_then(|b| {
                if x + w <= b.width && y + h <= b.height {
                    Some(b.crop(x, y, w, h))
                } else {
                    None
                }
            });
        let aux_crop = loaded_aux
            .get(&chunk_id)
            .and_then(|opt| opt.as_ref())
            .and_then(|b| {
                if x + w <= b.width && y + h <= b.height {
                    Some(b.crop(x, y, w, h))
                } else {
                    None
                }
            });

        // Overlay crop: only extract if overlay sheet exists and has non-zero alpha pixels in this rect
        let overlay_crop = loaded_overlay
            .get(&chunk_id)
            .and_then(|opt| opt.as_ref())
            .and_then(|b| {
                if x + w <= b.width && y + h <= b.height {
                    let c = b.crop(x, y, w, h);
                    if c.pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0) {
                        Some(c)
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

        crops.push(SpriteCrop {
            key: tex_key.clone(),
            width: w,
            height: h,
            albedo: albedo_crop,
            normal: normal_crop,
            orm: orm_crop,
            aux: aux_crop,
            overlay: overlay_crop,
            source_loc: sprite_loc.clone(),
        });

        // Check if there is an animated strip companion for this texture in source_atlas_map
        if !processed_anim_keys.contains(&tex_key) {
            if let Some(anim_loc) = source_atlas_map.lookup_animated_str(&tex_key) {
                processed_anim_keys.insert(tex_key.clone());
                let anim_chunk_id = anim_loc.chunk_id;
                let anim_stem = source_atlas_map
                    .get_chunk_meta(anim_chunk_id)
                    .map(|m| m.file_stem())
                    .unwrap_or_else(|| format!("blocks_anim_chunk_{:03}", anim_chunk_id + 1));

                if let std::collections::hash_map::Entry::Vacant(e) =
                    loaded_albedo.entry(anim_chunk_id)
                {
                    let alb = atlas_source.get_albedo(anim_chunk_id, &anim_stem)?;
                    let norm = atlas_source.get_normal(anim_chunk_id, &anim_stem)?;
                    let orm = atlas_source.get_orm(anim_chunk_id, &anim_stem)?;
                    let aux = atlas_source.get_aux(anim_chunk_id, &anim_stem)?;
                    e.insert(alb);
                    loaded_normal.insert(anim_chunk_id, norm);
                    loaded_orm.insert(anim_chunk_id, orm);
                    loaded_aux.insert(anim_chunk_id, aux);
                }

                if let Some(anim_alb_buf) =
                    loaded_albedo.get(&anim_chunk_id).and_then(|o| o.as_ref())
                {
                    let [ax, ay, aw, ah] = anim_loc.strip_pixel_rect;
                    if aw > 0
                        && ah > 0
                        && ax + aw <= anim_alb_buf.width
                        && ay + ah <= anim_alb_buf.height
                    {
                        let anim_alb_crop = anim_alb_buf.crop(ax, ay, aw, ah);
                        let anim_norm_crop = loaded_normal
                            .get(&anim_chunk_id)
                            .and_then(|o| o.as_ref())
                            .and_then(|b| {
                                if ax + aw <= b.width && ay + ah <= b.height {
                                    Some(b.crop(ax, ay, aw, ah))
                                } else {
                                    None
                                }
                            });
                        let anim_orm_crop = loaded_orm
                            .get(&anim_chunk_id)
                            .and_then(|o| o.as_ref())
                            .and_then(|b| {
                                if ax + aw <= b.width && ay + ah <= b.height {
                                    Some(b.crop(ax, ay, aw, ah))
                                } else {
                                    None
                                }
                            });
                        let anim_aux_crop = loaded_aux
                            .get(&anim_chunk_id)
                            .and_then(|o| o.as_ref())
                            .and_then(|b| {
                                if ax + aw <= b.width && ay + ah <= b.height {
                                    Some(b.crop(ax, ay, aw, ah))
                                } else {
                                    None
                                }
                            });

                        anim_crops.push(AnimatedSpriteCrop {
                            key: tex_key.clone(),
                            width: aw,
                            height: ah,
                            frame_count: anim_loc.frame_count.max(1),
                            animation: anim_loc.animation.clone(),
                            albedo: anim_alb_crop,
                            normal: anim_norm_crop,
                            orm: anim_orm_crop,
                            aux: anim_aux_crop,
                            source_loc: anim_loc.clone(),
                        });
                    }
                }
            }
        }
    }

    let sprite_count = crops.len();

    // 3. Stitch cropped sprites into a compact mini-atlas
    let mut mini_atlas_map = AtlasAddressMap::new();
    let mut mini_atlas_textures = Vec::new();
    let mut first_chunk_dims = [0u32, 0u32];

    // Helper closure to insert location with aliases
    let insert_sprite_aliases =
        |map: &mut AtlasAddressMap, key: &str, loc: &AtlasSpriteLocation| {
            if let Ok(res_loc) = ResourceLocation::parse_texture_path(key) {
                map.sprites.insert(res_loc, loc.clone());
            }
            if let Ok(res_loc) = ResourceLocation::parse(key) {
                map.sprites.insert(res_loc, loc.clone());
            }
            // Dynamic entity alias expansion
            if let Some(stripped) = key.strip_prefix("minecraft:skin/") {
                if let Ok(r) = ResourceLocation::parse(&format!("skin/{}", stripped)) {
                    map.sprites.insert(r, loc.clone());
                }
            } else if let Some(stripped) = key.strip_prefix("minecraft:banner/") {
                if let Ok(r) = ResourceLocation::parse(&format!("banner:{}", stripped)) {
                    map.sprites.insert(r, loc.clone());
                }
                if let Ok(r) = ResourceLocation::parse(&format!("banner/{}", stripped)) {
                    map.sprites.insert(r, loc.clone());
                }
            } else if let Some(stripped) = key.strip_prefix("banner:") {
                if let Ok(r) = ResourceLocation::parse(&format!("minecraft:banner/{}", stripped)) {
                    map.sprites.insert(r, loc.clone());
                }
            }
        };

    let insert_anim_aliases = |map: &mut AtlasAddressMap, key: &str, loc: &AtlasSpriteLocation| {
        if let Ok(res_loc) = ResourceLocation::parse_texture_path(key) {
            map.anim_sprites.insert(res_loc, loc.clone());
        }
        if let Ok(res_loc) = ResourceLocation::parse(key) {
            map.anim_sprites.insert(res_loc, loc.clone());
        }
    };

    // 3.1 Stitch Static Volume Chunks (is_animated = false)
    if !crops.is_empty() {
        let mut stitcher = Stitcher::new(
            config.max_atlas_size,
            config.max_atlas_size,
            config.mip_level,
        );
        for (idx, crop) in crops.iter().enumerate() {
            stitcher.register_sprite(idx, crop.width, crop.height, &crop.key);
        }

        let stitched_atlas = stitcher.stitch().map_err(MtkError::Texture)?;

        let has_any_normal = crops.iter().any(|c| c.normal.is_some());
        let has_any_orm = crops.iter().any(|c| c.orm.is_some());
        let has_any_aux = crops.iter().any(|c| c.aux.is_some());
        let has_any_overlay = crops.iter().any(|c| c.overlay.is_some());

        for (page_idx, chunk) in stitched_atlas.chunks.iter().enumerate() {
            let chunk_id = chunk.chunk_id;
            let stem = format!("scene_atlas_{:03}", page_idx + 1);

            if page_idx == 0 {
                first_chunk_dims = [chunk.width, chunk.height];
            }

            let mut chunk_albedo = RgbaBuffer::new(chunk.width, chunk.height);
            let mut chunk_normal = if has_any_normal {
                Some(RgbaBuffer::solid(
                    chunk.width,
                    chunk.height,
                    128,
                    128,
                    255,
                    255,
                ))
            } else {
                None
            };
            let mut chunk_orm = if has_any_orm {
                Some(RgbaBuffer::solid(
                    chunk.width,
                    chunk.height,
                    255,
                    255,
                    0,
                    255,
                ))
            } else {
                None
            };
            let mut chunk_aux = if has_any_aux {
                Some(RgbaBuffer::new(chunk.width, chunk.height))
            } else {
                None
            };
            let mut chunk_overlay = if has_any_overlay {
                Some(RgbaBuffer::new(chunk.width, chunk.height))
            } else {
                None
            };

            for slot in &chunk.slots {
                let crop = &crops[slot.entry];

                chunk_albedo.blit(&crop.albedo, 0, 0, slot.x, slot.y, slot.width, slot.height);
                if let (Some(cn), Some(sn)) = (chunk_normal.as_mut(), crop.normal.as_ref()) {
                    cn.blit(sn, 0, 0, slot.x, slot.y, slot.width, slot.height);
                }
                if let (Some(co), Some(so)) = (chunk_orm.as_mut(), crop.orm.as_ref()) {
                    co.blit(so, 0, 0, slot.x, slot.y, slot.width, slot.height);
                }
                if let (Some(ca), Some(sa)) = (chunk_aux.as_mut(), crop.aux.as_ref()) {
                    ca.blit(sa, 0, 0, slot.x, slot.y, slot.width, slot.height);
                }
                if let (Some(cv), Some(sv)) = (chunk_overlay.as_mut(), crop.overlay.as_ref()) {
                    cv.blit(sv, 0, 0, slot.x, slot.y, slot.width, slot.height);
                }

                // Standard UV calculation: Bottom-Left DCC UV coordinate space
                // v_min corresponds to bottom pixel edge (y + h), v_max to top pixel edge (y)
                let u_min = slot.x as f32 / chunk.width as f32;
                let u_max = (slot.x + slot.width) as f32 / chunk.width as f32;
                let v_min = 1.0 - (slot.y + slot.height) as f32 / chunk.height as f32;
                let v_max = 1.0 - slot.y as f32 / chunk.height as f32;

                let mut new_loc = crop.source_loc.clone();
                new_loc.chunk_id = chunk_id;
                new_loc.category = "scene".to_string();
                new_loc.is_animated = false;
                new_loc.texture_id = slot.entry as u32;
                new_loc.frame_0_uv_bounds = [u_min, v_min, u_max, v_max];
                new_loc.uv_bounds = [u_min, v_min, u_max, v_max];
                new_loc.frame_uv_step = [0.0, 0.0];
                new_loc.pixel_rect = [slot.x, slot.y, slot.width, slot.height];
                new_loc.strip_pixel_rect = [slot.x, slot.y, slot.width, slot.height];
                new_loc.frame_size = [slot.width, slot.height];
                new_loc.frame_count = 1;
                new_loc.has_normal = has_any_normal && crop.normal.is_some();
                new_loc.has_specular = false;
                new_loc.has_overlay = has_any_overlay && crop.overlay.is_some();

                insert_sprite_aliases(&mut mini_atlas_map, &crop.key, &new_loc);
            }

            let chunk_meta = AtlasChunkMeta {
                chunk_id,
                category: "scene".to_string(),
                is_animated: false,
                category_chunk_index: page_idx + 1,
                width: chunk.width,
                height: chunk.height,
                has_normal: has_any_normal,
                has_specular: false,
                has_overlay: has_any_overlay,
                has_orm: has_any_orm,
                has_aux: has_any_aux,
                has_tint: true,
                pbr: None,
                textures: Vec::new(),
            };
            mini_atlas_map.chunks.push(chunk_meta);

            let png_bytes = chunk_albedo.to_png_bytes().map_err(MtkError::Texture)?;
            mini_atlas_textures.push((format!("{}.png", stem), png_bytes));

            if let Some(norm) = chunk_normal {
                let png = norm.to_png_bytes().map_err(MtkError::Texture)?;
                mini_atlas_textures.push((format!("{}_n.png", stem), png));
            }
            if let Some(orm) = chunk_orm {
                let png = orm.to_png_bytes().map_err(MtkError::Texture)?;
                mini_atlas_textures.push((format!("{}_orm.png", stem), png));
            }
            if let Some(aux) = chunk_aux {
                let png = aux.to_png_bytes().map_err(MtkError::Texture)?;
                mini_atlas_textures.push((format!("{}_aux.png", stem), png));
            }
            if let Some(over) = chunk_overlay {
                let png = over.to_png_bytes().map_err(MtkError::Texture)?;
                mini_atlas_textures.push((format!("{}_overlay.png", stem), png));
            }
        }
    }

    // 3.2 Stitch Animated Volume Chunks (is_animated = true)
    let static_chunk_count = mini_atlas_map.chunks.len();
    if !anim_crops.is_empty() {
        let mut anim_stitcher = Stitcher::new(
            config.max_atlas_size,
            config.max_atlas_size,
            config.mip_level,
        );
        for (idx, crop) in anim_crops.iter().enumerate() {
            anim_stitcher.register_sprite(idx, crop.width, crop.height, &crop.key);
        }

        let stitched_anim = anim_stitcher.stitch().map_err(MtkError::Texture)?;

        let has_any_anim_normal = anim_crops.iter().any(|c| c.normal.is_some());
        let has_any_anim_orm = anim_crops.iter().any(|c| c.orm.is_some());
        let has_any_anim_aux = anim_crops.iter().any(|c| c.aux.is_some());

        for (page_idx, chunk) in stitched_anim.chunks.iter().enumerate() {
            let anim_chunk_id = (static_chunk_count + page_idx) as u16;
            let stem = format!("scene_anim_atlas_{:03}", page_idx + 1);

            let mut chunk_albedo = RgbaBuffer::new(chunk.width, chunk.height);
            let mut chunk_normal = if has_any_anim_normal {
                Some(RgbaBuffer::solid(
                    chunk.width,
                    chunk.height,
                    128,
                    128,
                    255,
                    255,
                ))
            } else {
                None
            };
            let mut chunk_orm = if has_any_anim_orm {
                Some(RgbaBuffer::solid(
                    chunk.width,
                    chunk.height,
                    255,
                    255,
                    0,
                    255,
                ))
            } else {
                None
            };
            let mut chunk_aux = if has_any_anim_aux {
                Some(RgbaBuffer::new(chunk.width, chunk.height))
            } else {
                None
            };

            for slot in &chunk.slots {
                let crop = &anim_crops[slot.entry];

                chunk_albedo.blit(&crop.albedo, 0, 0, slot.x, slot.y, slot.width, slot.height);
                if let (Some(cn), Some(sn)) = (chunk_normal.as_mut(), crop.normal.as_ref()) {
                    cn.blit(sn, 0, 0, slot.x, slot.y, slot.width, slot.height);
                }
                if let (Some(co), Some(so)) = (chunk_orm.as_mut(), crop.orm.as_ref()) {
                    co.blit(so, 0, 0, slot.x, slot.y, slot.width, slot.height);
                }
                if let (Some(ca), Some(sa)) = (chunk_aux.as_mut(), crop.aux.as_ref()) {
                    ca.blit(sa, 0, 0, slot.x, slot.y, slot.width, slot.height);
                }

                let u_min = slot.x as f32 / chunk.width as f32;
                let u_max = (slot.x + slot.width) as f32 / chunk.width as f32;
                let v_min = 1.0 - (slot.y + slot.height) as f32 / chunk.height as f32;
                let v_max = 1.0 - slot.y as f32 / chunk.height as f32;

                let frame_count = crop.frame_count.max(1);
                let frame_h = slot.height / frame_count;
                let v_frame_step = frame_h as f32 / chunk.height as f32;
                let frame_0_v_min = v_max - v_frame_step;

                let mut new_anim_loc = crop.source_loc.clone();
                new_anim_loc.chunk_id = anim_chunk_id;
                new_anim_loc.category = "scene".to_string();
                new_anim_loc.is_animated = true;
                new_anim_loc.texture_id = slot.entry as u32;
                new_anim_loc.uv_bounds = [u_min, v_min, u_max, v_max];
                new_anim_loc.frame_0_uv_bounds = [u_min, frame_0_v_min, u_max, v_max];
                new_anim_loc.frame_uv_step = [0.0, -v_frame_step];
                new_anim_loc.pixel_rect = [slot.x, slot.y, slot.width, frame_h];
                new_anim_loc.strip_pixel_rect = [slot.x, slot.y, slot.width, slot.height];
                new_anim_loc.frame_size = [slot.width, frame_h];
                new_anim_loc.frame_count = frame_count;
                new_anim_loc.animation = crop.animation.clone();
                new_anim_loc.has_normal = has_any_anim_normal && crop.normal.is_some();
                new_anim_loc.has_specular = false;
                new_anim_loc.has_overlay = false;

                insert_anim_aliases(&mut mini_atlas_map, &crop.key, &new_anim_loc);
            }

            let chunk_meta = AtlasChunkMeta {
                chunk_id: anim_chunk_id,
                category: "scene".to_string(),
                is_animated: true,
                category_chunk_index: page_idx + 1,
                width: chunk.width,
                height: chunk.height,
                has_normal: has_any_anim_normal,
                has_specular: false,
                has_overlay: false,
                has_orm: has_any_anim_orm,
                has_aux: has_any_anim_aux,
                has_tint: true,
                pbr: None,
                textures: Vec::new(),
            };
            mini_atlas_map.chunks.push(chunk_meta);

            let png_bytes = chunk_albedo.to_png_bytes().map_err(MtkError::Texture)?;
            mini_atlas_textures.push((format!("{}.png", stem), png_bytes));

            if let Some(norm) = chunk_normal {
                let png = norm.to_png_bytes().map_err(MtkError::Texture)?;
                mini_atlas_textures.push((format!("{}_n.png", stem), png));
            }
            if let Some(orm) = chunk_orm {
                let png = orm.to_png_bytes().map_err(MtkError::Texture)?;
                mini_atlas_textures.push((format!("{}_orm.png", stem), png));
            }
            if let Some(aux) = chunk_aux {
                let png = aux.to_png_bytes().map_err(MtkError::Texture)?;
                mini_atlas_textures.push((format!("{}_aux.png", stem), png));
            }
        }
    }

    // 4. Tree-shake models & remap UV coordinates to mini-atlas
    let mut tree_shaken_models = BakedModelDatabase::new();

    for state in needed_blockstates {
        if let Some(model) = source_models.get(&state) {
            let mut m = model.clone();
            m.remap_to_atlas_with(|tex_key| {
                mini_atlas_map
                    .lookup_str(tex_key)
                    .map(|loc| (loc.frame_0_uv_bounds, loc.chunk_id, loc.texture_id))
            });
            tree_shaken_models.insert(state.clone(), m);
        }

        if let Some(group) = source_models.get_variant_group(&state) {
            let mut g = group.clone();
            g.remap_to_atlas_with(|tex_key| {
                mini_atlas_map
                    .lookup_str(tex_key)
                    .map(|loc| (loc.frame_0_uv_bounds, loc.chunk_id, loc.texture_id))
            });
            tree_shaken_models.insert_variant_group(state, g);
        }
    }

    // 5. Assemble SceneMetadata
    let mut metadata = SceneMetadata::new(voxels.bounds, voxels.len());
    metadata.unique_states_count = voxels.state_palette.len();
    metadata.unique_biomes_count = voxels.biome_palette.len();
    metadata.unique_entities_count = voxels.entity_palette.len();
    metadata.atlas_resolution = first_chunk_dims;
    metadata.has_animated_atlas = !mini_atlas_map.anim_sprites.is_empty();

    Ok(SceneExportResult {
        metadata,
        mini_atlas_map,
        mini_atlas_textures,
        tree_shaken_models,
        sprite_count,
    })
}

/// End-to-end export function: builds the scene interchange and writes `.mtkscene` to disk.
pub fn export_scene_package(
    path: impl AsRef<Path>,
    voxels: &SparseVoxelPayload,
    source_models: &BakedModelDatabase,
    source_atlas_map: &AtlasAddressMap,
    atlas_source: &mut impl SceneAtlasSource,
    config: &SceneExporterConfig,
    dynamic_entity_atlases: Option<&HashMap<u16, Vec<u8>>>,
    write_options: Option<&ScenePackageWriteOptions>,
) -> Result<SceneExportResult, MtkError> {
    let result = build_scene_interchange(
        voxels,
        source_models,
        source_atlas_map,
        atlas_source,
        config,
        dynamic_entity_atlases,
    )?;

    write_scene_package(
        path,
        &result.metadata,
        voxels,
        Some(&result.tree_shaken_models),
        Some(&result.mini_atlas_map),
        &result.mini_atlas_textures,
        write_options,
    )?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtk_core::direction::Direction;
    use mtk_core::Vec2;
    use mtk_model::baker::baked_model::{BakedElement, BakedFace, BakedModel};
    use mtk_texture::atlas::AtlasChunkMeta;

    fn create_sample_cube(state: &str, tex: &str) -> BakedModel {
        let mut model = BakedModel {
            block_state: state.to_string(),
            is_cube: true,
            is_opaque: true,
            ..Default::default()
        };
        let mut faces = HashMap::new();
        for dir in Direction::ALL {
            let face = BakedFace {
                direction: dir,
                texture: tex.to_string(),
                cullface: Some(dir),
                uvs: [
                    Vec2::new(0.0, 0.0),
                    Vec2::new(1.0, 0.0),
                    Vec2::new(1.0, 1.0),
                    Vec2::new(0.0, 1.0),
                ],
                ..Default::default()
            };
            faces.insert(dir, face);
        }
        model.elements.push(BakedElement {
            from_pos: [0.0, 0.0, 0.0],
            to_pos: [16.0, 16.0, 16.0],
            faces,
        });
        model.rebuild_face_buckets();
        model
    }

    #[test]
    fn test_build_scene_interchange_pipeline() {
        // 1. Prepare fake source atlas texture and mapping
        let mut source_map = AtlasAddressMap::new();
        let chunk_meta = AtlasChunkMeta {
            chunk_id: 0,
            category: "blocks".to_string(),
            is_animated: false,
            category_chunk_index: 1,
            width: 64,
            height: 64,
            has_normal: false,
            has_specular: false,
            has_overlay: false,
            has_orm: false,
            has_aux: false,
            has_tint: false,
            pbr: None,
            textures: Vec::new(),
        };
        source_map.chunks.push(chunk_meta);

        // Register two sprites in 64x64 source sheet
        let loc_stone = AtlasSpriteLocation {
            chunk_id: 0,
            pixel_rect: [0, 0, 16, 16],
            frame_0_uv_bounds: [0.0, 0.0, 0.25, 0.25],
            ..Default::default()
        };
        let res_stone = ResourceLocation::new("minecraft", "block/stone");
        source_map.sprites.insert(res_stone, loc_stone);

        let loc_dirt = AtlasSpriteLocation {
            chunk_id: 0,
            pixel_rect: [16, 0, 16, 16],
            frame_0_uv_bounds: [0.25, 0.0, 0.5, 0.25],
            ..Default::default()
        };
        let res_dirt = ResourceLocation::new("minecraft", "block/dirt");
        source_map.sprites.insert(res_dirt, loc_dirt);

        // Fake image source with stone (red) and dirt (brown)
        let mut source_images = HashMap::new();
        let mut fake_source_sheet = RgbaBuffer::new(64, 64);
        fake_source_sheet.blit(
            &RgbaBuffer::solid(16, 16, 255, 0, 0, 255),
            0,
            0,
            0,
            0,
            16,
            16,
        );
        fake_source_sheet.blit(
            &RgbaBuffer::solid(16, 16, 139, 69, 19, 255),
            0,
            0,
            16,
            0,
            16,
            16,
        );
        source_images.insert("blocks_chunk_001.png".to_string(), fake_source_sheet);

        // 2. Prepare fake models
        let stone_model = create_sample_cube("minecraft:stone", "minecraft:block/stone");
        let dirt_model = create_sample_cube("minecraft:dirt", "minecraft:block/dirt");
        let bedrock_model = create_sample_cube("minecraft:bedrock", "minecraft:block/bedrock");

        let mut source_models = BakedModelDatabase::new();
        source_models.insert("minecraft:stone".to_string(), stone_model);
        source_models.insert("minecraft:dirt".to_string(), dirt_model);
        source_models.insert("minecraft:bedrock".to_string(), bedrock_model);

        // 3. Build a scene with only stone
        let mut voxels = SparseVoxelPayload::new();
        voxels.bounds = [0, 0, 0, 16, 16, 16];
        voxels.push(0, 0, 0, "minecraft:stone", "minecraft:plains", None, None);

        let config = SceneExporterConfig::default();
        let result = build_scene_interchange(
            &voxels,
            &source_models,
            &source_map,
            &mut source_images,
            &config,
            None,
        )
        .expect("Build scene interchange");

        // Verify tree-shaking: only stone model is retained!
        assert_eq!(result.tree_shaken_models.len(), 1);
        assert!(result.tree_shaken_models.get("minecraft:stone").is_some());
        assert!(result.tree_shaken_models.get("minecraft:bedrock").is_none());

        // Verify mini-atlas: contains 1 sprite (stone)
        assert_eq!(result.sprite_count, 1);
        assert_eq!(result.mini_atlas_textures.len(), 1);

        // Verify model UV remapping to mini-atlas bounds:
        let remapped_stone = result.tree_shaken_models.get("minecraft:stone").unwrap();
        let up_face = remapped_stone.culled_faces[0][0].clone();
        assert!(up_face.atlas_uvs.is_some());
        let atlas_uvs = up_face.atlas_uvs.unwrap();
        assert!(atlas_uvs[0].x >= 0.0 && atlas_uvs[0].x <= 1.0);
        assert!(atlas_uvs[0].y >= 0.0 && atlas_uvs[0].y <= 1.0);
    }

    #[test]
    fn test_uv_coordinate_formula_orientation() {
        let mut source_map = AtlasAddressMap::new();
        let chunk_meta = AtlasChunkMeta {
            chunk_id: 0,
            category: "blocks".to_string(),
            is_animated: false,
            category_chunk_index: 1,
            width: 32,
            height: 32,
            has_normal: false,
            has_specular: false,
            has_overlay: false,
            has_orm: false,
            has_aux: false,
            has_tint: false,
            pbr: None,
            textures: Vec::new(),
        };
        source_map.chunks.push(chunk_meta);

        let loc_stone = AtlasSpriteLocation {
            chunk_id: 0,
            pixel_rect: [0, 0, 16, 16],
            ..Default::default()
        };
        source_map
            .sprites
            .insert(ResourceLocation::new("minecraft", "block/stone"), loc_stone);

        let mut source_images = HashMap::new();
        source_images.insert(
            "blocks_chunk_001.png".to_string(),
            RgbaBuffer::solid(32, 32, 100, 100, 100, 255),
        );

        let mut source_models = BakedModelDatabase::new();
        source_models.insert(
            "minecraft:stone".to_string(),
            create_sample_cube("minecraft:stone", "minecraft:block/stone"),
        );

        let mut voxels = SparseVoxelPayload::new();
        voxels.push(0, 0, 0, "minecraft:stone", "minecraft:plains", None, None);

        let config = SceneExporterConfig::default();
        let result = build_scene_interchange(
            &voxels,
            &source_models,
            &source_map,
            &mut source_images,
            &config,
            None,
        )
        .expect("Build scene interchange");

        let loc = result
            .mini_atlas_map
            .lookup_str("minecraft:block/stone")
            .unwrap();
        // Since stone is the only 16x16 sprite in a 16x16 canvas, slot is x=0, y=0, w=16, h=16
        // Bottom edge in UV is 1.0 - (0 + 16)/16 = 0.0
        // Top edge in UV is 1.0 - 0/16 = 1.0
        assert_eq!(loc.frame_0_uv_bounds[0], 0.0); // u_min
        assert_eq!(loc.frame_0_uv_bounds[1], 0.0); // v_min
        assert_eq!(loc.frame_0_uv_bounds[2], 1.0); // u_max
        assert_eq!(loc.frame_0_uv_bounds[3], 1.0); // v_max
    }

    #[test]
    fn test_export_with_overlay_and_animated_strips() {
        let mut source_map = AtlasAddressMap::new();
        let static_chunk_meta = AtlasChunkMeta {
            chunk_id: 0,
            category: "blocks".to_string(),
            is_animated: false,
            category_chunk_index: 1,
            width: 32,
            height: 32,
            has_normal: false,
            has_specular: false,
            has_overlay: true,
            has_orm: false,
            has_aux: false,
            has_tint: true,
            pbr: None,
            textures: Vec::new(),
        };
        let anim_chunk_meta = AtlasChunkMeta {
            chunk_id: 1,
            category: "blocks".to_string(),
            is_animated: true,
            category_chunk_index: 1,
            width: 16,
            height: 32,
            has_normal: false,
            has_specular: false,
            has_overlay: false,
            has_orm: false,
            has_aux: false,
            has_tint: true,
            pbr: None,
            textures: Vec::new(),
        };
        source_map.chunks.push(static_chunk_meta);
        source_map.chunks.push(anim_chunk_meta);

        // Grass block side has overlay
        let loc_grass_side = AtlasSpriteLocation {
            chunk_id: 0,
            pixel_rect: [0, 0, 16, 16],
            has_overlay: true,
            ..Default::default()
        };
        source_map.sprites.insert(
            ResourceLocation::new("minecraft", "block/grass_block_side"),
            loc_grass_side,
        );

        // Water still is animated (16x32, 2 frames)
        let loc_water_static = AtlasSpriteLocation {
            chunk_id: 0,
            pixel_rect: [16, 0, 16, 16],
            is_animated: false,
            ..Default::default()
        };
        source_map.sprites.insert(
            ResourceLocation::new("minecraft", "block/water_still"),
            loc_water_static,
        );

        let loc_water_anim = AtlasSpriteLocation {
            chunk_id: 1,
            is_animated: true,
            strip_pixel_rect: [0, 0, 16, 32],
            frame_size: [16, 16],
            frame_count: 2,
            ..Default::default()
        };
        source_map.anim_sprites.insert(
            ResourceLocation::new("minecraft", "block/water_still"),
            loc_water_anim,
        );

        let mut source_images = HashMap::new();
        let mut static_sheet = RgbaBuffer::new(32, 32);
        static_sheet.blit(
            &RgbaBuffer::solid(16, 16, 120, 80, 50, 255),
            0,
            0,
            0,
            0,
            16,
            16,
        );
        source_images.insert("blocks_chunk_001.png".to_string(), static_sheet);

        let mut overlay_sheet = RgbaBuffer::new(32, 32);
        // Put non-zero alpha in overlay
        overlay_sheet.blit(
            &RgbaBuffer::solid(16, 16, 100, 200, 50, 255),
            0,
            0,
            0,
            0,
            16,
            16,
        );
        source_images.insert("blocks_chunk_001_overlay.png".to_string(), overlay_sheet);

        let anim_sheet = RgbaBuffer::solid(16, 32, 50, 100, 200, 255);
        source_images.insert("blocks_anim_chunk_001.png".to_string(), anim_sheet);

        let mut source_models = BakedModelDatabase::new();
        source_models.insert(
            "minecraft:grass_block".to_string(),
            create_sample_cube("minecraft:grass_block", "minecraft:block/grass_block_side"),
        );
        source_models.insert(
            "minecraft:water[level=0]".to_string(),
            create_sample_cube("minecraft:water[level=0]", "minecraft:block/water_still"),
        );

        let mut voxels = SparseVoxelPayload::new();
        voxels.push(
            0,
            0,
            0,
            "minecraft:grass_block",
            "minecraft:plains",
            None,
            None,
        );
        voxels.push(
            0,
            1,
            0,
            "minecraft:water[level=0]",
            "minecraft:plains",
            None,
            None,
        );

        let config = SceneExporterConfig::default();
        let result = build_scene_interchange(
            &voxels,
            &source_models,
            &source_map,
            &mut source_images,
            &config,
            None,
        )
        .expect("Build scene interchange with overlay and anim");

        assert!(result.metadata.has_animated_atlas);
        assert!(result
            .mini_atlas_textures
            .iter()
            .any(|(name, _)| name.contains("_overlay.png")));
        assert!(result
            .mini_atlas_textures
            .iter()
            .any(|(name, _)| name.contains("scene_anim_atlas_")));
        assert!(result
            .mini_atlas_map
            .anim_sprites
            .contains_key(&ResourceLocation::new("minecraft", "block/water_still")));
    }

    #[test]
    fn test_export_dynamic_entity_atlas_stitching() {
        let mut source_map = AtlasAddressMap::new();
        let chunk_meta = AtlasChunkMeta {
            chunk_id: 0,
            category: "blocks".to_string(),
            is_animated: false,
            category_chunk_index: 1,
            width: 32,
            height: 32,
            has_normal: false,
            has_specular: false,
            has_overlay: false,
            has_orm: false,
            has_aux: false,
            has_tint: false,
            pbr: None,
            textures: Vec::new(),
        };
        source_map.chunks.push(chunk_meta);

        // Add 0xFFF0 dynamic entity chunk to source map
        let dyn_chunk_meta = AtlasChunkMeta {
            chunk_id: DYNAMIC_CHUNK_START,
            category: "entities".to_string(),
            is_animated: false,
            category_chunk_index: 1,
            width: DYNAMIC_ATLAS_WIDTH,
            height: DYNAMIC_ATLAS_HEIGHT,
            has_normal: false,
            has_specular: false,
            has_overlay: false,
            has_orm: false,
            has_aux: false,
            has_tint: false,
            pbr: None,
            textures: Vec::new(),
        };
        source_map.chunks.push(dyn_chunk_meta);

        let skin_key = "test_player_skin";
        let skin_loc = AtlasSpriteLocation {
            chunk_id: DYNAMIC_CHUNK_START,
            pixel_rect: [0, 0, 64, 64],
            ..Default::default()
        };
        source_map.sprites.insert(
            ResourceLocation::new("minecraft", format!("skin/{}", skin_key)),
            skin_loc,
        );

        let mut source_images = HashMap::new();
        source_images.insert(
            "blocks_chunk_001.png".to_string(),
            RgbaBuffer::solid(32, 32, 100, 100, 100, 255),
        );

        let mut dynamic_entity_atlases = HashMap::new();
        let mut dyn_sheet = RgbaBuffer::new(DYNAMIC_ATLAS_WIDTH, DYNAMIC_ATLAS_HEIGHT);
        dyn_sheet.blit(
            &RgbaBuffer::solid(64, 64, 250, 180, 140, 255),
            0,
            0,
            0,
            0,
            64,
            64,
        );
        dynamic_entity_atlases.insert(DYNAMIC_CHUNK_START, dyn_sheet.pixels);

        let mut source_models = BakedModelDatabase::new();
        let skull_model = create_sample_cube(
            "minecraft:player_head[rotation=0]",
            &format!("minecraft:skin/{}", skin_key),
        );
        source_models.insert("minecraft:player_head[rotation=0]".to_string(), skull_model);

        let mut voxels = SparseVoxelPayload::new();
        let skull_entity = BlockEntityData::Skull {
            kind: mtk_model::entity::skull::SkullKind::Player,
            profile: Some(mtk_model::entity::skull::PlayerProfileData {
                id: Some(skin_key.to_string()),
                name: Some("TestPlayer".to_string()),
                skin_url: None,
                skin_base64: None,
            }),
            rotation: Some(0),
        };
        voxels.push(
            0,
            0,
            0,
            "minecraft:player_head[rotation=0]",
            "minecraft:plains",
            Some(&skull_entity),
            None,
        );

        let config = SceneExporterConfig::default();
        let result = build_scene_interchange(
            &voxels,
            &source_models,
            &source_map,
            &mut source_images,
            &config,
            Some(&dynamic_entity_atlases),
        )
        .expect("Build scene interchange with dynamic entities");

        // Verify that the skull model was remapped to mini-atlas chunk 0, NOT 0xFFF0!
        let remapped = result
            .tree_shaken_models
            .get("minecraft:player_head[rotation=0]")
            .unwrap();
        let face = &remapped.culled_faces[0][0];
        assert_eq!(
            face.atlas_chunk_id,
            Some(0),
            "Dynamic entity face must be stitched into normal mini-atlas chunk 0"
        );
    }

    #[test]
    fn test_export_scene_package_file_roundtrip() {
        let temp_path = std::env::temp_dir().join(format!(
            "test_export_scene_{}_{}.mtkscene",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        let mut source_map = AtlasAddressMap::new();
        let chunk_meta = AtlasChunkMeta {
            chunk_id: 0,
            category: "blocks".to_string(),
            is_animated: false,
            category_chunk_index: 1,
            width: 32,
            height: 32,
            has_normal: false,
            has_specular: false,
            has_overlay: false,
            has_orm: false,
            has_aux: false,
            has_tint: false,
            pbr: None,
            textures: Vec::new(),
        };
        source_map.chunks.push(chunk_meta);

        let loc_stone = AtlasSpriteLocation {
            chunk_id: 0,
            pixel_rect: [0, 0, 16, 16],
            frame_0_uv_bounds: [0.0, 0.0, 0.5, 0.5],
            ..Default::default()
        };
        source_map
            .sprites
            .insert(ResourceLocation::new("minecraft", "block/stone"), loc_stone);

        let mut source_images = HashMap::new();
        let mut fake_source_sheet = RgbaBuffer::new(32, 32);
        fake_source_sheet.blit(
            &RgbaBuffer::solid(16, 16, 100, 100, 100, 255),
            0,
            0,
            0,
            0,
            16,
            16,
        );
        source_images.insert("blocks_chunk_001.png".to_string(), fake_source_sheet);

        let mut source_models = BakedModelDatabase::new();
        source_models.insert(
            "minecraft:stone".to_string(),
            create_sample_cube("minecraft:stone", "minecraft:block/stone"),
        );

        let mut voxels = SparseVoxelPayload::new();
        voxels.bounds = [10, 60, 10, 5, 5, 5];
        voxels.push(
            10,
            60,
            10,
            "minecraft:stone",
            "minecraft:plains",
            None,
            Some(15),
        );

        let config = SceneExporterConfig::default();
        let export_res = export_scene_package(
            &temp_path,
            &voxels,
            &source_models,
            &source_map,
            &mut source_images,
            &config,
            None,
            None,
        )
        .expect("Export scene package file");

        assert_eq!(export_res.sprite_count, 1);

        // Read package back from disk
        let reader = crate::scene::ScenePackageReader::open_file(&temp_path)
            .expect("Open exported scene package");
        assert_eq!(reader.metadata().voxel_count, 1);
        assert_eq!(reader.metadata().bounds, [10, 60, 10, 5, 5, 5]);

        let read_voxels = reader.read_sparse_voxels().expect("Read voxels");
        assert_eq!(read_voxels.len(), 1);
        assert_eq!(read_voxels.state_palette, vec!["minecraft:stone"]);

        let read_models = reader
            .read_models()
            .expect("Read models")
            .expect("Models must exist");
        assert_eq!(read_models.len(), 1);
        assert!(read_models.get("minecraft:stone").is_some());

        let read_mapping = reader
            .read_atlas_mapping()
            .expect("Read atlas mapping")
            .expect("Mapping must exist");
        assert!(read_mapping.lookup_str("minecraft:block/stone").is_some());

        let read_textures = reader.read_atlas_textures().expect("Read atlas textures");
        assert_eq!(read_textures.len(), 1);

        let _ = std::fs::remove_file(&temp_path);
    }
}
