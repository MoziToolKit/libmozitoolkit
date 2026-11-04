//! # Scene Sprite Cropper
//!
//! Extracts only the referenced textures from the source global atlas,
//! dynamic entity buffers, or banner procedural synthesis into canonical `DecodedSprite`s.

use std::collections::{BTreeSet, HashMap, HashSet};

use mtk_resource::ResourceLocation;
use mtk_texture::atlas::{
    AtlasAddressMap, DYNAMIC_ATLAS_HEIGHT, DYNAMIC_ATLAS_WIDTH, DYNAMIC_CHUNK_START,
};
use mtk_texture::banner::BannerBaker;
use mtk_texture::{DecodedSprite, RgbaBuffer};

use crate::MtkError;

/// Source providing rasterized texture layers (Albedo, Normal, ORM, AUX, Overlay) for atlas chunks.
pub trait AtlasLayerSource {
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

/// Source providing resident colormap PNG buffers and BiomeResolver mapping JSON.
pub trait BiomeAssetSource {
    /// Loads a biome colormap PNG buffer (e.g. `"grass"`, `"foliage"`, `"dry_foliage"`).
    fn get_colormap(&mut self, name: &str) -> Result<Option<Vec<u8>>, MtkError>;

    /// Loads the authoritative BiomeResolver JSON if present in the cache.
    fn get_biome_resolver_json(&mut self) -> Result<Option<String>, MtkError>;
}

/// Source providing CTM (Connected Textures Mod) rules.
pub trait CtmAssetSource {
    /// Loads cached CTM rules JSON if present in the cache or source.
    fn get_ctm_rules_json(&mut self) -> Result<Option<String>, MtkError>;
}

/// Source providing procedural entity baking assets (e.g. banner base and patterns).
pub trait EntityAssetSource {
    /// Loads a pre-populated `BannerBaker` containing cached base cloth and pattern sprites.
    fn get_banner_baker(&mut self) -> Result<Option<BannerBaker>, MtkError>;
}

/// Capabilities exposed by a `SceneAtlasSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SceneSourceCapabilities {
    pub has_colormaps: bool,
    pub has_biome_resolver: bool,
    pub has_ctm_rules: bool,
    pub has_banner_baker: bool,
}

/// Composite trait bundling all asset sources required for scene package export.
pub trait SceneAtlasSource:
    AtlasLayerSource + BiomeAssetSource + CtmAssetSource + EntityAssetSource
{
    /// Returns the active capabilities supported by this asset source.
    fn capabilities(&self) -> SceneSourceCapabilities;
}

impl AtlasLayerSource for crate::prebake::AssetCacheReader {
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

impl BiomeAssetSource for crate::prebake::AssetCacheReader {
    fn get_colormap(&mut self, name: &str) -> Result<Option<Vec<u8>>, MtkError> {
        let chunk_path = format!("biome/colormap/{name}");
        if self.package_reader().has_chunk(&chunk_path) {
            let bytes = self.package_reader().read_chunk_decompressed(&chunk_path)?;
            Ok(Some(bytes))
        } else {
            Ok(None)
        }
    }

    fn get_biome_resolver_json(&mut self) -> Result<Option<String>, MtkError> {
        if self.package_reader().has_chunk("biome/mapping") {
            let json = self.package_reader().read_chunk_str("biome/mapping")?;
            Ok(Some(json))
        } else if self.package_reader().has_chunk("biome/resolver") {
            let json = self.package_reader().read_chunk_str("biome/resolver")?;
            Ok(Some(json))
        } else {
            Ok(None)
        }
    }
}

impl CtmAssetSource for crate::prebake::AssetCacheReader {
    fn get_ctm_rules_json(&mut self) -> Result<Option<String>, MtkError> {
        if self.package_reader().has_chunk("ctm/rules") {
            let json = self.package_reader().read_chunk_str("ctm/rules")?;
            Ok(Some(json))
        } else {
            Ok(None)
        }
    }
}

impl EntityAssetSource for crate::prebake::AssetCacheReader {
    fn get_banner_baker(&mut self) -> Result<Option<BannerBaker>, MtkError> {
        self.load_banner_baker().map(Some)
    }
}

impl SceneAtlasSource for crate::prebake::AssetCacheReader {
    fn capabilities(&self) -> SceneSourceCapabilities {
        SceneSourceCapabilities {
            has_colormaps: self.package_reader().has_chunk("biome/colormap/grass")
                || self.package_reader().has_chunk("biome/colormap/foliage")
                || self
                    .package_reader()
                    .has_chunk("biome/colormap/dry_foliage"),
            has_biome_resolver: self.package_reader().has_chunk("biome/mapping")
                || self.package_reader().has_chunk("biome/resolver"),
            has_ctm_rules: self.package_reader().has_chunk("ctm/rules"),
            has_banner_baker: self.package_reader().has_chunk("banner/base")
                || self.package_reader().has_chunk("banner/patterns"),
        }
    }
}

impl AtlasLayerSource for HashMap<String, RgbaBuffer> {
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

impl BiomeAssetSource for HashMap<String, RgbaBuffer> {
    fn get_colormap(&mut self, _name: &str) -> Result<Option<Vec<u8>>, MtkError> {
        Ok(None)
    }

