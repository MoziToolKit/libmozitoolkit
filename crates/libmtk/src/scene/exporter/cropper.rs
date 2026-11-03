//! # Scene Sprite Cropper
//!
//! Extracts only the referenced textures from the source global atlas,
//! dynamic entity buffers, or banner procedural synthesis into canonical `DecodedSprite`s.

use std::collections::{BTreeSet, HashMap};

use mtk_resource::ResourceLocation;
use mtk_texture::atlas::{
    AtlasAddressMap, DYNAMIC_ATLAS_HEIGHT, DYNAMIC_ATLAS_WIDTH, DYNAMIC_CHUNK_START,
};
use mtk_texture::banner::BannerBaker;
use mtk_texture::{DecodedSprite, RgbaBuffer};

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

/// A cropped sprite pairing its original string key with a canonical `DecodedSprite`.
#[derive(Debug, Clone)]
pub struct CroppedSprite {
    pub key: String,
    pub sprite: DecodedSprite,
}

fn parse_res_loc(key: &str) -> ResourceLocation {
    ResourceLocation::parse_texture_path(key)
        .or_else(|_| ResourceLocation::parse(key))
        .unwrap_or_else(|_| ResourceLocation::new("minecraft", key))
}

/// Crops referenced sprites from atlas source, dynamic entity buffers, or banner synthesis.
pub fn crop_needed_sprites(
    needed_textures: &BTreeSet<String>,
    source_atlas_map: &AtlasAddressMap,
    atlas_source: &mut impl SceneAtlasSource,
    dynamic_entity_atlases: Option<&HashMap<u16, Vec<u8>>>,
) -> Result<Vec<CroppedSprite>, MtkError> {
    let mut loaded_albedo: HashMap<u16, Option<RgbaBuffer>> = HashMap::new();
    let mut loaded_normal: HashMap<u16, Option<RgbaBuffer>> = HashMap::new();
    let mut loaded_overlay: HashMap<u16, Option<RgbaBuffer>> = HashMap::new();

    let mut crops = Vec::new();
    let mut banner_baker = BannerBaker::default();

    for tex_key in needed_textures {
        let maybe_loc = source_atlas_map.lookup_str(tex_key);

        // Case A: Dynamic entity sprite in chunk 0xFFF0..=0xFFFF
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
                                    sprite: DecodedSprite {
                                        sprite_id: parse_res_loc(tex_key),
                                        albedo,
                                        normal: None,
                                        specular: None,
                                        overlay: None,
                                        emissive: None,
                                        frame_width: w,
                                        frame_height: h,
                                        frame_count: 1,
                                        metadata: None,
                                    },
                                });
                                continue;
                            }
                        }
                    }
                }
                // Zero fake fallback: skip unregistered/missing dynamic entities cleanly
                continue;
            }
        }

        // Case B: Entity texture fallback when not registered in source atlas map
        if maybe_loc.is_none() {
            // Check if it's a banner parsed via authoritative banner model module
            if let Some((base_color, patterns)) =
                mtk_model::entity::banner::parse_banner_texture_identifier(tex_key)
            {
                let pat_refs: Vec<(&str, &str)> = patterns
                    .iter()
                    .map(|p| (p.pattern.as_str(), p.color.as_str()))
                    .collect();
                let baked = banner_baker.bake(&base_color, &pat_refs);
                let (w, h) = (baked.width, baked.height);
                crops.push(CroppedSprite {
                    key: tex_key.clone(),
                    sprite: DecodedSprite {
                        sprite_id: parse_res_loc(tex_key),
                        albedo: (*baked).clone(),
                        normal: None,
                        specular: None,
                        overlay: None,
                        emissive: None,
                        frame_width: w,
                        frame_height: h,
                        frame_count: 1,
                        metadata: None,
                    },
                });
                continue;
            }

            // Check if it's a player skin
            if tex_key.starts_with("minecraft:skin/") {
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
                            let albedo = full.crop(0, 0, 64, 64);
                            crops.push(CroppedSprite {
                                key: tex_key.clone(),
                                sprite: DecodedSprite {
                                    sprite_id: parse_res_loc(tex_key),
                                    albedo,
                                    normal: None,
                                    specular: None,
                                    overlay: None,
                                    emissive: None,
                                    frame_width: 64,
                                    frame_height: 64,
                                    frame_count: 1,
                                    metadata: None,
                                },
                            });
                            continue;
                        }
                    }
                }
                // Zero fake fallback: no pink RgbaBuffer::solid
                continue;
            }

            continue;
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
                e.insert(alb);
                loaded_normal.insert(anim_chunk_id, norm);
            }

            if let Some(anim_alb_buf) = loaded_albedo.get(&anim_chunk_id).and_then(|o| o.as_ref()) {
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

                    let frame_h = anim_loc.frame_size[1].max(1);
                    let frame_count = anim_loc.frame_count.max(1);

                    crops.push(CroppedSprite {
                        key: tex_key.clone(),
                        sprite: DecodedSprite {
                            sprite_id: parse_res_loc(tex_key),
                            albedo: anim_alb_crop,
                            normal: anim_norm_crop,
                            specular: None,
                            overlay: None,
                            emissive: None,
                            frame_width: aw,
                            frame_height: frame_h,
                            frame_count,
                            metadata: anim_loc.animation.clone(),
                        },
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
            let over = atlas_source.get_overlay(chunk_id, &stem)?;
            e.insert(alb);
            loaded_normal.insert(chunk_id, norm);
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

        crops.push(CroppedSprite {
            key: tex_key.clone(),
            sprite: DecodedSprite {
                sprite_id: parse_res_loc(tex_key),
                albedo: albedo_crop,
                normal: normal_crop,
                specular: None,
                overlay: overlay_crop,
                emissive: None,
                frame_width: w,
                frame_height: h,
                frame_count: 1,
                metadata: None,
            },
        });
    }

    Ok(crops)
}