    fn get_biome_resolver_json(&mut self) -> Result<Option<String>, MtkError> {
        Ok(None)
    }
}

impl CtmAssetSource for HashMap<String, RgbaBuffer> {
    fn get_ctm_rules_json(&mut self) -> Result<Option<String>, MtkError> {
        Ok(None)
    }
}

impl EntityAssetSource for HashMap<String, RgbaBuffer> {
    fn get_banner_baker(&mut self) -> Result<Option<BannerBaker>, MtkError> {
        Ok(None)
    }
}

impl SceneAtlasSource for HashMap<String, RgbaBuffer> {
    fn capabilities(&self) -> SceneSourceCapabilities {
        SceneSourceCapabilities::default()
    }
}

/// A cropped sprite pairing its original string key and aliases with a canonical `DecodedSprite`.
#[derive(Debug, Clone)]
pub struct CroppedSprite {
    pub key: String,
    pub aliases: Vec<String>,
    pub sprite: DecodedSprite,
    pub is_dynamic: bool,
}

fn parse_res_loc(key: &str) -> ResourceLocation {
    ResourceLocation::parse_texture_path(key)
        .or_else(|_| ResourceLocation::parse(key))
        .unwrap_or_else(|_| ResourceLocation::new("minecraft", key))
}

fn crop_optional_layer(
    layer_map: &HashMap<u16, Option<RgbaBuffer>>,
    chunk_id: u16,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Option<RgbaBuffer> {
    layer_map
        .get(&chunk_id)
        .and_then(|opt| opt.as_ref())
        .and_then(|b| {
            if x + w <= b.width && y + h <= b.height {
                Some(b.crop(x, y, w, h))
            } else {
                None
            }
        })
}

fn crop_optional_overlay_layer(
    layer_map: &HashMap<u16, Option<RgbaBuffer>>,
    chunk_id: u16,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Option<RgbaBuffer> {
    crop_optional_layer(layer_map, chunk_id, x, y, w, h)
        .filter(|c| c.pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0))
}

/// Crops referenced sprites from atlas source, dynamic entity buffers, or banner synthesis.
pub fn crop_needed_sprites(
    needed_textures: &BTreeSet<String>,
    source_atlas_map: &AtlasAddressMap,
    atlas_source: &mut impl SceneAtlasSource,
    dynamic_entity_atlases: Option<&HashMap<u16, Vec<u8>>>,
    dynamic_entity_slots: Option<&HashMap<String, (u16, u16)>>,
) -> Result<Vec<CroppedSprite>, MtkError> {
    let mut loaded_albedo: HashMap<u16, Option<RgbaBuffer>> = HashMap::new();
    let mut loaded_normal: HashMap<u16, Option<RgbaBuffer>> = HashMap::new();
    let mut loaded_orm: HashMap<u16, Option<RgbaBuffer>> = HashMap::new();
    let mut loaded_aux: HashMap<u16, Option<RgbaBuffer>> = HashMap::new();
    let mut loaded_overlay: HashMap<u16, Option<RgbaBuffer>> = HashMap::new();

    let mut crops = Vec::new();
    let mut banner_baker_opt = atlas_source.get_banner_baker()?;
    let mut visited_sprite_ids = HashSet::new();

    // Group needed textures by canonical ResourceLocation so no alias is lost
    let mut sprite_id_to_keys: HashMap<ResourceLocation, Vec<String>> = HashMap::new();
    for tex_key in needed_textures {
        let sprite_id = parse_res_loc(tex_key);
        sprite_id_to_keys
            .entry(sprite_id)
            .or_default()
            .push(tex_key.clone());
    }

    for tex_key in needed_textures {
        let sprite_id = parse_res_loc(tex_key);
        if !visited_sprite_ids.insert(sprite_id.clone()) {
            continue;
        }
        let aliases = sprite_id_to_keys
            .get(&sprite_id)
            .cloned()
            .unwrap_or_default();

        let maybe_loc = source_atlas_map.lookup_str(tex_key).or_else(|| {
            if let Some(stripped) = tex_key.strip_prefix("minecraft:skin/") {
                source_atlas_map
                    .lookup_str(&format!("skin/{}", stripped))
                    .or_else(|| source_atlas_map.lookup_str(stripped))
            } else if let Some(stripped) = tex_key.strip_prefix("skin/") {
                source_atlas_map
                    .lookup_str(&format!("minecraft:skin/{}", stripped))
                    .or_else(|| source_atlas_map.lookup_str(stripped))
            } else {
                None
            }
        });

        // Case A: Dynamic entity sprite in chunk 0xFFF0..=0xFFFF
        let mut cropped_from_dynamic = false;
        if let Some(sprite_loc) = maybe_loc {
            if sprite_loc.chunk_id >= DYNAMIC_CHUNK_START {
                let chunk_id = sprite_loc.chunk_id;
                let [x, y, w, h] = sprite_loc.pixel_rect;

                if let Some(dyn_map) = dynamic_entity_atlases {
                    if let Some(raw_bytes) = dyn_map.get(&chunk_id) {
                        if raw_bytes.len()
                            >= (DYNAMIC_ATLAS_WIDTH * DYNAMIC_ATLAS_HEIGHT * 4) as usize
                        {
                            let full_buf = RgbaBuffer {
                                width: DYNAMIC_ATLAS_WIDTH,
                                height: DYNAMIC_ATLAS_HEIGHT,
                                pixels: raw_bytes.clone(),
                            };
                            if w > 0 && h > 0 && x + w <= full_buf.width && y + h <= full_buf.height
                            {
                                let albedo = full_buf.crop(x, y, w, h);
                                crops.push(CroppedSprite {
                                    key: tex_key.clone(),
                                    aliases: aliases.clone(),
                                    sprite: DecodedSprite::simple(sprite_id.clone(), albedo, w, h),
                                    is_dynamic: true,
                                });
                                cropped_from_dynamic = true;
                            }
                        }
                    }
                }
            }
        }
        if cropped_from_dynamic {
            continue;
        }

        // Case B: Entity texture fallback when not cropped from dynamic chunk
        // 1. Procedural Banner Synthesis
        if let Some((base_color, patterns)) =
            mtk_model::entity::banner::parse_banner_texture_identifier(tex_key)
        {
            if let Some(ref mut baker) = banner_baker_opt {
                let pat_refs: Vec<(&str, &str)> = patterns
                    .iter()
                    .map(|p| (p.pattern.as_str(), p.color.as_str()))
                    .collect();
                let baked = baker.bake(&base_color, &pat_refs);
                let (w, h) = (baked.width, baked.height);
                crops.push(CroppedSprite {
                    key: tex_key.clone(),
                    aliases: aliases.clone(),
                    sprite: DecodedSprite::simple(sprite_id.clone(), (*baked).clone(), w, h),
                    is_dynamic: true,
                });
                continue;
            } else {
                return Err(MtkError::Resource(
                    mtk_resource::error::ResourceError::NotFound(format!(
                        "Banner texture '{tex_key}' referenced but no BannerBaker pattern assets are available in cache"
                    )),
                ));
            }
        }

        // 2. Player Skin Dynamic Crop
        let is_skin = tex_key.starts_with("minecraft:skin/") || tex_key.starts_with("skin/");
        if is_skin {
            if let Some(dyn_map) = dynamic_entity_atlases {
                let stripped = tex_key
                    .strip_prefix("minecraft:skin/")
                    .or_else(|| tex_key.strip_prefix("skin/"))
                    .unwrap_or(tex_key);

                let base_name = stripped
                    .strip_prefix("uuid_")
                    .or_else(|| stripped.strip_prefix("player_"))
                    .unwrap_or(stripped);

                let loc_opt = maybe_loc.or_else(|| {
                    source_atlas_map
                        .lookup_str(&format!("minecraft:skin/{base_name}"))
                        .or_else(|| source_atlas_map.lookup_str(&format!("skin/{base_name}")))
                        .or_else(|| source_atlas_map.lookup_str(base_name))
                });

                let resolved_slot: Option<(u16, u16)> = loc_opt
                    .and_then(|loc| {
                        if loc.chunk_id >= DYNAMIC_CHUNK_START {
                            if let Some(s) = loc.slot_index {
                                Some((loc.chunk_id, s))
                            } else {
                                let col = loc.pixel_rect[0] / 64;
                                let row = loc.pixel_rect[1] / 64;
                                Some((loc.chunk_id, (row * 16 + col) as u16))
                            }
                        } else {
                            None
                        }
                    })
                    .or_else(|| {
                        dynamic_entity_slots.and_then(|slots| {
                            slots
                                .get(tex_key)
                                .or_else(|| slots.get(stripped))
                                .or_else(|| slots.get(base_name))
                                .or_else(|| slots.get(&format!("minecraft:skin/{stripped}")))
                                .or_else(|| slots.get(&format!("minecraft:skin/{base_name}")))
                                .or_else(|| {
                                    slots.iter().find_map(|(k, v)| {
                                        if k.eq_ignore_ascii_case(tex_key)
                                            || k.eq_ignore_ascii_case(stripped)
                                            || k.eq_ignore_ascii_case(base_name)
                                        {
                                            Some(v)
                                        } else {
                                            None
                                        }
                                    })
                                })
                                .copied()
                        })
                    })
                    .or_else(|| {
                        stripped
                            .strip_prefix("slot_")
                            .and_then(|s| s.parse::<u16>().ok())
                            .map(|slot_idx| {
                                let chunk_offset = slot_idx / 256;
                                (DYNAMIC_CHUNK_START + chunk_offset, slot_idx % 256)
                            })
                    });

                if let Some((target_chunk_id, in_chunk_slot)) = resolved_slot {
                    let col = (in_chunk_slot as u32) % 16;
                    let row = (in_chunk_slot as u32) / 16;
                    let px = col * 64;
                    let py = row * 64;

                    if let Some(target_bytes) = dyn_map.get(&target_chunk_id) {
                        if target_bytes.len()
                            >= (DYNAMIC_ATLAS_WIDTH * DYNAMIC_ATLAS_HEIGHT * 4) as usize
                            && px + 64 <= DYNAMIC_ATLAS_WIDTH
                            && py + 64 <= DYNAMIC_ATLAS_HEIGHT
                        {
                            let full = RgbaBuffer {
                                width: DYNAMIC_ATLAS_WIDTH,
                                height: DYNAMIC_ATLAS_HEIGHT,
                                pixels: target_bytes.clone(),
                            };
                            let albedo = full.crop(px, py, 64, 64);
                            crops.push(CroppedSprite {
                                key: tex_key.clone(),
                                aliases: aliases.clone(),
                                sprite: DecodedSprite::simple(sprite_id.clone(), albedo, 64, 64),
                                is_dynamic: true,
                            });
                            continue;
                        } else {
                            return Err(MtkError::Resource(
                                mtk_resource::error::ResourceError::NotFound(format!(
                                    "Dynamic entity atlas buffer for chunk {} is invalid or too small for slot {} (texture '{}')",
                                    target_chunk_id, in_chunk_slot, tex_key
                                )),
                            ));
                        }
                    } else {
                        return Err(MtkError::Resource(
                            mtk_resource::error::ResourceError::NotFound(format!(
                                "Dynamic entity chunk {} not found in dynamic entity atlases for texture '{}'",
                                target_chunk_id, tex_key
                            )),
                        ));
                    }
                } else {
                    return Err(MtkError::Resource(
                        mtk_resource::error::ResourceError::NotFound(format!(
                            "Unresolved slot for dynamic entity texture '{}': not found in atlas address map or dynamic entity slots",
                            tex_key
                        )),
                    ));
                }
            } else {
                return Err(MtkError::Resource(
                    mtk_resource::error::ResourceError::NotFound(format!(
                        "Dynamic entity texture '{}' requires dynamic entity atlases, but none were provided",
                        tex_key
                    )),
                ));
            }
        }

        // Case C: Standard static or animated sprite from source atlas
        let sprite_loc = if let Some(loc) = maybe_loc {
            loc
        } else {
            continue;
        };

        // Check if there is an animated strip companion for this texture in source_atlas_map
        if let Some(anim_loc) = source_atlas_map.lookup_animated_str(tex_key) {
            let anim_chunk_id = anim_loc.chunk_id;
            let anim_stem = source_atlas_map
                .get_chunk_meta(anim_chunk_id)
                .map(|m| m.file_stem())
                .unwrap_or_else(|| format!("blocks_anim_chunk_{:03}", anim_chunk_id + 1));

            if let std::collections::hash_map::Entry::Vacant(e) = loaded_albedo.entry(anim_chunk_id)
            {
                let alb = atlas_source.get_albedo(anim_chunk_id, &anim_stem)?;
                let norm = atlas_source.get_normal(anim_chunk_id, &anim_stem)?;
                let orm = atlas_source.get_orm(anim_chunk_id, &anim_stem)?;
                let aux = atlas_source.get_aux(anim_chunk_id, &anim_stem)?;
                let over = atlas_source.get_overlay(anim_chunk_id, &anim_stem)?;
                e.insert(alb);
                loaded_normal.insert(anim_chunk_id, norm);
                loaded_orm.insert(anim_chunk_id, orm);
                loaded_aux.insert(anim_chunk_id, aux);
                loaded_overlay.insert(anim_chunk_id, over);
            }

            if let Some(anim_alb_buf) = loaded_albedo.get(&anim_chunk_id).and_then(|o| o.as_ref()) {
                let [ax, ay, aw, ah] = anim_loc.strip_pixel_rect;
                if aw > 0
                    && ah > 0
                    && ax + aw <= anim_alb_buf.width
                    && ay + ah <= anim_alb_buf.height
                {
                    let anim_alb_crop = anim_alb_buf.crop(ax, ay, aw, ah);
                    let anim_norm_crop =
                        crop_optional_layer(&loaded_normal, anim_chunk_id, ax, ay, aw, ah);
                    let anim_orm_crop =
                        crop_optional_layer(&loaded_orm, anim_chunk_id, ax, ay, aw, ah);
                    let anim_aux_crop =
                        crop_optional_layer(&loaded_aux, anim_chunk_id, ax, ay, aw, ah);
                    let anim_overlay_crop =
                        crop_optional_overlay_layer(&loaded_overlay, anim_chunk_id, ax, ay, aw, ah);

                    let frame_h = anim_loc.frame_size[1].max(1);
                    let frame_count = anim_loc.frame_count.max(1);

                    crops.push(CroppedSprite {
                        key: tex_key.clone(),
                        aliases: aliases.clone(),
                        sprite: DecodedSprite {
                            sprite_id: sprite_id.clone(),
                            albedo: anim_alb_crop,
                            normal: None,
                            specular: None,
                            overlay: anim_overlay_crop,
                            emissive: None,
                            normal_gl: anim_norm_crop,
                            orm: anim_orm_crop,
                            aux: anim_aux_crop,
                            frame_width: aw,
                            frame_height: frame_h,
                            frame_count,
                            metadata: anim_loc.animation.clone(),
                        },
                        is_dynamic: false,
                    });
                    continue;
                }
            }
        }

        // Static sprite path
        let chunk_id = sprite_loc.chunk_id;
        let chunk_meta = source_atlas_map.get_chunk_meta(chunk_id);
        let stem = chunk_meta
            .map(|m| m.file_stem())
            .unwrap_or_else(|| format!("blocks_chunk_{:03}", chunk_id + 1));

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
        let normal_crop = crop_optional_layer(&loaded_normal, chunk_id, x, y, w, h);
        let orm_crop = crop_optional_layer(&loaded_orm, chunk_id, x, y, w, h);
        let aux_crop = crop_optional_layer(&loaded_aux, chunk_id, x, y, w, h);
        let overlay_crop = crop_optional_overlay_layer(&loaded_overlay, chunk_id, x, y, w, h);

        crops.push(CroppedSprite {
            key: tex_key.clone(),
            aliases,
            sprite: DecodedSprite {
                sprite_id,
                albedo: albedo_crop,
                normal: None,
                specular: None,
                overlay: overlay_crop,
                emissive: None,
                normal_gl: normal_crop,
                orm: orm_crop,
                aux: aux_crop,
                frame_width: w,
                frame_height: h,
                frame_count: 1,
                metadata: None,
            },
            is_dynamic: false,
        });
    }

    Ok(crops)
}
